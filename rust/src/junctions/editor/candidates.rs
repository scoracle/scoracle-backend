//! The nomination sweep — mystery names become durable work (PLAN-one-rail 5.1/5.2).
//!
//! Runs in the Editor's handle after the resolver: every unresolved person-kind name and
//! every refused-ambiguous tie upserts an `entity_candidates` row (one candidate per
//! `sport:nrm(name)`, forever — repeat mentions bump counters, never duplicate) plus a
//! `candidate_mentions` evidence row whose quote is CODE-SLICED from the stored body (the
//! model never emits quotes). Clubs and national teams nominate too, but land as the
//! `rejected_out_of_scope` census (Appendix B D-3) and never enqueue.
//!
//! The enqueue rule (Scott 2026-08-01, 5.2 — direct relation beats the floor): a person
//! mention with a NON-EMPTY descriptor enqueues `investigate_entity` on FIRST sight; so
//! does any refused-ambiguous tie; descriptor-less bare names wait for the 2-mention floor.
//! Terminal candidates reopen on a new distinct-article mention 30 days after decision
//! (5.6) — the maintenance loop, person/tie classes only (the club census stays settled).

use super::derive::{RefusedName, Resolved};
use super::NameMention;
use crate::work::{enqueue, Item, Stage};
use anyhow::{Context, Result};
use sqlx::{PgPool, Row};
use tracing::{info, warn};

/// ±window (in chars) around the name's first occurrence for the evidence quote.
const QUOTE_WINDOW_CHARS: usize = 160;

/// One sweep outcome, for logging and tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SweepStats {
    pub candidates_touched: usize,
    pub new_mentions: usize,
    pub enqueued: usize,
    pub census_only: usize,
}

/// sweep_candidates processes one read's resolver leftovers. Call AFTER persist — the read
/// is the source of truth; the sweep is downstream bookkeeping. Errors bubble per name
/// class but the caller treats the whole sweep as best-effort.
pub async fn sweep_candidates(
    pool: &PgPool,
    sport: &str,
    article_id: i64,
    body: &str,
    resolved: &Resolved,
) -> Result<SweepStats> {
    let mut stats = SweepStats::default();

    for mention in &resolved.unresolved {
        match mention.kind_hint.to_lowercase().as_str() {
            "person" => {
                let enq = nominate_one(
                    pool, sport, article_id, body, mention, /* census */ false,
                    /* always_enqueue */ false,
                )
                .await?;
                stats.candidates_touched += 1;
                stats.new_mentions += enq.new_mention as usize;
                stats.enqueued += enq.enqueued as usize;
            }
            "club" | "national_team" => {
                let enq = nominate_one(pool, sport, article_id, body, mention, true, false).await?;
                stats.candidates_touched += 1;
                stats.new_mentions += enq.new_mention as usize;
                stats.census_only += 1;
            }
            // kind_hint `other` never nominates: the descriptor already told the resolver it
            // is not an entity we track ("city hosting the finale"), and B3 measured this
            // class as noise.
            _ => {}
        }
    }

    for refusal in &resolved.refused_ambiguous {
        let mention = NameMention {
            name: refusal.name.clone(),
            kind_hint: refusal.kind_hint.clone(),
            descriptor: refusal.descriptor.clone(),
        };
        let enq = nominate_one(pool, sport, article_id, body, &mention, false, true).await?;
        stats.candidates_touched += 1;
        stats.new_mentions += enq.new_mention as usize;
        stats.enqueued += enq.enqueued as usize;
        record_tie_target(pool, sport, refusal).await?;
    }

    if stats.candidates_touched > 0 {
        info!(
            article_id,
            sport,
            touched = stats.candidates_touched,
            new_mentions = stats.new_mentions,
            enqueued = stats.enqueued,
            census = stats.census_only,
            "editor nomination sweep"
        );
    }
    Ok(stats)
}

struct NominateOutcome {
    new_mention: bool,
    enqueued: bool,
}

/// nominate_one upserts the candidate + mention for one name and applies the enqueue rule.
/// `census` short-circuits clubs/NTs into `rejected_out_of_scope` (never enqueued);
/// `always_enqueue` is the refused-tie arm.
async fn nominate_one(
    pool: &PgPool,
    sport: &str,
    article_id: i64,
    body: &str,
    mention: &NameMention,
    census: bool,
    always_enqueue: bool,
) -> Result<NominateOutcome> {
    let mut tx = pool.begin().await.context("begin nomination sweep")?;

    // One candidate per sport:nrm(name), forever. nrm() runs IN SQL (mig 198: the database
    // owns the one normalizer). The 5.6 reopen rides the upsert: a terminal person/tie
    // candidate whose decision is >30d old goes back to pending on a fresh mention; the
    // club census (rejected_out_of_scope on a census-class row) never reopens here.
    let row = sqlx::query(
        r#"
        INSERT INTO public.entity_candidates
            (idempotency_key, norm_name, kind_hint, sport, state, first_seen_at, last_seen_at,
             decided_at)
        VALUES (
            lower($1) || ':' || public.nrm($2), public.nrm($2), $3, $1,
            CASE WHEN $4 THEN 'rejected_out_of_scope' ELSE 'pending' END,
            NOW(), NOW(),
            CASE WHEN $4 THEN NOW() ELSE NULL END
        )
        ON CONFLICT (idempotency_key) DO UPDATE SET
            last_seen_at = NOW(),
            kind_hint = COALESCE(public.entity_candidates.kind_hint, EXCLUDED.kind_hint),
            state = CASE
                WHEN NOT $4
                 AND public.entity_candidates.state NOT IN ('pending', 'accepted')
                 AND public.entity_candidates.decided_at IS NOT NULL
                 AND public.entity_candidates.decided_at < NOW() - interval '30 days'
                THEN 'pending'
                ELSE public.entity_candidates.state
            END
        RETURNING id, state, mention_count
        "#,
    )
    .bind(sport)
    .bind(&mention.name)
    .bind(&mention.kind_hint)
    .bind(census)
    .fetch_one(&mut *tx)
    .await
    .with_context(|| format!("upsert entity_candidate {}", mention.name))?;
    let candidate_id: i64 = row.get("id");
    let state: String = row.get("state");

    // The evidence row. New (candidate, article) pairs bump mention_count — the count is
    // DISTINCT ARTICLES by construction of the PK.
    let inserted = sqlx::query(
        r#"
        INSERT INTO public.candidate_mentions
            (candidate_id, article_id, quote, editor_descriptor, observed_at)
        VALUES ($1, $2, $3, NULLIF($4, ''), NOW())
        ON CONFLICT (candidate_id, article_id) DO NOTHING
        "#,
    )
    .bind(candidate_id)
    .bind(article_id)
    .bind(slice_quote(body, &mention.name))
    .bind(&mention.descriptor)
    .execute(&mut *tx)
    .await
    .context("insert candidate_mention")?
    .rows_affected()
        > 0;

    let mention_count: i32 = if inserted {
        sqlx::query_scalar(
            r#"
            UPDATE public.entity_candidates
            SET mention_count = mention_count + 1
            WHERE id = $1
            RETURNING mention_count
            "#,
        )
        .bind(candidate_id)
        .fetch_one(&mut *tx)
        .await
        .context("bump mention_count")?
    } else {
        row.get("mention_count")
    };

    tx.commit().await.context("commit nomination sweep")?;

    // The 5.2 enqueue rule, pending candidates only (accepted/terminal states wait for the
    // 5.6 reopen above). work::enqueue is idempotent — repeat sweeps cannot double-queue.
    let should_enqueue = !census
        && state == "pending"
        && (always_enqueue || !mention.descriptor.trim().is_empty() || mention_count >= 2);
    if should_enqueue {
        enqueue(
            pool,
            &Item {
                stage: Stage::InvestigateEntity,
                entity_type: "candidate".to_string(),
                entity_id: candidate_id,
                sport: sport.to_string(),
                input_version: None,
                attempts: 0,
            },
        )
        .await?;
    }

    Ok(NominateOutcome {
        new_mention: inserted,
        enqueued: should_enqueue,
    })
}

/// record_tie_target notes a refusal's near-match when the tie had a single leading
/// candidate pair-type — the `target_entity_*` hint mig 205 describes. Ties with 3+
/// candidates stay blank (no meaningful single target). Best-effort: a miss here loses a
/// hint, not evidence (the tie's candidates are in `editor_reads.resolved`).
async fn record_tie_target(pool: &PgPool, sport: &str, refusal: &RefusedName) -> Result<()> {
    if refusal.candidates.len() != 2 {
        return Ok(());
    }
    if let Err(e) = sqlx::query(
        r#"
        UPDATE public.entity_candidates
        SET target_entity_type = $3, target_entity_id = $4
        WHERE idempotency_key = lower($1) || ':' || public.nrm($2)
          AND target_entity_type IS NULL
        "#,
    )
    .bind(sport)
    .bind(&refusal.name)
    .bind(&refusal.candidates[0].0)
    .bind(refusal.candidates[0].1)
    .execute(pool)
    .await
    {
        warn!(name = %refusal.name, error = %e, "tie target hint update failed (continuing)");
    }
    Ok(())
}

/// slice_quote extracts ±QUOTE_WINDOW_CHARS around the name's first occurrence in the
/// stored body — char-boundary safe (the İ lesson from fetch.rs: never index the original
/// with offsets from a lowercased copy). Case-insensitive via per-char primary fold; a
/// name whose fold expands (rare) simply misses and yields None — a lost quote, never a
/// panic.
pub(crate) fn slice_quote(body: &str, name: &str) -> Option<String> {
    let needle: Vec<char> = name.chars().flat_map(|c| c.to_lowercase().next()).collect();
    if needle.is_empty() {
        return None;
    }
    let hay: Vec<(usize, char)> = body
        .char_indices()
        .map(|(i, c)| (i, c.to_lowercase().next().unwrap_or(c)))
        .collect();
    if hay.len() < needle.len() {
        return None;
    }
    let mut start_char_idx = None;
    'outer: for i in 0..=hay.len() - needle.len() {
        for (j, nc) in needle.iter().enumerate() {
            if hay[i + j].1 != *nc {
                continue 'outer;
            }
        }
        start_char_idx = Some(i);
        break;
    }
    let start_char_idx = start_char_idx?;
    let from_char = start_char_idx.saturating_sub(QUOTE_WINDOW_CHARS);
    let to_char = (start_char_idx + needle.len() + QUOTE_WINDOW_CHARS).min(hay.len());
    let from_byte = hay[from_char].0;
    let to_byte = if to_char == hay.len() {
        body.len()
    } else {
        hay[to_char].0
    };
    let quote = body[from_byte..to_byte].trim();
    if quote.is_empty() {
        None
    } else {
        Some(quote.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_slices_a_window_around_the_first_occurrence() {
        let body = format!(
            "{} Yan Diomande scored twice. {}",
            "x".repeat(500),
            "y".repeat(500)
        );
        let q = slice_quote(&body, "yan diomande").unwrap();
        assert!(q.contains("Yan Diomande scored twice."));
        // 160 chars each side + the name ≈ bounded window, never the whole body.
        assert!(q.chars().count() <= 2 * QUOTE_WINDOW_CHARS + 30);
    }

    #[test]
    fn quote_is_case_insensitive_and_boundary_safe() {
        let body = "İstanbul derby: Mauro İcardi opened the scoring for Galatasaray.";
        let q = slice_quote(body, "mauro icardi");
        // Dotted-capital fold may or may not match the ASCII needle char-for-char; the
        // contract is NO PANIC and a sensible Option either way.
        if let Some(q) = q {
            assert!(q.contains("Galatasaray"));
        }
        assert!(slice_quote(body, "Galatasaray")
            .unwrap()
            .contains("Galatasaray"));
    }

    #[test]
    fn quote_misses_yield_none() {
        assert_eq!(slice_quote("short body", "vinicius"), None);
        assert_eq!(slice_quote("", "vinicius"), None);
        assert_eq!(slice_quote("body", ""), None);
    }
}
