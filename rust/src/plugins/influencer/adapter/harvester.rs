//! One-source-at-a-time Influencer delivery from verified Harvester context.
//! The legacy packet path remains available for legacy work.
use super::{persist_to_vibe_scores, production_options, record_ledger, record_vibe_completed};
use crate::application::models::ExecutionCapabilities;
use crate::application::queue::publication::ClaimPublication;
use crate::application::queue::work::Item;
use crate::evidence::corpus::lookup_entity_name;
use crate::evidence::memories::{self, MemoryRequest, Mission};
use crate::plugins::harvester::delivery::{load_for_character, SourceContext};
use crate::plugins::influencer::cognition::{
    VibeOutput, VibeParser, VibeReply, VibeScore, VIBE_SYSTEM_PROMPT, VIBE_TEMPERATURE,
};
use crate::plugins::support::form;
use crate::studio::model::Inference;
use crate::studio::plugin::PluginOutcome;
use crate::studio::{Generation, GenerationCall, Parser, Studio};
use crate::util::hash_components;
use anyhow::{ensure, Result};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use std::time::Duration;

const PROMPT_VERSION: &str = "vibe-source-v2";
const GATE_PROMPT_VERSION: &str = "vibe-source-reaction-v2";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReactionReply {
    has_reaction: bool,
    evidence_quote: String,
}

struct ReactionParser<'a>(&'a SourceContext);

#[derive(Debug)]
struct ReactionQuoteMismatch;

impl std::fmt::Display for ReactionQuoteMismatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Influencer reaction quote is not an exact publisher span"
        )
    }
}

impl std::error::Error for ReactionQuoteMismatch {}

fn reaction_correction(error: &anyhow::Error) -> Option<String> {
    if error.is::<ReactionQuoteMismatch>() {
        Some("Your previous evidence_quote was not an exact continuous substring of the supplied headline or publisher opening. Re-evaluate the source. If there is a human emotional reaction, copy its exact words with identical spelling, punctuation, and HTML entities. Otherwise return has_reaction false with an empty quote. Return the complete JSON object again.".into())
    } else {
        crate::plugins::support::prompt::structured_correction(error)
    }
}

impl Parser<ReactionReply> for ReactionParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<ReactionReply>> {
        let mut reply: ReactionReply = serde_json::from_str(raw)?;
        if reply.has_reaction {
            if reply.evidence_quote.trim().is_empty()
                || reply.evidence_quote.chars().count() > 500
                || !(self.0.headline.contains(&reply.evidence_quote)
                    || self.0.context.contains(&reply.evidence_quote))
            {
                return Err(ReactionQuoteMismatch.into());
            }
        } else {
            // A no-reaction verdict cannot cite evidence. Drop a superfluous
            // quote while keeping the raw response in model provenance.
            reply.evidence_quote.clear();
        }
        Ok(Some(reply))
    }
}

struct ReactionDecision {
    has_reaction: bool,
    provenance: serde_json::Value,
}

async fn decide_reaction(
    backend: &dyn Inference,
    voice_num_ctx: i32,
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    source: &SourceContext,
) -> Result<ReactionDecision> {
    let built_prompt = format!(
        "Entity: {entity_type} {entity_name} ({sport})\nPublisher: {}\nArticle ID: {}\nExact headline: {}\nExact publisher opening (unchanged):\n{}\n\nDoes this exact source report an observed human emotional reaction to this entity? Cheering, chanting, booing, or a quoted fan/player/coach feeling can qualify. A routine announcement, administrative schedule, article tone, or the absence of emotion does not. Return JSON {{\"has_reaction\":true|false,\"evidence_quote\":\"\"}}. When true, copy one exact continuous span from the headline or opening that reports the human reaction. When false, use an empty quote. Do not infer a mood from neutral prose.",
        source.source, source.article_id, source.headline, source.context
    );
    let input_hash = hash_components(
        &json!({
            "prompt_version": GATE_PROMPT_VERSION,
            "entity_type": entity_type,
            "entity_name": entity_name,
            "sport": sport,
            "classification_id": source.classification_id,
            "article_id": source.article_id,
            "headline": source.headline,
            "opening": source.context,
        })
        .to_string(),
    );
    let options = crate::studio::model::GenerateOptions {
        system: Some("Judge whether one exact publisher source contains a human emotional reaction to the named entity. Do not score neutral article tone or routine logistics as mood. Copy exact source words for a positive.".into()),
        temperature: Some(0.0),
        num_predict: 180,
        num_ctx: voice_num_ctx,
        json_mode: true,
        format_schema: Some(json!({
            "type":"object","additionalProperties":false,
            "required":["has_reaction","evidence_quote"],
            "properties":{
                "has_reaction":{"type":"boolean"},
                "evidence_quote":{"type":"string"}
            }
        })),
        format_schema_raw: None,
    };
    let extracted = Studio::new(backend)
        .extract(
            &built_prompt,
            &options,
            &ReactionParser(source),
            reaction_correction,
        )
        .await?;
    let reply = extracted
        .value
        .ok_or_else(|| anyhow::anyhow!("Influencer reaction verdict did not commit"))?;
    Ok(ReactionDecision {
        has_reaction: reply.has_reaction,
        provenance: json!({
            "article_id": source.article_id,
            "model_version": extracted.model,
            "prompt_version": GATE_PROMPT_VERSION,
            "input_hash": input_hash,
            "has_reaction": reply.has_reaction,
            "evidence_quote": reply.evidence_quote,
            "raw_verdict": extracted.raw_response,
            "eval_count": extracted.eval_count,
            "wall_ms": extracted.wall_ms,
        }),
    })
}

struct Evaluation {
    output: Option<VibeOutput>,
    provenance: serde_json::Value,
}

struct OptionalVibeParser;

impl Parser<VibeReply> for OptionalVibeParser {
    fn parse(&self, raw: &str) -> Result<Option<VibeReply>> {
        if serde_json::from_str::<serde_json::Value>(raw)? == serde_json::Value::Null {
            return Ok(None);
        }
        VibeParser.parse(raw)
    }
}

fn prompt(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    source: &SourceContext,
    memory: &str,
) -> String {
    format!(
        "Entity: {entity_type} {entity_name} ({sport})\n\n{memory}\n\nPublisher: {}\nArticle ID: {}\nHeadline: {}\nExact publisher opening:\n{}\n\nEvaluate the emotional evidence in this source for this entity. Reporting tone alone is not a crowd reaction. If this source offers no grounded Influencer reading, return JSON null. A card must rely on this one source; do not turn background memory into a new claim.",
        source.source, source.article_id, source.headline, source.context
    )
}

async fn score_one(
    pool: &PgPool,
    backend: &dyn Inference,
    voice_num_ctx: i32,
    item: &Item,
    source: &SourceContext,
) -> Result<Evaluation> {
    let entity_id = item.entity_id_i32()?;
    let name = lookup_entity_name(pool, &item.entity_type, entity_id, &item.sport).await?;
    let reaction = decide_reaction(
        backend,
        voice_num_ctx,
        &item.entity_type,
        &name,
        &item.sport,
        source,
    )
    .await?;
    if !reaction.has_reaction {
        return Ok(Evaluation {
            output: None,
            provenance: reaction.provenance,
        });
    }
    let mut memory_request = MemoryRequest::new(
        Mission::Influencer,
        &item.entity_type,
        entity_id,
        &item.sport,
    );
    memory_request.include_storyline_history = false;
    memory_request.current_article_ids = std::slice::from_ref(&source.article_id);
    let memories = memories::load(pool, memory_request).await?;
    let memory = memories.render_for_model()?;
    let base_components = json!({
        "contract": crate::plugins::harvester::context::CONTRACT,
        "entity_type": item.entity_type,
        "entity_id": entity_id,
        "entity_name": name,
        "sport": item.sport.to_uppercase(),
        "classification_id": source.classification_id,
        "article_id": source.article_id,
        "source": source.source,
        "headline": source.headline,
        "context": source.context,
        "prompt_version": PROMPT_VERSION,
    })
    .to_string();
    let input_components_json = memories.with_input_components(&base_components)?;
    let input_hash = hash_components(&input_components_json);
    let built_prompt = prompt(&item.entity_type, &name, &item.sport, source, &memory);
    let mut options = production_options(VIBE_TEMPERATURE, voice_num_ctx);
    options.system = Some(format!(
        "{}\n\n{}",
        &*VIBE_SYSTEM_PROMPT,
        crate::plugins::support::prompt::ABSTENTION
    ));
    options.format_schema = Some(form::with_abstention(
        crate::plugins::support::prompt::card_schema(true),
    ));
    let extracted = Studio::new(backend)
        .extract(
            &built_prompt,
            &options,
            &OptionalVibeParser,
            crate::plugins::support::prompt::publishing_correction,
        )
        .await?;
    let provenance = json!({
        "article_id": source.article_id,
        "model_version": extracted.model,
        "prompt_version": PROMPT_VERSION,
        "input_hash": input_hash,
        "eval_count": extracted.eval_count,
        "wall_ms": extracted.wall_ms,
        "reaction_gate": reaction.provenance,
    });
    let Some(reply) = extracted.value.as_ref() else {
        return Ok(Evaluation {
            output: None,
            provenance,
        });
    };
    let call = GenerationCall::from(&extracted);
    Ok(Evaluation {
        output: Some(Generation::called(
            VibeScore {
                sentiment: Some(reply.sentiment),
                vibe_prompt: Some(reply.vibe_prompt.clone()),
                hook: reply.hook.clone(),
                input_components_json,
            },
            extracted.model,
            PROMPT_VERSION,
            vec![source.article_id],
            Some(input_hash),
            call,
        )),
        provenance,
    })
}

pub(super) async fn execute(
    pool: &PgPool,
    models: &ExecutionCapabilities,
    item: &Item,
) -> Result<PluginOutcome> {
    let backend = models.inference(crate::plugins::influencer::manifest::ROUTE)?;
    execute_with_backend(pool, backend.as_ref(), models.voice_num_ctx, item).await
}

pub(crate) async fn execute_with_backend(
    pool: &PgPool,
    backend: &dyn Inference,
    voice_num_ctx: i32,
    item: &Item,
) -> Result<PluginOutcome> {
    let entity_id = item.entity_id_i32()?;
    let sport = item.sport.to_uppercase();
    let plugin_id = crate::plugins::influencer::manifest::MANIFEST.id.as_str();
    let sources = load_for_character(pool, plugin_id, &item.entity_type, entity_id, &sport).await?;
    // Process oldest first so the latest published score reflects the freshest
    // source after a multi-source backlog drains.
    let Some(source) = sources.last() else {
        let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        record_vibe_completed(publication.transaction(), item).await?;
        publication.commit_final().await?;
        return Ok(PluginOutcome::Committed);
    };
    let evaluation = score_one(pool, backend, voice_num_ctx, item, source).await?;
    let output = evaluation.output.as_ref();
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    let product_row_id = if let Some(output) = output {
        Some(persist_to_vibe_scores(publication.transaction(), item, &sport, output).await?)
    } else {
        None
    };
    let changed = sqlx::query(
        "UPDATE public.harvester_assignments SET status=$3, reason=$4, product_ref=$5, updated_at=NOW() \
         WHERE classification_id=$1 AND plugin_id=$2 AND status='pending' \
           AND reason IS DISTINCT FROM 'delivery_held'",
    )
    .bind(source.classification_id)
    .bind(plugin_id)
    .bind(if output.is_some() { "used" } else { "abstained" })
    .bind(if output.is_some() { None } else { Some("Influencer passed on this source") })
    .bind({
        let mut provenance = evaluation.provenance;
        if let Some(id) = product_row_id {
            provenance["vibe_score_id"] = json!(id);
        }
        provenance
    })
    .execute(&mut **publication.transaction())
    .await?;
    ensure!(
        changed.rows_affected() == 1,
        "Influencer source assignment changed during call"
    );
    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.harvester_assignments d \
         JOIN public.harvester_classifications c ON c.id=d.classification_id \
         WHERE d.plugin_id=$1 AND d.status='pending' AND d.reason IS DISTINCT FROM $5 \
           AND c.entity_type=$2 AND c.entity_id=$3 AND c.sport=$4",
    )
    .bind(plugin_id)
    .bind(&item.entity_type)
    .bind(entity_id)
    .bind(&sport)
    .bind(crate::plugins::harvester::adapter::DELIVERY_HELD_REASON)
    .fetch_one(&mut **publication.transaction())
    .await?;
    if remaining > 0 {
        publication.commit_progress().await?;
        if let (Some(id), Some(output)) = (product_row_id, output) {
            record_ledger(pool, item, &sport, id, output).await?;
        }
        return Ok(PluginOutcome::deferred(
            format!("{remaining} Harvester source contexts remain for Influencer"),
            Duration::from_secs(1),
        ));
    }
    record_vibe_completed(publication.transaction(), item).await?;
    publication.commit_final().await?;
    if let (Some(id), Some(output)) = (product_row_id, output) {
        record_ledger(pool, item, &sport, id, output).await?;
    }
    Ok(PluginOutcome::Committed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};

    #[test]
    fn prompt_keeps_source_words_and_names_the_single_article() {
        let source = SourceContext {
            classification_id: 7,
            article_id: 42,
            headline: "Club statement".into(),
            context: "Fans said they were thrilled. The coach made no claim about them.".into(),
            source: "Wire".into(),
            published_at_epoch: None,
        };
        let rendered = prompt("team", "Club", "FOOTBALL", &source, "Prior mood: neutral");
        assert!(rendered.contains(&source.context));
        assert!(rendered.contains("Article ID: 42"));
        assert!(!rendered.contains("MOOD:"));
    }

    #[test]
    fn explicit_pass_is_not_a_fabricated_vibe_score() {
        assert!(OptionalVibeParser.parse("null").unwrap().is_none());
        assert!(OptionalVibeParser.parse("{}").is_err());
        let source = SourceContext {
            classification_id: 0,
            article_id: 1,
            headline: "Fans cheer".into(),
            context: "Fans cheered the team after the win.".into(),
            source: "Wire".into(),
            published_at_epoch: None,
        };
        let parser = ReactionParser(&source);
        assert!(parser
            .parse(r#"{"has_reaction":true,"evidence_quote":"Fans cheered the team"}"#)
            .is_ok());
        assert!(parser
            .parse(r#"{"has_reaction":true,"evidence_quote":"The crowd booed"}"#)
            .is_err());
        assert!(parser
            .parse(r#"{"has_reaction":false,"evidence_quote":""}"#)
            .is_ok());
        assert_eq!(
            parser
                .parse(r#"{"has_reaction":false,"evidence_quote":"unneeded source words"}"#)
                .unwrap()
                .unwrap()
                .evidence_quote,
            ""
        );
        assert!(reaction_correction(&ReactionQuoteMismatch.into())
            .unwrap()
            .contains("exact continuous substring"));
    }

    #[tokio::test]
    #[ignore = "requires a local Ollama model"]
    async fn local_real_model_source_reaction_and_pass() -> Result<()> {
        let backend = crate::runtime::providers::ollama::OllamaClient::with_think(
            &std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into()),
            &std::env::var("OLLAMA_MODEL")
                .unwrap_or_else(|_| crate::runtime::config::DEFAULT_OLLAMA_MODEL.into()),
            Duration::from_secs(600),
            Some(false),
        )?;
        let mut options = production_options(VIBE_TEMPERATURE, 4096);
        options.system = Some(format!(
            "{}\n\n{}",
            &*VIBE_SYSTEM_PROMPT,
            crate::plugins::support::prompt::ABSTENTION
        ));
        options.format_schema = Some(form::with_abstention(
            crate::plugins::support::prompt::card_schema(true),
        ));
        let reaction = SourceContext {
            classification_id: 0,
            article_id: 12,
            headline: "Pistons fans celebrate playoff win".into(),
            context: "Detroit Pistons fans outside the arena cheered after the team won its playoff game. Supporters chanted the players' names and said they were thrilled by the comeback.".into(),
            source: "Example Wire".into(),
            published_at_epoch: None,
        };
        let gate =
            decide_reaction(&backend, 4096, "team", "Detroit Pistons", "NBA", &reaction).await?;
        ensure!(gate.has_reaction, "clear fan reaction was missed by gate");
        let result = Studio::new(&backend)
            .extract(
                &prompt("team", "Detroit Pistons", "NBA", &reaction, ""),
                &options,
                &OptionalVibeParser,
                crate::plugins::support::prompt::publishing_correction,
            )
            .await?;
        ensure!(result.value.is_some(), "clear crowd reaction was missed");
        let quiet = SourceContext {
            classification_id: 0,
            article_id: 13,
            headline: "Pistons announce training schedule".into(),
            context: "The Detroit Pistons announced next week's training schedule. The team will practice on Tuesday and Thursday before traveling on Friday.".into(),
            source: "Example Wire".into(),
            published_at_epoch: None,
        };
        let gate =
            decide_reaction(&backend, 4096, "team", "Detroit Pistons", "NBA", &quiet).await?;
        ensure!(
            !gate.has_reaction,
            "routine schedule became a crowd reaction"
        );
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires INFLUENCER_SHADOW_CORPUS and a local Ollama model"]
    async fn local_real_model_reaction_corpus_replay() -> Result<()> {
        let path = std::env::var("INFLUENCER_SHADOW_CORPUS")?;
        let articles = BufReader::new(std::fs::File::open(path)?)
            .lines()
            .map(|line| -> Result<crate::plugins::harvester::Article> {
                Ok(serde_json::from_str(&line?)?)
            })
            .collect::<Result<Vec<_>>>()?;
        let backend = crate::runtime::providers::ollama::OllamaClient::with_think(
            &std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into()),
            &std::env::var("OLLAMA_MODEL")
                .unwrap_or_else(|_| crate::runtime::config::DEFAULT_OLLAMA_MODEL.into()),
            Duration::from_secs(600),
            Some(false),
        )?;
        let mut accepted = 0usize;
        let mut passed = 0usize;
        let mut errors = Vec::new();
        for article in articles
            .iter()
            .filter(|article| !article.body.trim().is_empty())
        {
            let opening = crate::plugins::harvester::cognition::first_paragraphs(&article.body, 3);
            ensure!(
                article.body.get(opening.start..opening.end) == Some(opening.text.as_str()),
                "Influencer corpus opening drift on article {}",
                article.article_id
            );
            let source = SourceContext {
                classification_id: 0,
                article_id: article.article_id,
                headline: article.title.clone(),
                context: opening.text,
                source: article.source.clone(),
                published_at_epoch: None,
            };
            match decide_reaction(
                &backend,
                4096,
                &article.hypothesis.entity_type,
                &article.hypothesis.name,
                &article.hypothesis.sport,
                &source,
            )
            .await
            {
                Ok(verdict) if verdict.has_reaction => accepted += 1,
                Ok(_) => passed += 1,
                Err(error) => errors.push((article.article_id, format!("{error:#}"))),
            }
        }
        eprintln!(
            "Influencer real-model reaction corpus: accepted={accepted} passed={passed} errors={}",
            errors.len()
        );
        if !errors.is_empty() {
            eprintln!("Influencer reaction errors: {errors:?}");
        }
        ensure!(
            errors.is_empty(),
            "Influencer reaction verdicts had failures"
        );
        Ok(())
    }
}
