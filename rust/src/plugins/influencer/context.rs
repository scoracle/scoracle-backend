//! One bounded fresh package. Historical reporting and memory studies are deferred.
use super::prompt::{Day, Parts, SOURCE_BUDGET_BYTES};
use crate::tools::{meta::EntityMeta, source::SourceContext};
use anyhow::Result;
use serde_json::json;
use sqlx::{Postgres, Transaction};

pub async fn load(
    tx: &mut Transaction<'_, Postgres>,
    subject: EntityMeta,
    day: Day,
) -> Result<Parts> {
    let sources = crate::plugins::classifier::delivery::load_for_character_on(
        tx,
        super::manifest::MANIFEST.id.as_str(),
        &subject.entity_type,
        subject.entity_id,
        &subject.sport,
    )
    .await?;
    pack(subject, day, sources)
}

fn pack(subject: EntityMeta, day: Day, sources: Vec<SourceContext>) -> Result<Parts> {
    let mut parts = Parts {
        subject,
        day,
        sources: vec![],
        excluded: vec![],
    };
    let mut seen = std::collections::HashSet::new();
    let mut bytes = 0;
    for source in sources {
        let key = (
            &source.headline,
            &source.context,
            &source.classifier_world,
            source.published_at_epoch,
        );
        let key = serde_json::to_string(&key)?;
        let size = serde_json::to_vec(&source)?.len();
        let reason = if source.published_at_epoch.is_none() {
            Some("unknown_publication_time")
        } else if source
            .published_at_epoch
            .is_some_and(|time| time > parts.day.cutoff)
        {
            Some("future_publication_time")
        } else if source
            .classifier_world
            .as_ref()
            .is_some_and(|world| world["target_identity_resolved"] == false)
        {
            Some("unresolved_target_identity")
        } else if seen.contains(&key) {
            Some("known_copy")
        } else {
            super::prompt::source_disposition(&source.context)
                .or((bytes + size > SOURCE_BUDGET_BYTES)
                    .then_some("daily_package_exceeds_context_budget"))
        };
        if let Some(reason) = reason {
            parts.excluded.push(json!({"article_id":source.article_id,
                "classification_id":source.classification_id,"in_day":true,"reason":reason}));
        } else {
            seen.insert(key);
            bytes += size;
            parts.sources.push(source);
        }
    }
    Ok(parts)
}

pub async fn validate(tx: &mut Transaction<'_, Postgres>, parts: &Parts) -> Result<()> {
    crate::plugins::classifier::delivery::validate_for_publication(
        tx,
        super::manifest::MANIFEST.id.as_str(),
        &parts.subject.entity_type,
        parts.subject.entity_id,
        &parts.subject.sport,
        &parts.sources,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overflow_holds_one_daily_package_without_clipping_a_denial() {
        let source = |id, text: String| SourceContext {
            classification_id: id,
            article_id: id,
            headline: "Club reporting".into(),
            context: text,
            source: "Wire".into(),
            published_at_epoch: Some(100),
            classifier_world: None,
        };
        let parts = pack(
            EntityMeta {
                name: "Cedar".into(),
                entity_type: "team".into(),
                entity_id: 1,
                sport: "NFL".into(),
            },
            Day {
                start: 0,
                end: 86400,
                cutoff: 86400,
            },
            vec![
                source(1, "Hopeful report. ".repeat(900)),
                source(2, "The club denied that report. ".repeat(600)),
            ],
        )
        .unwrap();
        assert_eq!(parts.sources.len(), 1);
        assert_eq!(parts.excluded[0]["article_id"], 2);
        assert_eq!(
            parts.hold_reason(),
            Some("daily_package_exceeds_context_budget")
        );
        assert!(!parts.assemble().contains("RELEVANT HISTORY"));
    }
}
