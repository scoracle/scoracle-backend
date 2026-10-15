//! Complete source preparation, linked mentions, measured history and model requests.
use crate::harness::route::RouteKey;
pub const MODEL: RouteKey = RouteKey::new("transfer-logic", "TRANSFER_LOGIC");
// Reserve room for a bounded full article, instructions, history and output.
pub const ARTICLE_NUM_CTX: i32 = 32768;
pub const CONTEXT_BUDGET_BYTES: usize = 24000;
use crate::harness::model::GenerateOptions;
use crate::harness::queue::work::Item;
use crate::plugins::classifier::{delivery::load_for_character, delivery::load_spectrum_source};
use crate::tools::memories::{self as study, HistoryItem, ReportingHistory, SourceRecord};
use crate::tools::meta::EntityMeta;
use crate::tools::source::SourceContext;
use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::PgPool;

const SYSTEM: &str = "Write an attributed transfer or trade reading for the named entity from the supplied publisher reports. The source text establishes what was reported, not whether a move happened. A co-mentioned name is only a candidate subject; do not infer a move from its presence. Preserve denials, uncertainty, source disagreements and report dates. Each report carries article-level classifier spectrum scores — routing measurements of the complete article, which do not establish relevance to the named entity, not proof; unmeasured criteria stay unresolved. Read the scores with the complete publisher text. Use prior reports only as history; weigh measured publisher records with their tracked sample sizes, and do not treat missing records as poor reliability. Return one JSON object with body and findings. Keep the body to two or three short sentences, at most 1200 characters, and return at most two of the most significant supported findings. The body must name the exact target from meta.name, even when no move is supported. If these reports do not support a move involving that target, explain that evidence limit and return findings as []; do not retell another subject's move. For each specific reported move or explicit denial involving this entity and a co-mentioned counterparty, give its zero-based report_index, exact counterparty name, status (reported or denied), and an exact continuous evidence_quote from that report's headline or publisher_excerpt. For reported moves, set stage to speculation, concrete_interest, advanced_talks, or here_we_go; for denials, set stage to null. A denial requires an explicit source statement about that move; absence, silence, and unrelated mentions produce no finding. Use [] when there are no supported findings. Do not report completed identity changes from transfer speculation.";

pub const PROMPT_VERSION: &str = "insider-source-v10-bounded-reports";
pub const OUTPUT_CONTRACT_VERSION: &str = "insider-reading-findings-v3-bounded";
pub const NUM_PREDICT: i32 = 1400;
pub const BODY_MAX_CHARS: usize = 1200;
pub const MAX_FINDINGS: usize = 2;
pub const EVIDENCE_QUOTE_MAX_CHARS: usize = 500;

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
                "body": {"type": "string", "maxLength": BODY_MAX_CHARS},
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
                            "evidence_quote": {"type": "string", "minLength":1, "maxLength":EVIDENCE_QUOTE_MAX_CHARS}
                        }
                    }
                }
            }
        })),
        format_schema_raw: None,
    }
}

pub fn options_for_reports(
    num_ctx: i32,
    subject: &EntityMeta,
    reports: &[Report],
) -> GenerateOptions {
    let mut options = options(num_ctx);
    let names: std::collections::BTreeSet<_> = reports
        .iter()
        .flat_map(|r| &r.co_mentions)
        .filter(|m| {
            if subject.entity_type == "team" {
                matches!(m.entity_type.as_str(), "player" | "person")
            } else {
                m.entity_type == "team"
            }
        })
        .map(|m| &m.name)
        .collect();
    let findings = &mut options.format_schema.as_mut().unwrap()["properties"]["findings"];
    findings["maxItems"] = serde_json::json!(if names.is_empty() {
        0
    } else {
        MAX_FINDINGS.min(reports.len() * 8)
    });
    findings["items"]["properties"]["report_index"]["maximum"] =
        serde_json::json!(reports.len().saturating_sub(1));
    if !names.is_empty() {
        findings["items"]["properties"]["counterparty"]["enum"] = serde_json::json!(names);
    }
    options
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
    pub(super) held: Vec<(SourceContext, &'static str)>,
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
    let mut reports = Vec::with_capacity(sources.len());
    let mut mentions = Vec::with_capacity(sources.len());
    for source in &sources {
        let native = load_spectrum_source(pool, source.classification_id).await?;
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
    let mut material = Material {
        subject,
        sources,
        reports,
        mentions,
        history,
        source_records,
        held: Vec::new(),
    };
    material.pack(crate::plugins::influencer::now());
    Ok(material)
}

impl Material {
    fn pack(&mut self, now: i64) {
        // Process oldest first so later batches cannot overwrite newer movement with old news.
        let reports = std::mem::take(&mut self.reports);
        let mentions = std::mem::take(&mut self.mentions);
        let sources = std::mem::take(&mut self.sources);
        for ((source, report), mentions) in sources.into_iter().zip(reports).zip(mentions).rev() {
            let reason = if source
                .classifier_world
                .as_ref()
                .is_some_and(|world| world["target_identity_resolved"] == false)
            {
                Some("unresolved_target_identity")
            } else if !source.published_at_epoch.is_some_and(|date| date <= now) {
                Some("unresolved_publication_time")
            } else if crate::tools::source::contains_instruction_override(&source.context) {
                Some("source_instruction_override")
            } else if assemble(
                &self.subject,
                std::slice::from_ref(&report),
                &self.history,
                &self.source_records,
            )
            .len()
                > CONTEXT_BUDGET_BYTES
            {
                Some("complete_report_exceeds_context_budget")
            } else {
                None
            };
            if let Some(reason) = reason {
                self.held.push((source, reason));
                continue;
            }
            if !self.sources.is_empty() {
                break;
            }
            self.reports.push(report);
            if assemble(
                &self.subject,
                &self.reports,
                &self.history,
                &self.source_records,
            )
            .len()
                > CONTEXT_BUDGET_BYTES
            {
                self.reports.pop();
                break;
            }
            self.sources.push(source);
            self.mentions.push(mentions);
        }
        // Publication and finding indices use newest-first order within the selected batch.
        self.sources.reverse();
        self.reports.reverse();
        self.mentions.reverse();
    }
}

pub(crate) async fn preview(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Option<(String, GenerateOptions)>> {
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
        (
            assemble(
                &material.subject,
                &material.reports,
                &material.history,
                &material.source_records,
            ),
            options_for_reports(0, &material.subject, &material.reports),
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
    fn batches_keep_complete_articles_oldest_first_and_hold_single_oversized_reports() {
        let source = |id, text: String| SourceContext {
            classifier_world: None,
            classification_id: id,
            article_id: id,
            headline: "Movement report".into(),
            context: text,
            source: "Wire".into(),
            published_at_epoch: Some(100 + id),
        };
        let mut sources = vec![
            source(3, "Newest report. ".repeat(850)),
            source(2, "Middle report. ".repeat(850)),
            source(1, "Oldest report. ".repeat(850)),
            source(0, "Oversized report. ".repeat(2000)),
        ];
        let mut unresolved = source(-1, "Other teams' movement.".into());
        unresolved.classifier_world = Some(serde_json::json!({"target_identity_resolved":false}));
        sources.push(unresolved);
        let reports = sources
            .iter()
            .map(|s| Report {
                classifier_world: None,
                publisher: s.source.clone(),
                published_at: None,
                headline: s.headline.clone(),
                publisher_excerpt: s.context.clone(),
                co_mentions: vec![],
            })
            .collect();
        let original = sources[2].context.clone();
        let mut material = Material {
            subject: EntityMeta {
                name: "Cedar".into(),
                entity_type: "team".into(),
                entity_id: 1,
                sport: "NFL".into(),
            },
            sources,
            reports,
            mentions: vec![vec![]; 5],
            history: vec![],
            source_records: vec![],
            held: vec![],
        };
        material.pack(200);
        assert_eq!(
            material
                .sources
                .iter()
                .map(|s| s.article_id)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(material.reports[0].publisher_excerpt, original);
        assert_eq!(material.held[0].0.article_id, -1);
        assert_eq!(material.held[0].1, "unresolved_target_identity");
        assert_eq!(material.held[1].0.article_id, 0);
        assert_eq!(material.held[1].1, "complete_report_exceeds_context_budget");
        assert!(
            assemble(
                &material.subject,
                &material.reports,
                &material.history,
                &material.source_records
            )
            .len()
                <= CONTEXT_BUDGET_BYTES
        );
    }

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

    #[test]
    fn native_findings_are_bounded_to_the_supplied_counterparties_and_reports() {
        let subject = EntityMeta {
            name: "Cedar".into(),
            entity_type: "team".into(),
            entity_id: 1,
            sport: "NFL".into(),
        };
        let mut report = Report {
            classifier_world: None,
            publisher: "Wire".into(),
            published_at: None,
            headline: "Movement".into(),
            publisher_excerpt: "Cedar reported talks.".into(),
            co_mentions: vec![
                Mention {
                    name: "Alex Reed".into(),
                    entity_type: "player".into(),
                },
                Mention {
                    name: "Other Club".into(),
                    entity_type: "team".into(),
                },
            ],
        };
        let options = options_for_reports(32768, &subject, std::slice::from_ref(&report));
        let findings = &options.format_schema.as_ref().unwrap()["properties"]["findings"];
        assert_eq!(
            findings["items"]["properties"]["counterparty"]["enum"],
            serde_json::json!(["Alex Reed"])
        );
        assert_eq!(
            findings["items"]["properties"]["report_index"]["maximum"],
            0
        );
        report.co_mentions.clear();
        assert_eq!(
            options_for_reports(32768, &subject, &[report])
                .format_schema
                .unwrap()["properties"]["findings"]["maxItems"],
            0
        );
    }
}
