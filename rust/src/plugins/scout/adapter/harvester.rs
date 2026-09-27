//! Source-level Scout review before measured-profile publication.
use super::{
    build_rating_request_inner, current_season, insert_stat_summary, record_ledger,
    record_rating_completed, LedgerSubject, RatingReq,
};
use crate::application::models::ExecutionCapabilities;
use crate::application::queue::publication::ClaimPublication;
use crate::application::queue::work::Item;
use crate::plugins::harvester::delivery::load_for_character;
use crate::plugins::harvester::delivery::SourceContext;
use crate::plugins::scout::cognition::{
    self as scout, RatingBuild, RatingOutput, RATING_TEMPERATURE,
};
use crate::studio::model::{GenerateOptions, Inference};
use crate::studio::plugin::PluginOutcome;
use crate::studio::{GenerationCall, Parser, Studio};
use anyhow::{ensure, Result};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use std::time::Duration;

pub(super) const PROMPT_VERSION: &str = "scout-source-v3";

/// Only a source-linked structured record can turn roster/availability reporting
/// into a Scout rating trigger. A Harvester tag or model quote alone is not enough.
pub(crate) async fn structured_record(
    pool: &PgPool,
    sport: &str,
    entity_type: &str,
    entity_id: i32,
    article_id: i64,
    kind: SourceKind,
) -> Result<Option<i64>> {
    match kind {
        SourceKind::Roster => Ok(sqlx::query_scalar(
            "SELECT id FROM public.transfer_identity_applications \
                 WHERE sport=$1 AND status='applied' AND reverted_at IS NULL \
                   AND evidence->'identity_evidence_article_ids' @> jsonb_build_array($4::bigint) \
                   AND (($2='player' AND player_id=$3) OR \
                        ($2='team' AND (old_team_id=$3 OR new_team_id=$3))) \
                 ORDER BY applied_at DESC,id DESC LIMIT 1",
        )
        .bind(sport)
        .bind(entity_type)
        .bind(entity_id)
        .bind(article_id)
        .fetch_optional(pool)
        .await?),
        SourceKind::Availability => Ok(sqlx::query_scalar(
            "SELECT id FROM public.player_availability \
                 WHERE sport=$1 AND status='applied' AND reverted_at IS NULL \
                   AND source_article_id=$4 \
                   AND (($2='player' AND player_id=$3) OR \
                        ($2='team' AND team_id=$3)) \
                 ORDER BY applied_at DESC,id DESC LIMIT 1",
        )
        .bind(sport)
        .bind(entity_type)
        .bind(entity_id)
        .bind(article_id)
        .fetch_optional(pool)
        .await?),
        _ => Ok(None),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceKind {
    None,
    Performance,
    Roster,
    Availability,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    kind: SourceKind,
    evidence_quote: String,
}

struct ReplyParser<'a>(&'a SourceContext);

#[derive(Debug)]
struct QuoteMismatch;

impl std::fmt::Display for QuoteMismatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Scout quote is not an exact bounded publisher span"
        )
    }
}

impl std::error::Error for QuoteMismatch {}

fn source_correction(error: &anyhow::Error) -> Option<String> {
    if error.is::<QuoteMismatch>() {
        Some("Your previous evidence_quote was not a verbatim continuous substring of the supplied headline or publisher opening. Re-evaluate the source. For a positive kind, copy a short exact substring with identical spelling, punctuation, and HTML entities. If no such span supports a positive, choose kind none and an empty evidence_quote. Return the complete JSON object again.".into())
    } else {
        crate::plugins::support::form::structured_correction(error)
    }
}

impl Parser<Reply> for ReplyParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<Reply>> {
        let mut reply: Reply = serde_json::from_str(raw)?;
        if reply.kind == SourceKind::None {
            // A negative verdict has no supporting quote. Discard a stray model
            // string rather than retrying the same safe abstention indefinitely.
            reply.evidence_quote.clear();
        } else {
            if reply.evidence_quote.trim().is_empty()
                || reply.evidence_quote.chars().count() > 500
                || !(self.0.context.contains(&reply.evidence_quote)
                    || self.0.headline.contains(&reply.evidence_quote))
            {
                return Err(QuoteMismatch.into());
            }
        }
        Ok(Some(reply))
    }
}

pub(super) struct SourceDecision {
    pub kind: SourceKind,
    pub quote: String,
    pub provenance: serde_json::Value,
}

fn prompt(entity_type: &str, name: &str, sport: &str, source: &SourceContext) -> String {
    format!(
        "Entity: {entity_type} {name} ({sport})\nPublisher: {}\nArticle ID: {}\nExact headline: {}\nExact publisher opening (unchanged):\n{}\n\nClassify this source for the exact entity above. Choose performance when it reports a completed match result, a player stat line, or a specific on-field performance by this entity. Choose roster when it reports an actual roster membership change, not a proposed trade or rumor. Choose availability when it reports an injury, suspension, or return affecting this entity. Choose none for a mere mention, opinion, unrelated team, or speculative move. This decision only says whether the article is a relevant trigger; numeric ratings come from a separate measured profile, never from this article. Return JSON {{\"kind\":\"none|performance|roster|availability\",\"evidence_quote\":\"\"}}. For a non-none kind, evidence_quote must be an exact continuous span from the headline or opening that supports your choice. For none, use an empty quote.",
        source.source, source.article_id, source.headline, source.context
    )
}

pub(super) async fn decide(
    backend: &dyn Inference,
    voice_num_ctx: i32,
    entity_type: &str,
    name: &str,
    sport: &str,
    source: &SourceContext,
) -> Result<SourceDecision> {
    let built_prompt = prompt(entity_type, name, sport, source);
    let input_hash = crate::util::hash_components(
        &json!({
            "prompt_version": PROMPT_VERSION,
            "entity_type": entity_type,
            "entity_name": name,
            "sport": sport,
            "classification_id": source.classification_id,
            "article_id": source.article_id,
            "headline": source.headline,
            "opening": source.context,
        })
        .to_string(),
    );
    let options = GenerateOptions {
        system: Some("Identify concrete performance, roster, or availability reporting for one resolved Scout entity. A positive is only a source trigger, not statistical evidence. Cite exact source words for a positive; do not invent facts.".into()),
        temperature: Some(0.0), num_predict: 220, num_ctx: voice_num_ctx,
        json_mode: true,
        format_schema: Some(json!({
            "type":"object","additionalProperties":false,
            "required":["kind","evidence_quote"],
            "properties":{
                "kind":{"type":"string","enum":["none","performance","roster","availability"]},
                "evidence_quote":{"type":"string"}
            }
        })),
        format_schema_raw: None,
    };
    let extracted = Studio::new(backend)
        .extract(
            &built_prompt,
            &options,
            &ReplyParser(source),
            source_correction,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let reply = extracted
        .value
        .ok_or_else(|| anyhow::anyhow!("Scout source verdict did not commit"))?;
    Ok(SourceDecision {
        kind: reply.kind,
        quote: reply.evidence_quote,
        provenance: json!({
            "article_id": source.article_id,
            "classification_id": source.classification_id,
            "model_version": extracted.model,
            "prompt_version": PROMPT_VERSION,
            "input_hash": input_hash,
            "raw_verdict": extracted.raw_response,
            "eval_count": call.eval_count,
            "wall_ms": call.wall_ms,
        }),
    })
}

pub(super) async fn execute(
    pool: &PgPool,
    models: &ExecutionCapabilities,
    item: &Item,
) -> Result<PluginOutcome> {
    let backend = models.inference(crate::plugins::scout::manifest::ROUTE)?;
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
    let plugin_id = crate::plugins::scout::manifest::MANIFEST.id.as_str();
    let sources = load_for_character(pool, plugin_id, &item.entity_type, entity_id, &sport).await?;
    let Some(source) = sources.last() else {
        let Some(publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        publication.commit_final().await?;
        return Ok(PluginOutcome::Committed);
    };
    let name =
        crate::evidence::corpus::lookup_entity_name(pool, &item.entity_type, entity_id, &sport)
            .await?;
    let decision = decide(
        backend,
        voice_num_ctx,
        &item.entity_type,
        &name,
        &sport,
        source,
    )
    .await?;
    // Query membership and a Laya assignment are advisory. A source-triggered
    // product also needs the independent canonical-name link for this article.
    let identity_resolved: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM public.harvester_resolved_links \
         WHERE article_id=$1 AND entity_type=$2 AND entity_id=$3 AND sport=$4)",
    )
    .bind(source.article_id)
    .bind(&item.entity_type)
    .bind(entity_id)
    .bind(&sport)
    .fetch_one(pool)
    .await?;
    let corroborating_record = if identity_resolved {
        structured_record(
            pool,
            &sport,
            &item.entity_type,
            entity_id,
            source.article_id,
            decision.kind,
        )
        .await?
    } else {
        None
    };
    let mut output: Option<RatingOutput> = None;
    if identity_resolved
        && (decision.kind == SourceKind::Performance || corroborating_record.is_some())
    {
        let season = current_season(pool, &sport).await?;
        let req = RatingReq {
            entity_type: item.entity_type.clone(),
            entity_id,
            entity_name: name,
            sport: sport.clone(),
            trigger_type: "periodic".into(),
            season: Some(season),
        };
        if let RatingBuild::Ready(mut assignment) =
            build_rating_request_inner(pool, voice_num_ctx, &req, RATING_TEMPERATURE, false, false)
                .await?
        {
            let mut components: serde_json::Value =
                serde_json::from_str(&assignment.input_components)?;
            components["harvester_trigger"] = json!({
                "contract": crate::plugins::harvester::context::CONTRACT,
                "article_id": source.article_id,
                "classification_id": source.classification_id,
                "source_role": "trigger_only",
                "scout_kind": match decision.kind { SourceKind::Performance => "performance", SourceKind::Roster => "roster", SourceKind::Availability => "availability", SourceKind::None => "none" },
                "source_quote": decision.quote,
                "structured_record_id": corroborating_record,
            });
            assignment.input_components = components.to_string();
            assignment.input_hash = crate::util::hash_components(&assignment.input_components);
            let mut generated = scout::create(&Studio::new(backend), *assignment).await?;
            generated.provenance.input_ids = vec![source.article_id];
            output = Some(generated);
        }
    }
    let status = match decision.kind {
        SourceKind::None => "abstained",
        _ if output.is_some() => "used",
        _ => "relevant_but_unused",
    };
    let reason = match decision.kind {
        SourceKind::None => {
            Some("Scout found no grounded performance, roster, or availability report")
        }
        _ if !identity_resolved => Some("No independently resolved article-to-entity link"),
        SourceKind::Roster | SourceKind::Availability if corroborating_record.is_none() => {
            Some("No source-linked structured roster or availability record")
        }
        _ if output.is_none() => Some("No measured profile available for a rating"),
        _ => None,
    };
    let trigger_payload = json!({
        "source_article_id": source.article_id,
        "classification_id": source.classification_id,
        "source_role": "trigger_only",
        "identity_resolved": identity_resolved,
        "structured_record_id": corroborating_record,
        "scout_kind": match decision.kind {
            SourceKind::None => "none", SourceKind::Performance => "performance",
            SourceKind::Roster => "roster", SourceKind::Availability => "availability",
        },
        "evidence_quote": decision.quote,
    });
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    let product_row_id = if let Some(out) = output.as_ref() {
        let id = insert_stat_summary(
            publication.transaction(),
            &item.entity_type,
            entity_id,
            &sport,
            "periodic",
            &trigger_payload,
            out,
        )
        .await?;
        record_rating_completed(publication.transaction(), item, true).await?;
        Some(id)
    } else {
        None
    };
    let changed = sqlx::query(
        "UPDATE public.harvester_assignments SET status=$3,reason=$4,product_ref=$5,updated_at=now() \
         WHERE classification_id=$1 AND plugin_id=$2 AND status='pending'",
    )
    .bind(source.classification_id).bind(plugin_id).bind(status).bind(reason)
    .bind(json!({
        "source_article_id": source.article_id,
        "source_role": "trigger_only",
        "identity_resolved": identity_resolved,
        "structured_record_id": corroborating_record,
        "source_decision": decision.provenance,
        "source_kind": match decision.kind {
            SourceKind::None => "none", SourceKind::Performance => "performance",
            SourceKind::Roster => "roster", SourceKind::Availability => "availability",
        },
        "source_quote": decision.quote,
        "stat_summary_id": product_row_id,
    }))
    .execute(&mut **publication.transaction()).await?;
    ensure!(
        changed.rows_affected() == 1,
        "Scout source assignment changed during call"
    );
    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.harvester_assignments d \
         JOIN public.harvester_classifications c ON c.id=d.classification_id \
         WHERE d.plugin_id=$1 AND d.status='pending' AND c.entity_type=$2 \
           AND c.entity_id=$3 AND c.sport=$4",
    )
    .bind(plugin_id)
    .bind(&item.entity_type)
    .bind(entity_id)
    .bind(&sport)
    .fetch_one(&mut **publication.transaction())
    .await?;
    if remaining > 0 {
        publication.commit_progress().await?;
        if let (Some(id), Some(out)) = (product_row_id, output.as_ref()) {
            record_ledger(
                pool,
                &LedgerSubject {
                    entity_type: &item.entity_type,
                    entity_id,
                    sport: &sport,
                    trigger_type: "periodic",
                    trigger_payload: &trigger_payload,
                },
                id,
                out,
            )
            .await?;
        }
        return Ok(PluginOutcome::deferred(
            format!("{remaining} Harvester source contexts remain for Scout"),
            Duration::from_secs(1),
        ));
    }
    publication.commit_final().await?;
    if let (Some(id), Some(out)) = (product_row_id, output.as_ref()) {
        record_ledger(
            pool,
            &LedgerSubject {
                entity_type: &item.entity_type,
                entity_id,
                sport: &sport,
                trigger_type: "periodic",
                trigger_payload: &trigger_payload,
            },
            id,
            out,
        )
        .await?;
    }
    Ok(PluginOutcome::Committed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use std::io::{BufRead, BufReader};

    fn source() -> SourceContext {
        SourceContext {
            classification_id: 4, article_id: 9,
            headline: "Club reports injury".into(),
            context: "The club confirmed that Morgan Example has a knee injury. The player will be assessed tomorrow.".into(),
            source: "Wire".into(), published_at_epoch: None,
        }
    }

    #[test]
    fn exact_quote_required_for_a_positive() {
        let source = source();
        let parser = ReplyParser(&source);
        assert!(parser
            .parse(r#"{"kind":"availability","evidence_quote":"Morgan Example has a knee injury"}"#)
            .is_ok());
        assert!(parser
            .parse(
                r#"{"kind":"availability","evidence_quote":"Morgan Example is out for six weeks"}"#
            )
            .is_err());
        assert!(parser
            .parse(r#"{"kind":"none","evidence_quote":""}"#)
            .is_ok());
        assert_eq!(
            parser
                .parse(r#"{"kind":"none","evidence_quote":"unneeded source words"}"#)
                .unwrap()
                .unwrap()
                .evidence_quote,
            ""
        );
        let repair = source_correction(&QuoteMismatch.into()).unwrap();
        assert!(repair.contains("verbatim continuous substring"));
        assert!(repair.contains("choose kind none"));
    }

    #[test]
    fn prompt_contains_the_exact_opening_and_article_id() {
        let source = source();
        let rendered = prompt("player", "Morgan Example", "FOOTBALL", &source);
        assert!(rendered.contains(&source.context));
        assert!(rendered.contains("Article ID: 9"));
        assert!(!rendered.contains("packet"));
    }

    #[tokio::test]
    #[ignore = "requires SCOUT_SHADOW_CORPUS, SCOUT_SHADOW_ARTICLE_ID, and a local Ollama model"]
    async fn local_real_model_source_verdict_uses_exact_publisher_opening() -> Result<()> {
        let path = std::env::var("SCOUT_SHADOW_CORPUS")?;
        let article_id: i64 = std::env::var("SCOUT_SHADOW_ARTICLE_ID")?.parse()?;
        let article = BufReader::new(std::fs::File::open(path)?)
            .lines()
            .map(|line| -> Result<crate::plugins::harvester::Article> {
                Ok(serde_json::from_str(&line?)?)
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .find(|article| article.article_id == article_id)
            .context("Scout shadow article missing")?;
        ensure!(!article.body.trim().is_empty(), "Scout shadow body missing");
        let opening = crate::plugins::harvester::cognition::first_sentences(&article.body, 3);
        ensure!(
            article.body.get(opening.start..opening.end) == Some(opening.text.as_str()),
            "Scout shadow opening is not an exact publisher span"
        );
        let source = SourceContext {
            classification_id: 0,
            article_id,
            headline: article.title,
            context: opening.text,
            source: article.source,
            published_at_epoch: None,
        };
        let backend = crate::runtime::providers::ollama::OllamaClient::with_think(
            &std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into()),
            &std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "granite4.2:3b".into()),
            Duration::from_secs(600),
            Some(false),
        )?;
        let result = decide(
            &backend,
            4096,
            &article.hypothesis.entity_type,
            &article.hypothesis.name,
            &article.hypothesis.sport,
            &source,
        )
        .await?;
        eprintln!(
            "Scout real-model source verdict: article={} kind={:?} quote_bytes={}",
            article_id,
            result.kind,
            result.quote.len()
        );
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires a local Ollama model"]
    async fn local_real_model_recognizes_clear_measured_performance() -> Result<()> {
        let source = SourceContext {
            classification_id: 0,
            article_id: 9,
            headline: "Detroit Pistons beat Boston Celtics 112-104".into(),
            context: "The Detroit Pistons beat the Boston Celtics 112-104 on Friday. The Pistons scored 30 points in the final quarter. Cade Cunningham finished with 28 points and eight assists for Detroit.".into(),
            source: "Example Wire".into(),
            published_at_epoch: None,
        };
        let backend = crate::runtime::providers::ollama::OllamaClient::with_think(
            &std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into()),
            &std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "granite4.2:3b".into()),
            Duration::from_secs(600),
            Some(false),
        )?;
        let result = decide(&backend, 4096, "team", "Detroit Pistons", "NBA", &source).await?;
        eprintln!(
            "Scout clear-source raw verdict: {}",
            result.provenance["raw_verdict"]
        );
        ensure!(
            result.kind == SourceKind::Performance,
            "clear source performance was missed: {:?}",
            result.kind
        );
        ensure!(
            source.context.contains(&result.quote) || source.headline.contains(&result.quote),
            "real-model performance quote is not exact"
        );
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires a local Ollama model"]
    async fn local_real_model_separates_availability_from_transfer_rumor() -> Result<()> {
        let backend = crate::runtime::providers::ollama::OllamaClient::with_think(
            &std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into()),
            &std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "granite4.2:3b".into()),
            Duration::from_secs(600),
            Some(false),
        )?;
        let availability = SourceContext {
            classification_id: 0,
            article_id: 10,
            headline: "Cade Cunningham ruled out for Detroit Pistons".into(),
            context: "Detroit Pistons guard Cade Cunningham was ruled out of Sunday's game with a left ankle injury. The team said he will be reassessed next week.".into(),
            source: "Example Wire".into(),
            published_at_epoch: None,
        };
        let injury = decide(
            &backend,
            4096,
            "team",
            "Detroit Pistons",
            "NBA",
            &availability,
        )
        .await?;
        ensure!(
            injury.kind == SourceKind::Availability,
            "clear availability report was missed: {:?}",
            injury.kind
        );
        ensure!(
            availability.context.contains(&injury.quote)
                || availability.headline.contains(&injury.quote),
            "availability quote is not exact"
        );
        let rumor = SourceContext {
            classification_id: 0,
            article_id: 11,
            headline: "Detroit Pistons exploring a trade".into(),
            context: "Detroit Pistons officials have discussed a possible trade for a veteran center, but no agreement has been reached and no roster move has taken place.".into(),
            source: "Example Wire".into(),
            published_at_epoch: None,
        };
        let speculative = decide(&backend, 4096, "team", "Detroit Pistons", "NBA", &rumor).await?;
        ensure!(
            speculative.kind == SourceKind::None && speculative.quote.is_empty(),
            "speculative transfer became Scout roster evidence: {:?}",
            speculative.kind
        );
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires SCOUT_SHADOW_CORPUS and a local Ollama model"]
    async fn local_real_model_corpus_verdict_replay() -> Result<()> {
        let path = std::env::var("SCOUT_SHADOW_CORPUS")?;
        let articles = BufReader::new(std::fs::File::open(path)?)
            .lines()
            .map(|line| -> Result<crate::plugins::harvester::Article> {
                Ok(serde_json::from_str(&line?)?)
            })
            .collect::<Result<Vec<_>>>()?;
        let backend = crate::runtime::providers::ollama::OllamaClient::with_think(
            &std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into()),
            &std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "granite4.2:3b".into()),
            Duration::from_secs(600),
            Some(false),
        )?;
        let mut counts = std::collections::BTreeMap::<String, usize>::new();
        let mut errors = Vec::new();
        for article in articles
            .iter()
            .filter(|article| !article.body.trim().is_empty())
        {
            let opening = crate::plugins::harvester::cognition::first_sentences(&article.body, 3);
            ensure!(
                article.body.get(opening.start..opening.end) == Some(opening.text.as_str()),
                "Scout corpus opening drift on article {}",
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
            match decide(
                &backend,
                4096,
                &article.hypothesis.entity_type,
                &article.hypothesis.name,
                &article.hypothesis.sport,
                &source,
            )
            .await
            {
                Ok(verdict) => {
                    let kind = format!("{:?}", verdict.kind).to_lowercase();
                    *counts.entry(kind).or_default() += 1;
                }
                Err(error) => errors.push((article.article_id, format!("{error:#}"))),
            }
        }
        eprintln!("Scout real-model corpus verdict counts: {counts:?}");
        if !errors.is_empty() {
            eprintln!("Scout real-model corpus verdict errors: {errors:?}");
        }
        ensure!(
            errors.is_empty(),
            "Scout corpus verdicts had model or quote failures"
        );
        Ok(())
    }
}
