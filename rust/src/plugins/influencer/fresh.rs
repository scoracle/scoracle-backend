//! Load, bound and present the daily emotional headline package and dated memories.
use super::prompt::{Assignment, Day, Parts, SOURCE_BUDGET_BYTES, VIBE_PROMPT_VERSION};
use crate::tools::{meta::EntityMeta, source::SourceContext};
use anyhow::Result;
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Transaction};

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
    let memories =
        crate::plugins::classifier::memories::load_on(tx, &subject, &sources, "influencer").await?;
    let mut parts = pack(subject, day, sources)?;
    parts.memories = memories;
    while !parts.memories.is_empty() && parts.assemble().len() > SOURCE_BUDGET_BYTES {
        parts.memories.pop();
    }
    Ok(parts)
}

fn pack(subject: EntityMeta, day: Day, sources: Vec<SourceContext>) -> Result<Parts> {
    let mut parts = Parts {
        subject,
        day,
        sources: vec![],
        memories: vec![],
        excluded: vec![],
    };
    let mut seen = std::collections::HashSet::new();
    for source in sources {
        let key = crate::tools::fresh::evidence_key(&source.source, &source.context);
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
            source_disposition(&source.context)
        };
        if let Some(reason) = reason {
            parts.excluded.push(json!({"article_id":source.article_id,
                "classification_id":source.classification_id,"in_day":true,"reason":reason}));
        } else {
            seen.insert(key);
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
    fn already_queued_spam_is_dispositioned() {
        assert_eq!(
            source_disposition("[+[LIVE@STREAMs]+] Cedar FC vs Maple United"),
            Some("livestream_spam")
        );
        assert_eq!(
            source_disposition("How to watch Cedar FC: live stream and TV channel"),
            None
        );
    }

    #[test]
    fn duplicate_headlines_do_not_consume_the_daily_budget() {
        let source = |id| SourceContext {
            classification_id: id,
            article_id: id,
            headline: "Cedar celebrates".into(),
            context: "Cedar celebrates".into(),
            source: "Wire".into(),
            published_at_epoch: Some(100),
            classifier_world: Some(
                json!({"evidence_scope":"headline","url":format!("https://example.invalid/{id}"),"spectrum":{"emotion":id}}),
            ),
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
            vec![source(1), source(2)],
        )
        .unwrap();
        assert_eq!(parts.sources.len(), 1);
        assert_eq!(parts.excluded[0]["reason"], "known_copy");
        assert!(parts.hold_reason().is_none());
    }

    #[test]
    fn large_daily_packages_keep_the_denial_for_exact_token_admission() {
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
        assert_eq!(parts.sources.len(), 2);
        assert!(parts.excluded.is_empty());
        assert_eq!(parts.hold_reason(), None);
        assert!(parts.assemble().len() > SOURCE_BUDGET_BYTES);
        assert!(parts
            .assemble()
            .contains(&"The club denied that report. ".repeat(600)));
        assert!(!parts.assemble().contains("RELEVANT HISTORY"));
    }
}

/// Reporting bounds and complete evidence for one daily character call.
#[derive(serde::Serialize)]
pub(super) struct Evidence<'a> {
    reporting_start: String,
    reporting_end: String,
    reports: Vec<crate::tools::fresh::Report<'a>>,
}

pub(super) fn evidence(parts: &Parts) -> Evidence<'_> {
    Evidence {
        reporting_start: crate::util::utc_timestamp(parts.day.start),
        reporting_end: crate::util::utc_timestamp(parts.day.end),
        reports: crate::tools::fresh::reports(&parts.sources),
    }
}

pub fn source_disposition(text: &str) -> Option<&'static str> {
    if text.trim().is_empty() {
        Some("empty_source")
    } else if crate::tools::source::is_headline_spam(text) {
        Some("livestream_spam")
    } else if crate::tools::source::contains_instruction_override(text) {
        Some("source_instruction_override")
    } else {
        None
    }
}

/// Freeze one Eastern calendar day; late headlines keep their original publication dates.
pub async fn prepare_assignment(
    pool: &PgPool,
    subject: EntityMeta,
    cutoff: i64,
) -> Result<(Assignment, Value)> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let (start, end, cutoff): (i64, i64, i64) = sqlx::query_as(
        "WITH clock AS (
            SELECT COALESCE((e.cutoff_at AT TIME ZONE 'America/Detroit')::date-1,
                (to_timestamp($1::double precision) AT TIME ZONE 'America/Detroit')::date) AS day,
                COALESCE(e.frozen_at,to_timestamp($1::double precision)) AS captured
            FROM coverage_schedule s LEFT JOIN coverage_editions e
                ON s.enabled AND s.phase='characters' AND e.edition_date=s.edition_date
        ) SELECT extract(epoch FROM (day::timestamp AT TIME ZONE 'America/Detroit'))::bigint,
            extract(epoch FROM ((day+1)::timestamp AT TIME ZONE 'America/Detroit'))::bigint,
            floor(extract(epoch FROM captured))::bigint FROM clock",
    )
    .bind(cutoff)
    .fetch_one(&mut *tx)
    .await?;
    let day = Day { start, end, cutoff };
    let parts = load(&mut tx, subject, day).await?;
    tx.commit().await?;
    let receipt = json!({"contract":VIBE_PROMPT_VERSION,"day":parts.day,"excluded":parts.excluded,"memories":parts.memories});
    Ok((Assignment::from_parts(parts)?, receipt))
}
