//! Load and bound fresh movement headlines and canonical candidate names.
use super::prompt::{assemble, assemble_with_memories, Parts, CONTEXT_BUDGET_BYTES};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classifier_world: Option<serde_json::Value>,
    pub publisher: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    pub headline: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub publisher_excerpt: String,
    pub co_mentions: Vec<Mention>,
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
    pub(super) memories: Vec<crate::plugins::classifier::memories::Headline>,
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
            classifier_world: source.classifier_world.clone(),
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
    let memories = crate::plugins::classifier::memories::load_on(
        &mut *pool.acquire().await?,
        &subject,
        &sources,
        "insider",
    )
    .await?;
    let mut material = Material {
        subject,
        memories,
        sources,
        reports,
        mentions,
        held: Vec::new(),
    };
    material.pack(crate::plugins::influencer::now());
    while !material.memories.is_empty()
        && assemble_with_memories(&material.subject, &material.reports, &material.memories).len()
            > CONTEXT_BUDGET_BYTES
    {
        material.memories.pop();
    }
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
            } else if crate::tools::source::contains_instruction_override(&source.context) {
                Some("source_instruction_override")
            } else if assemble(&self.subject, std::slice::from_ref(&report)).len()
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
        // A daily reading cannot omit a complete report that could qualify or deny another.
        if assemble(&self.subject, &self.reports).len() > CONTEXT_BUDGET_BYTES
            || self
                .held
                .iter()
                .any(|(_, reason)| *reason == "complete_report_exceeds_context_budget")
        {
            self.held.extend(
                self.sources
                    .drain(..)
                    .map(|s| (s, "daily_package_exceeds_context_budget")),
            );
            self.reports.clear();
            self.mentions.clear();
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
        memories: material.memories,
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
            memories: vec![],
            mentions: vec![vec![]; 2],
            held: vec![],
        };
        material.pack(200);
        assert_eq!(material.sources.len(), 1);
        assert_eq!(material.reports.len(), 1);
        assert_eq!(material.held.len(), 1);
        assert_eq!(material.held[0].1, "known_copy");
    }

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
        let mut material = Material {
            memories: vec![],
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
        assert!(material.sources.is_empty());
        assert_eq!(material.held.len(), 5);
        assert!(material
            .held
            .iter()
            .any(|(_, r)| *r == "daily_package_exceeds_context_budget"));
    }
}
