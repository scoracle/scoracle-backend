//! Direct accepted-source collection and the existing DuckDB reporting study.
use super::prompt::{Parts, Period, HISTORY_BUDGET_BYTES, SOURCE_BUDGET_BYTES};
use crate::plugins::classifier::delivery::load_accepted;
use crate::tools::source::SourceContext;
use crate::tools::{memories, meta::EntityMeta};
use anyhow::{ensure, Result};
use serde_json::json;
use sqlx::{Postgres, Transaction};

pub const LOOKBACK_SECONDS: i64 = 30 * 86400;

pub async fn load(
    tx: &mut Transaction<'_, Postgres>,
    subject: EntityMeta,
    period: Period,
) -> Result<Parts> {
    let before = period.end.min(period.cutoff);
    let candidates = load_accepted(
        tx,
        &subject,
        period.start - LOOKBACK_SECONDS,
        before,
        period.cutoff,
    )
    .await?;
    let ids: Vec<i64> = candidates.iter().map(|s| s.article_id).collect();
    let stored_canonical: Vec<(i64, i64)> =
        sqlx::query_as("SELECT id,COALESCE(duplicate_of,id) FROM news_articles WHERE id=ANY($1)")
            .bind(&ids)
            .fetch_all(&mut **tx)
            .await?;
    let stored_canonical: std::collections::HashMap<i64, i64> =
        stored_canonical.into_iter().collect();
    // A duplicate flag alone must not erase a changed title or late qualification.
    let mut copies = std::collections::HashMap::new();
    let canonical: std::collections::HashMap<i64, i64> = candidates
        .iter()
        .map(|s| {
            let root = *copies
                .entry((
                    stored_canonical[&s.article_id],
                    s.headline.as_str(),
                    s.context.as_str(),
                    serde_json::to_string(&s.classifier_world).expect("world serializes"),
                ))
                .or_insert(s.article_id);
            (s.article_id, root)
        })
        .collect();
    let observations = candidates
        .iter()
        .filter_map(|s| {
            Some(memories::Observation {
                article_id: s.article_id,
                canonical_id: *canonical.get(&s.article_id)?,
                topic: format!("article/{}", *canonical.get(&s.article_id)?),
                publisher: s.source.clone(),
                reported_at: s.published_at_epoch?,
                headline: s.headline.clone(),
            })
        })
        .collect();
    let findings = if ids.is_empty() {
        vec![]
    } else {
        memories::compute(&memories::Request {
            version: memories::VERSION.into(),
            from: period.start - LOOKBACK_SECONDS,
            before,
            limit: 20,
            per_topic: 1,
            observations,
        })
        .await?
    };
    let selected: Vec<i64> = findings
        .iter()
        .flat_map(|f| &f.reports)
        .map(|r| r.article_id)
        .collect();
    pack(subject, period, candidates, &canonical, &selected)
}

fn pack(
    subject: EntityMeta,
    period: Period,
    candidates: Vec<SourceContext>,
    canonical: &std::collections::HashMap<i64, i64>,
    selected: &[i64],
) -> Result<Parts> {
    let mut parts = Parts {
        subject,
        period,
        sources: vec![],
        history: vec![],
        excluded: vec![],
    };
    let (mut fresh_bytes, mut history_bytes) = (0, 0);
    for source in candidates {
        let fresh = source
            .published_at_epoch
            .is_some_and(|t| t >= parts.period.start);
        let reason = super::prompt::source_disposition(&source.context)
            .or_else(|| {
                source
                    .published_at_epoch
                    .is_none()
                    .then_some("publication_date_unavailable")
            })
            .or_else(|| (!selected.contains(&source.article_id)).then_some("study_selection"));
        let size = serde_json::to_vec(&source)?.len();
        let budget_exceeded = if fresh {
            fresh_bytes + size > SOURCE_BUDGET_BYTES
        } else {
            history_bytes + size > HISTORY_BUDGET_BYTES
        };
        let reason = reason.or(budget_exceeded.then_some("packet_budget_exceeded"));
        if let Some(reason) = reason {
            // Fresh exclusions hold the whole period; never publish an omitted denial.
            let known_copy = canonical.get(&source.article_id).is_some_and(|root| {
                canonical
                    .iter()
                    .any(|(id, r)| r == root && selected.contains(id))
            });
            parts.excluded.push(
                json!({"article_id":source.article_id,"classification_id":source.classification_id,
                "in_period":fresh,"reason":if known_copy && reason=="study_selection" {"known_copy"} else {reason}}),
            );
        } else if fresh {
            fresh_bytes += size;
            parts.sources.push(source);
        } else {
            history_bytes += size;
            parts.history.push(source);
        }
    }
    let retained: Vec<i64> = parts
        .sources
        .iter()
        .chain(&parts.history)
        .map(|s| s.article_id)
        .collect();
    for copy in parts
        .excluded
        .iter_mut()
        .filter(|e| e["reason"] == "known_copy" && e["in_period"] == true)
    {
        let id = copy["article_id"].as_i64().unwrap();
        let root = canonical[&id];
        if !canonical
            .iter()
            .any(|(id, r)| *r == root && retained.contains(id))
        {
            copy["reason"] = json!("canonical_source_excluded");
        }
    }
    Ok(parts)
}

/// Recheck every supplied receipt under the claim-fenced publication transaction.
pub async fn validate(tx: &mut Transaction<'_, Postgres>, parts: &Parts) -> Result<()> {
    let sources: Vec<SourceContext> = parts
        .sources
        .iter()
        .chain(&parts.history)
        .cloned()
        .collect();
    crate::plugins::classifier::delivery::lock_sources(
        tx,
        super::manifest::MANIFEST.id.as_str(),
        &parts.subject.sport,
        &sources,
    )
    .await?;
    let current = load_accepted(
        tx,
        &parts.subject,
        parts.period.start - LOOKBACK_SECONDS,
        parts.period.end.min(parts.period.cutoff),
        parts.period.cutoff,
    )
    .await?;
    ensure!(
        sources.iter().all(|s| current.contains(s)),
        "Influencer source changed during generation"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn an_overflowing_denial_holds_the_whole_period_but_old_exclusions_do_not() {
        let source = |id, date, text: String| SourceContext {
            article_id: id,
            classification_id: id,
            headline: "Cedar reporting".into(),
            context: text,
            source: "Wire".into(),
            published_at_epoch: Some(date),
            classifier_world: None,
        };
        let subject = EntityMeta {
            name: "Cedar".into(),
            entity_type: "team".into(),
            entity_id: 1,
            sport: "NFL".into(),
        };
        let period = Period {
            season: 2026,
            week: 1,
            start: 100,
            end: 200,
            cutoff: 190,
        };
        let roots = HashMap::from([(1, 1), (2, 2)]);
        let parts = pack(
            subject.clone(),
            period.clone(),
            vec![
                source(1, 110, "Hopeful reporting. ".repeat(800)),
                source(2, 120, "The club denied that report. ".repeat(600)),
            ],
            &roots,
            &[1, 2],
        )
        .unwrap();
        assert_eq!(parts.sources.len(), 1);
        assert_eq!(parts.excluded[0]["article_id"], 2);
        assert_eq!(parts.hold_reason(), Some("packet_budget_exceeded"));
        let historical = pack(
            subject,
            period,
            vec![
                source(1, 90, "Old context. ".repeat(500)),
                source(2, 120, "A complete current denial.".into()),
            ],
            &roots,
            &[1, 2],
        )
        .unwrap();
        assert!(historical.hold_reason().is_none());
        assert_eq!(historical.sources[0].article_id, 2);
        assert_eq!(historical.excluded[0]["in_period"], false);
    }
}
