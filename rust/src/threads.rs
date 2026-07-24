//! threads — progressing-narrative thread attach (Characters Phase C, mig 181).
//!
//! A thread is a storyline's DURABLE IDENTITY: an entity-keyed `narrative_threads` row that
//! every telling (news_summaries row) attaches to by embedding similarity, not title string.
//! This replaces the F5 seam — `persist_narratives`' DISTINCT ON (narrative_title) prior-impact
//! match — where any title drift reset a storyline's trajectory to `developing_story` (proven
//! live by the 07-22 n10 wave resetting ALL trajectories). With threads, `classify_delta` runs
//! vs the THREAD's last_impact, so heating_up/cooling_off survive the model re-titling a story.
//!
//! Mechanics per generation (one `attach_generation` call, inside the persist transaction):
//!   1. Load the entity's OPEN threads (`FOR UPDATE` — the row set is per-entity and small).
//!   2. Per storyline: cosine(title+body embedding, thread centroid) — best match >= 0.80
//!      attaches; otherwise a new thread opens. Storylines in the same generation can attach
//!      to a thread opened earlier in that generation (the in-memory state is the truth).
//!   3. Trajectory anchors on the thread's last_impact AS OF BEFORE this generation (siblings
//!      in one generation all compare against the prior state, not each other).
//!   4. Centroids update by EWMA (alpha 0.2, re-normalized) — recency-weighted so the centroid
//!      FOLLOWS an evolving story instead of freezing on its opening chapter, slow enough not
//!      to be hijacked by one adjacent storyline. `canonical_title` becomes the latest telling's
//!      title: the thread is always named by its freshest chapter.
//!
//! Sealing is NOT done here: the nightly sweep (`seal_narrative_threads`, mig 181, in
//! cron-narrative-links.sh) fades quiet threads (>= 21d) and resolves ground-truth-confirmed
//! ones. The one exception is `seal_stale_open_threads`, the BACKFILL's chronological stand-in
//! for that sweep: replaying 45 days of history must fade a thread that went quiet mid-window
//! before a lookalike story revives, exactly as the nightly sweep would have.

use crate::embed::cosine_similarity;
use crate::harness::Vector;
use crate::trajectory::classify_delta;
use anyhow::{Context, Result};
use sqlx::{Postgres, Row, Transaction};
use std::collections::BTreeSet;

/// Cosine floor for attaching a telling to an existing thread (plan §C: 0.80). Below it, the
/// storyline opens a new thread. BGE-small vectors are unit-normalized, so cosine == dot.
pub const THREAD_MATCH_THRESHOLD: f32 = 0.80;

/// EWMA weight of the NEW telling in the centroid update. 0.2 ≈ a 3-entry half-life: fast
/// enough to follow a story that evolves (holdout → trade demand → trade), slow enough that
/// one adjacent storyline can't drag the centroid off its thread.
pub const CENTROID_ALPHA: f32 = 0.2;

/// One storyline of the generation being attached: the model's telling plus its embedding
/// (title+body, computed by the caller so live and backfill can batch differently).
pub struct AttachItem<'a> {
    pub title: &'a str,
    pub impact: i32,
    pub source_names: &'a [String],
    pub vector: Vector,
}

/// What the attach decided for one storyline — everything the persist path needs to write the
/// news_summaries row (thread_id, trajectory) and its audit trail (trajectory_components).
pub struct AttachOutcome {
    pub thread_id: i64,
    pub trajectory: &'static str,
    pub delta_reason: &'static str,
    pub impact_delta: Option<i32>,
    pub previous_impact: Option<i32>,
    /// Cosine to the matched centroid; `None` when the storyline opened a new thread.
    pub cosine: Option<f32>,
    pub opened: bool,
}

/// In-memory state of one open thread while a generation attaches. Loaded rows carry their DB
/// state; rows opened this generation are inserted immediately (so later siblings can match
/// them) and updated again at the end if touched again.
struct ThreadState {
    id: i64,
    canonical_title: String,
    centroid: Vector,
    /// last_impact BEFORE this generation — the classify_delta anchor for every sibling.
    prior_impact: Option<i32>,
    last_impact: Option<i32>,
    peak_impact: Option<i32>,
    entry_count: i32,
    last_trajectory: &'static str,
    source_names: BTreeSet<String>,
    touched: bool,
}

/// ewma_centroid folds a new unit vector into a centroid and re-normalizes, so the result
/// stays a unit vector and downstream cosine stays a dot product. A zero blend (degenerate
/// opposite vectors) falls back to the new vector.
pub fn ewma_centroid(old: &[f32], v: &[f32], alpha: f32) -> Vector {
    let mut m: Vector = old
        .iter()
        .zip(v)
        .map(|(o, n)| (1.0 - alpha) * o + alpha * n)
        .collect();
    let norm = m.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm == 0.0 {
        return v.to_vec();
    }
    for x in &mut m {
        *x /= norm;
    }
    m
}

/// best_match returns the (index, cosine) of the highest-cosine centroid at or above
/// `threshold`, or `None` — the storyline should open a new thread.
fn best_match(states: &[ThreadState], v: &[f32], threshold: f32) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32)> = None;
    for (i, s) in states.iter().enumerate() {
        let c = cosine_similarity(&s.centroid, v);
        if c >= threshold && best.is_none_or(|(_, bc)| c > bc) {
            best = Some((i, c));
        }
    }
    best
}

/// attach_generation runs the thread attach for one entity's generation of storylines, inside
/// the caller's transaction. `at_epoch` is `None` live (thread timestamps = the transaction's
/// NOW(), matching the news_summaries generated_at) and `Some(epoch)` for the backfill's
/// chronological replay. Returns one outcome per item, in item order.
pub async fn attach_generation(
    tx: &mut Transaction<'_, Postgres>,
    sport: &str,
    entity_type: &str,
    entity_id: i32,
    items: &[AttachItem<'_>],
    at_epoch: Option<i64>,
) -> Result<Vec<AttachOutcome>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT id, canonical_title, centroid, last_impact::int AS last_impact,
               peak_impact::int AS peak_impact, entry_count, source_names
        FROM narrative_threads
        WHERE sport = $1 AND entity_type = $2 AND entity_id = $3 AND status = 'open'
        FOR UPDATE
        "#,
    )
    .bind(sport)
    .bind(entity_type)
    .bind(entity_id)
    .fetch_all(&mut **tx)
    .await
    .context("load open narrative threads")?;

    let mut states: Vec<ThreadState> = rows
        .into_iter()
        .map(|r| {
            let names: Vec<String> = r.get("source_names");
            ThreadState {
                id: r.get("id"),
                canonical_title: r.get("canonical_title"),
                centroid: r.get::<Option<Vector>, _>("centroid").unwrap_or_default(),
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

    const INSERT_THREAD: &str = r#"
        INSERT INTO narrative_threads (
            sport, entity_type, entity_id, canonical_title, status,
            opened_at, last_progressed_at,
            entry_count, peak_impact, last_impact, last_trajectory,
            distinct_sources, source_names, centroid
        ) VALUES (
            $1, $2, $3, $4, 'open',
            COALESCE(to_timestamp($5::double precision), NOW()),
            COALESCE(to_timestamp($5::double precision), NOW()),
            1, $6, $6, $7, $8, $9, $10
        )
        RETURNING id"#;

    let mut outcomes = Vec::with_capacity(items.len());
    for item in items {
        match best_match(&states, &item.vector, THREAD_MATCH_THRESHOLD) {
            Some((i, cosine)) => {
                // Anchor on the pre-generation impact so same-generation siblings all
                // classify against the prior state, never each other.
                let prior = states[i].prior_impact;
                let (trajectory, delta_reason, delta) = classify_delta(prior, Some(item.impact));
                let s = &mut states[i];
                s.canonical_title = item.title.to_string();
                s.centroid = ewma_centroid(&s.centroid, &item.vector, CENTROID_ALPHA);
                s.entry_count += 1;
                s.peak_impact = Some(s.peak_impact.unwrap_or(item.impact).max(item.impact));
                s.last_impact = Some(item.impact);
                s.last_trajectory = trajectory;
                s.source_names.extend(item.source_names.iter().cloned());
                s.touched = true;
                outcomes.push(AttachOutcome {
                    thread_id: s.id,
                    trajectory,
                    delta_reason,
                    impact_delta: delta,
                    previous_impact: prior,
                    cosine: Some(cosine),
                    opened: false,
                });
            }
            None => {
                let (trajectory, delta_reason, delta) = classify_delta(None, Some(item.impact));
                let source_names: BTreeSet<String> = item.source_names.iter().cloned().collect();
                let names_vec: Vec<String> = source_names.iter().cloned().collect();
                let id: i64 = sqlx::query(INSERT_THREAD)
                    .bind(sport)
                    .bind(entity_type)
                    .bind(entity_id)
                    .bind(item.title)
                    .bind(at_epoch)
                    .bind(item.impact as i16)
                    .bind(trajectory)
                    .bind(names_vec.len() as i32)
                    .bind(&names_vec)
                    .bind(&item.vector)
                    .fetch_one(&mut **tx)
                    .await
                    .context("open narrative thread")?
                    .get("id");
                states.push(ThreadState {
                    id,
                    canonical_title: item.title.to_string(),
                    centroid: item.vector.clone(),
                    // Siblings later in THIS generation still anchor on "no prior".
                    prior_impact: None,
                    last_impact: Some(item.impact),
                    peak_impact: Some(item.impact),
                    entry_count: 1,
                    last_trajectory: trajectory,
                    source_names,
                    touched: false,
                });
                outcomes.push(AttachOutcome {
                    thread_id: id,
                    trajectory,
                    delta_reason,
                    impact_delta: delta,
                    previous_impact: None,
                    cosine: None,
                    opened: true,
                });
            }
        }
    }

    // One UPDATE per thread that took an attach (a freshly-opened thread only needs this if a
    // later sibling attached to it after the INSERT).
    const UPDATE_THREAD: &str = r#"
        UPDATE narrative_threads SET
            canonical_title = $2,
            last_progressed_at = COALESCE(to_timestamp($3::double precision), NOW()),
            entry_count = $4, peak_impact = $5, last_impact = $6, last_trajectory = $7,
            distinct_sources = $8, source_names = $9, centroid = $10
        WHERE id = $1"#;
    for s in states.iter().filter(|s| s.touched) {
        let names_vec: Vec<String> = s.source_names.iter().cloned().collect();
        sqlx::query(UPDATE_THREAD)
            .bind(s.id)
            .bind(&s.canonical_title)
            .bind(at_epoch)
            .bind(s.entry_count)
            .bind(s.peak_impact.map(|v| v as i16))
            .bind(s.last_impact.map(|v| v as i16))
            .bind(s.last_trajectory)
            .bind(names_vec.len() as i32)
            .bind(&names_vec)
            .bind(&s.centroid)
            .execute(&mut **tx)
            .await
            .context("progress narrative thread")?;
    }

    Ok(outcomes)
}

/// seal_stale_open_threads is the BACKFILL's chronological seal: before replaying a generation
/// at `before_epoch`, fade the entity's open threads that had already been quiet >= 21 days at
/// that moment — otherwise a revived lookalike story would attach to a thread the nightly
/// sweep would have sealed. Live sealing stays in SQL (`seal_narrative_threads`).
pub async fn seal_stale_open_threads(
    tx: &mut Transaction<'_, Postgres>,
    sport: &str,
    entity_type: &str,
    entity_id: i32,
    before_epoch: i64,
) -> Result<u64> {
    let res = sqlx::query(
        r#"
        UPDATE narrative_threads
           SET status = 'sealed', outcome = 'faded',
               sealed_at = last_progressed_at + interval '21 days'
         WHERE sport = $1 AND entity_type = $2 AND entity_id = $3 AND status = 'open'
           AND last_progressed_at < to_timestamp($4::double precision) - interval '21 days'
        "#,
    )
    .bind(sport)
    .bind(entity_type)
    .bind(entity_id)
    .bind(before_epoch)
    .execute(&mut **tx)
    .await
    .context("seal stale open threads (backfill)")?;
    Ok(res.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(v: Vec<f32>) -> Vector {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.into_iter().map(|x| x / n).collect()
    }

    fn state(centroid: Vector) -> ThreadState {
        ThreadState {
            id: 1,
            canonical_title: "t".into(),
            centroid,
            prior_impact: None,
            last_impact: None,
            peak_impact: None,
            entry_count: 0,
            last_trajectory: "",
            source_names: BTreeSet::new(),
            touched: false,
        }
    }

    #[test]
    fn ewma_stays_unit_and_moves_toward_new() {
        let old = unit(vec![1.0, 0.0]);
        let new = unit(vec![0.0, 1.0]);
        let c = ewma_centroid(&old, &new, 0.2);
        let norm = c.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-5,
            "centroid must stay unit, got {norm}"
        );
        // Moved toward the new vector but still dominated by the old.
        assert!(c[0] > c[1] && c[1] > 0.0);
        // Degenerate exact-opposite blend at alpha 0.5 collapses to zero → falls back to new.
        let opp = ewma_centroid(&[1.0, 0.0], &[-1.0, 0.0], 0.5);
        assert_eq!(opp, vec![-1.0, 0.0]);
    }

    #[test]
    fn best_match_picks_highest_above_threshold() {
        let states = vec![
            state(unit(vec![1.0, 0.0, 0.0])),
            state(unit(vec![0.9, 0.1, 0.0])),
            state(unit(vec![0.0, 0.0, 1.0])),
        ];
        let v = unit(vec![0.9, 0.1, 0.0]);
        let (i, c) = best_match(&states, &v, THREAD_MATCH_THRESHOLD).expect("match");
        assert_eq!(i, 1, "nearest centroid wins");
        assert!(c > 0.999);
        // Orthogonal query clears no threshold → opens a new thread.
        assert!(best_match(&states, &unit(vec![0.0, 1.0, 0.0]), THREAD_MATCH_THRESHOLD).is_none());
    }
}
