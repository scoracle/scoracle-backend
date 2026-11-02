//! Load and bound fresh movement headlines and canonical candidate names.

const GUIDE: crate::tools::prompt::Guide = crate::tools::prompt::Guide {
    relationship: "new_content contains movement reporting. Source wording establishes whether the target moves, recruits, is a former affiliation, or discusses another entity's move. Do not substitute the target for a report's named actor. An injury absence is not a departure or appointment.",
    fields: &[
        ("reports.publisher", "supplying outlet, distinct from any speaker in its text."),
        ("reports.headline", "supplied title."),
        ("reports.publisher_excerpt", "supplied passage."),
        ("reports.report_index", "zero-based citation for this exact report."),
        ("reports.co_mentions", "candidate identities, not established relationships or movement roles."),
        ("reports.classifier_world.target_identity_resolved", "upstream identity match, not claim verification."),
        ("reports.classifier_world.evidence_scope", "available text only; headline supplies no unseen article contents."),
        ("reports.classifier_world.qualified_claims", "selected claims with their qualifications."),
    ],
};

use super::request::Parts;
use crate::harness::queue::work::Item;
use crate::plugins::classifier::delivery::{load_for_character, load_spectrum_sources_on};
use crate::tools::{meta::EntityMeta, source::SourceContext};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mention {
    pub name: String,
    pub entity_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub publisher: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    pub headline: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub publisher_excerpt: String,
    pub co_mentions: Vec<Mention>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classifier_world: Option<serde_json::Value>,
}

pub(super) fn select(reports: &[Report]) -> Vec<Report> {
    crate::tools::source::top_articles(reports, "movement", |r| r.classifier_world.as_ref())
}

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
    pub(super) database_record: Option<crate::tools::meta::DatabaseRecord>,
    pub(super) mentions: Vec<Vec<Match>>,
    pub(super) held: Vec<(SourceContext, &'static str)>,
}

pub(super) async fn load_material(pool: &PgPool, item: &Item) -> Result<Material> {
    let sport = item.sport.to_uppercase();
    let entity_id = item.entity_id_i32()?;
    let name =
        crate::tools::meta::lookup_entity_name(pool, &item.entity_type, entity_id, &sport).await?;
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
    let ids: Vec<_> = sources.iter().map(|s| s.classification_id).collect();
    let native = load_spectrum_sources_on(&mut *pool.acquire().await?, &ids).await?;
    let native: std::collections::HashMap<_, _> =
        native.into_iter().map(|s| (s.article_id, s)).collect();
    for source in &sources {
        let native = native
            .get(&source.article_id)
            .context("native source missing")?;
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
            classifier_world: Some({
                let mut provenance = source
                    .classifier_world
                    .clone()
                    .unwrap_or_else(|| serde_json::json!({}));
                provenance["article_id"] = serde_json::json!(source.article_id);
                provenance
            }),
            publisher: source.source.clone(),
            published_at: source.published_at_epoch.map(crate::util::utc_timestamp),
            headline: source.headline.clone(),
            publisher_excerpt: if source
                .classifier_world
                .as_ref()
                .is_some_and(|w| w["evidence_scope"] == "headline")
            {
                String::new()
            } else {
                crate::tools::fetch::decode_entities(&source.context)
            },
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
    let database_record = crate::tools::meta::load_character_identity(
        pool,
        &subject.entity_type,
        subject.entity_id,
        &subject.sport,
    )
    .await?;
    let mut material = Material {
        subject,
        database_record,
        sources,
        reports,
        mentions,
        held: Vec::new(),
    };
    material.pack(crate::plugins::influencer::now());
    Ok(material)
}

impl Material {
    fn pack(&mut self, now: i64) {
        let sources = std::mem::take(&mut self.sources);
        let reports = std::mem::take(&mut self.reports);
        let mentions = std::mem::take(&mut self.mentions);
        let mut seen = std::collections::HashSet::new();
        for ((source, report), mentions) in sources.into_iter().zip(reports).zip(mentions) {
            let reason = if source
                .classifier_world
                .as_ref()
                .is_some_and(|w| w["target_identity_resolved"] == false)
            {
                Some("unresolved_target_identity")
            } else if !source.published_at_epoch.is_some_and(|t| t <= now) {
                Some("unresolved_publication_time")
            } else if crate::tools::source::is_headline_spam(&source.context) {
                Some("livestream_spam")
            } else if crate::tools::source::contains_instruction_override(&source.context) {
                Some("source_instruction_override")
            } else {
                None
            };
            if let Some(reason) = reason {
                self.held.push((source, reason));
                continue;
            }
            if !seen.insert(crate::tools::fresh::evidence_key(
                &source.source,
                &source.context,
            )) {
                self.held.push((source, "known_copy"));
                continue;
            }
            self.sources.push(source);
            self.reports.push(report);
            self.mentions.push(mentions);
        }
    }
}

pub(crate) async fn preview(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Option<Parts>> {
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
    Ok((!material.sources.is_empty()).then_some(Parts {
        subject: material.subject,
        reports: material.reports,
        database_record: material.database_record,
        memories: vec![],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_headlines_preserve_one_report_and_both_obligations() {
        let sources=(1..=2).map(|id| SourceContext{classification_id:id,article_id:id,headline:"Cedar deny talks".into(),
            context:"Cedar deny talks".into(),source:"Wire".into(),published_at_epoch:Some(100),
            classifier_world:Some(serde_json::json!({"evidence_scope":"headline","url":format!("https://example.invalid/{id}")}))}).collect::<Vec<_>>();
        let reports = sources
            .iter()
            .map(|s| Report {
                classifier_world: s.classifier_world.clone(),
                publisher: s.source.clone(),
                published_at: None,
                headline: s.headline.clone(),
                publisher_excerpt: String::new(),
                co_mentions: vec![],
            })
            .collect();
        let mut material = Material {
            subject: EntityMeta {
                name: "Cedar".into(),
                entity_type: "team".into(),
                entity_id: 1,
                sport: "NFL".into(),
            },
            sources,
            reports,
            database_record: None,
            mentions: vec![vec![]; 2],
            held: vec![],
        };
        let mut spam = material.sources[0].clone();
        spam.classification_id = 3;
        spam.article_id = 3;
        spam.context = "[+[LIVE@STREAMs]+] Cedar FC vs Maple United".into();
        spam.headline = spam.context.clone();
        let mut report = material.reports[0].clone();
        report.headline = spam.headline.clone();
        material.sources.push(spam);
        material.reports.push(report);
        material.mentions.push(vec![]);
        material.pack(200);
        assert_eq!(material.sources.len(), 1);
        assert_eq!(material.reports.len(), 1);
        assert_eq!(material.held.len(), 2);
        assert_eq!(material.held[0].1, "known_copy");
        assert_eq!(material.held[1].1, "livestream_spam");
    }

    #[test]
    fn large_reports_reach_exact_token_admission_without_losing_identity_holds() {
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
        let mut material = Material {
            database_record: None,
            subject: EntityMeta {
                name: "Cedar".into(),
                entity_type: "team".into(),
                entity_id: 1,
                sport: "NFL".into(),
            },
            sources,
            reports,
            mentions: vec![vec![]; 5],
            held: vec![],
        };
        material.pack(200);
        assert_eq!(material.sources.len(), 4);
        assert_eq!(material.reports.len(), 4);
        assert_eq!(material.mentions.len(), 4);
        assert_eq!(material.held.len(), 1);
        assert_eq!(material.held[0].1, "unresolved_target_identity");
        assert_eq!(
            material.sources[3].context,
            "Oversized report. ".repeat(2000)
        );
    }
}

#[derive(Serialize)]
struct IndexedReport {
    report_index: usize,
    #[serde(flatten)]
    report: Report,
}
#[derive(Serialize)]
pub(super) struct Fresh {
    reports: Vec<IndexedReport>,
}

pub(super) fn context(reports: &[Report]) -> crate::tools::prompt::Context<Fresh> {
    crate::tools::prompt::Context {
        guide: GUIDE,
        data: Fresh {
            reports: select(reports)
                .into_iter()
                .enumerate()
                .map(|(report_index, mut report)| {
                    report.published_at = None;
                    report.classifier_world =
                        crate::tools::fresh::evidence_world(report.classifier_world.as_ref());
                    IndexedReport {
                        report_index,
                        report,
                    }
                })
                .collect(),
        },
    }
}
