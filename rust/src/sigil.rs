//! Sigil stage — the L3 stage port: the crown convergence, re-expressed as a
//! composition of the capability library's primitives.
//!
//! Sigil = `read 3 pillars + route(StatsLogic) + extract(SigilParser) + persist`, with a
//! `debounce_unchanged` gate on the pillar `input_hash`. The Go source is the spec, mirrored
//! line-for-line so the temp-0 parity diff holds. The Go sources mirrored here:
//! `go/internal/ml/sigil.go` (Generate, the three pillar loaders, prompt, parse,
//! input-components/hash, persist, the SkipUnchanged gate); `go/internal/ml/rating.go`
//! (`hashComponents` / `round1`, shared package helpers); `go/internal/derive/derive.go`
//! (drainSigil: queue Item → SigilRequest, current-season + SkipUnchanged, the terminal stage).
//!
//! This is the first NEW derivation on the library (the primitives don't move): the first
//! `Role::StatsLogic` consumer, the first user of `Persist::debounce_unchanged`, and the first
//! user of the `Provenance.input_hash` envelope field — all three shipped real but unexercised
//! by vibe. Everything that can differ between the two implementations — the SQL reads, the
//! deterministic slope/trend math, the prompt bytes, the canonical input-components JSON (whose
//! SHA-256 is the `input_hash`), the parse — lives here and is mirrored exactly. See
//! `src/bin/sigil_parity.rs` for the harness and migration 107 for the shadow table.
//!
//! Fail-closed semantics reproduced verbatim: when an entity has NO narrative pillar AND no
//! rating pillar AND no momentum pillar, we skip the model and persist a NULL-score/NULL-blurb
//! marker row (the read path returns "no synthesis yet"). The SkipUnchanged debounce skips the
//! Gemma call when the three pillars hash identically to the entity-season's latest synthesis.
//! Sigil is the TERMINAL stage — unlike vibe it enqueues nothing downstream.

use crate::harness::{EntityKey, Harness, Parser, Provenance};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::util::truncate;
use crate::work::{Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

/// Prompt version — mirrors `sigilPromptVersion` in sigil.go. s4: the stat-identity pillar's
/// divined-strength label is read from `stat_summaries.divined_peak`, the input-component key
/// is `divined_peak`, and the P2 section is labelled PEAK. Bump in lockstep with the Go const
/// so the two stages stamp identical provenance.
pub const SIGIL_PROMPT_VERSION: &str = "s4";

/// Production synthesis temperature (sigil.go uses 0.6). The parity harness overrides this
/// with an explicit 0.
pub const SIGIL_TEMPERATURE: f64 = 0.6;

/// Token cap for the (SCORE + 1-2 sentence BLURB) answer. Mirrors sigil.go's NumPredict: 1000.
pub const SIGIL_NUM_PREDICT: i32 = 1000;

/// sigilSystemPrompt, byte-for-byte from sigil.go. The em-dashes, straight quotes, and the
/// 4-space-indented SCORE/BLURB lines are significant — at temp 0 a single changed byte here
/// would change the model's output.
pub const SIGIL_SYSTEM_PROMPT: &str = r#"You are a holistic sports analyst synthesizing three signals — news narrative, statistical identity, and momentum — into a single SIGIL score and a short blurb.

The vibe is SLOW-MOVING and SEASON-AWARE: it reflects the entity's whole-season arc, not a single game.

Rules:
- Weigh all three signals. One weak signal does not override the others.
- The score is 1-100: 1 = deeply troubled/in freefall, 50 = neutral/steady, 100 = dominant/surging.
- The blurb is 1-2 sentences of plain prose: what STORY this entity is telling right now. No headlines, no bullet points.
- Respond on EXACTLY two lines:
    SCORE: <integer>
    BLURB: <1-2 sentences>
- No other text, no preamble, no explanation."#;

// ---------------------------------------------------------------------------
// Pillar value types — mirror the Go synth* structs.
// ---------------------------------------------------------------------------

/// One narrative from the entity's latest generation (P1). Mirrors `synthNarrative`.
/// `impact` is `f64` to mirror Go (the column is `smallint`, read as integer then widened);
/// it is only ever rendered with `%.0f`, so the integer value reproduces exactly.
#[derive(Clone, Debug)]
pub struct SynthNarrative {
    pub title: String,
    pub body: String,
    pub impact: f64,
}

/// The stat-identity pillar (P2). Mirrors `synthRating`. `None` (suppressed) when there is no
/// commentary row, or when the latest generation is a no-stats marker (`body` NULL).
#[derive(Clone, Debug)]
pub struct SynthRating {
    pub divined_peak: String,
    pub body: String,
    pub notability: i32,
}

/// The momentum pillar (P3). Mirrors `synthMomentum`. The slopes feed `trend_dir` (bucketed
/// text in the prompt); the latest values feed both the prompt and the input-components hash.
#[derive(Clone, Debug, Default)]
pub struct SynthMomentum {
    /// Positive = trending up; OLS slope over the last 14 vibe_scores rows.
    pub sentiment_slope: f64,
    /// OLS slope over the last 10 event composite scores.
    pub composite_slope: f64,
    pub latest_sentiment: Option<i32>,
    pub latest_composite: Option<f64>,
    /// The Vibe end product's felt-read prose (latest vibe_scores.prompt).
    pub latest_vibe_prompt: String,
}

impl SynthMomentum {
    /// empty mirrors `synthMomentum.empty()`: no momentum signal at all.
    pub fn empty(&self) -> bool {
        self.latest_sentiment.is_none() && self.latest_composite.is_none()
    }
}

/// The validated two-line synthesis answer — SCORE (1-100) + the 1-2 sentence BLURB. The sigil
/// Extract output shape (the `T` in `Parser<T>` / `Extracted<T>`).
#[derive(Clone, Debug)]
pub struct SigilReply {
    pub score: i32,
    pub blurb: String,
}

/// The result of running the sigil core for one entity, before persistence. Captures
/// everything both the production handler (→ sigil_synthesis) and the parity harness
/// (→ sigil_synthesis_shadow) need to persist.
#[derive(Clone, Debug)]
pub struct SigilOutput {
    /// `None` ⇒ no-pillar NULL marker (no model call was made).
    pub score: Option<i32>,
    /// `None` ⇒ marker; `Some` (possibly empty) ⇒ scored. Sigil stores an empty blurb as "",
    /// not NULL (unlike vibe's felt-read) — mirroring sigil.go's persist.
    pub blurb: Option<String>,
    /// The season this convergence is for (current_season, resolved + stamped). Never NULL.
    pub season: i32,
    /// The canonical input-components JSON — BYTE-IDENTICAL to Go's `json.Marshal(ic)`, so it
    /// is both the persisted `input_components` and the pre-image of `input_hash`. `"{}"` for
    /// the no-pillar marker.
    pub input_components_json: String,
    /// SHA-256 (128-bit hex prefix) of `input_components_json` — the debounce key. `None` for
    /// the marker (sigil.go writes NULL `input_hash` for a no-pillar row).
    pub input_hash: Option<String>,
    /// no-pillar → the role's configured model name; scored → the model echoed in the response.
    pub model: String,
    pub prompt_version: &'static str,
    /// The exact user prompt sent to the model; `None` for the no-pillar marker.
    pub built_prompt: Option<String>,
    /// The exact Ollama request body that was POSTed — captured by `extract` from the same
    /// backend + opts the call used (single source of truth). `None` for the marker.
    pub request_body: Option<serde_json::Value>,
    pub skipped_no_pillars: bool,
}

impl SigilOutput {
    /// provenance lifts the moat fields into the shared `Provenance` envelope (Plan §1.6).
    /// Sigil DEBOUNCES, so `input_hash` is carried (vibe left it `None`); it persists
    /// `input_components` rather than `input_news_ids`, so `input_ids` is empty.
    fn provenance(&self) -> Provenance {
        Provenance {
            model_version: self.model.clone(),
            prompt_version: self.prompt_version,
            input_ids: Vec::new(),
            input_hash: self.input_hash.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Pillar loaders — byte-for-byte the same SQL the Go stage runs.
// ---------------------------------------------------------------------------

/// resolve_season returns the concrete season this synthesis is for: the caller's explicit
/// season when given, else the sport's `current_season`. Mirrors `SigilGenerator.resolveSeason`.
/// `sport` is the upper-cased value (the SQL key).
pub async fn resolve_season(pool: &PgPool, sport: &str, want: Option<i32>) -> Result<i32> {
    if let Some(s) = want {
        return Ok(s);
    }
    let cur: i32 = sqlx::query_scalar("SELECT current_season FROM public.sports WHERE id = $1")
        .bind(sport)
        .fetch_one(pool)
        .await
        .with_context(|| format!("resolve current_season for {sport}"))?;
    Ok(cur)
}

/// load_narrative_pillar (P1) returns the narratives from the entity's most recent generation
/// (news_summaries), hottest first. Empty when the latest generation was a no-narratives marker
/// (body NULL) or the entity has none. Mirrors `loadNarrativePillar` — the SAME SQL vibe's
/// narrative loader runs, minus the input_news_ids column (sigil persists components, not ids).
pub async fn load_narrative_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<SynthNarrative>> {
    // COALESCE(impact, 0): impact is int2 but the `0` literal is int4, so the result is int4 →
    // scan as i32 (matches Go scanning into a value later assigned to float64).
    let rows: Vec<(String, String, i32)> = sqlx::query_as(
        r#"
        SELECT narrative_title, body, COALESCE(impact, 0)
        FROM news_summaries
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
          AND body IS NOT NULL
          AND generated_at = (
              SELECT max(generated_at) FROM news_summaries
              WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
          )
        ORDER BY impact DESC NULLS LAST
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load narrative pillar {entity_type}/{entity_id}"))?;

    Ok(rows
        .into_iter()
        .map(|(title, body, impact)| SynthNarrative {
            title,
            body,
            impact: impact as f64,
        })
        .collect())
}

/// load_rating_pillar (P2) reads the entity-season's LATEST stat commentary regardless of
/// nullability, then suppresses the pillar if that latest generation is a no-stats marker
/// (body NULL) — never falling back to an older real commentary a marker has superseded
/// (FIRST-GPT-AUDIT Session 11 / F-023). Mirrors `loadRatingPillar`.
pub async fn load_rating_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: Option<i32>,
) -> Result<Option<SynthRating>> {
    // COALESCE(notability, 0): int2 coalesced with int4 → int4 → scan i32 (Go: `var notability int`).
    let row: Option<(String, Option<String>, i32)> = sqlx::query_as(
        r#"
        SELECT COALESCE(divined_peak, ''), body, COALESCE(notability, 0)
        FROM stat_summaries
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
          AND ($4::int IS NULL OR season = $4)
        ORDER BY generated_at DESC
        LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load rating pillar {entity_type}/{entity_id}"))?;

    match row {
        None => Ok(None),                  // pgx.ErrNoRows → pillar absent
        Some((_, None, _)) => Ok(None),    // latest generation is a marker (body NULL) → suppressed
        Some((divined_peak, Some(body), notability)) => Ok(Some(SynthRating {
            divined_peak,
            body,
            notability,
        })),
    }
}

/// load_momentum_pillar (P3) computes the sentiment trend (last 14 vibe_scores rows) + the
/// composite trend (last 10 event composite scores), capturing the latest of each plus the
/// latest felt-read prompt. Mirrors `loadMomentumPillar`.
pub async fn load_momentum_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: Option<i32>,
) -> Result<SynthMomentum> {
    let mut m = SynthMomentum::default();

    // Sentiment trend: last 14 rows from vibe_scores; also capture the latest row's felt-read.
    // sentiment is int2 → scan i16, widened to i32/f64 below (matches Go scanning into `int`).
    let sent_rows: Vec<(i16, String)> = sqlx::query_as(
        r#"
        SELECT sentiment, COALESCE(prompt, '') FROM vibe_scores
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
          AND sentiment IS NOT NULL
        ORDER BY generated_at DESC
        LIMIT 14
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load sentiment trend {entity_type}/{entity_id}"))?;

    let mut sent_scores: Vec<f64> = Vec::with_capacity(sent_rows.len());
    for (idx, (v, p)) in sent_rows.into_iter().enumerate() {
        if idx == 0 {
            m.latest_vibe_prompt = p; // first row is the most recent (DESC)
        }
        sent_scores.push(v as f64);
    }
    if !sent_scores.is_empty() {
        m.latest_sentiment = Some(sent_scores[0] as i32);
        // Reverse to chronological order for the slope (Go reverses in place after reading [0]).
        sent_scores.reverse();
        m.sentiment_slope = linear_slope(&sent_scores);
    }

    // Composite trend: last 10 events from the entity's event table. rating_composite_pct is
    // `numeric`; sqlx can't scan numeric → f64 without a decimal feature, so cast `::float8` in
    // SQL — VALUE-IDENTICAL to Go's pgx numeric→float64 (both yield the nearest double), and it
    // only feeds the bucketed trend_dir + a `%.1f` render, so the prompt is unaffected.
    let (comp_table, id_col) = match entity_type {
        "player" => ("event_box_scores", "player_id"),
        _ => ("event_team_stats", "team_id"),
    };
    // comp_table / id_col are stage-controlled literals (never user input) — no injection surface.
    let q = format!(
        r#"
        SELECT e.rating_composite_pct::float8 FROM public.{comp_table} e
            JOIN public.fixtures f ON f.id = e.fixture_id
            WHERE e.{id_col} = $1 AND e.sport = $2
              AND e.rating_composite_pct IS NOT NULL
              AND ($3::int IS NULL OR e.season = $3)
            ORDER BY f.start_time DESC
            LIMIT 10
        "#
    );
    let comp_rows: Vec<(f64,)> = sqlx::query_as(&q)
        .bind(entity_id)
        .bind(sport)
        .bind(season)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load composite trend {entity_type}/{entity_id}"))?;

    let mut comp_scores: Vec<f64> = comp_rows.into_iter().map(|(v,)| v).collect();
    if !comp_scores.is_empty() {
        m.latest_composite = Some(comp_scores[0]);
        comp_scores.reverse();
        m.composite_slope = linear_slope(&comp_scores);
    }

    Ok(m)
}

/// load_pillars resolves the season and loads all three pillars season-exact — the shared
/// front half of both `generate_sigil` (parity) and `SigilHandler::handle` (production).
async fn load_pillars(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    sport: &str, // upper-cased
) -> Result<(i32, Vec<SynthNarrative>, Option<SynthRating>, SynthMomentum)> {
    let season = resolve_season(&hx.pool, sport, None).await?;
    let narratives = load_narrative_pillar(&hx.pool, entity_type, entity_id, sport)
        .await
        .context("narrative pillar")?;
    let rating = load_rating_pillar(&hx.pool, entity_type, entity_id, sport, Some(season))
        .await
        .context("rating pillar")?;
    let momentum = load_momentum_pillar(&hx.pool, entity_type, entity_id, sport, Some(season))
        .await
        .context("momentum pillar")?;
    Ok((season, narratives, rating, momentum))
}

// ---------------------------------------------------------------------------
// Deterministic trend math — mirrors linearSlope + trendDir (sigil.go).
// ---------------------------------------------------------------------------

/// linear_slope computes the slope of a simple OLS regression on the series [0..N-1] → values.
/// Positive = trending up. Mirrors `linearSlope` exactly (same accumulation order, same
/// near-singular guard), so the f64 result is bit-identical to Go; only its trend_dir bucket
/// reaches the prompt, so even FP noise could not move the bytes.
fn linear_slope(vals: &[f64]) -> f64 {
    let n = vals.len() as f64;
    if vals.len() < 2 {
        return 0.0;
    }
    let (mut sum_x, mut sum_y, mut sum_xy, mut sum_xx) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for (i, v) in vals.iter().enumerate() {
        let x = i as f64;
        sum_x += x;
        sum_y += *v;
        sum_xy += x * *v;
        sum_xx += x * x;
    }
    let denom = n * sum_xx - sum_x * sum_x;
    if denom.abs() < 1e-9 {
        return 0.0;
    }
    (n * sum_xy - sum_x * sum_y) / denom
}

/// trend_dir buckets a slope into the prompt's trend phrase. Mirrors `trendDir` (same
/// thresholds, same evaluation order).
fn trend_dir(slope: f64) -> &'static str {
    if slope > 1.5 {
        "trending up strongly"
    } else if slope > 0.3 {
        "trending up"
    } else if slope < -1.5 {
        "trending down strongly"
    } else if slope < -0.3 {
        "trending down"
    } else {
        "steady"
    }
}

/// round1 rounds to one decimal place. Mirrors `round1` (`math.Round(x*10)/10`); Rust's
/// `f64::round` rounds half away from zero, like `math.Round`.
fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

// ---------------------------------------------------------------------------
// Input components + hash — the debounce key (Provenance.input_hash).
//
// Reproduces Go's `buildSynthesisInputComponents` + `hashComponents`: the canonical JSON is
// BYTE-IDENTICAL to `json.Marshal(map[string]any{...})` (sorted keys, HTML-escaped strings,
// Go's shortest float form), so its SHA-256 128-bit hex prefix equals Go's `input_hash`. This
// is the strict 4th parity axis AND keeps the cutover clean (no spurious regens vs Go rows).
// ---------------------------------------------------------------------------

/// build_synthesis_input_components returns the canonical input-components JSON — the exact
/// bytes Go's `json.Marshal(orEmptyMap(ic))` produces for the same pillars. Mirrors
/// `buildSynthesisInputComponents`: `narrative_titles` is ALWAYS present (even `[]`); the rest
/// are conditional. Keys are emitted in sorted order (Go marshals maps with sorted keys).
pub fn build_synthesis_input_components(
    narratives: &[SynthNarrative],
    rating: Option<&SynthRating>,
    mom: &SynthMomentum,
) -> String {
    let mut pairs: Vec<(&'static str, String)> = Vec::new();

    // narrative_titles — sorted titles, always present (Go: out["narrative_titles"] = titles).
    let mut titles: Vec<String> = narratives.iter().map(|n| n.title.clone()).collect();
    titles.sort(); // sort.Strings: byte-wise lexicographic == Rust str Ord for valid UTF-8
    let mut titles_json = String::from("[");
    for (i, t) in titles.iter().enumerate() {
        if i > 0 {
            titles_json.push(',');
        }
        titles_json.push_str(&go_json_string(t));
    }
    titles_json.push(']');
    pairs.push(("narrative_titles", titles_json));

    if let Some(r) = rating {
        pairs.push(("divined_peak", go_json_string(&r.divined_peak)));
        pairs.push(("notability", r.notability.to_string()));
    }
    if let Some(s) = mom.latest_sentiment {
        pairs.push(("latest_sentiment", s.to_string()));
    }
    if let Some(c) = mom.latest_composite {
        pairs.push(("latest_composite", go_json_float(round1(c))));
    }
    if !mom.latest_vibe_prompt.is_empty() {
        pairs.push(("latest_vibe_prompt", go_json_string(&mom.latest_vibe_prompt)));
    }

    pairs.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = String::from("{");
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&go_json_string(k));
        out.push(':');
        out.push_str(v);
    }
    out.push('}');
    out
}

/// hash_components is the stable hash of the canonical components JSON — the debounce signal.
/// Mirrors `hashComponents`: SHA-256, then the lowercase hex of the first 16 bytes (128-bit
/// prefix is ample for a change signal).
pub fn hash_components(canonical_json: &str) -> String {
    let digest = Sha256::digest(canonical_json.as_bytes());
    hex::encode(&digest[..16])
}

/// go_json_string quotes + escapes a string EXACTLY as Go's `encoding/json` does by default
/// (HTMLEscape on): `"` `\` and the control chars `\n` `\r` `\t` get short escapes, every
/// other byte < 0x20 becomes `\u00XX` (lowercase), `<` `>` `&` become `</3e/26`, and
/// U+2028 / U+2029 become ` /9`. Everything else passes through as UTF-8. This is what
/// makes the SHA-256 over the components match Go's `input_hash`.
fn go_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// go_json_float renders an f64 EXACTLY as Go's `encoding/json` does for our domain. Go uses
/// `strconv.AppendFloat(f, 'f', -1, 64)` for |f| in [1e-6, 1e21) — the shortest positional
/// form with NO trailing ".0". Our inputs are `round1` of a 0..100 percentile, so Rust's f64
/// `Display` (also shortest-positional, also no ".0" — that is the Debug form) matches Go
/// byte-for-byte. (serde_json would emit "85.0"; this deliberately does not.)
fn go_json_float(f: f64) -> String {
    format!("{f}")
}

// ---------------------------------------------------------------------------
// Prompt assembly — must be byte-identical to buildSynthesisPrompt.
// ---------------------------------------------------------------------------

/// build_synthesis_prompt assembles the user prompt, byte-for-byte the same as
/// `buildSynthesisPrompt` in sigil.go. `sport_raw` is the original-case value the Go prompt
/// uses (`req.Sport`); `entity_type` is used RAW (no title-casing, unlike vibe).
pub fn build_synthesis_prompt(
    entity_type: &str,
    entity_name: &str,
    sport_raw: &str,
    narratives: &[SynthNarrative],
    rating: Option<&SynthRating>,
    mom: &SynthMomentum,
) -> String {
    let mut b = String::new();

    // header = "<Sport> <entityType>" (raw entity_type), e.g. "NBA player".
    b.push_str(&format!("Entity: {entity_name} ({sport_raw} {entity_type})\n"));

    // P1 — News narrative
    if !narratives.is_empty() {
        b.push_str("\n=== NEWS NARRATIVE ===\n");
        for n in narratives {
            b.push_str(&format!("[impact {:.0}] {}\n{}\n\n", n.impact, n.title, n.body));
        }
    } else {
        b.push_str("\n=== NEWS NARRATIVE ===\n(no recent narratives)\n");
    }

    // P2 — Rating identity (the stat end product)
    b.push_str("\n=== PEAK IDENTITY (how the entity performs, not how well) ===\n");
    if let Some(r) = rating {
        if !r.divined_peak.is_empty() {
            b.push_str(&format!(
                "Peak: {} (notability {}/100)\n",
                r.divined_peak, r.notability
            ));
        }
        if !r.body.is_empty() {
            b.push_str(&r.body);
            b.push('\n');
        }
    } else {
        b.push_str("(no stat commentary available)\n");
    }

    // P3 — Momentum
    b.push_str("\n=== MOMENTUM ===\n");
    if let Some(s) = mom.latest_sentiment {
        let dir = trend_dir(mom.sentiment_slope);
        b.push_str(&format!("News sentiment: {s}/100 ({dir})\n"));
    }
    if !mom.latest_vibe_prompt.is_empty() {
        b.push_str(&format!("Vibe (the felt read): {}\n", mom.latest_vibe_prompt));
    }
    if let Some(c) = mom.latest_composite {
        let dir = trend_dir(mom.composite_slope);
        b.push_str(&format!("Composite rating: {c:.1}/100 ({dir})\n"));
    }
    if mom.latest_sentiment.is_none() && mom.latest_composite.is_none() {
        b.push_str("(no momentum data)\n");
    }

    b.push_str("\nRespond now.");
    b
}

// ---------------------------------------------------------------------------
// Output parsing — mirrors parseSynthesisResponse.
// ---------------------------------------------------------------------------

/// parse_synthesis_response extracts SCORE and BLURB from the model's two-line reply. Mirrors
/// `parseSynthesisResponse`: case-sensitive `"SCORE: "` / `"BLURB: "` prefixes (note the
/// space), the score clamped 1-100 only when the whole value parses, and blurb continuation
/// lines absorbed. `score == 0` means no parseable SCORE line (the caller treats it as a
/// failure — there is NO first-integer fallback, unlike vibe).
pub fn parse_synthesis_response(raw: &str) -> (i32, String) {
    let mut score: i32 = 0;
    let mut blurb = String::new();
    let lines: Vec<&str> = raw.trim().split('\n').collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("SCORE: ") {
            // strconv.Atoi parses the WHOLE value; a trailing non-digit ⇒ no update (score 0).
            if let Ok(n) = rest.trim().parse::<i64>() {
                score = n.clamp(1, 100) as i32;
            }
        } else if let Some(rest) = trimmed.strip_prefix("BLURB: ") {
            blurb = rest.trim().to_string();
            for extra in &lines[i + 1..] {
                let e = extra.trim();
                if !e.is_empty() {
                    blurb.push(' ');
                    blurb.push_str(e);
                }
            }
            break;
        }
    }
    (score, blurb)
}

/// SigilParser is the sigil stage's `Parser` plug-in: it wraps `parse_synthesis_response`
/// behind the capability library's `Parser<T>` seam. It never returns the fail-closed
/// `Ok(None)` — sigil's only fail-closed path is the pre-model no-pillar marker; an
/// unparseable reply (no SCORE line ⇒ score 0) is a genuine failure → `Err` → the work item
/// backs off, exactly as sigil.go's `if score == 0 { return error }`.
pub struct SigilParser;

impl Parser<SigilReply> for SigilParser {
    fn parse(&self, raw: &str) -> Result<Option<SigilReply>> {
        let (score, blurb) = parse_synthesis_response(raw);
        if score == 0 {
            bail!(
                "synthesis: could not parse score from response (raw={:?})",
                truncate(raw, 200)
            );
        }
        Ok(Some(SigilReply { score, blurb }))
    }
}

// ---------------------------------------------------------------------------
// The core generate + the production handler.
// ---------------------------------------------------------------------------

/// generate_sigil runs the full sigil derivation for one entity at the given temperature and
/// returns the un-persisted result — the L3 composition `read 3 pillars + route(StatsLogic) +
/// extract(SigilParser)`. Shared by the parity harness (temp 0 → writes the shadow table). It
/// does NOT debounce or persist (the parity harness always dumps); the production handler adds
/// the SkipUnchanged debounce + the typed persist. Mirrors `SigilGenerator.Generate` minus the
/// SkipUnchanged/persist/previous-score steps the parity path intentionally skips.
pub async fn generate_sigil(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport_raw: &str,
    temperature: f64,
) -> Result<SigilOutput> {
    if entity_id <= 0 || entity_name.is_empty() || sport_raw.is_empty() || entity_type.is_empty() {
        bail!("sigil: entity context incomplete");
    }
    // Reads use the upper-cased sport; the prompt uses the original-case value (req.Sport).
    let sport = sport_raw.to_uppercase();

    let (season, narratives, rating, momentum) =
        load_pillars(hx, entity_type, entity_id, &sport).await?;

    // No-pillar path: persist a marker (handled by the caller) without a model call. The
    // marker's model_version is the role's configured model (no response to echo).
    if narratives.is_empty() && rating.is_none() && momentum.empty() {
        return Ok(SigilOutput {
            score: None,
            blurb: None,
            season,
            input_components_json: "{}".to_string(),
            input_hash: None,
            model: hx.router.for_role(Role::StatsLogic).model().to_string(),
            prompt_version: SIGIL_PROMPT_VERSION,
            built_prompt: None,
            request_body: None,
            skipped_no_pillars: true,
        });
    }

    let input_components_json =
        build_synthesis_input_components(&narratives, rating.as_ref(), &momentum);
    let input_hash = hash_components(&input_components_json);

    let prompt =
        build_synthesis_prompt(entity_type, entity_name, sport_raw, &narratives, rating.as_ref(), &momentum);
    let opts = GenerateOptions {
        system: Some(SIGIL_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict: SIGIL_NUM_PREDICT,
        json_mode: false,
    };

    // sigil = route(StatsLogic) + extract(SigilParser). The fail-closed contract lives in the
    // parser (an unparseable reply → Err → item backs off); `extract` records the exact wire body.
    let extracted = hx
        .extract(Role::StatsLogic, &prompt, &opts, &SigilParser)
        .await?;
    let reply = extracted
        .value
        .ok_or_else(|| anyhow!("sigil: parser returned no value"))?;

    Ok(SigilOutput {
        score: Some(reply.score),
        blurb: Some(reply.blurb),
        season,
        input_components_json,
        input_hash: Some(input_hash),
        model: extracted.model,
        prompt_version: SIGIL_PROMPT_VERSION,
        built_prompt: Some(extracted.built_prompt),
        request_body: Some(extracted.request_body),
        skipped_no_pillars: false,
    })
}

/// persist_to_sigil_synthesis writes one row to the LIVE sigil_synthesis table — both the
/// scored row and the no-pillar NULL marker, which differ only in the bound values. Mirrors
/// `SigilGenerator.persist`: trigger_type 'periodic', trigger_payload `{}` (marshal of an
/// empty trigger map — NOT vibe's JSON null), empty blurb stored as "" for a scored row, NULL
/// for a marker. The moat fields route through the shared `Provenance` envelope; the typed
/// INSERT stays the stage's own (Postgres-as-serializer).
async fn persist_to_sigil_synthesis(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    season: i32,
    out: &SigilOutput,
    previous_score: Option<i16>,
) -> Result<()> {
    let prov = out.provenance();
    let score: Option<i16> = out.score.map(|n| n as i16);
    sqlx::query(
        r#"
        INSERT INTO sigil_synthesis (
            entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            score, previous_score, blurb, input_components, input_hash,
            model_version, prompt_version
        ) VALUES ($1,$2,$3,$4,'periodic','{}'::jsonb, $5,$6,$7,$8::jsonb,$9, $10,$11)
        "#,
    )
    .bind(&item.entity_type)
    .bind(item.entity_id)
    .bind(sport)
    .bind(season)
    .bind(score)
    .bind(previous_score)
    .bind(out.blurb.as_deref())
    .bind(out.input_components_json.as_str())
    .bind(prov.input_hash.as_deref())
    .bind(prov.model_version.as_str())
    .bind(prov.prompt_version)
    .execute(pool)
    .await
    .context("persist sigil")?;
    Ok(())
}

/// last_score returns the previous_score baseline for the delta display: the entity-season's
/// LATEST generation's score, or 0 when there is none or the latest is a marker (score NULL).
/// Mirrors `SigilGenerator.lastScore` (Session 11 / F-023 canonical latest-generation rule).
async fn last_score(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: i32,
) -> Result<i32> {
    // Nullable column → Option<Option<i16>>: no row OR a marker's NULL score both ⇒ 0.
    let score: Option<Option<i16>> = sqlx::query_scalar(
        r#"
        SELECT score FROM sigil_synthesis
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4
        ORDER BY generated_at DESC
        LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("last score {entity_type}/{entity_id}"))?;
    Ok(score.flatten().map(|v| v as i32).unwrap_or(0))
}

/// SigilHandler drains the durable `sigil` stage — the terminal convergence. It reads the
/// three pillars season-exact, SKIPS the Gemma call when the pillar hash is unchanged
/// (`debounce_unchanged`), else synthesizes and persists to sigil_synthesis. Unlike vibe it
/// enqueues nothing downstream. Mirrors `drainSigil` (current-season, SkipUnchanged=true).
/// Registered in main.rs for the per-stage cutover; the parity harness reuses the loaders +
/// `generate_sigil` core but writes the shadow table.
pub struct SigilHandler;

impl SigilHandler {
    pub fn new() -> Self {
        SigilHandler
    }
}

impl Default for SigilHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for SigilHandler {
    fn stage(&self) -> Stage {
        Stage::Sigil
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        // nameOf: the name lookup uses the queue's raw sport value (drainSigil → corpus lookup).
        let name = crate::vibe::lookup_entity_name(
            &hx.pool,
            &item.entity_type,
            item.entity_id,
            &item.sport,
        )
        .await?;

        let sport = item.sport.to_uppercase();
        let (season, narratives, rating, momentum) =
            load_pillars(hx, &item.entity_type, item.entity_id, &sport).await?;

        // No-pillar marker (no model call).
        if narratives.is_empty() && rating.is_none() && momentum.empty() {
            let out = SigilOutput {
                score: None,
                blurb: None,
                season,
                input_components_json: "{}".to_string(),
                input_hash: None,
                model: hx.router.for_role(Role::StatsLogic).model().to_string(),
                prompt_version: SIGIL_PROMPT_VERSION,
                built_prompt: None,
                request_body: None,
                skipped_no_pillars: true,
            };
            return persist_to_sigil_synthesis(&hx.pool, item, &sport, season, &out, None).await;
        }

        // SkipUnchanged debounce (drainSigil sets SkipUnchanged=true): skip the Gemma call when
        // the pillar input hash matches the entity-season's latest synthesis. This is the first
        // real consumer of the Persist `debounce_unchanged` primitive.
        let input_components_json =
            build_synthesis_input_components(&narratives, rating.as_ref(), &momentum);
        let input_hash = hash_components(&input_components_json);
        let key = EntityKey {
            entity_type: item.entity_type.clone(),
            entity_id: item.entity_id,
            sport: sport.clone(),
            season: Some(season),
        };
        if hx
            .debounce_unchanged("sigil_synthesis", &key, &input_hash)
            .await?
        {
            return Ok(()); // unchanged → cheap no-op (no model call, no persist)
        }

        let prev = last_score(&hx.pool, &item.entity_type, item.entity_id, &sport, season).await?;

        let prompt = build_synthesis_prompt(
            &item.entity_type,
            &name,
            &item.sport,
            &narratives,
            rating.as_ref(),
            &momentum,
        );
        let opts = GenerateOptions {
            system: Some(SIGIL_SYSTEM_PROMPT.to_string()),
            temperature: Some(SIGIL_TEMPERATURE),
            num_predict: SIGIL_NUM_PREDICT,
            json_mode: false,
        };
        let extracted = hx
            .extract(Role::StatsLogic, &prompt, &opts, &SigilParser)
            .await?;
        let reply = extracted
            .value
            .ok_or_else(|| anyhow!("sigil: parser returned no value"))?;

        let out = SigilOutput {
            score: Some(reply.score),
            blurb: Some(reply.blurb),
            season,
            input_components_json,
            input_hash: Some(input_hash),
            model: extracted.model,
            prompt_version: SIGIL_PROMPT_VERSION,
            built_prompt: Some(extracted.built_prompt),
            request_body: Some(extracted.request_body),
            skipped_no_pillars: false,
        };
        let prev_score: Option<i16> = if prev > 0 { Some(prev as i16) } else { None };
        persist_to_sigil_synthesis(&hx.pool, item, &sport, season, &out, prev_score).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_line_reply() {
        let (score, blurb) =
            parse_synthesis_response("SCORE: 73\nBLURB: A quiet, season-long ascent.");
        assert_eq!(score, 73);
        assert_eq!(blurb, "A quiet, season-long ascent.");
    }

    #[test]
    fn clamps_and_absorbs_trailing_blurb_lines() {
        let (score, blurb) = parse_synthesis_response("SCORE: 250\nBLURB: line one\nline two");
        assert_eq!(score, 100);
        assert_eq!(blurb, "line one line two");
    }

    #[test]
    fn score_zero_when_no_score_line() {
        // No "SCORE: " prefix ⇒ score 0 (the caller fails the item — no first-integer fallback).
        let (score, _) = parse_synthesis_response("the sigil feels like a 64 today");
        assert_eq!(score, 0);
    }

    #[test]
    fn sigil_parser_wraps_valid_reply_as_some() {
        let reply = SigilParser
            .parse("SCORE: 73\nBLURB: A quiet ascent.")
            .unwrap()
            .expect("a valid reply is Some, never the fail-closed None");
        assert_eq!(reply.score, 73);
        assert_eq!(reply.blurb, "A quiet ascent.");
    }

    #[test]
    fn sigil_parser_errors_without_score() {
        // score 0 ⇒ Err (retry/back-off), NOT Ok(None): sigil's only fail-closed path is the
        // pre-model no-pillar marker, never an unparseable reply.
        assert!(SigilParser.parse("no score here").is_err());
    }

    #[test]
    fn trend_dir_buckets() {
        assert_eq!(trend_dir(2.0), "trending up strongly");
        assert_eq!(trend_dir(0.5), "trending up");
        assert_eq!(trend_dir(0.0), "steady");
        assert_eq!(trend_dir(-0.5), "trending down");
        assert_eq!(trend_dir(-2.0), "trending down strongly");
        // Boundary: exactly 0.3 is NOT > 0.3 → steady (mirrors Go's strict `>`).
        assert_eq!(trend_dir(0.3), "steady");
    }

    #[test]
    fn linear_slope_of_a_rising_series() {
        // A perfectly linear +5/step series ⇒ slope 5.
        assert!((linear_slope(&[10.0, 15.0, 20.0, 25.0]) - 5.0).abs() < 1e-9);
        // Fewer than two points ⇒ 0.
        assert_eq!(linear_slope(&[42.0]), 0.0);
        assert_eq!(linear_slope(&[]), 0.0);
    }

    #[test]
    fn round1_matches_go() {
        assert_eq!(round1(73.04), 73.0);
        assert_eq!(round1(73.05), 73.1); // half away from zero
        assert_eq!(round1(73.0), 73.0);
    }

    #[test]
    fn go_json_float_omits_trailing_zero() {
        // The serde_json divergence trap: Go (and this) emit "73", not "73.0".
        assert_eq!(go_json_float(73.0), "73");
        assert_eq!(go_json_float(72.4), "72.4");
        assert_eq!(go_json_float(0.0), "0");
    }

    #[test]
    fn go_json_string_html_escapes_like_go() {
        // & < > are HTML-escaped (Go default); " and control chars get the JSON escapes.
        // These escaped forms are what makes input_hash match Go. Expected values use a
        // runtime backslash (bs) so the source carries no literal backslash-u token.
        let bs = '\\';
        assert_eq!(go_json_string("A & B"), format!("\"A {bs}u0026 B\""));
        assert_eq!(go_json_string("<x>"), format!("\"{bs}u003cx{bs}u003e\""));
        assert_eq!(go_json_string("a\"b\nc"), r#""a\"b\nc""#);
        assert_eq!(go_json_string("plain"), r#""plain""#);
    }

    #[test]
    fn input_components_are_byte_identical_to_go_marshal() {
        // Validates sorted keys + HTML escaping + Go float form + int form together — the
        // canonical JSON whose SHA-256 is the input_hash. Compare against the exact bytes Go's
        // json.Marshal would emit for the same map.
        let narratives = vec![
            SynthNarrative { title: "B & C".into(), body: "x".into(), impact: 5.0 },
            SynthNarrative { title: "Alpha".into(), body: "y".into(), impact: 3.0 },
        ];
        let rating = SynthRating { divined_peak: "Rim Protector".into(), body: "z".into(), notability: 88 };
        let mom = SynthMomentum {
            sentiment_slope: 1.0,
            composite_slope: 0.0,
            latest_sentiment: Some(60),
            latest_composite: Some(73.0), // round1 → 73 → "73" (no ".0")
            latest_vibe_prompt: "Quietly surging".into(),
        };
        let got = build_synthesis_input_components(&narratives, Some(&rating), &mom);
        // "B & C"'s ampersand is HTML-escaped (the backslash-u form), exactly as Go's
        // json.Marshal emits it. Built via format! with a runtime backslash (bs) so the
        // source carries no literal backslash-u token (the editor would decode it).
        let bs = '\\';
        let want = format!(
            r#"{{"divined_peak":"Rim Protector","latest_composite":73,"latest_sentiment":60,"latest_vibe_prompt":"Quietly surging","narrative_titles":["Alpha","B {bs}u0026 C"],"notability":88}}"#
        );
        assert_eq!(got, want);
    }

    #[test]
    fn input_components_narrative_titles_always_present() {
        // Rating-only entity: narrative_titles is still present as [] (Go adds it unconditionally).
        let rating = SynthRating { divined_peak: "Spacer".into(), body: "b".into(), notability: 40 };
        let got = build_synthesis_input_components(&[], Some(&rating), &SynthMomentum::default());
        assert_eq!(
            got,
            r#"{"divined_peak":"Spacer","narrative_titles":[],"notability":40}"#
        );
    }

    #[test]
    fn hash_is_stable_and_128_bit_hex() {
        let json = r#"{"narrative_titles":[],"notability":40}"#;
        let h = hash_components(json);
        assert_eq!(h.len(), 32); // 16 bytes → 32 lowercase hex chars
        assert_eq!(h, hash_components(json)); // deterministic
        assert!(h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn builds_prompt_raw_entity_type_and_sections() {
        // entity_type is raw ("player", not "Player"); sport uses the passed (raw) case.
        let narratives = vec![SynthNarrative { title: "Trade buzz".into(), body: "details".into(), impact: 7.0 }];
        let mom = SynthMomentum {
            sentiment_slope: 0.5,
            composite_slope: 0.0,
            latest_sentiment: Some(62),
            latest_composite: None,
            latest_vibe_prompt: "On the rise".into(),
        };
        let p = build_synthesis_prompt("player", "Test Player", "NBA", &narratives, None, &mom);
        assert_eq!(
            p,
            "Entity: Test Player (NBA player)\n\n=== NEWS NARRATIVE ===\n[impact 7] Trade buzz\ndetails\n\n\n=== PEAK IDENTITY (how the entity performs, not how well) ===\n(no stat commentary available)\n\n=== MOMENTUM ===\nNews sentiment: 62/100 (trending up)\nVibe (the felt read): On the rise\n\nRespond now."
        );
    }

    #[test]
    fn no_momentum_data_line_when_both_absent() {
        let p = build_synthesis_prompt("team", "Test Team", "NFL", &[], None, &SynthMomentum::default());
        assert_eq!(
            p,
            "Entity: Test Team (NFL team)\n\n=== NEWS NARRATIVE ===\n(no recent narratives)\n\n=== PEAK IDENTITY (how the entity performs, not how well) ===\n(no stat commentary available)\n\n=== MOMENTUM ===\n(no momentum data)\n\nRespond now."
        );
    }
}
