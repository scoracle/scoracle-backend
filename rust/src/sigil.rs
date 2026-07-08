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
//! local model call when the three pillars hash identically to the entity-season's latest synthesis.
//! Sigil is the TERMINAL stage — unlike vibe it enqueues nothing downstream.

use crate::harness::{EntityKey, Harness, Parser, Provenance};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::util::{go_json_float, go_json_string, hash_components, round1, truncate};
use crate::work::{Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use sqlx::PgPool;

/// Prompt version for the Sigil synthesis contract.
pub const SIGIL_PROMPT_VERSION: &str = "s7";

/// Production synthesis temperature (sigil.go uses 0.6). The parity harness overrides this
/// with an explicit 0.
pub const SIGIL_TEMPERATURE: f64 = 0.6;

/// Token cap for the SCORE + short BLURB answer.
pub const SIGIL_NUM_PREDICT: i32 = 512;

/// System prompt for the Sigil synthesis contract.
pub const SIGIL_SYSTEM_PROMPT: &str = r#"Task: synthesize News Narrative, Rating Identity, and Momentum into one Sigil score and blurb.

Voice: direct, sports-literate, grounded. No purple prose, no headline language, no invented facts.

SCORE (1-100):
- 1 = deeply troubled or in freefall.
- 50 = steady or genuinely mixed.
- 100 = dominant or surging.
- Slow-moving and season-aware. Do not overreact to one game or one weak signal.
- Use Momentum to capture recent trajectory when it conflicts with season-long profile.

BLURB:
- About two sentences; use a third only when several major signals converge.
- Include: what the entity is, the defining news storyline, and current trajectory.
- Do not recite percentiles or per-x details; Rating already carries that.
- Name the real storyline, but do not catalogue every rumor or item.

Reply with exactly these two lines:
SCORE: <integer 1-100>
BLURB: <the story>"#;

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
    pub trajectory: String,
}

/// The stat-identity pillar (P2). Mirrors `synthRating`. `None` (suppressed) when there is no
/// commentary row, or when the latest generation is a no-stats marker (`body` NULL).
#[derive(Clone, Debug)]
pub struct SynthRating {
    pub divined_peak: String,
    pub body: String,
    pub notability: i32,
    pub peak_trajectory: String,
    pub peak_trajectory_label: String,
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
    let rows: Vec<(String, String, i32, String)> = sqlx::query_as(
        r#"
        SELECT narrative_title, body, COALESCE(impact, 0), COALESCE(trajectory, 'developing_story')
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
        .map(|(title, body, impact, trajectory)| SynthNarrative {
            title,
            body,
            impact: impact as f64,
            trajectory,
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
    let row: Option<(String, Option<String>, i32, String, String)> = sqlx::query_as(
        r#"
        SELECT COALESCE(divined_peak, ''), body, COALESCE(notability, 0),
               COALESCE(peak_trajectory, 'steady'), COALESCE(peak_trajectory_label, '')
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
        None => Ok(None),                     // pgx.ErrNoRows → pillar absent
        Some((_, None, _, _, _)) => Ok(None), // latest generation is a marker (body NULL) → suppressed
        Some((divined_peak, Some(body), notability, peak_trajectory, peak_trajectory_label)) => {
            Ok(Some(SynthRating {
                divined_peak,
                body,
                notability,
                peak_trajectory,
                peak_trajectory_label,
            }))
        }
    }
}

/// load_momentum_pillar (P3) computes the trajectory signal Sigil needs: the sentiment trend
/// (last 14 vibe_scores rows) plus the composite trend (last 10 event composite scores), capturing
/// the latest of each plus the latest felt-read prompt. Mirrors `loadMomentumPillar`.
///
/// Product note: Momentum remains its own endpoint/product. This pillar is the Sigil-facing read of
/// the same trajectory so a strong season-long stats profile can still be tempered by recent form,
/// sentiment collapse, injuries, coaching churn, or other directional changes.
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
///
/// DO NOT merge with `rating::linear_slope` — different accumulation order (this sum form vs
/// rating's mean-centered form), each claims Go bit-parity. See plan A6 / E3: consolidating
/// could flip boundary values and destabilize rating's `input_hash` debounce.
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

/// momentum_score turns the trend pillar into a single trajectory number for Sigil and the
/// Momentum product. It is directional force, not entity quality: 50 is flat, above 50 is rising,
/// below 50 is sliding. Latest values still render separately; this score is driven by slope.
fn momentum_score(mom: &SynthMomentum) -> Option<i32> {
    if mom.empty() {
        return None;
    }
    let mut score = 50.0_f64;
    if mom.latest_sentiment.is_some() {
        score += (mom.sentiment_slope * 12.5).clamp(-25.0, 25.0);
    }
    if mom.latest_composite.is_some() {
        score += (mom.composite_slope * 12.5).clamp(-25.0, 25.0);
    }
    Some(score.clamp(1.0, 100.0).round() as i32)
}

fn momentum_score_label(score: i32) -> &'static str {
    if score >= 70 {
        "surging"
    } else if score >= 56 {
        "rising"
    } else if score <= 30 {
        "falling"
    } else if score <= 44 {
        "sliding"
    } else {
        "steady"
    }
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

    let mut trajectory_pairs: Vec<String> = narratives
        .iter()
        .map(|n| format!("{}:{}", n.title, n.trajectory))
        .collect();
    trajectory_pairs.sort();
    let mut trajectory_json = String::from("[");
    for (i, t) in trajectory_pairs.iter().enumerate() {
        if i > 0 {
            trajectory_json.push(',');
        }
        trajectory_json.push_str(&go_json_string(t));
    }
    trajectory_json.push(']');
    pairs.push(("narrative_trajectories", trajectory_json));

    if let Some(r) = rating {
        pairs.push(("divined_peak", go_json_string(&r.divined_peak)));
        pairs.push(("notability", r.notability.to_string()));
        pairs.push(("peak_trajectory", go_json_string(&r.peak_trajectory)));
        if !r.peak_trajectory_label.is_empty() {
            pairs.push((
                "peak_trajectory_label",
                go_json_string(&r.peak_trajectory_label),
            ));
        }
    }
    if let Some(s) = mom.latest_sentiment {
        pairs.push(("latest_sentiment", s.to_string()));
    }
    if let Some(c) = mom.latest_composite {
        pairs.push(("latest_composite", go_json_float(round1(c))));
    }
    if !mom.latest_vibe_prompt.is_empty() {
        pairs.push((
            "latest_vibe_prompt",
            go_json_string(&mom.latest_vibe_prompt),
        ));
    }
    if let Some(score) = momentum_score(mom) {
        pairs.push(("momentum_score", score.to_string()));
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

// hash_components + the Go-JSON leaf encoders (`go_json_string` / `go_json_float`) are
// single-homed in `crate::util` (single-homing landed in L12 for rating; sigil was the L3
// original home and kept its own copies to avoid perturbing the proven stage — the L12
// carry closed post-Step-3). The behavior is byte-identical; the existing shadow-table
// parity gate is the regression check.

// ---------------------------------------------------------------------------
// Prompt assembly.
// ---------------------------------------------------------------------------

/// build_synthesis_prompt assembles the user prompt. `sport_raw` is the original-case value used in
/// the prompt; `entity_type` is used RAW (no title-casing, unlike vibe).
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
    b.push_str(&format!(
        "Entity: {entity_name} ({sport_raw} {entity_type})\n"
    ));

    // P1 — News narrative
    if !narratives.is_empty() {
        b.push_str("\n=== NEWS NARRATIVE ===\n");
        for n in narratives {
            b.push_str(&format!(
                "[impact {:.0}, {}] {}\n{}\n\n",
                n.impact,
                narrative_trajectory_label(&n.trajectory),
                n.title,
                n.body
            ));
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
        if !r.peak_trajectory_label.is_empty() {
            b.push_str(&format!("Peak trajectory: {}\n", r.peak_trajectory_label));
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
    if let Some(score) = momentum_score(mom) {
        b.push_str(&format!(
            "Momentum score: {score}/100 ({})\n",
            momentum_score_label(score)
        ));
    }
    if let Some(s) = mom.latest_sentiment {
        let dir = trend_dir(mom.sentiment_slope);
        b.push_str(&format!("News sentiment: {s}/100 ({dir})\n"));
    }
    if !mom.latest_vibe_prompt.is_empty() {
        b.push_str(&format!(
            "Vibe (the felt read): {}\n",
            mom.latest_vibe_prompt
        ));
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

fn narrative_trajectory_label(raw: &str) -> &'static str {
    match raw {
        "heating_up" => "Heating up",
        "cooling_off" => "Cooling off",
        _ => "Developing story...",
    }
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

    let prompt = build_synthesis_prompt(
        entity_type,
        entity_name,
        sport_raw,
        &narratives,
        rating.as_ref(),
        &momentum,
    );
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
    let entity_id = item.entity_id_i32()?;
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
    .bind(entity_id)
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

/// SigilHandler drains the durable `sigil` stage — the terminal convergence. It reads the
/// three pillars season-exact, SKIPS the local model call when the pillar hash is unchanged
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
        let entity_id = item.entity_id_i32()?;
        // nameOf: the name lookup uses the queue's raw sport value (drainSigil → corpus lookup).
        let name =
            crate::corpus::lookup_entity_name(&hx.pool, &item.entity_type, entity_id, &item.sport)
                .await?;

        let sport = item.sport.to_uppercase();
        let (season, narratives, rating, momentum) =
            load_pillars(hx, &item.entity_type, entity_id, &sport).await?;

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

        // SkipUnchanged debounce (drainSigil sets SkipUnchanged=true): skip the local model call when
        // the pillar input hash matches the entity-season's latest synthesis. This is the first
        // real consumer of the Persist `debounce_unchanged` primitive.
        let input_components_json =
            build_synthesis_input_components(&narratives, rating.as_ref(), &momentum);
        let input_hash = hash_components(&input_components_json);
        let key = EntityKey {
            entity_type: item.entity_type.clone(),
            entity_id,
            sport: sport.clone(),
            season: Some(season),
        };
        // One round-trip to the entity-season's latest synthesis row for BOTH the debounce hash
        // and the previous-score baseline (plan A1 — was two identical latest-row queries).
        let (prev_score_raw, latest_hash) = hx.latest_with_hash("sigil_synthesis", &key).await?;
        if latest_hash.as_deref() == Some(input_hash.as_str()) {
            return Ok(()); // unchanged → cheap no-op (no model call, no persist)
        }
        let prev = prev_score_raw.map(|v| v as i32).unwrap_or(0);

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
    fn momentum_score_tracks_direction_not_quality() {
        let surging = SynthMomentum {
            sentiment_slope: 2.0,
            composite_slope: 2.0,
            latest_sentiment: Some(45),
            latest_composite: Some(48.0),
            latest_vibe_prompt: String::new(),
        };
        assert_eq!(momentum_score(&surging), Some(100));
        assert_eq!(momentum_score_label(100), "surging");

        let sliding = SynthMomentum {
            sentiment_slope: -2.0,
            composite_slope: -1.0,
            latest_sentiment: Some(90),
            latest_composite: Some(88.0),
            latest_vibe_prompt: String::new(),
        };
        assert_eq!(momentum_score(&sliding), Some(13));
        assert_eq!(momentum_score_label(13), "falling");

        assert_eq!(momentum_score(&SynthMomentum::default()), None);
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
    fn go_json_encoders_single_homed_in_util() {
        // The leaf encoders + hash now live in `crate::util` (single-homed post-L12); the
        // shadow-table parity gate is the regression check, and util.rs carries its own
        // pinning tests for them. Sigil re-pins the COMPOSITION here (see the next test).
    }

    #[test]
    fn input_components_are_byte_identical_to_go_marshal() {
        // Validates sorted keys + HTML escaping + Go float form + int form together — the
        // canonical JSON whose SHA-256 is the input_hash. Compare against the exact bytes Go's
        // json.Marshal would emit for the same map.
        let narratives = vec![
            SynthNarrative {
                title: "B & C".into(),
                body: "x".into(),
                impact: 5.0,
                trajectory: "heating_up".into(),
            },
            SynthNarrative {
                title: "Alpha".into(),
                body: "y".into(),
                impact: 3.0,
                trajectory: "developing_story".into(),
            },
        ];
        let rating = SynthRating {
            divined_peak: "Rim Protector".into(),
            body: "z".into(),
            notability: 88,
            peak_trajectory: "falling".into(),
            peak_trajectory_label: "Composite and PEAK z-scores trending down over recent games"
                .into(),
        };
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
            r#"{{"divined_peak":"Rim Protector","latest_composite":73,"latest_sentiment":60,"latest_vibe_prompt":"Quietly surging","momentum_score":63,"narrative_titles":["Alpha","B {bs}u0026 C"],"narrative_trajectories":["Alpha:developing_story","B {bs}u0026 C:heating_up"],"notability":88,"peak_trajectory":"falling","peak_trajectory_label":"Composite and PEAK z-scores trending down over recent games"}}"#
        );
        assert_eq!(got, want);
    }

    #[test]
    fn input_components_narrative_titles_always_present() {
        // Rating-only entity: narrative_titles is still present as [] (Go adds it unconditionally).
        let rating = SynthRating {
            divined_peak: "Spacer".into(),
            body: "b".into(),
            notability: 40,
            peak_trajectory: "steady".into(),
            peak_trajectory_label: String::new(),
        };
        let got = build_synthesis_input_components(&[], Some(&rating), &SynthMomentum::default());
        assert_eq!(
            got,
            r#"{"divined_peak":"Spacer","narrative_titles":[],"narrative_trajectories":[],"notability":40,"peak_trajectory":"steady"}"#
        );
    }

    #[test]
    fn builds_prompt_raw_entity_type_and_sections() {
        // entity_type is raw ("player", not "Player"); sport uses the passed (raw) case.
        let narratives = vec![SynthNarrative {
            title: "Trade buzz".into(),
            body: "details".into(),
            impact: 7.0,
            trajectory: "heating_up".into(),
        }];
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
            "Entity: Test Player (NBA player)\n\n=== NEWS NARRATIVE ===\n[impact 7, Heating up] Trade buzz\ndetails\n\n\n=== PEAK IDENTITY (how the entity performs, not how well) ===\n(no stat commentary available)\n\n=== MOMENTUM ===\nMomentum score: 56/100 (rising)\nNews sentiment: 62/100 (trending up)\nVibe (the felt read): On the rise\n\nRespond now."
        );
    }

    #[test]
    fn no_momentum_data_line_when_both_absent() {
        let p = build_synthesis_prompt(
            "team",
            "Test Team",
            "NFL",
            &[],
            None,
            &SynthMomentum::default(),
        );
        assert_eq!(
            p,
            "Entity: Test Team (NFL team)\n\n=== NEWS NARRATIVE ===\n(no recent narratives)\n\n=== PEAK IDENTITY (how the entity performs, not how well) ===\n(no stat commentary available)\n\n=== MOMENTUM ===\n(no momentum data)\n\nRespond now."
        );
    }
}
