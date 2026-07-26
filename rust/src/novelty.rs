//! Novelty — the near-verbatim repost gate. Pure string math, NO MODEL, no embeddings.
//!
//! One question: "is this article a syndicated copy of one already held?" A newly-scrubbed article
//! X is compared against recent CANONICAL coverage (`news_articles.duplicate_of IS NULL`) of X's
//! own vetted entities, within a lookback window. X is SUPPRESSED — its `duplicate_of` pointed at
//! the canonical original R — only when the two texts are near-VERBATIM
//! (token-Jaccard ≥ `verbatim_jaccard`). Everything else PASSES THROUGH as canonical.
//!
//! **Two restraints were deleted here (teardown plan §2.2), and the gate got MORE precise.**
//!
//!   * The embedding prefilter (`cosine ≥ novelty_cosine`) is gone with the rest of the candle.
//!     It was never doing the work: the cross-outlet branch already required `token_jaccard`, and
//!     the text it embedded was `"{title} — {description}"` where description is 99.7% the title
//!     repeated plus the outlet name — so the vector was the headline counted twice.
//!   * The same-outlet branch (`source_eq`) is gone. It suppressed on cosine ALONE, with no text
//!     check, and produced 855 of 868 measured suppressions — of which only 107 were actually
//!     near-verbatim. It was deleting genuinely different articles: two opposite takes on Cade
//!     Cunningham collapsed to one; two different Bayern fixtures collapsed because betting
//!     boilerplate dominated the embedding. Scott's call: a publication posting twice about the
//!     same thing is signal about that publication, not grounds to throw the second one out —
//!     narratives stack sources and timestamps, so repetition becomes visible rather than hidden.
//!
//! What is left is auditable (you can print the overlapping tokens), model-free, and cannot
//! silently delete a contradiction. The asymmetry justifies the direction: a missed suppression
//! costs one reader call, a wrong suppression is a permanent hole in the narrative.
//!
//! Suppression sets `duplicate_of` ONLY; it never touches `news_article_entities.vetted`, so the
//! membership the derive trigger keys on is unchanged and no re-derive is provoked.

use crate::config::ScrubConfig;
use crate::harness::{EntityType, Harness};
use anyhow::{Context, Result};
use sqlx::Row;
use std::collections::HashSet;

/// A defensive ceiling on the comparison set — NOT a data cap. The candidate set is already bounded
/// to one article's entities ∩ canonical ∩ the lookback window (naturally tens of rows), ordered
/// freshest-first (reposts are recent, so a truncation would only ever drop the LEAST likely
/// matches). It exists so a pathological hot entity can never make one scrub pass unbounded.
const MAX_NOVELTY_CANDIDATES: i64 = 500;

/// ArticleNovelty is one freshly-scrubbed article's inputs to the gate. `context` is the exact
/// text ([`article_text`]) used as the token-Jaccard basis.
pub struct ArticleNovelty<'a> {
    pub article_id: i64,
    /// Upper-case sport, matching `news_article_entities.sport`.
    pub sport: &'a str,
    pub context: &'a str,
    /// The article's vetted membership after this scrub.
    pub vetted_entities: &'a [(EntityType, i32)],
}

/// gate runs the near-verbatim novelty pass for one article. Returns `Some(R)` when the article was
/// suppressed as a syndicated copy of canonical article `R` (its `duplicate_of` was set), or `None`
/// when it is canonical. A no-op returning `None` when the article kept no entities.
///
/// No embedder, no vector store, no cache: candidate texts are read live from `news_articles`, so
/// there is no stale-fingerprint class of bug to defend against. Set intersection over ~20 tokens
/// against at most `MAX_NOVELTY_CANDIDATES` rows is microseconds.
pub async fn gate(hx: &Harness, article: ArticleNovelty<'_>) -> Result<Option<i64>> {
    if article.vetted_entities.is_empty() {
        return Ok(None); // nothing to compare against; canonical
    }
    let cfg = &hx.scrub;

    let cands = load_candidates(hx, &article).await?;
    if let Some(canonical_id) = pick_repost(article.context, &cands, cfg) {
        mark_duplicate(hx, article.article_id, canonical_id).await?;
        return Ok(Some(canonical_id));
    }
    Ok(None)
}

/// article_text is the single source of truth for one article's model-visible context string —
/// `"<title> — <description>"`, or just the title when there is no description. The scrub gate builds
/// its `context` through this, and the novelty store fingerprints and re-tokenizes through it, so the
/// embedded text, the persisted fingerprint, and a later candidate's re-derived text are provably the
/// same string. The `—` is U+2014 (matches the retired Go/topic-heat basis).
pub fn article_text(title: &str, description: &str) -> String {
    if description.is_empty() {
        title.to_string()
    } else {
        format!("{title} — {description}")
    }
}

/// Cand is one recent canonical article to compare X against: its current text, read live.
struct Cand {
    article_id: i64,
    text: String,
}

/// load_candidates pulls the recent CANONICAL articles that share ≥1 vetted entity with X. Scoped
/// to X's own entities so unrelated stories are never compared.
async fn load_candidates(hx: &Harness, article: &ArticleNovelty<'_>) -> Result<Vec<Cand>> {
    let entity_types: Vec<String> = article
        .vetted_entities
        .iter()
        .map(|(t, _)| t.as_str().to_string())
        .collect();
    let entity_ids: Vec<i32> = article.vetted_entities.iter().map(|(_, id)| *id).collect();

    let rows = sqlx::query(
        r#"
        SELECT na.id                         AS article_id,
               na.title                      AS title,
               COALESCE(na.description, '')  AS description
        FROM public.news_articles na
        WHERE na.id <> $1
          AND na.duplicate_of IS NULL
          AND na.fetched_at >= NOW() - make_interval(secs => $2)
          AND EXISTS (
              SELECT 1
              FROM news_article_entities nae
              JOIN unnest($3::text[], $4::int[]) AS want(entity_type, entity_id)
                ON nae.entity_type = want.entity_type AND nae.entity_id = want.entity_id
              WHERE nae.article_id = na.id
                AND nae.sport = $5
                AND nae.vetted = TRUE
          )
        ORDER BY na.fetched_at DESC
        LIMIT $6
        "#,
    )
    .bind(article.article_id)
    .bind(hx.scrub.novelty_lookback.as_secs_f64())
    .bind(&entity_types)
    .bind(&entity_ids)
    .bind(article.sport)
    .bind(MAX_NOVELTY_CANDIDATES)
    .fetch_all(&hx.pool)
    .await
    .context("load novelty candidates")?;

    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        let title: String = r.get("title");
        let description: String = r.get("description");
        out.push(Cand {
            article_id: r.get("article_id"),
            text: article_text(&title, &description),
        });
    }
    Ok(out)
}

/// pick_repost is the pure decision: among the canonical candidates, the one X is a REPOST of, or
/// `None` if X is novel/corroborating. FIRST-SEEN wins, so the winner is always an existing
/// canonical id. A candidate is a repost of X when the text is near-VERBATIM
/// (`token_jaccard ≥ verbatim_jaccard`) — regardless of outlet. Anyone writing their own words on
/// the same story, including the same outlet writing a second piece, passes through. Ties break to
/// the highest Jaccard.
fn pick_repost(x_text: &str, cands: &[Cand], cfg: &ScrubConfig) -> Option<i64> {
    let mut best: Option<(i64, f32)> = None;
    for c in cands {
        let j = token_jaccard(x_text, &c.text);
        if j >= cfg.verbatim_jaccard && best.is_none_or(|(_, bj)| j > bj) {
            best = Some((c.article_id, j));
        }
    }
    best.map(|(id, _)| id)
}

/// mark_duplicate points a suppressed article at its canonical original. `duplicate_of` only — the
/// article's vetted membership is untouched, so no derive re-fires (Phase 3 consumes this flag).
async fn mark_duplicate(hx: &Harness, article_id: i64, canonical_id: i64) -> Result<()> {
    sqlx::query("UPDATE public.news_articles SET duplicate_of = $2 WHERE id = $1")
        .bind(article_id)
        .bind(canonical_id)
        .execute(&hx.pool)
        .await
        .context("mark duplicate_of")?;
    Ok(())
}

/// token_jaccard is |A∩B| / |A∪B| over the two texts' alphanumeric tokens (lower-cased). ~1.0 =
/// near-verbatim (syndicated wire copy); low = independently written coverage. Both empty ⇒ 0.
fn token_jaccard(a: &str, b: &str) -> f32 {
    let ta = tokenize(a);
    let tb = tokenize(b);
    let union = ta.union(&tb).count();
    if union == 0 {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count();
    inter as f32 / union as f32
}

fn tokenize(s: &str) -> HashSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}


#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ScrubConfig {
        ScrubConfig {
            verbatim_jaccard: 0.90,
            ..ScrubConfig::default()
        }
    }

    fn cand(id: i64, text: &str) -> Cand {
        Cand {
            article_id: id,
            text: text.to_string(),
        }
    }

    #[test]
    fn article_text_joins_with_em_dash() {
        assert_eq!(article_text("Title", "Body"), "Title — Body");
        assert_eq!(article_text("Title", ""), "Title");
    }

    #[test]
    fn token_jaccard_bounds() {
        assert_eq!(token_jaccard("", ""), 0.0);
        assert!((token_jaccard("a b c", "a b c") - 1.0).abs() < 1e-6);
        assert_eq!(token_jaccard("a b", "c d"), 0.0);
        // {a,b,c} vs {a,b} → 2/3.
        assert!((token_jaccard("a b c", "a b") - 2.0 / 3.0).abs() < 1e-6);
        // Punctuation/case fold to the same tokens → verbatim.
        assert!((token_jaccard("Chelsea sign him!", "chelsea, sign him") - 1.0).abs() < 1e-6);
    }

    #[test]
    fn verbatim_is_suppressed_regardless_of_outlet() {
        // The ONE thing this gate still does: syndicated wire copy carries no new prose, so
        // re-reading it buys nothing. Outlet is irrelevant to the decision now.
        let text = "Chelsea have completed the signing of the midfielder on a five year deal";
        assert_eq!(pick_repost(text, &[cand(30, text)], &cfg()), Some(30));
    }

    #[test]
    fn same_outlet_writing_a_second_piece_passes_through() {
        // THE behaviour change (teardown §3.2). The old gate suppressed on cosine alone whenever
        // the outlet matched, which collapsed two OPPOSITE takes into one. Both of these are real
        // Yahoo Sports headlines that the old gate deleted; both must now survive.
        let x = "Pistons called out for mistake that could hurt Cade Cunningham";
        let cands = vec![cand(80, "Cade Cunningham Is Giving the Pistons a Real Shot")];
        assert_eq!(pick_repost(x, &cands, &cfg()), None);
    }

    #[test]
    fn different_fixtures_sharing_boilerplate_pass_through() {
        // FanDuel's odds pages: near-identical boilerplate, different fixtures. The embedding could
        // not tell them apart; token overlap can, because the club names differ.
        let x = "Real Madrid v Bayern Munich Odds, Prediction and Betting Preview";
        let cands = vec![cand(
            81,
            "Manchester City v Bayern Munich Odds, Prediction and Betting Preview",
        )];
        assert_eq!(pick_repost(x, &cands, &cfg()), None);
    }

    #[test]
    fn corroboration_passes_through() {
        // A DIFFERENT outlet writing its OWN piece on the same story is corroboration — the signal
        // the source-blind dedup destroyed. Low token overlap ⇒ passes.
        let cands = vec![cand(
            10,
            "City close on a deal for the winger after weeks of talks",
        )];
        assert_eq!(
            pick_repost(
                "Manchester City agree terms to sign the wide man",
                &cands,
                &cfg()
            ),
            None
        );
    }

    #[test]
    fn no_candidates_passes_as_canonical() {
        // The base case: with nothing to compare against, an article is always canonical.
        assert_eq!(pick_repost("any text at all", &[], &cfg()), None);
    }

    #[test]
    fn verbatim_jaccard_is_the_boundary() {
        // `verbatim_jaccard` alone decides repost vs corroboration — the line the
        // COGNITION_NOVELTY_VERBATIM_JACCARD config tunes. Just UNDER it is independent coverage
        // (PASSES); AT/OVER is syndicated copy (SUPPRESSED).
        let base = "one two three four five six seven eight nine ten \
                    eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty";
        let x_text = format!("{base} alpha beta gamma"); // 23 distinct tokens (superset of base's 20)
                                                         // Candidate carries only the 20 shared tokens → jaccard 20/23 ≈ 0.87 < 0.90 → corroboration.
        assert_eq!(pick_repost(&x_text, &[cand(70, base)], &cfg()), None);
        // Identical wording → jaccard 1.0 ≥ 0.90 → syndicated repost, suppressed.
        assert_eq!(pick_repost(&x_text, &[cand(71, &x_text)], &cfg()), Some(71));
    }

    #[test]
    fn highest_jaccard_wins_among_reposts() {
        let verbatim = "the exact same syndicated wire sentence for both rows";
        let cands = vec![
            cand(60, "the exact same syndicated wire sentence for both rows plus a trailing clause"),
            cand(61, verbatim), // identical → jaccard 1.0
        ];
        assert_eq!(pick_repost(verbatim, &cands, &cfg()), Some(61));
    }
}
