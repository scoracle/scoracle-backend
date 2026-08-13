//! story_parts — progressing the entity's PART in a storyline (mig 219, the
//! narrative_threads collapse).
//!
//! A part is a storyline's DURABLE IDENTITY at the entity grain: the `storyline_entities`
//! row every telling (news_summaries row) progresses. This replaces the F5-successor that
//! threads.rs was — on the packet rail a telling's storyline is a FACT, not a match: every
//! corpus article reaches the Journalist through a packet (storyline_id NOT NULL), every
//! article belongs to exactly one storyline, and every persisted narrative is grounded on
//! cited article ids. The caller derives each telling's storyline with [`mode_storyline`]
//! over its citations; this module progresses the parts under it.
//!
//! What survives from the thread engine is the progression DISCIPLINE, not its machinery:
//!
//!   1. Load the generation's parts (`FOR UPDATE` — the row set is per-entity and small).
//!      `ORDER BY storyline_id` is load-bearing, not cosmetic: two transactions taking the
//!      same row set in different orders is a textbook deadlock, and the concurrent drain is
//!      what makes two transactions on one entity reachable at all.
//!   2. Trajectory anchors on the part's last_impact AS OF BEFORE this generation (siblings
//!      in one generation all compare against the prior state, not each other).
//!   3. A telling whose citations resolve to no storyline — or whose part row is missing —
//!      persists un-progressed. The Journalist updates parts; it never invents story
//!      identity (creation belongs to the Desk, §1b).
//!
//! Sealing is NOT done here: storylines go dormant in the worker (`mark_dormant`, 14d) and
//! resolve on ground truth in the nightly sweep (`seal_storylines`, mig 219, in
//! cron-narrative-links.sh).

use crate::trajectory::classify_delta;
use anyhow::{Context, Result};
use sqlx::{Postgres, Row, Transaction};
use std::collections::BTreeSet;

/// One telling of the generation being progressed: the storyline its citations resolved to
/// (None = nothing it cited belongs to a storyline), its deterministic impact, and the
/// source names it carries.
pub struct PartItem<'a> {
    pub storyline_id: Option<i64>,
    pub impact: i32,
    pub source_names: &'a [String],
}

/// What the progression decided for one telling — everything the persist path needs to write
/// the news_summaries row (storyline_id, trajectory) and its audit trail
/// (trajectory_components).
pub struct PartOutcome {
    pub storyline_id: Option<i64>,
    pub trajectory: &'static str,
    pub delta_reason: &'static str,
    pub impact_delta: Option<i32>,
    pub previous_impact: Option<i32>,
    /// True when no part was progressed for this telling (unresolved citations or a missing
    /// part row) — the chapter persists, the progression is skipped.
    pub unresolved: bool,
}

/// In-memory state of one part while a generation progresses. Loaded rows carry their DB
/// state; a part touched by several tellings in one generation accumulates all of them
/// before the single UPDATE lands.
struct PartState {
    storyline_id: i64,
    /// last_impact BEFORE this generation — the classify_delta anchor for every sibling.
    prior_impact: Option<i32>,
    last_impact: Option<i32>,
    peak_impact: Option<i32>,
    entry_count: i32,
    last_trajectory: &'static str,
    source_names: BTreeSet<String>,
    touched: bool,
}

/// mode_storyline resolves one telling's storyline from the storylines of its cited
/// articles: most cites wins, ties break toward the LOWEST storyline id — deterministic, so
/// a replay of the same corpus writes the same chapters. Empty input = unresolved.
pub fn mode_storyline(storyline_ids: &[i64]) -> Option<i64> {
    let mut counts: std::collections::BTreeMap<i64, usize> = std::collections::BTreeMap::new();
    for id in storyline_ids {
        *counts.entry(*id).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(id, _)| id)
}

/// progress_generation runs the part progression for one entity's generation of tellings,
/// inside the caller's transaction (the chapters and the parts they progress commit
/// atomically). Returns one outcome per item, in item order.
pub async fn progress_generation(
    tx: &mut Transaction<'_, Postgres>,
    sport: &str,
    entity_type: &str,
    entity_id: i32,
    items: &[PartItem<'_>],
) -> Result<Vec<PartOutcome>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }

    let mut ids: Vec<i64> = items.iter().filter_map(|i| i.storyline_id).collect();
    ids.sort_unstable();
    ids.dedup();

    let rows = if ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query(
            r#"
            SELECT storyline_id, last_impact::int AS last_impact,
                   peak_impact::int AS peak_impact, entry_count, source_names
            FROM storyline_entities
            WHERE sport = $1 AND entity_type = $2 AND entity_id = $3
              AND storyline_id = ANY($4)
            ORDER BY storyline_id
            FOR UPDATE
            "#,
        )
        .bind(sport)
        .bind(entity_type)
        .bind(entity_id)
        .bind(&ids)
        .fetch_all(&mut **tx)
        .await
        .context("load storyline parts")?
    };

    let mut states: Vec<PartState> = rows
        .into_iter()
        .map(|r| {
            let names: Vec<String> = r.get("source_names");
            PartState {
                storyline_id: r.get("storyline_id"),
                prior_impact: r.get::<Option<i32>, _>("last_impact"),
                last_impact: r.get::<Option<i32>, _>("last_impact"),
                peak_impact: r.get::<Option<i32>, _>("peak_impact"),
                entry_count: r.get("entry_count"),
                last_trajectory: "",
                source_names: names.into_iter().collect(),
                touched: false,
            }
        })
        .collect();

    let mut outcomes = Vec::with_capacity(items.len());
    for item in items {
        let matched = item
            .storyline_id
            .and_then(|id| states.iter_mut().find(|s| s.storyline_id == id));
        match matched {
            Some(s) => {
                // Anchor on the pre-generation impact so same-generation siblings all
                // classify against the prior state, never each other.
                let prior = s.prior_impact;
                let (trajectory, delta_reason, delta) = classify_delta(prior, Some(item.impact));
                s.entry_count += 1;
                s.peak_impact = Some(s.peak_impact.unwrap_or(item.impact).max(item.impact));
                s.last_impact = Some(item.impact);
                s.last_trajectory = trajectory;
                s.source_names.extend(item.source_names.iter().cloned());
                s.touched = true;
                outcomes.push(PartOutcome {
                    storyline_id: Some(s.storyline_id),
                    trajectory,
                    delta_reason,
                    impact_delta: delta,
                    previous_impact: prior,
                    unresolved: false,
                });
            }
            None => {
                let (trajectory, delta_reason, delta) = classify_delta(None, Some(item.impact));
                outcomes.push(PartOutcome {
                    storyline_id: item.storyline_id,
                    trajectory,
                    delta_reason,
                    impact_delta: delta,
                    previous_impact: None,
                    unresolved: true,
                });
            }
        }
    }

    // One UPDATE per part that took a telling.
    const UPDATE_PART: &str = r#"
        UPDATE storyline_entities SET
            last_progressed_at = NOW(),
            entry_count = $2, peak_impact = $3, last_impact = $4, last_trajectory = $5,
            distinct_sources = $6, source_names = $7
        WHERE storyline_id = $1 AND sport = $8 AND entity_type = $9 AND entity_id = $10"#;
    for s in states.iter().filter(|s| s.touched) {
        let names_vec: Vec<String> = s.source_names.iter().cloned().collect();
        sqlx::query(UPDATE_PART)
            .bind(s.storyline_id)
            .bind(s.entry_count)
            .bind(s.peak_impact.map(|v| v as i16))
            .bind(s.last_impact.map(|v| v as i16))
            .bind(s.last_trajectory)
            .bind(names_vec.len() as i32)
            .bind(&names_vec)
            .bind(sport)
            .bind(entity_type)
            .bind(entity_id)
            .execute(&mut **tx)
            .await
            .context("progress storyline part")?;
    }

    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_picks_the_most_cited_storyline() {
        assert_eq!(mode_storyline(&[7, 7, 3]), Some(7));
        assert_eq!(mode_storyline(&[3]), Some(3));
        assert_eq!(mode_storyline(&[]), None);
    }

    #[test]
    fn mode_ties_break_to_the_lowest_storyline_id() {
        // Deterministic under replay: one article from each of two storylines is not a
        // coin flip — the established (lowest-id) storyline wins, mirroring the Desk's
        // own tie-break in storyline::pick.
        assert_eq!(mode_storyline(&[9, 3]), Some(3));
        assert_eq!(mode_storyline(&[5, 5, 2, 2]), Some(2));
    }
}
