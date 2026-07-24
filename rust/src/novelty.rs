//! Novelty — the candle's source-aware near-duplicate gate (Cognition refactor Phase 2).
//!
//! The relevance gate ([`crate::resolve::Harness::resolve_set`]) answers the candle's first
//! question — "is this article about the entity?". This gate answers the second — "is this a
//! REPOST of something we already have?" — and it answers it in a way the retired
//! `narratives::dedup_corpus` did NOT: source-aware.
//!
//! The old dedup clustered article embeddings source-BLIND: N distinct outlets covering the same
//! story collapsed to one representative and the rest were dropped — silently destroying the
//! cross-outlet corroboration signal (how many independent sources carry a story is its heat and
//! its credibility). This gate inverts that default:
//!
//!   * FIRST-SEEN wins. A newly-scrubbed article X is compared against recent CANONICAL coverage
//!     (`news_articles.duplicate_of IS NULL`) of X's own vetted entities, within a lookback window.
//!   * X is SUPPRESSED — its `duplicate_of` pointed at the canonical original R — only when it is a
//!     near-dup (cosine ≥ `novelty_cosine`) AND it is either the SAME outlet reposting (the 24h
//!     thread-repost noise) OR a near-VERBATIM copy (token-Jaccard ≥ `verbatim_jaccard`, i.e.
//!     syndicated wire copy republished across outlets).
//!   * Everything else PASSES THROUGH as canonical — most importantly a DIFFERENT outlet writing
//!     its OWN piece on the same story: that is corroboration, and every such source is counted.
//!
//! Suppression sets `duplicate_of` ONLY; it never touches `news_article_entities.vetted`, so the
//! membership the derive trigger keys on is unchanged and no re-derive is provoked. The consumers
//! of `duplicate_of` (the narratives corpus filter, the vetted-trigger canonical membership count)
//! land in Phase 3 — until then this gate accumulates the signal without acting on it downstream.
//!
//! Each CANONICAL article's embedding is persisted to `topic_heat_embeddings` (repurposed from the
//! retired topic-heat cache into the article-embedding store) so the next arrival is compared
//! against it without re-embedding. The store is canonical-only and, like before, reproducible from
//! `news_articles` text — safe to truncate. The embedding basis is reused from the relevance gate
//! when it ran (the article context was already embedded there); an all-primary article that never
//! entered `resolve_set` is embedded fresh here — one CPU call, never the GPU.

use crate::config::ScrubConfig;
use crate::embed::cosine_similarity;
use crate::harness::{EntityType, Harness, Vector};
use anyhow::{Context, Result};
use sqlx::Row;
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

/// A defensive ceiling on the comparison set — NOT a data cap. The candidate set is already bounded
/// to one article's entities ∩ canonical ∩ the lookback window (naturally tens of rows), ordered
/// freshest-first (reposts are recent, so a truncation would only ever drop the LEAST likely
/// matches). It exists so a pathological hot entity can never make one scrub pass unbounded.
const MAX_NOVELTY_CANDIDATES: i64 = 500;

/// ArticleNovelty is one freshly-scrubbed article's inputs to the gate. `context` is the exact
/// model-visible text ([`article_text`]) that was (or will be) embedded — it is both the fingerprint
/// basis and the token-Jaccard basis. `context_vector` is the relevance gate's already-computed
/// embedding of that same text, reused when present (`None` ⇒ embed fresh here).
pub struct ArticleNovelty<'a> {
    pub article_id: i64,
    /// Upper-case sport, matching `news_article_entities.sport`.
    pub sport: &'a str,
    pub source: &'a str,
    pub context: &'a str,
    pub context_vector: Option<&'a Vector>,
    /// The entities kept by the relevance gate (the article's vetted membership after this scrub).
    pub vetted_entities: &'a [(EntityType, i32)],
}

/// gate runs the source-aware novelty pass for one article. Returns `Some(R)` when the article was
/// suppressed as a duplicate of canonical article `R` (its `duplicate_of` was set), or `None` when
/// it is canonical (its embedding was persisted for future comparisons). A no-op returning `None`
/// when there is no embedder (offline/parity bins) or the article kept no entities.
pub async fn gate(hx: &Harness, article: ArticleNovelty<'_>) -> Result<Option<i64>> {
    // No embedder (offline/parity) or no kept entities → nothing to compare against; canonical.
    let Some(embedder) = hx.embedder.as_ref() else {
        return Ok(None);
    };
    if article.vetted_entities.is_empty() {
        return Ok(None);
    }
    let cfg = &hx.scrub;
    let dim = embedder.dim;

    // Reuse the relevance gate's context embedding when it ran; embed fresh only when there were no
    // secondaries (an all-primary article never entered resolve_set) — one CPU call, off the GPU.
    let fresh: Option<Vector> = match article.context_vector {
        Some(_) => None,
        None => Some(
            hx.embed(std::slice::from_ref(&article.context.to_string()))
                .await
                .context("embed article for novelty")?
                .pop()
                .context("embed returned no vector")?,
        ),
    };
    let x_vec: &Vector = article
        .context_vector
        .or(fresh.as_ref())
        .expect("exactly one of context_vector / fresh is Some");

    let cands = load_candidates(hx, &article, &embedder.identity, dim).await?;
    if let Some(canonical_id) = pick_repost(x_vec, article.source, article.context, &cands, cfg) {
        mark_duplicate(hx, article.article_id, canonical_id).await?;
        return Ok(Some(canonical_id));
    }

    // Canonical: persist its embedding so the next arrival compares against it without re-embedding.
    persist_embedding(
        hx,
        article.article_id,
        article.context,
        x_vec,
        &embedder.identity,
    )
    .await?;
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

/// Cand is one recent canonical article to compare X against: its current text (for the token-Jaccard
/// and the freshness fingerprint check) and its persisted embedding.
struct Cand {
    article_id: i64,
    source: String,
    text: String,
    fingerprint: i64,
    embedding: Vector,
}

/// load_candidates pulls the recent CANONICAL articles that share ≥1 vetted entity with X, joined to
/// their persisted embeddings. Scoped to X's own entities so unrelated stories are never compared;
/// filtered to the configured embedder identity so a model swap invalidates the store implicitly.
async fn load_candidates(
    hx: &Harness,
    article: &ArticleNovelty<'_>,
    model: &str,
    dim: usize,
) -> Result<Vec<Cand>> {
    let entity_types: Vec<String> = article
        .vetted_entities
        .iter()
        .map(|(t, _)| t.as_str().to_string())
        .collect();
    let entity_ids: Vec<i32> = article.vetted_entities.iter().map(|(_, id)| *id).collect();

    let rows = sqlx::query(
        r#"
        SELECT the.article_id                 AS article_id,
               COALESCE(na.source, '')        AS source,
               na.title                       AS title,
               COALESCE(na.description, '')   AS description,
               the.fingerprint                AS fingerprint,
               the.embedding                  AS embedding
        FROM public.topic_heat_embeddings the
        JOIN public.news_articles na ON na.id = the.article_id
        WHERE the.model = $1
          AND na.id <> $2
          AND na.duplicate_of IS NULL
          AND na.fetched_at >= NOW() - make_interval(secs => $3)
          AND EXISTS (
              SELECT 1
              FROM news_article_entities nae
              JOIN unnest($4::text[], $5::int[]) AS want(entity_type, entity_id)
                ON nae.entity_type = want.entity_type AND nae.entity_id = want.entity_id
              WHERE nae.article_id = the.article_id
                AND nae.sport = $6
                AND nae.vetted = TRUE
          )
        ORDER BY na.fetched_at DESC
        LIMIT $7
        "#,
    )
    .bind(model)
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
        let bytes: Vec<u8> = r.get("embedding");
        let Some(embedding) = decode_vector(&bytes, dim) else {
            continue; // wrong-dimension row (stale model) → skip, never mis-compare
        };
        let title: String = r.get("title");
        let description: String = r.get("description");
        out.push(Cand {
            article_id: r.get("article_id"),
            source: r.get("source"),
            text: article_text(&title, &description),
            fingerprint: r.get("fingerprint"),
            embedding,
        });
    }
    Ok(out)
}

/// pick_repost is the pure decision: among the canonical candidates, the one X is a REPOST of, or
/// `None` if X is novel/corroborating. FIRST-SEEN wins, so the winner is always an existing canonical
/// id. A candidate is a repost of X when the embeddings are near-dup (`cosine ≥ novelty_cosine`) AND
/// either the outlet is the SAME (a thread-repost) or the text is near-VERBATIM (`token_jaccard ≥
/// verbatim_jaccard`, cross-outlet syndication). A different outlet writing its own piece — high
/// cosine but low Jaccard — is corroboration and passes through. Ties break to the highest cosine.
fn pick_repost(
    x_vec: &[f32],
    x_source: &str,
    x_text: &str,
    cands: &[Cand],
    cfg: &ScrubConfig,
) -> Option<i64> {
    let mut best: Option<(i64, f32)> = None;
    for c in cands {
        // Skip a stale cache row (the article's text changed since it was embedded) — erring toward
        // pass-through, the safe direction: we only ever SUPPRESS on a certain match.
        if text_fingerprint(&c.text) as i64 != c.fingerprint {
            continue;
        }
        let cosine = cosine_similarity(x_vec, &c.embedding);
        if cosine < cfg.novelty_cosine {
            continue;
        }
        let is_repost = source_eq(x_source, &c.source)
            || token_jaccard(x_text, &c.text) >= cfg.verbatim_jaccard;
        if is_repost && best.is_none_or(|(_, bc)| cosine > bc) {
            best = Some((c.article_id, cosine));
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

/// persist_embedding write-through caches a canonical article's embedding into the repurposed store,
/// keyed by article with the text fingerprint (so a re-scrub of edited text re-embeds) and the
/// embedder identity (so a model swap invalidates it).
async fn persist_embedding(
    hx: &Harness,
    article_id: i64,
    text: &str,
    vec: &Vector,
    model: &str,
) -> Result<()> {
    let fingerprint = text_fingerprint(text) as i64;
    let blob = encode_vector(vec);
    sqlx::query(
        r#"
        INSERT INTO public.topic_heat_embeddings (article_id, fingerprint, model, embedding)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (article_id) DO UPDATE
            SET fingerprint = EXCLUDED.fingerprint,
                model       = EXCLUDED.model,
                embedding   = EXCLUDED.embedding,
                embedded_at = now()
        "#,
    )
    .bind(article_id)
    .bind(fingerprint)
    .bind(model)
    .bind(blob)
    .execute(&hx.pool)
    .await
    .context("persist article embedding")?;
    Ok(())
}

/// source_eq is same-outlet identity, case/space-insensitive. An EMPTY (unknown) source is never
/// "the same" — an unknown outlet can't establish a thread-repost, so it falls through to the
/// verbatim test (the conservative call).
fn source_eq(a: &str, b: &str) -> bool {
    let a = a.trim();
    !a.is_empty() && a.eq_ignore_ascii_case(b.trim())
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

/// text_fingerprint hashes the model-visible text to a u64 (stored as i64, same bit pattern) — the
/// cache-freshness key, identical in spirit to the retired topic-heat cache's fingerprint.
fn text_fingerprint(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// encode_vector packs an embedding as little-endian f32 bytes for the `bytea` column (bge-small =
/// 384 dims = 1536 bytes). Nothing queries by similarity in SQL — the cosine stays in Rust — so
/// plain bytes, no pgvector dependency (mig 151's original rationale, carried forward).
fn encode_vector(v: &Vector) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// decode_vector is encode_vector's inverse; `None` unless the bytes are exactly `dim` f32s (a
/// wrong-dimension row from a different embedding model is skipped, never mis-compared).
fn decode_vector(bytes: &[u8], dim: usize) -> Option<Vector> {
    if dim == 0 || bytes.len() != dim * 4 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ScrubConfig {
        ScrubConfig {
            novelty_cosine: 0.85,
            verbatim_jaccard: 0.90,
            ..ScrubConfig::default()
        }
    }

    /// A candidate with a FRESH fingerprint (matches its own text, so it is not skipped as stale).
    fn cand(id: i64, source: &str, text: &str, embedding: Vec<f32>) -> Cand {
        Cand {
            article_id: id,
            source: source.to_string(),
            text: text.to_string(),
            fingerprint: text_fingerprint(text) as i64,
            embedding,
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
    fn source_eq_is_case_insensitive_and_rejects_empty() {
        assert!(source_eq("BBC Sport", "bbc sport"));
        assert!(source_eq(" ESPN ", "espn"));
        assert!(!source_eq("", ""));
        assert!(!source_eq("", "BBC"));
        assert!(!source_eq("BBC", "Sky"));
    }

    #[test]
    fn vector_roundtrips_through_bytes() {
        let v = vec![1.0_f32, -0.5, 0.25, 0.0];
        assert_eq!(decode_vector(&encode_vector(&v), 4), Some(v));
        // Wrong dimension → rejected, never mis-decoded.
        assert_eq!(decode_vector(&encode_vector(&vec![1.0, 2.0]), 4), None);
    }

    #[test]
    fn fingerprint_is_stable_and_text_sensitive() {
        assert_eq!(text_fingerprint("abc"), text_fingerprint("abc"));
        assert_ne!(text_fingerprint("abc"), text_fingerprint("abc (updated)"));
    }

    #[test]
    fn corroboration_passes_through() {
        // THE point of the whole gate: a DIFFERENT outlet, near-dup embedding (same story) but its
        // OWN words (low Jaccard) → NOT a repost. This is what the source-blind dedup destroyed.
        let x = vec![1.0_f32, 0.0];
        let cands = vec![cand(
            10,
            "The Guardian",
            "City close on a deal for the winger after weeks of talks",
            vec![1.0, 0.0], // cosine 1.0 with x
        )];
        assert_eq!(
            pick_repost(
                &x,
                "BBC Sport",
                "Manchester City agree terms to sign the wide man",
                &cands,
                &cfg()
            ),
            None
        );
    }

    #[test]
    fn same_source_repost_is_suppressed() {
        // Same outlet, near-dup embedding → the 24h thread-repost noise, suppressed even though the
        // wording differs (low Jaccard).
        let x = vec![1.0_f32, 0.0];
        let cands = vec![cand(
            20,
            "BBC Sport",
            "Totally different words here entirely",
            vec![0.99, 0.14],
        )];
        assert_eq!(
            pick_repost(
                &x,
                "bbc sport",
                "The club confirm the signing officially",
                &cands,
                &cfg()
            ),
            Some(20)
        );
    }

    #[test]
    fn verbatim_cross_outlet_is_suppressed() {
        // Different outlet but near-VERBATIM text (syndicated wire copy) → suppressed.
        let x = vec![1.0_f32, 0.0];
        let text = "Chelsea have completed the signing of the midfielder on a five year deal";
        let cands = vec![cand(30, "Yahoo", text, vec![1.0, 0.0])];
        assert_eq!(pick_repost(&x, "Reuters", text, &cands, &cfg()), Some(30));
    }

    #[test]
    fn low_cosine_never_suppresses() {
        // Verbatim text but the embeddings are far apart (different story reusing boilerplate) →
        // pass-through. The cosine line is a hard precondition.
        let x = vec![1.0_f32, 0.0];
        let text = "identical boilerplate transfer sentence used everywhere today";
        let cands = vec![cand(40, "Same Wire", text, vec![0.0, 1.0])]; // cosine 0
        assert_eq!(pick_repost(&x, "Same Wire", text, &cands, &cfg()), None);
    }

    #[test]
    fn stale_fingerprint_candidate_is_skipped() {
        // The stored embedding predates an edit to the article text → skip rather than mis-compare.
        let x = vec![1.0_f32, 0.0];
        let mut c = cand(50, "BBC Sport", "current text", vec![1.0, 0.0]);
        c.fingerprint = text_fingerprint("older text it was embedded from") as i64;
        assert_eq!(
            pick_repost(&x, "BBC Sport", "current text", &[c], &cfg()),
            None
        );
    }

    #[test]
    fn no_candidates_passes_as_canonical() {
        // The base case: with nothing to compare against, an article is always canonical. (This is
        // what `gate` sees for the first-ever article about an entity, or after the lookback window
        // clears — corroboration/heat can only ever ACCUMULATE from a canonical first-seen.)
        let x = vec![1.0_f32, 0.0];
        assert_eq!(
            pick_repost(&x, "BBC Sport", "any text at all", &[], &cfg()),
            None
        );
    }

    #[test]
    fn verbatim_jaccard_is_the_cross_outlet_boundary() {
        // For DIFFERENT outlets with near-dup embeddings, `verbatim_jaccard` alone decides repost vs
        // corroboration — the line the COGNITION_NOVELTY_VERBATIM_JACCARD config tunes. Just UNDER it
        // is independent coverage (PASSES, counted as its own source); AT/OVER is syndicated wire copy
        // (SUPPRESSED). This is the boundary the source-blind dedup could never draw.
        let x = vec![1.0_f32, 0.0];
        let base = "one two three four five six seven eight nine ten \
                    eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty";
        let x_text = format!("{base} alpha beta gamma"); // 23 distinct tokens (superset of base's 20)
                                                         // Candidate carries only the 20 shared tokens → jaccard 20/23 ≈ 0.87 < 0.90 → corroboration.
        let corroborating = vec![cand(70, "Associated Press", base, vec![1.0, 0.0])];
        assert_eq!(
            pick_repost(&x, "Reuters", &x_text, &corroborating, &cfg()),
            None
        );
        // Identical wording → jaccard 1.0 ≥ 0.90 → syndicated repost across outlets, suppressed.
        let syndicated = vec![cand(71, "Associated Press", &x_text, vec![1.0, 0.0])];
        assert_eq!(
            pick_repost(&x, "Reuters", &x_text, &syndicated, &cfg()),
            Some(71)
        );
    }

    #[test]
    fn highest_cosine_wins_among_reposts() {
        let x = vec![1.0_f32, 0.0];
        let verbatim = "the exact same syndicated wire sentence for both rows";
        let cands = vec![
            cand(60, "Wire A", verbatim, vec![0.90, 0.44]), // lower cosine
            cand(61, "Wire B", verbatim, vec![1.0, 0.0]),   // higher cosine
        ];
        assert_eq!(
            pick_repost(&x, "Original", verbatim, &cands, &cfg()),
            Some(61)
        );
    }
}
