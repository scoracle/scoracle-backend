//! Complete source preparation, linked mentions, measured history and model requests.
use crate::harness::route::RouteKey;
pub const MODEL: RouteKey = RouteKey::new("transfer-logic", "TRANSFER_LOGIC");
// Reserve room for a bounded full article, instructions, history and output.
pub const ARTICLE_NUM_CTX: i32 = 32768;
pub const CONTEXT_BUDGET_BYTES: usize = 24000;
use crate::harness::model::GenerateOptions;
use crate::harness::queue::work::Item;
use crate::plugins::classifier::{adapter::load_measurement, delivery::load_for_character};
use crate::tools::memories::{self as study, HistoryItem, ReportingHistory, SourceRecord};
use crate::tools::meta::EntityMeta;
use crate::tools::source::SourceContext;
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use sqlx::PgPool;

const SYSTEM: &str = "Write an attributed transfer or trade reading for the named entity from the supplied publisher reports. The source text establishes what was reported, not whether a move happened. A co-mentioned name is only a candidate subject; do not infer a move from its presence. Preserve denials, uncertainty, source disagreements and report dates. Classifier worlds contain proposed literal claim relationships, not independent confirmation. Unassessed signals and unknown relationships remain unresolved; read them with the complete publisher text. Use prior reports only as history; weigh measured publisher records with their tracked sample sizes, and do not treat missing records as poor reliability. Return one JSON object with body and findings. For each specific reported move or explicit denial involving this entity and a co-mentioned counterparty, give its zero-based report_index, exact counterparty name, status (reported or denied), and an exact continuous evidence_quote from that report's headline or publisher_excerpt. For reported moves, set stage to speculation, concrete_interest, advanced_talks, or here_we_go; for denials, set stage to null. A denial requires an explicit source statement about that move; absence, silence, and unrelated mentions produce no finding. Use [] when there are no supported findings. Do not report completed identity changes from transfer speculation.";

pub const PROMPT_VERSION: &str = "insider-source-v7-classifier-world";
pub const OUTPUT_CONTRACT_VERSION: &str = "insider-reading-findings-v2";
pub const NUM_PREDICT: i32 = 1400;

pub fn options(num_ctx: i32) -> GenerateOptions {
    GenerateOptions {
        system: Some(SYSTEM.into()),
        temperature: Some(0.3),
        num_predict: NUM_PREDICT,
        num_ctx: num_ctx.max(ARTICLE_NUM_CTX),
        json_mode: false,
        format_schema: Some(serde_json::json!({
            "type": "object", "additionalProperties": false,
            "required": ["body", "findings"],
            "properties": {
                "body": {"type": "string"},
                "findings": {
                    "type": "array",
                    "items": {
                        "type": "object", "additionalProperties": false,
                        "required": ["report_index", "counterparty", "status", "stage", "evidence_quote"],
                        "properties": {
                            "report_index": {"type": "integer", "minimum": 0},
                            "counterparty": {"type": "string"},
                            "status": {"type": "string", "enum": ["reported", "denied"]},
                            "stage": {"type": ["string", "null"], "enum": ["speculation", "concrete_interest", "advanced_talks", "here_we_go", null]},
                            "evidence_quote": {"type": "string"}
                        }
                    }
                }
            }
        })),
        format_schema_raw: None,
    }
}

const HISTORY: ReportingHistory = ReportingHistory {
    lookback_seconds: 90 * 24 * 60 * 60,
    max_reports: 6,
    budget_bytes: 1800,
    grouped: false,
};

#[derive(Clone)]
pub(super) struct Match {
    pub(super) name: String,
    pub(super) entity_type: String,
    pub(super) entity_id: i32,
}

pub(super) struct Material {
    pub(super) subject: EntityMeta,
    pub(super) sources: Vec<SourceContext>,
    pub(super) reports: Vec<Report>,
    pub(super) mentions: Vec<Vec<Match>>,
    pub(super) history: Vec<HistoryItem>,
    pub(super) source_records: Vec<study::SourceRecord>,
}

pub(super) async fn load_material(pool: &PgPool, item: &Item) -> Result<Material> {
    let sport = item.sport.to_uppercase();
    let entity_id = item.entity_id_i32()?;
    let name: String = if item.entity_type == "person" {
        sqlx::query_scalar(
            "SELECT full_name FROM public.persons WHERE id=$1 AND sport=$2 AND kind='coach'",
        )
        .bind(entity_id)
        .bind(&sport)
        .fetch_one(pool)
        .await?
    } else {
        crate::tools::meta::lookup_entity_name(pool, &item.entity_type, entity_id, &sport).await?
    };
    let subject = EntityMeta {
        name,
        entity_type: item.entity_type.clone(),
        entity_id,
        sport: sport.clone(),
    };
    let sources = load_for_character(
        pool,
        crate::plugins::insider::manifest::MANIFEST.id.as_str(),
        &item.entity_type,
        entity_id,
        &sport,
    )
    .await?;
    ensure!(
        sources.iter().all(|s| s
            .published_at_epoch
            .is_some_and(|t| t <= crate::plugins::influencer::now())),
        "Insider publication time unresolved; source work remains pending"
    );
    let mut reports = Vec::with_capacity(sources.len());
    let mut mentions = Vec::with_capacity(sources.len());
    for source in &sources {
        let (native, _, _) = load_measurement(pool, source.classification_id).await?;
        let found = native.provenance["identity_candidates"]
            .as_array()
            .context("native identities missing")?
            .iter()
            .filter(|candidate| {
                candidate["entity_type"] != item.entity_type || candidate["entity_id"] != entity_id
            })
            .map(|candidate| {
                Ok(Match {
                    name: candidate["name"].as_str().context("native name")?.into(),
                    entity_type: candidate["entity_type"]
                        .as_str()
                        .context("native type")?
                        .into(),
                    entity_id: i32::try_from(
                        candidate["entity_id"].as_i64().context("native id")?,
                    )?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        reports.push(Report {
            classifier_world: source.classifier_world.clone(),
            publisher: source.source.clone(),
            published_at: source.published_at_epoch.map(crate::util::utc_timestamp),
            headline: source.headline.clone(),
            publisher_excerpt: crate::tools::fetch::decode_entities(&source.context),
            co_mentions: found
                .iter()
                .map(|m| Mention {
                    name: m.name.clone(),
                    entity_type: m.entity_type.clone(),
                })
                .collect(),
        });
        mentions.push(found);
    }
    let history = load_history(pool, &subject, &sources).await?;
    let publishers = sources
        .iter()
        .map(|source| source.source.clone())
        .collect::<Vec<_>>();
    let source_records = study::source_records(pool, &sport, &publishers).await?;
    Ok(Material {
        subject,
        sources,
        reports,
        mentions,
        history,
        source_records,
    })
}

pub(crate) async fn preview(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Option<String>> {
    let item = Item {
        stage: crate::plugins::insider::manifest::TASK,
        entity_type: entity_type.into(),
        entity_id: i64::from(entity_id),
        sport: sport.into(),
        input_version: None,
        attempts: 0,
        claim_token: None,
    };
    let material = load_material(pool, &item).await?;
    Ok((!material.sources.is_empty()).then(|| {
        assemble(
            &material.subject,
            &material.reports,
            &material.history,
            &material.source_records,
        )
    }))
}

async fn load_history(
    pool: &PgPool,
    subject: &EntityMeta,
    sources: &[SourceContext],
) -> Result<Vec<HistoryItem>> {
    if sources.is_empty() || subject.entity_type == "person" {
        return Ok(Vec::new());
    }
    let before = sources.iter().filter_map(|s| s.published_at_epoch).min();
    let Some(before) = before else {
        return Ok(Vec::new());
    };
    let excluded: Vec<i64> = sources.iter().map(|s| s.article_id).collect();
    let prior: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT u.article_id FROM public.transfer_rumors r \
         CROSS JOIN LATERAL unnest(r.input_news_ids) AS u(article_id) \
         WHERE r.sport=$1 AND r.is_rumor IS NOT NULL \
           AND (($2='team' AND r.team_id=$3) OR ($2='player' AND r.subject_type='player' AND r.player_id=$3)) \
         ORDER BY u.article_id LIMIT 200"
    ).bind(&subject.sport).bind(&subject.entity_type).bind(subject.entity_id).fetch_all(pool).await?;
    if prior.is_empty() {
        return Ok(Vec::new());
    }
    let report = study::reporting_scope(
        pool,
        subject,
        before - HISTORY.lookback_seconds,
        before,
        &excluded,
        HISTORY.max_reports,
        2,
        &prior,
        None,
    )
    .await?;
    HISTORY.select(&report, subject, Some(before), &excluded, |_| true)
}

#[derive(Clone, Debug, Serialize)]
pub struct Mention {
    pub name: String,
    pub entity_type: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classifier_world: Option<serde_json::Value>,
    pub publisher: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    pub headline: String,
    pub publisher_excerpt: String,
    pub co_mentions: Vec<Mention>,
}

pub fn assemble(
    subject: &EntityMeta,
    reports: &[Report],
    history: &[HistoryItem],
    source_records: &[SourceRecord],
) -> String {
    #[derive(Serialize)]
    struct Input<'a> {
        meta: crate::tools::meta::WritingIdentity<'a>,
        fresh: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        memories: Option<serde_json::Value>,
        voice: &'static str,
        form: serde_json::Value,
    }
    let memories = (!history.is_empty() || !source_records.is_empty()).then(|| {
        serde_json::json!({
            "prior_reports": history,
            "source_records": source_records,
        })
    });
    serde_json::to_string(&Input {
        meta: subject.for_writing(),
        fresh: serde_json::json!({ "reports": reports }),
        memories,
        voice: crate::plugins::insider::voice::VOICE,
        form: serde_json::json!({
            "keys": ["body", "findings"],
            "paragraphs": "concise",
            "findings": {
                "type": "array",
                "item_keys": ["report_index", "counterparty", "status", "stage", "evidence_quote"]
            }
        }),
    })
    .expect("insider world serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_source_world_contains_only_the_approved_parts() {
        let subject = EntityMeta {
            name: "Jordan Sample".into(),
            entity_type: "player".into(),
            entity_id: 7,
            sport: "NFL".into(),
        };
        let report = Report {
            classifier_world: None,
            publisher: "Wire".into(),
            published_at: Some("2026-10-02".into()),
            headline: "Browns consider Jordan Sample".into(),
            publisher_excerpt: "Cleveland is monitoring Jordan Sample.".into(),
            co_mentions: vec![Mention {
                name: "Cleveland Browns".into(),
                entity_type: "team".into(),
            }],
        };
        let world = assemble(&subject, &[report], &[], &[]);
        let fresh_at = world.find(r#""fresh":"#).unwrap();
        let voice_at = world.find(r#""voice":"#).unwrap();
        let form_at = world.find(r#""form":"#).unwrap();
        assert!(world.starts_with(r#"{"meta":"#) && fresh_at < voice_at && voice_at < form_at);
        let value: serde_json::Value = serde_json::from_str(&world).unwrap();
        assert_eq!(value["meta"]["sport"], "American football");
        assert_eq!(
            value["fresh"]["reports"][0]["co_mentions"][0]["name"],
            "Cleveland Browns"
        );
        assert!(value["meta"].get("entity_id").is_none());
        assert!(value.get("memories").is_none());
        assert_eq!(
            value["form"]["keys"],
            serde_json::json!(["body", "findings"])
        );
        assert_eq!(
            value["form"]["findings"]["item_keys"],
            serde_json::json!([
                "report_index",
                "counterparty",
                "status",
                "stage",
                "evidence_quote"
            ])
        );
    }

    #[test]
    fn measured_source_record_keeps_its_sample_size() {
        let subject = EntityMeta {
            name: "Jordan Sample".into(),
            entity_type: "player".into(),
            entity_id: 7,
            sport: "NFL".into(),
        };
        let world = assemble(
            &subject,
            &[],
            &[],
            &[SourceRecord {
                publisher: "Wire".into(),
                confirmed: 2,
                tracked: 3,
                reliability: 25,
            }],
        );
        let value: serde_json::Value = serde_json::from_str(&world).unwrap();
        assert_eq!(value["memories"]["source_records"][0]["tracked"], 3);
        assert_eq!(value["memories"]["source_records"][0]["reliability"], 25);
    }
}
