//! Journalist evidence preparation, model selection, publication, and durable work coordination.
//!
//! Studio owns creation from prepared material. This adapter owns concrete packet and memory
//! retrieval, debounce, routing, exact-claim multi-row publication, storyline progression, and
//! the optional diagnostic ledger.

use crate::application::models::ExecutionCapabilities;
use crate::application::products::EntityKey;
use crate::application::queue::publication::ClaimPublication;
use crate::application::queue::work::Item;
use crate::evidence::memories::{self, MemoryRequest, Mission};
use crate::evidence::story_parts::{mode_storyline, progress_generation, PartItem};
use crate::evidence::trajectory::DEFAULT_TRAJECTORY;
use crate::plugins::journalist::cognition::{
    self as journalist, Assignment, CorpusExclusions, CorpusItem, Narrative, NarrativesOutput,
    Subject, NARRATIVES_NUM_PREDICT_PACKET, NARRATIVES_OUTPUT_CONTRACT_VERSION,
    NARRATIVES_TEMPERATURE,
};
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::studio::Studio;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::json;
use sqlx::{PgPool, Postgres, Row, Transaction};
use tracing::debug;

pub(crate) const NARRATIVES_COMPLETED: &str = "narratives_completed";

pub(crate) async fn record_narratives_completed(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
) -> Result<()> {
    crate::application::queue::outbox::record(
        tx,
        item,
        crate::application::queue::outbox::NewEvent {
            kind: NARRATIVES_COMPLETED,
            entity_type: &item.entity_type,
            entity_id: item.entity_id_i32()?,
            source_input_version: item.input_version.as_deref(),
        },
    )
    .await
    .context("record narratives completion outbox")
}

pub const PACKET_LOOKBACK_HOURS: i64 = 72;
pub const MAX_PACKETS_PER_ENTITY: usize = 5;
const PACKET_NEWS_BUDGET_CHARS: usize = 5_000;
const SOURCE_NEWS_BUDGET_CHARS: usize = 5_000;

const NARRATIVES_LEDGER: LedgerSpec = LedgerSpec {
    plugin_id: crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
    stage: "narratives",
    lens: "narratives",
    role: crate::plugins::journalist::manifest::ROUTE,
    product_table: "news_summaries",
    output_contract_version: NARRATIVES_OUTPUT_CONTRACT_VERSION,
};

/// Durable subject and invocation policy used to prepare a Studio Journalist assignment.
#[derive(Clone, Debug)]
pub struct NarrativesReq {
    pub entity_type: String,
    pub entity_id: i32,
    pub entity_name: String,
    pub sport: String,
    pub trigger_type: String,
}

struct PacketArticle {
    source: String,
    published_at_epoch: Option<i64>,
    facts: Vec<String>,
}

/// Load compiled storylines as prepared, numbered evidence plus explicit exclusions and framing.
pub async fn load_packet_corpus(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    entity_name: &str,
) -> Result<(Vec<CorpusItem>, CorpusExclusions, String)> {
    use crate::evidence::news::render;

    let loaded = crate::evidence::news::packet::load_packets_for_entity(
        pool,
        entity_type,
        entity_id,
        sport,
        PACKET_LOOKBACK_HOURS,
        MAX_PACKETS_PER_ENTITY as i64 + 1,
    )
    .await?;

    let mut exclusions = CorpusExclusions::default();
    let mut framing = String::new();
    let mut by_article: Vec<(i64, PacketArticle)> = Vec::new();
    for (index, (view, mut part)) in loaded.into_iter().enumerate() {
        if index >= MAX_PACKETS_PER_ENTITY {
            exclusions
                .budget_truncated_ids
                .extend(view.claims.iter().map(|claim| claim.article_id));
            continue;
        }
        part.name = entity_name.to_string();
        if !framing.is_empty() {
            framing.push('\n');
        }
        framing.push_str(&render::framing(
            &view,
            Some(&part),
            render::Voice::Journalist,
        ));

        for marked in render::mark_contested(&view.claims) {
            let fact = if marked.marked {
                format!("⇄ {}", marked.claim.fact)
            } else {
                marked.claim.fact.clone()
            };
            match by_article
                .iter_mut()
                .find(|(article_id, _)| *article_id == marked.claim.article_id)
            {
                Some((_, article)) => article.facts.push(fact),
                None => by_article.push((
                    marked.claim.article_id,
                    PacketArticle {
                        source: marked.claim.source.clone(),
                        published_at_epoch: marked.claim.published_at,
                        facts: vec![fact],
                    },
                )),
            }
        }
    }

    let corpus = by_article
        .into_iter()
        .map(|(id, article)| {
            let mut facts = article.facts.into_iter();
            CorpusItem {
                id,
                title: facts.next().unwrap_or_default(),
                description: facts.collect::<Vec<_>>().join(" · "),
                harvested_context: None,
                source: article.source,
                published_at_epoch: article.published_at_epoch,
            }
        })
        .collect();
    let (corpus, over_budget) = journalist::apply_news_budget(corpus, PACKET_NEWS_BUDGET_CHARS);
    exclusions.budget_truncated_ids.extend(over_budget);
    exclusions.budget_truncated_ids.sort_unstable();
    exclusions.budget_truncated_ids.dedup();
    Ok((corpus, exclusions, framing))
}

pub struct NarrativesMaterial {
    memories: memories::Package,
    corpus: Vec<CorpusItem>,
    corpus_exclusions: CorpusExclusions,
    pub input_hash: String,
    packet_framing: Option<String>,
    harvester_sources: Vec<crate::plugins::harvester::delivery::SourceContext>,
}

/// Load and fingerprint concrete material without assembling a prompt or calling a model.
pub async fn load_narratives_material(
    pool: &sqlx::PgPool,
    req: &NarrativesReq,
    from_harvester: bool,
) -> Result<NarrativesMaterial> {
    let sport = req.sport.to_uppercase();
    let (corpus, corpus_exclusions, packet_framing, harvester_sources) = if from_harvester {
        let sources = crate::plugins::harvester::delivery::load_for_character(
            pool,
            crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
            &req.entity_type,
            req.entity_id,
            &sport,
        )
        .await?;
        let corpus = sources
            .iter()
            .map(|source| CorpusItem {
                id: source.article_id,
                title: source.headline.clone(),
                description: String::new(),
                harvested_context: Some(source.context.clone()),
                source: source.source.clone(),
                published_at_epoch: source.published_at_epoch,
            })
            .collect();
        let (corpus, dropped) = journalist::apply_news_budget(corpus, SOURCE_NEWS_BUDGET_CHARS);
        (
            corpus,
            CorpusExclusions {
                budget_truncated_ids: dropped,
            },
            String::new(),
            sources,
        )
    } else {
        let (corpus, exclusions, framing) = load_packet_corpus(
            pool,
            &req.entity_type,
            req.entity_id,
            &sport,
            &req.entity_name,
        )
        .await?;
        (corpus, exclusions, framing, Vec::new())
    };
    let article_ids: Vec<i64> = corpus.iter().map(|item| item.id).collect();
    let mut request = MemoryRequest::new(
        Mission::Journalist,
        &req.entity_type,
        req.entity_id,
        &req.sport,
    );
    request.current_article_ids = &article_ids;
    request.include_storyline_history = !from_harvester;
    let memories = memories::load(pool, request).await?;
    let mut base_components = journalist::build_narratives_input_components(&corpus);
    if from_harvester {
        base_components = serde_json::json!({
            "corpus": serde_json::from_str::<serde_json::Value>(&base_components)?,
            "harvester_contexts": harvester_sources.iter().map(|source| {
                (source.classification_id, source.article_id, &source.headline, &source.context)
            }).collect::<Vec<_>>(),
        })
        .to_string();
    }
    let input_components = memories.with_input_components(&base_components)?;
    let input_hash = crate::util::hash_components(&input_components);
    Ok(NarrativesMaterial {
        memories,
        corpus,
        corpus_exclusions,
        input_hash,
        packet_framing: Some(packet_framing),
        harvester_sources,
    })
}

/// Convert loaded application material into the complete service-free Studio assignment.
pub fn finish_narratives_assignment(
    models: &ExecutionCapabilities,
    req: &NarrativesReq,
    material: NarrativesMaterial,
    temperature: f64,
) -> Result<Assignment> {
    let NarrativesMaterial {
        memories,
        corpus,
        corpus_exclusions,
        input_hash,
        packet_framing,
        harvester_sources: _,
    } = material;
    let memory = if corpus.is_empty() {
        None
    } else {
        Some(memories.render_for_model()?)
    };
    Ok(Assignment {
        subject: Subject {
            entity_type: req.entity_type.clone(),
            entity_name: req.entity_name.clone(),
            sport: req.sport.clone(),
        },
        corpus,
        corpus_exclusions,
        memory,
        packet_framing,
        input_hash,
        card_score_prev: memories.previous_score,
        options: journalist::generation_options(temperature, models.voice_num_ctx),
    })
}

type ClassifiedRow<'a> = (&'a Narrative, &'static str, serde_json::Value, Option<i64>);

async fn insert_narratives(
    tx: &mut Transaction<'_, Postgres>,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    trigger_type: &str,
    trigger_payload: &serde_json::Value,
    output: &NarrativesOutput,
    from_harvester: bool,
) -> Result<Vec<i64>> {
    let article_ids: Vec<i64> = output
        .narratives
        .iter()
        .flat_map(|narrative| narrative.input_news_ids.iter().copied())
        .collect();
    let storyline_of: std::collections::HashMap<i64, i64> =
        if from_harvester || article_ids.is_empty() {
            std::collections::HashMap::new()
        } else {
            sqlx::query(
            "SELECT article_id, storyline_id FROM storyline_articles WHERE article_id = ANY($1)",
        )
        .bind(&article_ids)
        .fetch_all(&mut **tx)
        .await
        .context("load article storylines")?
        .into_iter()
        .map(|row| (row.get("article_id"), row.get("storyline_id")))
        .collect()
        };
    let items: Vec<PartItem> = output
        .narratives
        .iter()
        .map(|narrative| {
            let cited = narrative
                .input_news_ids
                .iter()
                .filter_map(|article_id| storyline_of.get(article_id).copied())
                .collect::<Vec<_>>();
            PartItem {
                storyline_id: mode_storyline(&cited),
                impact: narrative.impact,
                source_names: &narrative.source_names,
            }
        })
        .collect();
    let outcomes = if from_harvester {
        Vec::new()
    } else {
        progress_generation(tx, sport, entity_type, entity_id, &items).await?
    };
    let classified: Vec<ClassifiedRow> = if from_harvester {
        output.narratives.iter().map(|narrative| (
            narrative,
            DEFAULT_TRAJECTORY,
            json!({"reason": "independent_source_context", "current_impact": narrative.impact}),
            None,
        )).collect()
    } else {
        output
            .narratives
            .iter()
            .zip(&outcomes)
            .map(|(narrative, outcome)| {
                let reason = match outcome.delta_reason {
                    "up" => "impact_up",
                    "down" => "impact_down",
                    "stable" => "impact_stable",
                    other => other,
                };
                let components = if outcome.unresolved {
                    json!({
                        "previous_impact": serde_json::Value::Null,
                        "current_impact": narrative.impact,
                        "impact_delta": serde_json::Value::Null,
                        "reason": "storyline_unresolved",
                    })
                } else {
                    json!({
                        "previous_impact": outcome.previous_impact,
                        "current_impact": narrative.impact,
                        "impact_delta": outcome.impact_delta,
                        "reason": reason,
                        "storyline_id": outcome.storyline_id,
                    })
                };
                (
                    narrative,
                    outcome.trajectory,
                    components,
                    outcome.storyline_id,
                )
            })
            .collect()
    };

    const INSERT: &str = r#"
        INSERT INTO news_summaries (
            entity_type, entity_id, sport, trigger_type, trigger_payload,
            narrative_title, body, impact, impact_components,
            input_news_ids,
            narrative_updated_at, source_count, source_names, source_latest_at, source_oldest_at,
            trajectory, trajectory_components,
            model_version, prompt_version, input_hash, storyline_id,
            card_score, card_score_prev, headline, generated_at
        ) VALUES (
            $1,$2,$3,$4,$5::jsonb, $6,$7,$8,$9::jsonb, $10,
            COALESCE(to_timestamp($11::double precision), NOW()), $12, $13,
            to_timestamp($14::double precision), to_timestamp($15::double precision),
            $16, $17::jsonb,
            $18,$19,$20,$21,
            $22,$23,$24,NOW()
        )
        RETURNING id"#;

    let rows: Vec<Option<ClassifiedRow>> = if classified.is_empty() {
        vec![None]
    } else {
        classified.into_iter().map(Some).collect()
    };
    let provenance = &output.provenance;
    let trigger_json = trigger_payload.to_string();
    let mut product_row_ids = Vec::with_capacity(rows.len());
    for row in rows {
        let impact_components_json;
        let trajectory_json;
        let empty_names = Vec::<String>::new();
        let title: Option<&str>;
        let body: Option<&str>;
        let impact: Option<i16>;
        let input_news_ids: &Vec<i64>;
        let narrative_updated_at: Option<i64>;
        let source_count: i32;
        let source_names: &Vec<String>;
        let source_latest_at: Option<i64>;
        let source_oldest_at: Option<i64>;
        let trajectory: &str;
        let storyline_id: Option<i64>;
        let context: &str;

        match &row {
            Some((narrative, row_trajectory, row_components, row_storyline_id)) => {
                impact_components_json = narrative.impact_components.to_string();
                trajectory_json = row_components.to_string();
                title = Some(narrative.title.as_str());
                body = Some(narrative.body.as_str());
                impact = Some(narrative.impact as i16);
                input_news_ids = &narrative.input_news_ids;
                narrative_updated_at = narrative.source_latest_epoch;
                source_count = narrative.source_count;
                source_names = &narrative.source_names;
                source_latest_at = narrative.source_latest_epoch;
                source_oldest_at = narrative.source_oldest_epoch;
                trajectory = row_trajectory;
                storyline_id = *row_storyline_id;
                context = "persist narrative row";
            }
            None => {
                impact_components_json = "{}".to_string();
                trajectory_json = "{}".to_string();
                title = None;
                body = None;
                impact = None;
                input_news_ids = &provenance.input_ids;
                narrative_updated_at = None;
                source_count = 0;
                source_names = &empty_names;
                source_latest_at = None;
                source_oldest_at = None;
                trajectory = DEFAULT_TRAJECTORY;
                storyline_id = None;
                context = "persist narratives marker";
            }
        }

        let inserted = sqlx::query(INSERT)
            .bind(entity_type)
            .bind(entity_id)
            .bind(sport)
            .bind(trigger_type)
            .bind(&trigger_json)
            .bind(title)
            .bind(body)
            .bind(impact)
            .bind(&impact_components_json)
            .bind(input_news_ids)
            .bind(narrative_updated_at)
            .bind(source_count)
            .bind(source_names)
            .bind(source_latest_at)
            .bind(source_oldest_at)
            .bind(trajectory)
            .bind(&trajectory_json)
            .bind(provenance.model_version.as_str())
            .bind(provenance.prompt_version)
            .bind(provenance.input_hash.as_deref())
            .bind(storyline_id)
            .bind(output.card_score)
            .bind(output.card_score_prev)
            .bind(output.headline.as_deref())
            .fetch_one(&mut **tx)
            .await
            .context(context)?;
        product_row_ids.push(inserted.get("id"));
    }
    Ok(product_row_ids)
}

struct LedgerSubject<'a> {
    entity_type: &'a str,
    entity_id: i32,
    sport: &'a str,
    trigger_type: &'a str,
    trigger_payload: &'a serde_json::Value,
    from_harvester: bool,
}

async fn record_ledger(
    pool: &PgPool,
    subject: &LedgerSubject<'_>,
    product_row_ids: Vec<i64>,
    output: &NarrativesOutput,
) -> Result<()> {
    let narratives = output
        .narratives
        .iter()
        .map(|narrative| {
            json!({
                "title": &narrative.title,
                "input_news_ids": &narrative.input_news_ids,
                "source_count": narrative.source_count,
                "source_names": &narrative.source_names,
                "impact": narrative.impact,
            })
        })
        .collect::<Vec<_>>();
    let included_evidence = json!({
        "input_news_ids": &output.provenance.input_ids,
        "narratives": narratives,
    });
    let num_ctx = output
        .request_body()
        .and_then(|body| body.pointer("/options/num_ctx"))
        .and_then(|value| value.as_i64())
        .unwrap_or(crate::studio::model::VOICE_NUM_CTX_PACKET as i64) as i32;
    let mut excluded = Vec::new();
    if !output.budget_truncated_ids.is_empty() {
        excluded.push(if subject.from_harvester {
            json!({
                "reason": "budget_truncated",
                "dropped_count": output.budget_truncated_ids.len(),
                "dropped_news_ids": &output.budget_truncated_ids,
                "source_context_budget_chars": SOURCE_NEWS_BUDGET_CHARS,
            })
        } else {
            json!({
                "reason": "budget_truncated",
                "dropped_count": output.budget_truncated_ids.len(),
                "dropped_news_ids": &output.budget_truncated_ids,
                "packet_count_limit": MAX_PACKETS_PER_ENTITY,
                "news_budget_chars": PACKET_NEWS_BUDGET_CHARS,
            })
        });
    }
    insert_generation_ledger_best_effort(
        pool,
        output,
        NARRATIVES_LEDGER,
        LedgerEvent {
            entity_type: subject.entity_type,
            entity_id: subject.entity_id,
            sport: subject.sport,
            pair_entity: None,
            trigger_type: subject.trigger_type,
            trigger_payload: subject.trigger_payload.clone(),
            product_row_ids,
            included_evidence,
            excluded_evidence: json!(excluded),
            context_budget: output.context_budget(json!({
                "num_predict": output.request_body()
                    .and_then(|body| body.pointer("/options/num_predict"))
                    .and_then(|value| value.as_i64())
                    .unwrap_or(NARRATIVES_NUM_PREDICT_PACKET as i64),
                "num_ctx": num_ctx,
            })),
            parser_outcome: if !output.was_called() {
                "no_call"
            } else if output.narratives.is_empty() {
                "parsed_empty"
            } else {
                "parsed"
            },
        },
    )
    .await;
    Ok(())
}

enum Prepared<'a> {
    Debounced,
    Product(&'a NarrativesOutput),
}

async fn commit_claimed(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    trigger_type: &str,
    trigger_payload: &serde_json::Value,
    prepared: &Prepared<'_>,
    harvester_sources: &[crate::plugins::harvester::delivery::SourceContext],
) -> Result<(PluginOutcome, Vec<i64>)> {
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok((PluginOutcome::Superseded, Vec::new()));
    };
    let product_row_ids = match prepared {
        Prepared::Debounced => Vec::new(),
        Prepared::Product(output) => {
            insert_narratives(
                publication.transaction(),
                &item.entity_type,
                item.entity_id_i32()?,
                sport,
                trigger_type,
                trigger_payload,
                output,
                !harvester_sources.is_empty(),
            )
            .await?
        }
    };
    if !harvester_sources.is_empty() {
        let cited: std::collections::HashSet<i64> = match prepared {
            Prepared::Debounced => std::collections::HashSet::new(),
            Prepared::Product(output) => output
                .narratives
                .iter()
                .flat_map(|narrative| narrative.input_news_ids.iter().copied())
                .collect(),
        };
        for source in harvester_sources {
            let status = if matches!(prepared, Prepared::Debounced) {
                "redundant"
            } else if cited.contains(&source.article_id) {
                "used"
            } else {
                "abstained"
            };
            sqlx::query(
                "UPDATE public.harvester_assignments SET status=$3, reason=$4, \
                 product_ref=$5, updated_at=NOW() \
                 WHERE classification_id=$1 AND plugin_id=$2 AND status='pending'",
            )
            .bind(source.classification_id)
            .bind(crate::plugins::journalist::manifest::MANIFEST.id.as_str())
            .bind(status)
            .bind(if status == "abstained" {
                Some("not cited by Journalist")
            } else {
                None
            })
            .bind(serde_json::json!({"news_summary_ids": product_row_ids}))
            .execute(&mut **publication.transaction())
            .await?;
        }
    }
    record_narratives_completed(publication.transaction(), item).await?;
    publication.commit_final().await?;
    Ok((PluginOutcome::Committed, product_row_ids))
}

pub struct NarrativesHandler {
    pool: sqlx::PgPool,
    models: ExecutionCapabilities,
}

impl NarrativesHandler {
    pub fn new(pool: sqlx::PgPool, models: ExecutionCapabilities) -> Self {
        Self { pool, models }
    }
}

#[async_trait]
impl StudioPlugin for NarrativesHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &crate::plugins::journalist::manifest::MANIFEST
    }

    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        let pool = &self.pool;
        let models = &self.models;
        let entity_id = item.entity_id_i32()?;
        let sport = item.sport.to_uppercase();
        let name = crate::evidence::corpus::lookup_entity_name(
            pool,
            &item.entity_type,
            entity_id,
            &item.sport,
        )
        .await?;
        let from_harvester = item.input_version.as_deref().is_some_and(|version| {
            version.starts_with(crate::plugins::harvester::context::CONTRACT)
        });
        let req = NarrativesReq {
            entity_type: item.entity_type.clone(),
            entity_id,
            entity_name: name,
            sport: item.sport.clone(),
            trigger_type: "periodic".to_string(),
        };
        let material = load_narratives_material(pool, &req, from_harvester).await?;
        let harvester_sources = material.harvester_sources.clone();
        if from_harvester && harvester_sources.is_empty() {
            return Ok(commit_claimed(
                pool,
                item,
                &sport,
                &req.trigger_type,
                &serde_json::Value::Null,
                &Prepared::Debounced,
                &[],
            )
            .await?
            .0);
        }
        let unchanged = crate::application::products::debounce_unchanged(
            pool,
            "news_summaries",
            &EntityKey {
                entity_type: item.entity_type.clone(),
                entity_id,
                sport: sport.clone(),
                season: None,
            },
            &material.input_hash,
        )
        .await?;
        let trigger_payload = if from_harvester {
            json!({"source": "harvester"})
        } else {
            serde_json::Value::Null
        };
        if unchanged {
            debug!(
                entity_type = %item.entity_type,
                entity_id,
                sport = %sport,
                "narratives: inputs unchanged, skipping generation"
            );
            return Ok(commit_claimed(
                pool,
                item,
                &sport,
                &req.trigger_type,
                &trigger_payload,
                &Prepared::Debounced,
                &harvester_sources,
            )
            .await?
            .0);
        }

        let assignment =
            finish_narratives_assignment(models, &req, material, NARRATIVES_TEMPERATURE)?;
        let backend = models.inference(crate::plugins::journalist::manifest::ROUTE)?;
        let output =
            journalist::create(&Studio::new(backend.as_ref()), &assignment, now_unix()).await?;
        let (outcome, product_row_ids) = commit_claimed(
            pool,
            item,
            &sport,
            &req.trigger_type,
            &trigger_payload,
            &Prepared::Product(&output),
            &harvester_sources,
        )
        .await?;
        if outcome == PluginOutcome::Committed {
            record_ledger(
                pool,
                &LedgerSubject {
                    entity_type: &item.entity_type,
                    entity_id,
                    sport: &sport,
                    trigger_type: &req.trigger_type,
                    trigger_payload: &trigger_payload,
                    from_harvester,
                },
                product_row_ids,
                &output,
            )
            .await?;
        }
        Ok(outcome)
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
