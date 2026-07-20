//! Sigil stage — the L3 stage port: the crown convergence, re-expressed as a
//! composition of the capability library's primitives.
//!
//! Sigil = `read pillars + route(SynthesisLogic) + extract(SigilParser) + persist`, with a
//! `debounce_unchanged` gate on the pillar `input_hash`. This module began as a Go parity port, and
//! the deterministic plumbing still follows that shape. Wave 5 rebaselines the product contract:
//! the prompt now composes PEAK, Vibe, Momentum, and current narratives as distinct pillars.
//! Phase 5.1 adds a fifth: the transfer-heat pillar (the transfer lens the trigger gate already
//! watches), so the synthesis can finally see the served rumors that can fire its own re-run.
//! Phase 5.2 feeds the previous Sigil (score + blurb) back into the prompt as continuity — a
//! prompt-only anchor, deliberately kept OUT of the `input_hash` (the score always moves, so
//! hashing it would self-trigger every re-run).
//! Phase 5.3 makes panel DISAGREEMENT a first-class output: the synthesis reply gained three
//! OPTIONAL lines (`CONVERGENCE:` / `DISAGREEMENT:` / `WHY_NOW:`) alongside the required
//! SCORE + BLURB, persisted to the additive nullable `convergence`/`disagreement`/`why_now`
//! columns (mig 143). They are model OUTPUTS, not inputs — the `input_hash` stays
//! pillar-inputs-only, so old rows stay valid and populate lazily on the next real re-synthesis.
//! The Go sources originally mirrored here:
//! `go/internal/ml/sigil.go` (Generate, the three pillar loaders, prompt, parse,
//! input-components/hash, persist, the SkipUnchanged gate); `go/internal/ml/rating.go`
//! (`hashComponents` / `round1`, shared package helpers); `go/internal/derive/derive.go`
//! (drainSigil: queue Item → SigilRequest, current-season + SkipUnchanged, the terminal stage).
//!
//! This is the first NEW derivation on the library (the primitives don't move): the first
//! `Role::SynthesisLogic` consumer, the first user of `Persist::debounce_unchanged`, and the first
//! user of the `Provenance.input_hash` envelope field — all three shipped real but unexercised
//! by vibe. Everything that can differ between the two implementations — the SQL reads, the
//! deterministic slope/trend math, the canonical input-components JSON (whose SHA-256 is the
//! `input_hash`), and the parse — lives here. See `src/bin/sigil_parity.rs` for the historical
//! harness and migration 107 for the shadow table.
//!
//! Fail-closed semantics reproduced verbatim: when an entity has NO narrative pillar AND no
//! rating pillar AND no vibe pillar AND no momentum pillar AND no transfer pillar, we skip the model
//! and persist a NULL-score/NULL-blurb
//! marker row (the read path returns "no synthesis yet"). The SkipUnchanged debounce skips the
//! local model call when the pillars hash identically to the entity-season's latest synthesis.
//! Since the Oracle lens (2026-07-12), Sigil is no longer terminal: every handle outcome —
//! scored, marker, AND debounce-skip — enqueues the `oracle` stage. The skip-path enqueue is
//! deliberate self-healing: the Oracle debounces on the consumed sigil generation itself, so a
//! previously lost hand-off catches up as a cheap no-op instead of staying lost.

use crate::corpus::{load_transfer_heat, write_heat_lines, HeatItem};
use crate::harness::{EntityKey, Harness, Parser, Provenance};
use crate::ledger::{insert_cognition_ledger_best_effort, CognitionLedgerEntry};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::trajectory::{trajectory_label, DEFAULT_TRAJECTORY};
use crate::util::{go_json_float, go_json_string, hash_components, round1, truncate};
use crate::work::{Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Row};

/// Prompt version for the Sigil synthesis contract. Bumped s10→s11 for the Phase 5.3 panel-output
/// contract: the reply now carries three OPTIONAL lines (CONVERGENCE / DISAGREEMENT / WHY_NOW)
/// after the required SCORE + BLURB. Provenance-only: those are model OUTPUTS, not pillar inputs,
/// so the `input_hash` is unchanged and nothing regenerates on the bump — only a real pillar
/// change flips the hash. (The output-contract version distinct from prompt_version is deferred to
/// the Phase 2 ledger; prompt_version s11 already marks rows generated under the new output shape.)
pub const SIGIL_PROMPT_VERSION: &str = "s15"; // s14: de-parrotable DISAGREEMENT rubric; s15: relational memory card (per-entity arc memory, mig 163 — junction rollout step 3)

/// Output contract captured separately in the Phase 2 diagnostic ledger.
pub const SIGIL_OUTPUT_CONTRACT_VERSION: &str = "sigil-panel-v1";

/// Production synthesis temperature (sigil.go uses 0.6). The parity harness overrides this
/// with an explicit 0.
pub const SIGIL_TEMPERATURE: f64 = 0.6;

/// Token cap for the SCORE + short BLURB answer.
pub const SIGIL_NUM_PREDICT: i32 = 512;

/// System prompt for the Sigil synthesis contract.
pub const SIGIL_SYSTEM_PROMPT: &str = r#"Task: synthesize PEAK scouting report, Vibe, Momentum, current narratives, and transfer heat into one Sigil score and blurb.

Voice: direct, sports-literate, grounded. No purple prose, no headline language, no invented facts.

SCORE (1-100):
- 1 = deeply troubled or in freefall.
- 50 = steady or genuinely mixed.
- 100 = dominant or surging.
- Slow-moving and season-aware. Do not overreact to one game or one weak signal.
- When a PREVIOUS SIGIL is shown, treat its score as your prior: move from it deliberately and hold steady unless the new signals justify a change. This is memory, not a reset.
- Use Momentum to capture recent trajectory when it conflicts with the PEAK report or Vibe.
- A credible, advanced transfer/trade situation is a real signal; weigh it by its stage and direction, not by rumor volume.

CONVERGENCE (1-100):
- How strongly the lenses agree. 100 = PEAK, narrative, vibe, momentum, and transfer all tell the same story; 50 = mixed; low = the lenses conflict.
- Judge agreement of DIRECTION, not raw scores: a steady 50 across every lens still converges.
- When a PILLAR AGREEMENT section is shown, it is computed, not an opinion: ground CONVERGENCE in it. Every DISAGREE line lowers convergence; if half or more of the pairs DISAGREE, convergence must be below 50.

DISAGREEMENT:
- One line naming, in your own words, which specific pillars conflict and how — name the actual pillars (PEAK strength, PEAK trajectory, Vibe, Momentum, narrative, transfer) from THIS entity's data, never boilerplate.
- When PILLAR AGREEMENT lists DISAGREE lines, name those conflicts — do not invent a conflict it does not list, and do not omit one it does.
- Omit this line entirely when the lenses agree. Do not invent a conflict.

WHY_NOW:
- One line on what moved recently to justify re-reading now — a new transfer stage, a breaking narrative, a rating swing.
- Omit this line entirely when nothing is genuinely fresh. Do not manufacture urgency.

BLURB:
- About two sentences; use a third only when several major signals converge.
- Include: what the entity is, the defining felt state, the PEAK context, and current trajectory.
- Do not recite percentiles or per-x details; PEAK already carries that.
- Name the real storyline — including a live transfer/trade situation when it is the story — but do not catalogue every rumor or item.

Reply with these lines. SCORE and BLURB are required; include CONVERGENCE, DISAGREEMENT, and WHY_NOW only when they apply (omit the whole line otherwise). Keep BLURB last:
SCORE: <integer 1-100>
CONVERGENCE: <integer 1-100>
DISAGREEMENT: <one line, or omit>
WHY_NOW: <one line, or omit>
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
    /// Corroboration + freshness (Phase 1) — PROMPT-ONLY: deliberately excluded from
    /// `build_synthesis_input_components`, so a storyline's age ticking over a day boundary
    /// never flips the debounce hash and regenerates an otherwise-unchanged Sigil.
    pub source_count: i32,
    pub source_age_days: Option<i32>,
}

/// The PEAK scouting-report pillar (P2). `None` (suppressed) when there is no
/// commentary row, or when the latest generation is a no-stats marker (`body` NULL).
#[derive(Clone, Debug)]
pub struct SynthRating {
    pub divined_peak: String,
    pub body: String,
    pub notability: i32,
    pub peak_trajectory: String,
    pub peak_trajectory_label: String,
}

/// The vibe pillar (P3): the latest felt-read product, distinct from the Momentum trajectory.
#[derive(Clone, Debug)]
pub struct SynthVibe {
    pub sentiment: i32,
    pub prompt: String,
}

/// The momentum pillar (P4): durable trajectory values from `momentum_scores`.
#[derive(Clone, Debug, Default)]
pub struct SynthMomentum {
    pub direction: Option<String>,
    pub blurb: Option<String>,
    pub input_hash: Option<String>,
    pub vibe_slope: Option<f64>,
    pub vibe_samples: i32,
    pub rating_slope: Option<f64>,
    pub rating_samples: i32,
    pub momentum_score: Option<f64>,
}

impl SynthMomentum {
    /// empty mirrors `synthMomentum.empty()`: no momentum signal at all.
    pub fn empty(&self) -> bool {
        self.direction.is_none()
            && self.blurb.is_none()
            && self.vibe_slope.is_none()
            && self.rating_slope.is_none()
            && self.momentum_score.is_none()
    }
}

/// The validated synthesis answer — the required SCORE (1-100) + BLURB, plus the OPTIONAL Phase 5.3
/// panel outputs. The sigil Extract output shape (the `T` in `Parser<T>` / `Extracted<T>`). The
/// three panel fields are `Option` because the model omits the whole line when it does not apply
/// (convergent lenses, nothing fresh) — a missing field persists as NULL, never a stage failure.
#[derive(Clone, Debug)]
pub struct SigilReply {
    pub score: i32,
    pub blurb: String,
    /// Phase 5.3: how strongly the lenses agree (1-100). `None` when the model omitted it.
    pub convergence: Option<i32>,
    /// Phase 5.3: one-line summary of where the rails diverge. `None`/omitted when they agree.
    pub disagreement: Option<String>,
    /// Phase 5.3: one-line breaking-news freshness note. `None`/omitted when nothing is fresh.
    pub why_now: Option<String>,
}

/// The result of running the sigil core for one entity, before persistence. Captures
/// the production row payload for `sigil_synthesis`; parity-only prompt/body
/// capture lives in `src/bin/sigil_parity.rs`.
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
    /// Phase 5.3 panel outputs — all `None` for the marker and whenever the model omitted the
    /// line. Persisted to the additive nullable columns (mig 143); NOT part of the `input_hash`.
    pub convergence: Option<i32>,
    pub disagreement: Option<String>,
    pub why_now: Option<String>,
    pub built_prompt: Option<String>,
    pub request_body: Option<serde_json::Value>,
    pub eval_count: Option<i32>,
    pub wall_ms: Option<u64>,
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
            trigger_payload: None,
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
    let rows: Vec<(String, String, i32, String, i32, Option<i32>)> = sqlx::query_as(
        r#"
        SELECT narrative_title, body, COALESCE(impact, 0), COALESCE(trajectory, $4),
               COALESCE(source_count, 0) AS source_count,
               EXTRACT(day FROM NOW() - source_latest_at)::int AS source_age_days
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
    .bind(DEFAULT_TRAJECTORY)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load narrative pillar {entity_type}/{entity_id}"))?;

    Ok(rows
        .into_iter()
        .map(
            |(title, body, impact, trajectory, source_count, source_age_days)| SynthNarrative {
                title,
                body,
                impact: impact as f64,
                trajectory,
                source_count,
                source_age_days,
            },
        )
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

/// load_vibe_pillar (P3) reads the latest Vibe felt-state product. A latest NULL-sentiment marker
/// suppresses the pillar instead of falling back to an older real Vibe.
pub async fn load_vibe_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Option<SynthVibe>> {
    let row: Option<(Option<i16>, String)> = sqlx::query_as(
        r#"
        SELECT sentiment, COALESCE(prompt, '')
        FROM vibe_scores
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
        ORDER BY generated_at DESC
        LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load vibe pillar {entity_type}/{entity_id}"))?;

    match row {
        Some((Some(sentiment), prompt)) => Ok(Some(SynthVibe {
            sentiment: sentiment as i32,
            prompt,
        })),
        _ => Ok(None),
    }
}

/// load_momentum_pillar (P4) reads the generated Momentum product. The deterministic
/// `momentum_scores` projection remains the numeric backbone, but Sigil now consumes the durable
/// `momentum_summaries` row so the Momentum lens has the same generated-product lifecycle as PEAK,
/// Vibe, narratives, and transfers.
pub async fn load_momentum_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: Option<i32>,
) -> Result<SynthMomentum> {
    #[allow(clippy::type_complexity)]
    let row: Option<(
        Option<String>,
        Option<i16>,
        Option<String>,
        Option<String>,
        serde_json::Value,
    )> = sqlx::query_as(
        r#"
        SELECT direction, score, blurb, input_hash, COALESCE(input_components, '{}'::jsonb)
        FROM public.momentum_summaries
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
    .with_context(|| format!("load momentum pillar {entity_type}/{entity_id}"))?;

    let Some((direction, score, blurb, input_hash, components)) = row else {
        return Ok(SynthMomentum::default());
    };
    let rating_slope = components
        .get("momentum_rating_slope")
        .and_then(serde_json::Value::as_f64);
    let rating_samples = components
        .get("momentum_rating_samples")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_default() as i32;
    let vibe_slope = components
        .get("momentum_vibe_slope")
        .and_then(serde_json::Value::as_f64);
    let vibe_samples = components
        .get("momentum_vibe_samples")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_default() as i32;
    Ok(SynthMomentum {
        direction: direction.filter(|s| !s.trim().is_empty()),
        blurb: blurb.filter(|s| !s.trim().is_empty()),
        input_hash,
        vibe_slope,
        vibe_samples,
        rating_slope,
        rating_samples,
        momentum_score: score.map(f64::from),
    })
}

/// load_pillars resolves the season and loads all pillars season-exact — the shared
/// front half of both `generate_sigil` (parity) and `SigilHandler::handle` (production).
/// `pub` so the `sigil` eval task (`eval_tasks::SigilTask`) builds the same synthesis prompt as
/// production from one source, rather than reconstructing it from the individual pillar loaders.
pub async fn load_pillars(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    sport: &str, // upper-cased
) -> Result<(
    i32,
    Vec<SynthNarrative>,
    Option<SynthRating>,
    Option<SynthVibe>,
    SynthMomentum,
    Vec<HeatItem>,
)> {
    let season = resolve_season(&hx.pool, sport, None).await?;
    // The pillars are fully independent once the season is known — load them concurrently
    // (plan A3). Each future keeps its own error context; on multi-failure which context lands in
    // pipeline_work.last_error is racy (cosmetic). The transfer pillar reuses the shared
    // `corpus::load_transfer_heat` — the SAME served-rumor read the /transfers card and the
    // vibe/narratives heat lines use — so the synthesis sees exactly what the trigger gate saw.
    let (narratives, rating, vibe, momentum, transfers) = tokio::try_join!(
        async {
            load_narrative_pillar(&hx.pool, entity_type, entity_id, sport)
                .await
                .context("narrative pillar")
        },
        async {
            load_rating_pillar(&hx.pool, entity_type, entity_id, sport, Some(season))
                .await
                .context("rating pillar")
        },
        async {
            load_vibe_pillar(&hx.pool, entity_type, entity_id, sport)
                .await
                .context("vibe pillar")
        },
        async {
            load_momentum_pillar(&hx.pool, entity_type, entity_id, sport, Some(season))
                .await
                .context("momentum pillar")
        },
        async {
            load_transfer_heat(&hx.pool, entity_type, entity_id, sport)
                .await
                .context("transfer pillar")
        },
    )?;
    Ok((season, narratives, rating, vibe, momentum, transfers))
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
#[cfg(test)]
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

/// momentum_score reads the durable signed Momentum trajectory value. It is directional force,
/// not entity quality: positive is rising, negative is sliding, zero is flat.
fn momentum_score(mom: &SynthMomentum) -> Option<i32> {
    mom.momentum_score.map(|s| s.round() as i32)
}

fn momentum_score_label(score: i32) -> &'static str {
    if score >= 3 {
        "surging"
    } else if score >= 1 {
        "rising"
    } else if score <= -3 {
        "falling"
    } else if score <= -1 {
        "sliding"
    } else {
        "steady"
    }
}

// ---------------------------------------------------------------------------
// Input components + hash — the debounce key (Provenance.input_hash).
//
// The canonical JSON keeps Go's stable map encoding shape (sorted keys, HTML-escaped strings,
// shortest float form), so its SHA-256 128-bit hex prefix remains a deterministic debounce key.
// Wave 5 intentionally changes the fields by adding Vibe and durable Momentum as first-class
// Sigil inputs, so this is a product-contract hash now rather than a Go parity axis.
//
// F1 (2026-07-12) narrows the key to MATERIAL signals only: upstream model prose (the vibe
// felt-read, the momentum blurb) stays in the prompt but never enters the hash — the same
// "exclude derived commentary" rule narratives applies to heat summaries. Prose from a
// temp-0.7 upstream re-run must not be able to flip this hash when nothing material moved.
// ---------------------------------------------------------------------------

/// build_synthesis_input_components returns the canonical input-components JSON. The
/// `narrative_titles` key is ALWAYS present (even `[]`); the rest are conditional. Keys are
/// emitted in sorted order to preserve a stable hash pre-image.
pub fn build_synthesis_input_components(
    narratives: &[SynthNarrative],
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
    transfers: &[HeatItem],
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
    if let Some(v) = vibe {
        // Sentiment only — the vibe felt-read prose is PROMPT-ONLY (F1, material-only
        // debounce): vibe generates at temp 0.7, so hashing its prose flipped this hash on
        // every vibe re-run even when nothing material moved.
        pairs.push(("vibe_sentiment", v.sentiment.to_string()));
    }
    if let Some(s) = mom.vibe_slope {
        pairs.push(("momentum_vibe_slope", go_json_float(round1(s))));
        pairs.push(("momentum_vibe_samples", mom.vibe_samples.to_string()));
    }
    if let Some(s) = mom.rating_slope {
        pairs.push(("momentum_rating_slope", go_json_float(round1(s))));
        pairs.push(("momentum_rating_samples", mom.rating_samples.to_string()));
    }
    if let Some(score) = mom.momentum_score {
        pairs.push(("momentum_score", go_json_float(round1(score))));
    }
    if let Some(direction) = &mom.direction {
        pairs.push(("momentum_direction", go_json_string(direction)));
    }
    // momentum_blurb is PROMPT-ONLY (F1, material-only debounce): the blurb is momentum's
    // model prose, so hashing it made every momentum regeneration flip sigil's hash even when
    // the material signals were unchanged. momentum_summary_hash below is momentum's own
    // input_hash — material-only after F1 — so sigil still re-runs when momentum's INPUTS
    // genuinely move.
    if let Some(input_hash) = &mom.input_hash {
        pairs.push(("momentum_summary_hash", go_json_string(input_hash)));
    }

    // transfer_heat (Phase 5.1) — CONDITIONAL (emitted only when there is served heat), so an
    // entity with no rumors keeps its pre-Phase-5.1 hash and does NOT spuriously re-synthesize on
    // deploy. One canonical `counterparty:heat:direction:stage` line per rumor, sorted for a stable
    // pre-image — the same shape convention as `narrative_trajectories`. This is what makes a
    // transfer-only enqueue real work instead of a debounced skip.
    if !transfers.is_empty() {
        let mut lines: Vec<String> = transfers
            .iter()
            .map(|t| format!("{}:{}:{}:{}", t.counterparty, t.heat, t.direction, t.stage))
            .collect();
        lines.sort();
        let mut heat_json = String::from("[");
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                heat_json.push(',');
            }
            heat_json.push_str(&go_json_string(line));
        }
        heat_json.push(']');
        pairs.push(("transfer_heat", heat_json));
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
// carry closed post-Step-3). The behavior stays byte-identical to Go's leaf encoding; the Sigil
// component field set above is now Rust-owned product shape.

// ---------------------------------------------------------------------------
// Prompt assembly.
// ---------------------------------------------------------------------------

/// The previous Sigil read fed back into the prompt for continuity (Phase 5.2). Prompt-only: it
/// is NOT part of `build_synthesis_input_components` / the `input_hash` — the score always moves,
/// so hashing it would self-trigger every re-run. This mirrors how `previous_score` is
/// persisted-but-not-hashed. Constructed only for a real prior read (`previous_score` present).
#[derive(Clone, Debug)]
pub struct PrevSigil {
    pub score: i32,
    /// The prior blurb; may be empty (a scored row can carry an empty blurb) — then only the
    /// Score line renders.
    pub blurb: String,
}

/// build_synthesis_prompt assembles the user prompt. `sport_raw` is the original-case value used in
/// the prompt; `entity_type` is used RAW (no title-casing, unlike vibe). `previous` is the prior
/// Sigil read for continuity (Phase 5.2) — rendered as a lead-in anchor, `None` for the parity
/// path and an entity's first synthesis. `memory` is the per-entity relational memory card
/// (s15, mig 163) — `None` when the graph holds none, and for the parity/eval paths.
#[allow(clippy::too_many_arguments)]
/// One deterministic cross-pillar direction comparison, computed in code before the model ever
/// sees the pillars. The PEAK `ScoutingDecision` lesson (2026-07-10) applied to Sigil: the
/// fixture-measured failure was convergence scored 70-80 on disagreement-heavy inputs — asking
/// the model to NOTICE rail conflict in unstructured prose fails the same way asking it to
/// infer the PEAK label from a stat list did. So the conflict detection moves into code and the
/// model's job becomes explaining a handed decision.
#[derive(Clone, Debug, PartialEq)]
pub struct PillarComparison {
    pub label: String,
    pub agree: bool,
}

/// Reduce a value to a direction sign: `None` = not directional (skip the comparison).
fn trajectory_sign(key: &str) -> Option<i8> {
    match key {
        "rising" | "heating_up" => Some(1),
        "falling" | "cooling_off" => Some(-1),
        _ => None,
    }
}

fn sentiment_sign(sentiment: i32) -> Option<i8> {
    if sentiment >= 60 {
        Some(1)
    } else if sentiment <= 40 {
        Some(-1)
    } else {
        None
    }
}

fn sign_word(s: i8) -> &'static str {
    if s > 0 {
        "positive"
    } else {
        "negative"
    }
}

/// build_pillar_divergence emits one comparison per directional pillar pair that is actually
/// present. Neutral/steady/absent signals produce NO line (a steady lens neither agrees nor
/// disagrees — the system prompt's own convergence rule). Pure and deterministic; the card is
/// prompt-only and derives entirely from values already in the input hash, so it can never
/// trigger a regeneration by itself.
pub fn build_pillar_divergence(
    narratives: &[SynthNarrative],
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
) -> Vec<PillarComparison> {
    let mut out = Vec::new();

    let peak_sign = rating.and_then(|r| trajectory_sign(&r.peak_trajectory));
    let vibe_sign = vibe.and_then(|v| sentiment_sign(v.sentiment));
    let mom_sign = mom.direction.as_deref().and_then(trajectory_sign);
    // PEAK strength: the LEVEL sign (is this an elite or a weak profile), distinct from the
    // trajectory sign. The classic rails conflict the fixtures measure — "strong PEAK vs
    // sliding momentum and negative narrative" — is a LEVEL-vs-direction disagreement that
    // trajectory pairs alone cannot see. (Narrative heating_up/cooling_off is deliberately NOT
    // compared: it measures story intensity, not valence — a negative story heating up must
    // not read as "positive narrative".)
    let strength_sign = rating.and_then(|r| {
        if r.notability >= 70 {
            Some(1i8)
        } else if r.notability <= 35 {
            Some(-1i8)
        } else {
            None
        }
    });
    let strength_word = |s: i8| if s > 0 { "strong" } else { "weak" };
    // Narratives carry no valence signal here (see above); the parameter stays for the card's
    // future evolution (e.g. narrative-sentiment once the stage emits one).
    let _ = narratives;

    let mut push = |label: String, a: i8, b: i8| {
        out.push(PillarComparison {
            label,
            agree: (i32::from(a) * i32::from(b)) > 0,
        });
    };

    if let (Some(p), Some(m)) = (peak_sign, mom_sign) {
        push(
            format!(
                "PEAK trajectory ({}) vs Momentum ({})",
                sign_word(p),
                sign_word(m)
            ),
            p,
            m,
        );
    }
    if let (Some(v), Some(m)) = (vibe_sign, mom_sign) {
        push(
            format!("Vibe ({}) vs Momentum ({})", sign_word(v), sign_word(m)),
            v,
            m,
        );
    }
    if let (Some(p), Some(v)) = (peak_sign, vibe_sign) {
        push(
            format!(
                "PEAK trajectory ({}) vs Vibe ({})",
                sign_word(p),
                sign_word(v)
            ),
            p,
            v,
        );
    }
    if let (Some(s), Some(m)) = (strength_sign, mom_sign) {
        push(
            format!(
                "PEAK strength ({}) vs Momentum ({})",
                strength_word(s),
                sign_word(m)
            ),
            s,
            m,
        );
    }
    if let (Some(s), Some(v)) = (strength_sign, vibe_sign) {
        push(
            format!(
                "PEAK strength ({}) vs Vibe ({})",
                strength_word(s),
                sign_word(v)
            ),
            s,
            v,
        );
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn build_synthesis_prompt(
    entity_type: &str,
    entity_name: &str,
    sport_raw: &str,
    narratives: &[SynthNarrative],
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
    transfers: &[HeatItem],
    previous: Option<&PrevSigil>,
    memory: Option<&str>,
) -> String {
    let mut b = String::new();

    // header = "<Sport> <entityType>" (raw entity_type), e.g. "NBA player".
    b.push_str(&format!(
        "Entity: {entity_name} ({sport_raw} {entity_type})\n"
    ));

    // Previous Sigil (Phase 5.2) — a continuity anchor set BEFORE the fresh pillars so the model
    // reads its prior before the new evidence. Omitted entirely when there is no prior read (this
    // section is prompt-only and outside the hash, so it needs no stable no-data placeholder).
    if let Some(p) = previous {
        b.push_str("\n=== PREVIOUS SIGIL ===\n");
        b.push_str(&format!("Score: {}/100\n", p.score));
        if !p.blurb.is_empty() {
            b.push_str(&p.blurb);
            b.push('\n');
        }
    }

    // PILLAR AGREEMENT (Phase 2) — the deterministic divergence card, rendered after the prior
    // but BEFORE the raw pillars (same placement discipline as PEAK's SCOUTING DECISION:
    // decision first, evidence after). Omitted entirely when no directional pair exists, so
    // quiet entities' prompts are unchanged.
    let comparisons = build_pillar_divergence(narratives, rating, vibe, mom);
    if !comparisons.is_empty() {
        let agree_count = comparisons.iter().filter(|c| c.agree).count();
        b.push_str("\n=== PILLAR AGREEMENT (computed) ===\n");
        for c in &comparisons {
            b.push_str(&format!(
                "- {}: {}\n",
                c.label,
                if c.agree { "agree" } else { "DISAGREE" }
            ));
        }
        b.push_str(&format!(
            "Agreement: {agree_count} of {} directional pairs agree.\n",
            comparisons.len()
        ));
    }

    // P1 — News narrative
    if !narratives.is_empty() {
        b.push_str("\n=== NEWS NARRATIVE ===\n");
        for n in narratives {
            let mut tags = format!(
                "impact {:.0}, {}",
                n.impact,
                trajectory_label(&n.trajectory)
            );
            // Corroboration + freshness (Phase 1): the synthesis should weigh how much a
            // pillar can be trusted, not just what it says.
            if n.source_count > 0 {
                tags.push_str(&format!(", {} sources", n.source_count));
            }
            if let Some(d) = n.source_age_days {
                tags.push_str(&format!(", latest {d}d ago"));
            }
            b.push_str(&format!("[{tags}] {}\n{}\n\n", n.title, n.body));
        }
    } else {
        b.push_str("\n=== NEWS NARRATIVE ===\n(no recent narratives)\n");
    }

    // P2 — PEAK scouting report (the stat end product)
    b.push_str("\n=== PEAK SCOUTING REPORT ===\n");
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

    // P3 — Vibe felt-state
    b.push_str("\n=== VIBE ===\n");
    if let Some(v) = vibe {
        b.push_str(&format!("Sentiment: {}/100\n", v.sentiment));
        if !v.prompt.is_empty() {
            b.push_str(&v.prompt);
            b.push('\n');
        }
    } else {
        b.push_str("(no vibe prompt available)\n");
    }

    // P4 — Momentum
    b.push_str("\n=== MOMENTUM ===\n");
    if mom.blurb.is_some() || mom.direction.is_some() {
        let direction = mom.direction.as_deref().unwrap_or("steady");
        if let Some(score) = momentum_score(mom) {
            b.push_str(&format!("Momentum: {direction} (score {score})\n"));
        } else {
            b.push_str(&format!("Momentum: {direction}\n"));
        }
        if let Some(blurb) = &mom.blurb {
            b.push_str(blurb);
            b.push('\n');
        }
    } else if let Some(score) = momentum_score(mom) {
        b.push_str(&format!(
            "Momentum score: {score} ({})\n",
            momentum_score_label(score)
        ));
    }
    if let Some(s) = mom.vibe_slope {
        let dir = trend_dir(s);
        b.push_str(&format!(
            "Vibe trajectory: {s:.1} over {} samples ({dir})\n",
            mom.vibe_samples
        ));
    }
    if let Some(s) = mom.rating_slope {
        let dir = trend_dir(s);
        b.push_str(&format!(
            "PEAK trajectory: {s:.1} over {} samples ({dir})\n",
            mom.rating_samples
        ));
    }
    if mom.empty() {
        b.push_str("(no momentum data)\n");
    }

    // P5 — Transfer heat (the transfer lens). Rendered through the SHARED `write_heat_lines`, so a
    // Sigil sees the served rumors in the same format as the vibe/narratives heat lines and the
    // /transfers card.
    b.push_str("\n=== TRANSFER HEAT ===\n");
    if transfers.is_empty() {
        b.push_str("(no active transfer rumors)\n");
    } else {
        write_heat_lines(&mut b, transfers);
    }

    // Relational memory card (s15, mig 163): the graph's per-entity history — prior
    // stories with outcomes, current stories with likelihood, ground-truth moves.
    // CONTINUITY, NOT CORROBORATION (the echo-chamber rule): memory frames the arc the
    // synthesis sits in; it is never itself evidence for a new claim. Rendered only when
    // the graph holds memory; deliberately NOT part of the input_hash (the PREVIOUS
    // SIGIL precedent: enrichment rides along, it never self-triggers).
    if let Some(m) = memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\n=== RELATIONAL MEMORY (computed history) ===\n");
        b.push_str("Use for arc and continuity: what fizzled before, what is live now, what actually happened. Do NOT treat a prior story as evidence for a new claim.\n");
        for line in m.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }

    b.push_str("\nRespond now.");
    b
}

// ---------------------------------------------------------------------------
// Output parsing — mirrors parseSynthesisResponse.
// ---------------------------------------------------------------------------

/// The parsed synthesis reply. `score`/`blurb` are the required core (a `score == 0` means no
/// parseable SCORE line — the caller fails the item); the three Phase 5.3 panel fields are
/// `Option` because the model omits the whole line when it does not apply.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParsedSynthesis {
    pub score: i32,
    pub blurb: String,
    pub convergence: Option<i32>,
    pub disagreement: Option<String>,
    pub why_now: Option<String>,
}

/// is_synthesis_label reports whether a trimmed line begins a known reply field. Blurb
/// continuation stops here (so BLURB never swallows a later panel field), which is what makes the
/// parse ORDER-INDEPENDENT — the model may emit the panel lines before or after BLURB. Matched
/// WITHOUT the trailing space so a spacing slip (`SCORE:73`) still terminates blurb absorption.
fn is_synthesis_label(trimmed: &str) -> bool {
    [
        "SCORE:",
        "CONVERGENCE:",
        "DISAGREEMENT:",
        "WHY_NOW:",
        "BLURB:",
    ]
    .iter()
    .any(|p| trimmed.starts_with(p))
}

/// normalize_panel_line cleans a DISAGREEMENT / WHY_NOW value and treats a placeholder as ABSENT.
/// Both fields are contractually OMITTED when they do not apply, but `mistral:7b` instead writes
/// `DISAGREEMENT: N/A` — often wrapped in quotes, echoing the prompt's `e.g. "…"` example — so a
/// literal capture would persist "N/A" (or the quotes) onto the served /sigil card. This unwraps a
/// FULLY quoted line (leaving internal quotes intact) and maps `N/A` / `none` / `-` / empty to
/// `None`. Returns the cleaned value, or `None` when the line carries no real content.
fn normalize_panel_line(rest: &str) -> Option<String> {
    let t = rest.trim();
    // Unwrap only when the WHOLE value is quoted, so `"washed" narrative vs elite PEAK` keeps its
    // inner quotes while `"strong PEAK vs sliding momentum"` loses the surrounding pair.
    let t = if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1].trim()
    } else {
        t
    };
    if t.is_empty() {
        return None;
    }
    match t.to_ascii_lowercase().as_str() {
        "n/a" | "na" | "n.a." | "none" | "null" | "-" => None,
        _ => Some(t.to_string()),
    }
}

fn parse_panel_score(rest: &str) -> Option<i32> {
    let t = rest.trim();
    if t.is_empty() {
        return None;
    }
    let head = t.split_whitespace().next().unwrap_or("");
    let head = head.split_once('/').map(|(n, _)| n).unwrap_or(head).trim();
    // The contract asks for an integer, but the model sometimes emits a decimal
    // ("SCORE: 91.6") — 46 live sigil items sat permanently failed on exactly that.
    // Accept and round; anything non-numeric still fails closed (no score → the
    // caller fails the item).
    let n = match head.parse::<i64>() {
        Ok(n) => n,
        Err(_) => head.parse::<f64>().ok().filter(|f| f.is_finite())?.round() as i64,
    };
    Some(n.clamp(1, 100) as i32)
}

/// parse_synthesis_response extracts the synthesis reply. SCORE + BLURB mirror
/// `parseSynthesisResponse`: case-sensitive `"SCORE: "` / `"BLURB: "` prefixes (note the space),
/// the score clamped 1-100 when the value starts with an integer or common `N/100` form, blurb continuation lines absorbed.
/// `score == 0` means no parseable SCORE line (the caller treats it as a failure — there is NO
/// first-integer fallback, unlike vibe).
///
/// Phase 5.3 adds three OPTIONAL single-line fields (CONVERGENCE / DISAGREEMENT / WHY_NOW), each
/// extracted by the same case-sensitive `"<LABEL>: "` convention. They degrade gracefully: a
/// missing (or empty) line ⇒ `None` ⇒ NULL column, never a parse failure — only SCORE is required.
/// DISAGREEMENT / WHY_NOW additionally run through `normalize_panel_line`, so a `N/A` placeholder or
/// a fully quoted line never reaches the persisted column (the served card stays clean). Blurb
/// absorption stops at any known label (see `is_synthesis_label`), so the panel fields are captured
/// regardless of whether the model emits them before or after BLURB.
pub fn parse_synthesis_response(raw: &str) -> ParsedSynthesis {
    let mut out = ParsedSynthesis::default();
    let lines: Vec<&str> = raw.trim().split('\n').collect();

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if let Some(rest) = trimmed.strip_prefix("SCORE: ") {
            if let Some(n) = parse_panel_score(rest) {
                out.score = n;
            }
        } else if let Some(rest) = trimmed.strip_prefix("CONVERGENCE: ") {
            if let Some(n) = parse_panel_score(rest) {
                out.convergence = Some(n);
            }
        } else if let Some(rest) = trimmed.strip_prefix("DISAGREEMENT: ") {
            if let Some(v) = normalize_panel_line(rest) {
                out.disagreement = Some(v);
            }
        } else if let Some(rest) = trimmed.strip_prefix("WHY_NOW: ") {
            if let Some(v) = normalize_panel_line(rest) {
                out.why_now = Some(v);
            }
        } else if let Some(rest) = trimmed.strip_prefix("BLURB: ") {
            let mut blurb = rest.trim().to_string();
            let mut j = i + 1;
            while j < lines.len() {
                let e = lines[j].trim();
                if is_synthesis_label(e) {
                    break; // a later field begins — do not absorb it into the blurb
                }
                if !e.is_empty() {
                    blurb.push(' ');
                    blurb.push_str(e);
                }
                j += 1;
            }
            out.blurb = blurb;
            i = j;
            continue; // j already points past the absorbed continuation lines
        }
        i += 1;
    }
    out
}

/// SigilParser is the sigil stage's `Parser` plug-in: it wraps `parse_synthesis_response`
/// behind the capability library's `Parser<T>` seam. It never returns the fail-closed
/// `Ok(None)` — sigil's only fail-closed path is the pre-model no-pillar marker; an
/// unparseable reply (no SCORE line ⇒ score 0) is a genuine failure → `Err` → the work item
/// backs off, exactly as sigil.go's `if score == 0 { return error }`.
pub struct SigilParser;

impl Parser<SigilReply> for SigilParser {
    fn parse(&self, raw: &str) -> Result<Option<SigilReply>> {
        let p = parse_synthesis_response(raw);
        if p.score == 0 {
            bail!(
                "synthesis: could not parse score from response (raw={:?})",
                truncate(raw, 200)
            );
        }
        Ok(Some(SigilReply {
            score: p.score,
            blurb: p.blurb,
            convergence: p.convergence,
            disagreement: p.disagreement,
            why_now: p.why_now,
        }))
    }
}

// ---------------------------------------------------------------------------
// The core generate + the production handler.
// ---------------------------------------------------------------------------

/// generate_sigil runs the full sigil derivation for one entity at the given temperature and
/// returns the un-persisted result — the composition `read pillars + route(SynthesisLogic) +
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
    let (out, _, _) = generate_sigil_inner(
        hx,
        entity_type,
        entity_id,
        entity_name,
        sport_raw,
        temperature,
        false,
    )
    .await?;
    Ok(out)
}

/// generate_sigil_parity runs the same core as production while returning the
/// parity-era prompt and request-body axes. Removed with the parity bins (see
/// plan C1).
pub async fn generate_sigil_parity(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport_raw: &str,
    temperature: f64,
) -> Result<(SigilOutput, Option<String>, Option<serde_json::Value>)> {
    generate_sigil_inner(
        hx,
        entity_type,
        entity_id,
        entity_name,
        sport_raw,
        temperature,
        true,
    )
    .await
}

async fn generate_sigil_inner(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport_raw: &str,
    temperature: f64,
    capture_parity: bool,
) -> Result<(SigilOutput, Option<String>, Option<serde_json::Value>)> {
    if entity_id <= 0 || entity_name.is_empty() || sport_raw.is_empty() || entity_type.is_empty() {
        bail!("sigil: entity context incomplete");
    }
    // Reads use the upper-cased sport; the prompt uses the original-case value (req.Sport).
    let sport = sport_raw.to_uppercase();

    let (season, narratives, rating, vibe, momentum, transfers) =
        load_pillars(hx, entity_type, entity_id, &sport).await?;

    // No-pillar path: persist a marker (handled by the caller) without a model call. The
    // marker's model_version is the role's configured model (no response to echo).
    if narratives.is_empty()
        && rating.is_none()
        && vibe.is_none()
        && momentum.empty()
        && transfers.is_empty()
    {
        return Ok((
            SigilOutput {
                score: None,
                blurb: None,
                season,
                input_components_json: "{}".to_string(),
                input_hash: None,
                model: hx.router.for_role(Role::SynthesisLogic).model().to_string(),
                prompt_version: SIGIL_PROMPT_VERSION,
                convergence: None,
                disagreement: None,
                why_now: None,
                built_prompt: None,
                request_body: None,
                eval_count: None,
                wall_ms: None,
            },
            None,
            None,
        ));
    }

    let input_components_json = build_synthesis_input_components(
        &narratives,
        rating.as_ref(),
        vibe.as_ref(),
        &momentum,
        &transfers,
    );
    let input_hash = hash_components(&input_components_json);

    let prompt = build_synthesis_prompt(
        entity_type,
        entity_name,
        sport_raw,
        &narratives,
        rating.as_ref(),
        vibe.as_ref(),
        &momentum,
        &transfers,
        // Parity is deterministic at temp 0 and intentionally skips the previous-Sigil
        // continuity AND the relational memory card (as it skips SkipUnchanged/persist),
        // so the prompt stays byte-stable.
        None,
        None,
    );
    let opts = GenerateOptions {
        system: Some(SIGIL_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict: SIGIL_NUM_PREDICT,
        num_ctx: 0,
        json_mode: false,
        format_schema: None,
    };

    // sigil = route(SynthesisLogic) + extract(SigilParser). The fail-closed contract lives in the
    // parser (an unparseable reply → Err → item backs off); `extract` records the exact wire body.
    let extracted = hx
        .extract(Role::SynthesisLogic, &prompt, &opts, &SigilParser)
        .await?;
    let reply = extracted
        .value
        .ok_or_else(|| anyhow!("sigil: parser returned no value"))?;

    let built_prompt = extracted.built_prompt;
    let request_body = extracted.request_body;
    let eval_count = extracted.eval_count;
    let wall_ms = extracted.wall_ms;

    Ok((
        SigilOutput {
            score: Some(reply.score),
            blurb: Some(reply.blurb),
            season,
            input_components_json,
            input_hash: Some(input_hash),
            model: extracted.model,
            prompt_version: SIGIL_PROMPT_VERSION,
            convergence: reply.convergence,
            disagreement: reply.disagreement,
            why_now: reply.why_now,
            built_prompt: Some(built_prompt.clone()),
            request_body: Some(request_body.clone()),
            eval_count: Some(eval_count),
            wall_ms: Some(wall_ms),
        },
        capture_parity.then_some(built_prompt),
        capture_parity.then_some(request_body),
    ))
}

fn sigil_input_components_value(out: &SigilOutput) -> serde_json::Value {
    serde_json::from_str(&out.input_components_json).unwrap_or_else(|_| {
        serde_json::json!({
            "raw_input_components": &out.input_components_json,
        })
    })
}

fn sigil_included_evidence(out: &SigilOutput) -> serde_json::Value {
    serde_json::json!({
        "input_components": sigil_input_components_value(out),
        "score": out.score,
        "convergence": out.convergence,
        "disagreement": &out.disagreement,
        "why_now": &out.why_now,
    })
}

fn sigil_excluded_evidence(out: &SigilOutput) -> serde_json::Value {
    if out.built_prompt.is_none() {
        serde_json::json!([{
            "reason": "no_narrative_rating_vibe_momentum_or_transfer_pillar",
        }])
    } else {
        serde_json::json!([])
    }
}

fn sigil_parser_outcome(out: &SigilOutput) -> &'static str {
    if out.built_prompt.is_none() {
        "no_call"
    } else {
        "parsed"
    }
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
    voice: &crate::oracle::Voice,
) -> Result<i64> {
    let prov = out.provenance();
    let entity_id = item.entity_id_i32()?;
    let score: Option<i16> = out.score.map(|n| n as i16);
    // Phase 5.3 panel outputs — nullable columns (mig 143). All None for a marker or when the
    // model omitted the line; convergence rides the same smallint 1-100 shape as `score`.
    let convergence: Option<i16> = out.convergence.map(|n| n as i16);
    // Voice columns (mig 152): the decided card and its voice persist as ONE row. voiced_at:
    // a freshly drawn reading stamps NOW(); a carried one keeps its original drawn-at (bound
    // as text — sqlx runs without date-time features; the value is only moved, never used).
    let row = sqlx::query(
        r#"
        INSERT INTO sigil_synthesis (
            entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            score, previous_score, blurb, input_components, input_hash,
            model_version, prompt_version, convergence, disagreement, why_now,
            reading, omen, voiced_score, voiced_at, voice_model_version, voice_prompt_version
        ) VALUES ($1,$2,$3,$4,'periodic','{}'::jsonb, $5,$6,$7,$8::jsonb,$9, $10,$11,$12,$13,$14,
            $15,$16,$17,
            COALESCE($18::timestamptz, CASE WHEN $15 IS NOT NULL THEN NOW() END),
            $19,$20)
        RETURNING id
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
    .bind(convergence)
    .bind(out.disagreement.as_deref())
    .bind(out.why_now.as_deref())
    .bind(voice.reading.as_deref())
    .bind(voice.omen.as_deref())
    .bind(voice.voiced_score)
    .bind(voice.voiced_at.as_deref())
    .bind(voice.model_version.as_deref())
    .bind(voice.prompt_version.as_deref())
    .fetch_one(pool)
    .await
    .context("persist sigil")?;
    Ok(row.get("id"))
}

async fn write_sigil_ledger(
    pool: &PgPool,
    item: &Item,
    entity_id: i32,
    sport: &str,
    out: &SigilOutput,
    product_row_id: i64,
) {
    insert_cognition_ledger_best_effort(
        pool,
        CognitionLedgerEntry {
            stage: "sigil".to_string(),
            lens: "sigil".to_string(),
            role: Role::SynthesisLogic.as_str().to_string(),
            entity_type: item.entity_type.clone(),
            entity_id,
            sport: sport.to_string(),
            pair_entity_type: None,
            pair_entity_id: None,
            trigger_type: "periodic".to_string(),
            trigger_payload: serde_json::json!({}),
            product_table: "sigil_synthesis".to_string(),
            product_row_ids: vec![product_row_id],
            model_version: out.model.clone(),
            prompt_version: out.prompt_version.to_string(),
            output_contract_version: SIGIL_OUTPUT_CONTRACT_VERSION.to_string(),
            input_ids: Vec::new(),
            input_hash: out.input_hash.clone(),
            request_body: out.request_body.clone(),
            built_prompt: out.built_prompt.clone(),
            included_evidence: sigil_included_evidence(out),
            excluded_evidence: sigil_excluded_evidence(out),
            context_budget: serde_json::json!({
                "num_predict": SIGIL_NUM_PREDICT,
                "eval_count": out.eval_count,
                "wall_ms": out.wall_ms,
            }),
            parser_outcome: sigil_parser_outcome(out).to_string(),
        },
    )
    .await;
}

/// SigilHandler drains the durable `sigil` stage — the crown convergence, decided then
/// VOICED in one work item (Session B: the oracle stage folded in as an in-process second
/// step). It reads the pillars season-exact, SKIPS both model calls when the pillar hash is
/// unchanged (`debounce_unchanged`), else: decide (SynthesisLogic), apply the re-voice rule
/// (North Star #8 — omen flip / archetype band cross ±2pt / first reading), voice
/// (OracleLogic) or carry the prior reading forward, and persist ONE sigil_synthesis row
/// carrying both — the row Go serves (Session C; `oracle_readings` is frozen history).
/// Terminal stage — enqueues nothing downstream. The parity harness
/// reuses the loaders + `generate_sigil` core but writes the shadow table (decide only).
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
        let (season, narratives, rating, vibe, momentum, transfers) =
            load_pillars(hx, &item.entity_type, entity_id, &sport).await?;

        // No-pillar marker (no model call).
        if narratives.is_empty()
            && rating.is_none()
            && vibe.is_none()
            && momentum.empty()
            && transfers.is_empty()
        {
            let out = SigilOutput {
                score: None,
                blurb: None,
                season,
                input_components_json: "{}".to_string(),
                input_hash: None,
                model: hx.router.for_role(Role::SynthesisLogic).model().to_string(),
                prompt_version: SIGIL_PROMPT_VERSION,
                convergence: None,
                disagreement: None,
                why_now: None,
                built_prompt: None,
                request_body: None,
                eval_count: None,
                wall_ms: None,
            };
            // The marker carries NULL voice columns — a no-pillar card has no voice, and
            // serve-latest ignores markers, so the last real reading keeps serving.
            let product_row_id = persist_to_sigil_synthesis(
                &hx.pool,
                item,
                &sport,
                season,
                &out,
                None,
                &crate::oracle::Voice::marker(),
            )
            .await?;
            write_sigil_ledger(&hx.pool, item, entity_id, &sport, &out, product_row_id).await;
            return Ok(());
        }

        // SkipUnchanged debounce (drainSigil sets SkipUnchanged=true): skip the local model call when
        // the pillar input hash matches the entity-season's latest synthesis. This is the first
        // real consumer of the Persist `debounce_unchanged` primitive.
        let input_components_json = build_synthesis_input_components(
            &narratives,
            rating.as_ref(),
            vibe.as_ref(),
            &momentum,
            &transfers,
        );
        let input_hash = hash_components(&input_components_json);
        let key = EntityKey {
            entity_type: item.entity_type.clone(),
            entity_id,
            sport: sport.clone(),
            season: Some(season),
        };
        // One round-trip to the entity-season's latest synthesis row for the debounce hash, the
        // previous-score baseline, AND the previous blurb (plan A1 — a consistent, non-torn read
        // of the one prior synthesis).
        let (prev_score_raw, prev_blurb, latest_hash) =
            hx.latest_with_hash("sigil_synthesis", &key).await?;
        if latest_hash.as_deref() == Some(input_hash.as_str()) {
            // Unchanged → no model calls, no persist. The voice needs no self-healing hop
            // anymore: it runs in-process with the decide, and a voice failure fails the whole
            // item before anything persists — a persisted row always carries its voice.
            return Ok(());
        }
        let prev = prev_score_raw.map(|v| v as i32).unwrap_or(0);
        // Previous Sigil as prompt-only continuity (Phase 5.2): built only for a real prior read
        // (prev > 0 ⇒ a scored row, not a marker). Deliberately NOT folded into input_hash — the
        // score always moves, so hashing it would self-trigger every re-run; this mirrors how
        // previous_score is persisted-but-not-hashed.
        let previous = (prev > 0).then(|| PrevSigil {
            score: prev,
            blurb: prev_blurb.unwrap_or_default(),
        });

        // Relational memory card (s15): loaded after the hash gate (a skip never pays the
        // query); load failure degrades to an unenriched prompt (the n8/v12 discipline —
        // the pillars are the primary signal, memory is enrichment).
        let memory = match crate::narratives::load_entity_memory(
            &hx.pool,
            &sport,
            &item.entity_type,
            entity_id,
        )
        .await
        {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    entity_type = %item.entity_type,
                    entity_id,
                    sport = %sport,
                    error = %e,
                    "sigil: relational memory load failed (continuing without memory)"
                );
                None
            }
        };

        let prompt = build_synthesis_prompt(
            &item.entity_type,
            &name,
            &item.sport,
            &narratives,
            rating.as_ref(),
            vibe.as_ref(),
            &momentum,
            &transfers,
            previous.as_ref(),
            memory.as_deref(),
        );
        let opts = GenerateOptions {
            system: Some(SIGIL_SYSTEM_PROMPT.to_string()),
            temperature: Some(SIGIL_TEMPERATURE),
            num_predict: SIGIL_NUM_PREDICT,
            num_ctx: 0,
            json_mode: false,
            format_schema: None,
        };
        let extracted = hx
            .extract(Role::SynthesisLogic, &prompt, &opts, &SigilParser)
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
            convergence: reply.convergence,
            disagreement: reply.disagreement,
            why_now: reply.why_now,
            built_prompt: Some(extracted.built_prompt),
            request_body: Some(extracted.request_body),
            eval_count: Some(extracted.eval_count),
            wall_ms: Some(extracted.wall_ms),
        };
        // The VOICE step (Session B): the decided card is in hand — never re-read from the
        // DB. Apply the re-voice rule against the entity-season's current voice; draw a new
        // reading only when the story changed, else carry the prior one forward verbatim.
        let consumed = crate::oracle::ConsumedSigil {
            score: reply.score,
            blurb: out.blurb.clone().unwrap_or_default(),
            convergence: out.convergence,
            disagreement: out.disagreement.clone(),
            why_now: out.why_now.clone(),
            input_hash: out.input_hash.clone(),
        };
        let prior =
            crate::oracle::load_prior_voice(&hx.pool, &item.entity_type, entity_id, &sport, season)
                .await?;
        let (omen, omen_reason) =
            crate::oracle::compute_omen(&consumed, rating.as_ref(), &momentum);
        let voice = match prior {
            Some(p) if !crate::oracle::voice_should_regenerate(Some(&p), omen, consumed.score) => {
                crate::oracle::Voice::carried(p)
            }
            _ => {
                crate::oracle::voice_decided_sigil(
                    hx,
                    &item.entity_type,
                    &name,
                    &item.sport,
                    &consumed,
                    &narratives,
                    rating.as_ref(),
                    vibe.as_ref(),
                    &momentum,
                    &transfers,
                    omen,
                    &omen_reason,
                )
                .await?
            }
        };

        let prev_score: Option<i16> = if prev > 0 { Some(prev as i16) } else { None };
        let product_row_id =
            persist_to_sigil_synthesis(&hx.pool, item, &sport, season, &out, prev_score, &voice)
                .await?;
        write_sigil_ledger(&hx.pool, item, entity_id, &sport, &out, product_row_id).await;
        // Fresh reading → the OracleLogic ledger row (two ledger rows per generation when
        // the voice ran; one when it carried).
        crate::oracle::finish_fresh_voice(hx, item, &sport, &voice, product_row_id).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_line_reply() {
        // A bare SCORE + BLURB reply: the three Phase 5.3 panel fields degrade to None (the model
        // omitted them), never a parse failure.
        let p = parse_synthesis_response("SCORE: 73\nBLURB: A quiet, season-long ascent.");
        assert_eq!(p.score, 73);
        assert_eq!(p.blurb, "A quiet, season-long ascent.");
        assert_eq!(p.convergence, None);
        assert_eq!(p.disagreement, None);
        assert_eq!(p.why_now, None);
    }

    #[test]
    fn clamps_and_absorbs_trailing_blurb_lines() {
        let p = parse_synthesis_response("SCORE: 250\nBLURB: line one\nline two");
        assert_eq!(p.score, 100);
        assert_eq!(p.blurb, "line one line two");
    }

    #[test]
    fn score_zero_when_no_score_line() {
        // No "SCORE: " prefix ⇒ score 0 (the caller fails the item — no first-integer fallback).
        let p = parse_synthesis_response("the sigil feels like a 64 today");
        assert_eq!(p.score, 0);
    }

    #[test]
    fn parses_panel_output_fields() {
        // Full Phase 5.3 reply: SCORE + all three panel fields + BLURB, in prompt order.
        let p = parse_synthesis_response(
            "SCORE: 71\nCONVERGENCE: 40\nDISAGREEMENT: strong PEAK vs sliding momentum\nWHY_NOW: advanced transfer talks broke today\nBLURB: A star under real pressure.",
        );
        assert_eq!(p.score, 71);
        assert_eq!(p.convergence, Some(40));
        assert_eq!(
            p.disagreement.as_deref(),
            Some("strong PEAK vs sliding momentum")
        );
        assert_eq!(
            p.why_now.as_deref(),
            Some("advanced transfer talks broke today")
        );
        assert_eq!(p.blurb, "A star under real pressure.");
    }

    #[test]
    fn parses_score_and_convergence_slash_100_forms() {
        let p = parse_synthesis_response(
            "SCORE: 48/100\nCONVERGENCE: 75/100 (mixed lenses)\nDISAGREEMENT: momentum vs PEAK\nBLURB: under pressure",
        );
        assert_eq!(p.score, 48);
        assert_eq!(p.convergence, Some(75));
        assert_eq!(p.disagreement.as_deref(), Some("momentum vs PEAK"));
        assert_eq!(p.blurb, "under pressure");
    }

    #[test]
    fn convergence_clamped_like_score() {
        let p = parse_synthesis_response("SCORE: 50\nCONVERGENCE: 250\nBLURB: mixed signals");
        assert_eq!(p.convergence, Some(100));
        // A non-numeric CONVERGENCE leaves it None (no unlabeled first-integer fallback).
        let p2 = parse_synthesis_response("SCORE: 50\nCONVERGENCE: high\nBLURB: mixed signals");
        assert_eq!(p2.convergence, None);
    }

    #[test]
    fn parses_decimal_score_by_rounding() {
        // The live failure that stranded 46 sigil items: "SCORE: 91.6" from a model that
        // ignored the integer contract. Rounds instead of failing the item.
        let p = parse_synthesis_response("SCORE: 91.6\nBLURB: dual-threat winger");
        assert_eq!(p.score, 92);
        // Still fail-closed on genuinely non-numeric scores.
        let p2 = parse_synthesis_response("SCORE: elite\nBLURB: nope");
        assert_eq!(p2.score, 0);
    }

    #[test]
    fn blurb_absorption_stops_at_a_later_panel_label() {
        // Order-independence: even when the model emits BLURB BEFORE the panel fields, the blurb
        // must not swallow them — absorption stops at the next known label, and each field is still
        // captured on its own line.
        let p = parse_synthesis_response(
            "SCORE: 64\nBLURB: The story continues\nover two lines.\nDISAGREEMENT: narrative up, stats flat\nWHY_NOW: coaching change confirmed",
        );
        assert_eq!(p.score, 64);
        assert_eq!(p.blurb, "The story continues over two lines.");
        assert_eq!(p.disagreement.as_deref(), Some("narrative up, stats flat"));
        assert_eq!(p.why_now.as_deref(), Some("coaching change confirmed"));
    }

    #[test]
    fn empty_panel_lines_are_none_not_empty_string() {
        // The model emitting a label with no content ⇒ None (persisted NULL), not Some("").
        let p = parse_synthesis_response(
            "SCORE: 55\nDISAGREEMENT: \nWHY_NOW:\nBLURB: steady across the board",
        );
        assert_eq!(p.disagreement, None);
        assert_eq!(p.why_now, None);
        assert_eq!(p.blurb, "steady across the board");
    }

    #[test]
    fn placeholder_panel_lines_normalize_to_none() {
        // mistral:7b writes `DISAGREEMENT: N/A` (sometimes quoted) instead of OMITTING the line when
        // the lenses agree — the placeholder must persist as NULL, never reach the served card.
        for placeholder in ["N/A", "\"N/A\"", "n/a", "none", "None", "-", "null"] {
            let p = parse_synthesis_response(&format!(
                "SCORE: 88\nCONVERGENCE: 95\nDISAGREEMENT: {placeholder}\nWHY_NOW: {placeholder}\nBLURB: aligned across the board"
            ));
            assert_eq!(p.disagreement, None, "disagreement for {placeholder:?}");
            assert_eq!(p.why_now, None, "why_now for {placeholder:?}");
            assert_eq!(p.blurb, "aligned across the board");
        }
    }

    #[test]
    fn fully_quoted_panel_lines_are_unwrapped_inner_quotes_kept() {
        // The model wraps the whole line in quotes (echoing the prompt's example) — strip the pair.
        let p = parse_synthesis_response(
            "SCORE: 68\nDISAGREEMENT: \"strong PEAK vs sliding momentum\"\nWHY_NOW: \"trade talks broke today\"\nBLURB: under pressure",
        );
        assert_eq!(
            p.disagreement.as_deref(),
            Some("strong PEAK vs sliding momentum")
        );
        assert_eq!(p.why_now.as_deref(), Some("trade talks broke today"));
        // An INTERNAL quote (not a surrounding pair) is preserved.
        let p2 = parse_synthesis_response(
            "SCORE: 60\nDISAGREEMENT: \"washed\" narrative vs elite PEAK\nBLURB: mixed",
        );
        assert_eq!(
            p2.disagreement.as_deref(),
            Some("\"washed\" narrative vs elite PEAK")
        );
    }

    #[test]
    fn sigil_parser_carries_panel_fields() {
        let reply = SigilParser
            .parse("SCORE: 80\nCONVERGENCE: 90\nWHY_NOW: rating jumped tonight\nBLURB: Surging.")
            .unwrap()
            .expect("a valid reply is Some");
        assert_eq!(reply.score, 80);
        assert_eq!(reply.convergence, Some(90));
        assert_eq!(reply.disagreement, None);
        assert_eq!(reply.why_now.as_deref(), Some("rating jumped tonight"));
        assert_eq!(reply.blurb, "Surging.");
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
            vibe_slope: Some(2.0),
            vibe_samples: 4,
            rating_slope: Some(2.0),
            rating_samples: 5,
            momentum_score: Some(4.0),
            ..SynthMomentum::default()
        };
        assert_eq!(momentum_score(&surging), Some(4));
        assert_eq!(momentum_score_label(4), "surging");

        let sliding = SynthMomentum {
            vibe_slope: Some(-2.0),
            vibe_samples: 3,
            rating_slope: Some(-1.0),
            rating_samples: 3,
            momentum_score: Some(-1.5),
            ..SynthMomentum::default()
        };
        assert_eq!(momentum_score(&sliding), Some(-2));
        assert_eq!(momentum_score_label(-2), "sliding");

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
    fn input_components_use_stable_go_json_shape() {
        // Validates sorted keys + HTML escaping + Go float form + int form together — the
        // canonical JSON whose SHA-256 is the input_hash. Compare against the exact bytes Go's
        // json.Marshal would emit for the same map.
        let narratives = vec![
            SynthNarrative {
                title: "B & C".into(),
                body: "x".into(),
                impact: 5.0,
                trajectory: "heating_up".into(),
                source_count: 0,
                source_age_days: None,
            },
            SynthNarrative {
                title: "Alpha".into(),
                body: "y".into(),
                impact: 3.0,
                trajectory: DEFAULT_TRAJECTORY.into(),
                source_count: 0,
                source_age_days: None,
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
            vibe_slope: Some(1.0),
            vibe_samples: 4,
            rating_slope: Some(0.0),
            rating_samples: 5,
            momentum_score: Some(2.5),
            blurb: Some("PEAK is sliding while Vibe holds.".into()),
            input_hash: Some("a1b2c3d4e5f60718293a4b5c6d7e8f90".into()),
            ..SynthMomentum::default()
        };
        let vibe = SynthVibe {
            sentiment: 60,
            prompt: "Quietly surging".into(),
        };
        let got =
            build_synthesis_input_components(&narratives, Some(&rating), Some(&vibe), &mom, &[]);
        // "B & C"'s ampersand is HTML-escaped (the backslash-u form), exactly as Go's
        // json.Marshal emits it. Built via format! with a runtime backslash (bs) so the
        // source carries no literal backslash-u token (the editor would decode it).
        // The vibe prompt and momentum blurb are non-empty on purpose: the golden proves the
        // upstream model prose is NOT in the hash pre-image (F1 material-only debounce) —
        // vibe contributes only vibe_sentiment, momentum its material-only summary hash.
        let bs = '\\';
        let want = format!(
            r#"{{"divined_peak":"Rim Protector","momentum_rating_samples":5,"momentum_rating_slope":0,"momentum_score":2.5,"momentum_summary_hash":"a1b2c3d4e5f60718293a4b5c6d7e8f90","momentum_vibe_samples":4,"momentum_vibe_slope":1,"narrative_titles":["Alpha","B {bs}u0026 C"],"narrative_trajectories":["Alpha:developing_story","B {bs}u0026 C:heating_up"],"notability":88,"peak_trajectory":"falling","peak_trajectory_label":"Composite and PEAK z-scores trending down over recent games","vibe_sentiment":60}}"#
        );
        assert_eq!(got, want);
    }

    #[test]
    fn input_components_narrative_titles_always_present() {
        // Rating-only entity: narrative_titles is still present as [] by contract.
        let rating = SynthRating {
            divined_peak: "Spacer".into(),
            body: "b".into(),
            notability: 40,
            peak_trajectory: "steady".into(),
            peak_trajectory_label: String::new(),
        };
        let got = build_synthesis_input_components(
            &[],
            Some(&rating),
            None,
            &SynthMomentum::default(),
            &[],
        );
        assert_eq!(
            got,
            r#"{"divined_peak":"Spacer","narrative_titles":[],"narrative_trajectories":[],"notability":40,"peak_trajectory":"steady"}"#
        );
    }

    #[test]
    fn transfer_heat_enters_components_only_when_present() {
        // No transfers → NO transfer_heat key at all (so a pre-Phase-5.1 entity keeps its hash).
        let without =
            build_synthesis_input_components(&[], None, None, &SynthMomentum::default(), &[]);
        assert_eq!(
            without,
            r#"{"narrative_titles":[],"narrative_trajectories":[]}"#
        );

        // Served heat → one sorted "counterparty:heat:direction:stage" line per rumor. The two
        // rumors are given OUT of sorted order to prove the pre-image sorts (stable hash).
        let transfers = vec![
            HeatItem {
                counterparty: "Real Madrid".into(),
                heat: 71,
                stage: "advanced_talks".into(),
                direction: "outgoing".into(),
                summary: String::new(),
                confidence: None,
            },
            HeatItem {
                counterparty: "Arsenal".into(),
                heat: 40,
                stage: "speculation".into(),
                direction: "incoming".into(),
                summary: String::new(),
                confidence: None,
            },
        ];
        let with = build_synthesis_input_components(
            &[],
            None,
            None,
            &SynthMomentum::default(),
            &transfers,
        );
        assert_eq!(
            with,
            r#"{"narrative_titles":[],"narrative_trajectories":[],"transfer_heat":["Arsenal:40:incoming:speculation","Real Madrid:71:outgoing:advanced_talks"]}"#
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
            source_count: 3,
            source_age_days: Some(1),
        }];
        let mom = SynthMomentum {
            vibe_slope: Some(0.5),
            vibe_samples: 4,
            rating_slope: None,
            rating_samples: 0,
            momentum_score: Some(1.0),
            ..SynthMomentum::default()
        };
        let vibe = SynthVibe {
            sentiment: 62,
            prompt: "On the rise".into(),
        };
        let p = build_synthesis_prompt(
            "player",
            "Test Player",
            "NBA",
            &narratives,
            None,
            Some(&vibe),
            &mom,
            &[],
            None,
            None,
        );
        assert_eq!(
            p,
            "Entity: Test Player (NBA player)\n\n=== NEWS NARRATIVE ===\n[impact 7, Heating up, 3 sources, latest 1d ago] Trade buzz\ndetails\n\n\n=== PEAK SCOUTING REPORT ===\n(no stat commentary available)\n\n=== VIBE ===\nSentiment: 62/100\nOn the rise\n\n=== MOMENTUM ===\nMomentum score: 1 (rising)\nVibe trajectory: 0.5 over 4 samples (trending up)\n\n=== TRANSFER HEAT ===\n(no active transfer rumors)\n\nRespond now."
        );
    }

    #[test]
    fn pillar_divergence_names_the_rail_conflict() {
        // The fixture-measured sigil failure: strong-but-falling PEAK, positive vibe, falling
        // momentum. The card must hand the model both DISAGREE pairs deterministically.
        let rating = SynthRating {
            divined_peak: "Rim protection".into(),
            body: "b".into(),
            notability: 88,
            peak_trajectory: "falling".into(),
            peak_trajectory_label: String::new(),
        };
        let vibe = SynthVibe {
            sentiment: 75,
            prompt: "warm".into(),
        };
        let mom = SynthMomentum {
            direction: Some("falling".into()),
            momentum_score: Some(-2.0),
            ..SynthMomentum::default()
        };
        let c = build_pillar_divergence(&[], Some(&rating), Some(&vibe), &mom);
        let rendered: Vec<(String, bool)> = c.into_iter().map(|x| (x.label, x.agree)).collect();
        assert_eq!(
            rendered,
            vec![
                (
                    "PEAK trajectory (negative) vs Momentum (negative)".to_string(),
                    true
                ),
                ("Vibe (positive) vs Momentum (negative)".to_string(), false),
                (
                    "PEAK trajectory (negative) vs Vibe (positive)".to_string(),
                    false
                ),
                (
                    "PEAK strength (strong) vs Momentum (negative)".to_string(),
                    false
                ),
                (
                    "PEAK strength (strong) vs Vibe (positive)".to_string(),
                    true
                ),
            ]
        );
    }

    #[test]
    fn pillar_divergence_skips_neutral_and_absent_signals() {
        // Steady momentum, mid-band vibe, no rating: nothing directional -> empty card,
        // and the prompt section is omitted entirely.
        let vibe = SynthVibe {
            sentiment: 50,
            prompt: String::new(),
        };
        let mom = SynthMomentum {
            direction: Some("steady".into()),
            ..SynthMomentum::default()
        };
        assert!(build_pillar_divergence(&[], None, Some(&vibe), &mom).is_empty());
        let p = build_synthesis_prompt(
            "player",
            "X",
            "NBA",
            &[],
            None,
            Some(&vibe),
            &mom,
            &[],
            None,
            None,
        );
        assert!(!p.contains("PILLAR AGREEMENT"));
    }

    #[test]
    fn no_momentum_data_line_when_both_absent() {
        let p = build_synthesis_prompt(
            "team",
            "Test Team",
            "NFL",
            &[],
            None,
            None,
            &SynthMomentum::default(),
            &[],
            None,
            None,
        );
        assert_eq!(
            p,
            "Entity: Test Team (NFL team)\n\n=== NEWS NARRATIVE ===\n(no recent narratives)\n\n=== PEAK SCOUTING REPORT ===\n(no stat commentary available)\n\n=== VIBE ===\n(no vibe prompt available)\n\n=== MOMENTUM ===\n(no momentum data)\n\n=== TRANSFER HEAT ===\n(no active transfer rumors)\n\nRespond now."
        );
    }

    #[test]
    fn transfer_heat_renders_as_prompt_pillar() {
        // A team with one served rumor: the P5 section renders through the shared write_heat_lines
        // format (`- <counterparty> — heat <n>, <direction>, <stage>`).
        let transfers = vec![HeatItem {
            counterparty: "Liverpool".into(),
            heat: 66,
            stage: "advanced_talks".into(),
            direction: "incoming".into(),
            summary: String::new(),
            confidence: None,
        }];
        let p = build_synthesis_prompt(
            "team",
            "Test Team",
            "FOOTBALL",
            &[],
            None,
            None,
            &SynthMomentum::default(),
            &transfers,
            None,
            None,
        );
        assert!(p.contains(
            "=== TRANSFER HEAT ===\n- Liverpool — heat 66, incoming, advanced_talks\n\nRespond now."
        ));
    }

    #[test]
    fn previous_sigil_renders_as_continuity_lead_in() {
        // A prior read renders a `=== PREVIOUS SIGIL ===` block right after the Entity line, BEFORE
        // the fresh pillars — the model reads its prior before the new evidence.
        let previous = PrevSigil {
            score: 68,
            blurb: "A quiet, season-long ascent.".into(),
        };
        let p = build_synthesis_prompt(
            "player",
            "Test Player",
            "NBA",
            &[],
            None,
            None,
            &SynthMomentum::default(),
            &[],
            Some(&previous),
            None,
        );
        assert!(p.starts_with(
            "Entity: Test Player (NBA player)\n\n=== PREVIOUS SIGIL ===\nScore: 68/100\nA quiet, season-long ascent.\n\n=== NEWS NARRATIVE ==="
        ));
    }

    #[test]
    fn previous_sigil_empty_blurb_renders_score_only() {
        // A scored prior row can carry an empty blurb: only the Score line renders (no blank body).
        let previous = PrevSigil {
            score: 55,
            blurb: String::new(),
        };
        let p = build_synthesis_prompt(
            "team",
            "Test Team",
            "NFL",
            &[],
            None,
            None,
            &SynthMomentum::default(),
            &[],
            Some(&previous),
            None,
        );
        assert!(p.starts_with(
            "Entity: Test Team (NFL team)\n\n=== PREVIOUS SIGIL ===\nScore: 55/100\n\n=== NEWS NARRATIVE ==="
        ));
    }

    #[test]
    fn relational_memory_renders_as_final_section() {
        // The s15 memory card renders AFTER the pillars (evidence placement, the n8/v12
        // position) with the echo-chamber instruction line, bulleted lines, and sits
        // immediately before the reply cue. None/blank ⇒ no section (s14 byte-shape
        // preserved — pinned by no_momentum_data_line_when_both_absent).
        let mem = "Prior story: Real Madrid — fizzled (Jun 2026, peak coverage 82/100).\nGround truth: completed a confirmed move to Arsenal on Jul 01 2026.";
        let p = build_synthesis_prompt(
            "player",
            "Test Player",
            "FOOTBALL",
            &[],
            None,
            None,
            &SynthMomentum::default(),
            &[],
            None,
            Some(mem),
        );
        assert!(p.contains(
            "=== TRANSFER HEAT ===\n(no active transfer rumors)\n\n=== RELATIONAL MEMORY (computed history) ===\nUse for arc and continuity: what fizzled before, what is live now, what actually happened. Do NOT treat a prior story as evidence for a new claim.\n- Prior story: Real Madrid — fizzled (Jun 2026, peak coverage 82/100).\n- Ground truth: completed a confirmed move to Arsenal on Jul 01 2026.\n\nRespond now."
        ));
        let blank = build_synthesis_prompt(
            "player",
            "Test Player",
            "FOOTBALL",
            &[],
            None,
            None,
            &SynthMomentum::default(),
            &[],
            None,
            Some("  \n "),
        );
        assert!(!blank.contains("RELATIONAL MEMORY"));
    }

    #[test]
    fn previous_sigil_is_prompt_only_not_hashed() {
        // The continuity read must never touch the debounce hash: build_synthesis_input_components
        // takes no `previous` argument, so the hash pre-image is structurally independent of it.
        // This test pins that intent — the same pillars yield the same components regardless of
        // any prior read (which the prompt, not the hash, consumes).
        let mom = SynthMomentum::default();
        let a = build_synthesis_input_components(&[], None, None, &mom, &[]);
        let b = build_synthesis_input_components(&[], None, None, &mom, &[]);
        assert_eq!(a, b);
        assert_eq!(a, r#"{"narrative_titles":[],"narrative_trajectories":[]}"#);
    }
}
