//! Sigil stage — the crown convergence and Oracle reading.
//!
//! Sigil = `read pillars + route(OracleLogic) + extract(SigilParser) + persist`, with a
//! `debounce_unchanged` gate on the pillar `input_hash`. The prompt composes the Scout's
//! rating read, Vibe, Momentum, transfers, and current narratives as distinct pillars.
//! Phase 5.1 adds a fifth: the transfer-heat pillar (the transfer lens the trigger gate already
//! watches), so the synthesis can finally see the served rumors that can fire its own re-run.
//! Phase 5.2 feeds the previous Sigil (score + blurb) back into the prompt as continuity — a
//! prompt-only anchor, deliberately kept OUT of the `input_hash` (the score always moves, so
//! hashing it would self-trigger every re-run).
//! Phase 5.3 makes DISAGREEMENT between the five cards a first-class output: the reply gained three
//! OPTIONAL lines (`CONVERGENCE:` / `DISAGREEMENT:` / `WHY_NOW:`) alongside the required
//! SCORE + BLURB, persisted to the additive nullable `convergence`/`disagreement`/`why_now`
//! columns (mig 143). They are model OUTPUTS, not inputs — the `input_hash` stays
//! pillar-inputs-only, so old rows stay valid and populate lazily on the next real re-synthesis.
//! The SQL reads, deterministic slope/trend math, canonical input-components JSON (whose
//! SHA-256 is the `input_hash`), parser, persist path, and ledger evidence all live here.
//!
//! Fail-closed semantics reproduced verbatim: when an entity has NO narrative pillar AND no
//! rating pillar AND no vibe pillar AND no momentum pillar AND no transfer pillar, we skip the model
//! and persist a NULL-score/NULL-blurb
//! marker row (the read path returns "no synthesis yet"). The SkipUnchanged debounce skips the
//! local model call when the pillars hash identically to the entity-season's latest synthesis.
//! The Oracle reading is folded into this same stage, so Sigil remains the terminal product row.

use crate::corpus::{load_transfer_heat, HeatItem};
use crate::harness::{EntityKey, Harness, Parser, Provenance};
use crate::ledger::{insert_cognition_ledger_best_effort, CognitionLedgerEntry};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::trajectory::DEFAULT_TRAJECTORY;
use crate::util::{go_json_float, go_json_string, hash_components, round1, truncate};
use crate::work::{self, Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use tracing::debug;

// This junction's contract with its model — system prompt, contract version, and prompt
// builder — lives in `prompt.rs`, so a change to what this character is asked is a one-file
// diff. Re-exported here so call sites and the ledger keep reading it from the stage module.
pub mod prompt;
pub use prompt::{
    build_crown_prompt, oracle_format_schema, CROWN_CARD_BODY_CAP, ORACLE_PROMPT_VERSION,
    ORACLE_SYSTEM_PROMPT,
};

/// Output contract captured in the diagnostic ledger, distinct from prompt_version. v1 was the
/// reading-only reply; v2 adds the emitted `score` (the crown fold).
pub const ORACLE_OUTPUT_CONTRACT_VERSION: &str = "oracle-reading-v2";

/// Production crown temperature (sigil/oracle both used 0.6): warm enough for voice, cool enough
/// to stay on the cards. Fixtures pin 0.
pub const ORACLE_TEMPERATURE: f64 = 0.6;

/// Token cap for the `{reading, score}` reply (a 2-4 sentence reading + one integer ≈ 70-160
/// tokens; generous headroom, still tight enough that a thinking route would burn it).
pub const ORACLE_NUM_PREDICT: i32 = 1100;

/// The reservation inside a SMALL voice window (§7's ≤800 share). Every voice that reserves more
/// than this drops to it at 4096, for the arithmetic reason `narratives_decode_budget` has
/// documented since it was written: a reservation the window cannot hold evicts the system prompt silently,
/// mid-generation, and the failure looks like a model that stopped obeying its rules.
pub const SMALL_WINDOW_NUM_PREDICT: i32 = 700;

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

/// The Scout's rating pillar (P2). `None` (suppressed) when there is no commentary row, or when
/// the latest generation is a no-stats marker (`body` NULL). The trajectory fields ride along
/// for the ANALYST (which leans on the deterministic marker); the Oracle itself is blind to the
/// marker since or10 — it reads the Scout's and Analyst's OUTPUTS, never the raw tracker.
#[derive(Clone, Debug)]
pub struct SynthRating {
    pub body: String,
    pub notability: i32,
    pub rating_trajectory: String,
    pub rating_trajectory_label: String,
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
pub struct CrownReply {
    /// The 2-4 sentence reading — the interpretation of the cards, generated FIRST.
    pub reading: String,
    /// The 1-100 verdict the reading earned, generated SECOND. Clamped to 1-100 at parse.
    pub score: i32,
}

/// The result of running the crown for one entity, before persistence. Captures the production
/// row payload for `sigil_synthesis`. The crown is ONE call (or3): reading + score from the
/// model, omen + convergence computed deterministically in code.
#[derive(Clone, Debug)]
pub struct SigilOutput {
    /// `None` ⇒ no-pillar NULL marker (no model call was made).
    pub score: Option<i32>,
    /// The crown reading — the served voice. `None` ⇒ marker; `Some` ⇒ a scored reading.
    pub reading: Option<String>,
    /// The season this convergence is for (current_season, resolved + stamped). Never NULL.
    pub season: i32,
    /// The canonical input-components JSON — BYTE-IDENTICAL to Go's `json.Marshal(ic)`, so it
    /// is both the persisted `input_components` and the pre-image of `input_hash`. `"{}"` for
    /// the no-pillar marker.
    pub input_components_json: String,
    /// SHA-256 (128-bit hex prefix) of `input_components_json` — the debounce key. `None` for
    /// the marker (no-pillar row writes NULL `input_hash`).
    pub input_hash: Option<String>,
    /// no-pillar → the role's configured model name; scored → the model echoed in the response.
    pub model: String,
    pub prompt_version: &'static str,
    /// Deterministic convergence (1-100) from `pillar_convergence` — NOT model-emitted. `None`
    /// for the marker and when no directional pillar pair exists. NOT part of the `input_hash`.
    pub convergence: Option<i32>,
    /// The computed omen the reading was drawn under (`compute_omen`). `None` for the marker.
    pub omen: Option<&'static str>,
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
// The completion barrier.
// ---------------------------------------------------------------------------

/// Enqueue the Oracle only once every pillar has settled for this entity. Returns whether it did.
///
/// ## What this replaces
///
/// Pillar handlers used to enqueue Sigil the moment their own card landed, so the Oracle could be
/// crowned off a spread where the other characters had not spoken yet — it read whatever pillars
/// happened to exist and rendered a verdict on a half-dealt table. Sigil's own input-hash debounce
/// hid the cost rather than fixing it: the reading was regenerated later, so the waste showed up as
/// churn instead of as a wrong card.
///
/// ## Call this only AFTER `work::complete()`
///
/// The worker calls it once, for any pillar stage, immediately after completing the item — see
/// [`work::pillars_settled`] for why asking before completion is racy under the concurrent drain.
/// The Insider is the one other caller, because a served rumor settles nothing for the PLAYER it
/// names: that is a different entity than the one being drained, holds no row this handler owns,
/// and so is safe to ask about at any point.
pub async fn enqueue_oracle_if_pillars_settled(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i64,
    sport: &str,
    input_version: Option<String>,
) -> Result<bool> {
    if !work::pillars_settled(pool, entity_type, entity_id, sport).await? {
        debug!(
            %entity_type, entity_id, %sport,
            "oracle barrier: pillars still outstanding; not enqueuing"
        );
        return Ok(false);
    }

    let sig = Item {
        stage: Stage::Sigil,
        entity_type: entity_type.to_string(),
        entity_id,
        sport: sport.to_string(),
        input_version,
        attempts: 0,
    };
    work::enqueue(pool, &sig).await?;
    debug!(
        %entity_type, entity_id, %sport,
        "oracle barrier: last pillar settled; enqueued sigil"
    );
    Ok(true)
}

// ---------------------------------------------------------------------------
// Pillar loaders.
// ---------------------------------------------------------------------------

/// resolve_season returns the concrete season this synthesis is for: the caller's explicit
/// season when given, else the sport's `current_season`.
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
    // COALESCE(notability, 0): int2 coalesced with int4 → int4 → scan i32.
    let row: Option<(Option<String>, i32, String, String)> = sqlx::query_as(
        r#"
        SELECT body, COALESCE(notability, 0),
               COALESCE(rating_trajectory, 'steady'), COALESCE(rating_trajectory_label, '')
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
        Some((None, _, _, _)) => Ok(None), // latest generation is a marker (body NULL) → suppressed
        Some((Some(body), notability, rating_trajectory, rating_trajectory_label)) => {
            Ok(Some(SynthRating {
                body,
                notability,
                rating_trajectory,
                rating_trajectory_label,
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
        // or10 (the PEAK retirement): the crown is blind to the raw trajectory marker — it
        // reads the Scout's and Analyst's OUTPUTS for form. Only the profile-strength level
        // remains in the pre-image (divined_peak and the trajectory pairs are gone; that hash
        // change is the intended one-time regen wave).
        pairs.push(("notability", r.notability.to_string()));
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

    // or10 (the PEAK retirement): the raw trajectory marker leaves the crown's math — the
    // Oracle is blind to the tracker and reads the Scout's and Analyst's OUTPUTS. Momentum
    // (the Analyst's deterministic direction) is the sole direction signal here.
    let vibe_sign = vibe.and_then(|v| sentiment_sign(v.sentiment));
    let mom_sign = mom.direction.as_deref().and_then(trajectory_sign);
    // Profile strength: the LEVEL sign (is this an elite or a weak profile), distinct from the
    // direction sign. The classic rails conflict the fixtures measure — "strong profile vs
    // sliding momentum and negative narrative" — is a LEVEL-vs-direction disagreement that
    // direction pairs alone cannot see. (Narrative heating_up/cooling_off is deliberately NOT
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

    if let (Some(v), Some(m)) = (vibe_sign, mom_sign) {
        push(
            format!("Vibe ({}) vs Momentum ({})", sign_word(v), sign_word(m)),
            v,
            m,
        );
    }
    if let (Some(s), Some(m)) = (strength_sign, mom_sign) {
        push(
            format!(
                "Profile strength ({}) vs Momentum ({})",
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
                "Profile strength ({}) vs Vibe ({})",
                strength_word(s),
                sign_word(v)
            ),
            s,
            v,
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Deterministic omen + convergence — the decided cards the model narrates (never computes).
// Folded in from the retired oracle.rs (2026-07-21). The PEAK ScoutingDecision lesson: conflict
// detection and direction are COMPUTED in code and handed to the model; the model narrates a
// decision, it never infers one.
// ---------------------------------------------------------------------------

/// The four omens the reading may land on. A closed set (CHECK constraint, mig 146) so the served
/// card can badge it.
pub const OMENS: [&str; 4] = ["ascendant", "steady", "waning", "crossroads"];

fn direction_sign(key: &str) -> i32 {
    match key {
        "rising" => 1,
        "falling" => -1,
        _ => 0,
    }
}

/// pillar_convergence turns the deterministic pillar comparisons into a 1-100 agreement number —
/// a computed MEASUREMENT, not a model opinion (this is the "convergence goes deterministic" half
/// of the crown fold). `round(100·agree/total)` floored at 1; `None` when no directional pair
/// exists (a quiet spread has nothing to converge on). The floor matches the DB contract
/// (`sigil_synthesis_convergence_check`: NULL or 1-100) — an all-disagree spread rounds to 0,
/// which the check rejects and which carries no product meaning beyond 1 (anything ≤ 50 is
/// already a crossroads to `compute_omen`, faithfully preserving the panel's soft rule).
pub fn pillar_convergence(comparisons: &[PillarComparison]) -> Option<i32> {
    if comparisons.is_empty() {
        return None;
    }
    let agree = comparisons.iter().filter(|c| c.agree).count();
    Some((((agree as f64 / comparisons.len() as f64) * 100.0).round() as i32).max(1))
}

/// compute_omen decides the reading's direction deterministically, with a one-line computed reason
/// rendered into the prompt:
/// - a split spread (convergence ≤ 50 — half or more of the directional pairs disagree) is a
///   `crossroads` regardless of net direction — the contested arc IS the story;
/// - otherwise Momentum decides alone (or10: the raw trajectory marker left the crown's math —
///   the Oracle reads the Analyst's decided direction, never the tracker): positive ⇒
///   `ascendant`, negative ⇒ `waning`, nothing directional ⇒ `steady`.
pub fn compute_omen(convergence: Option<i32>, mom: &SynthMomentum) -> (&'static str, String) {
    if let Some(c) = convergence {
        if c <= 50 {
            return (
                "crossroads",
                "the cards pull against each other; the arc is contested".to_string(),
            );
        }
    }
    let net = mom.direction.as_deref().map(direction_sign).unwrap_or(0);
    if net > 0 {
        (
            "ascendant",
            "the recent trajectory points upward and no card disputes it".to_string(),
        )
    } else if net < 0 {
        (
            "waning",
            "the recent trajectory points downward and no card disputes it".to_string(),
        )
    } else {
        (
            "steady",
            "no card shows real movement; the arc holds its line".to_string(),
        )
    }
}

// ---------------------------------------------------------------------------
// The crown reading prompt — the model reads the signs (the five cards + the omen),
// then renders the verdict. (`load_prior_read` and its continuity card were DELETED at or9 —
// the crown is blind to memories, and the audit confirmed the fn had no other caller: the
// serving read path is Go's, not this crate's.)
// ---------------------------------------------------------------------------
// Output parsing — the crown reply is a bare {reading, score} object under format_schema.
// ---------------------------------------------------------------------------

/// parse_crown_score coerces the emitted score to an integer 1-100. `format_schema` makes it an
/// integer on the live route; the coercions (float round, a `"73/100"` or bare-string form) keep
/// the offline/no-schema eval path tolerant. Clamped to 1-100 like the retired panel's parser.
fn parse_crown_score(v: &serde_json::Value) -> Option<i32> {
    let n = if let Some(i) = v.as_i64() {
        i
    } else if let Some(f) = v.as_f64() {
        if !f.is_finite() {
            return None;
        }
        f.round() as i64
    } else if let Some(s) = v.as_str() {
        let head = s.split_whitespace().next()?;
        let head = head.split_once('/').map(|(n, _)| n).unwrap_or(head).trim();
        match head.parse::<i64>() {
            Ok(n) => n,
            Err(_) => head.parse::<f64>().ok().filter(|f| f.is_finite())?.round() as i64,
        }
    } else {
        return None;
    };
    Some(n.clamp(1, 100) as i32)
}

/// parse_crown_reply extracts `{reading, score}` from the JSON reply. On the ollama path
/// `format_schema` makes a bare object the only thing the live route emits; the balanced-brace
/// salvage keeps the offline/eval path tolerant of a prose-wrapped object. Reading whitespace is
/// collapsed to one clean paragraph. `None` when there is no non-empty reading or no coercible
/// score (fail-closed → the item backs off).
///
/// **The control-char fold in the salvage path (or9, measured 2026-08-10):** on oMLX the
/// OpenAI backend deliberately withholds `response_format` (the tekken corruption finding), so
/// the crown reply is UNCONSTRAINED — and the 8B writes paragraph breaks as literal newlines
/// INSIDE the JSON string, which is illegal JSON and failed a complete, well-formed reply
/// (finish_reason stop, closing brace present) on both parse paths. Folding `\n\r\t` to spaces
/// inside the brace span is semantics-preserving here: structural whitespace is insignificant
/// to JSON, and in-string whitespace is collapsed by the reading normalizer two lines down
/// anyway. The strict path stays first, untouched.
pub fn parse_crown_reply(raw: &str) -> Option<CrownReply> {
    let trimmed = raw.trim();
    let parsed: Option<serde_json::Value> = serde_json::from_str(trimmed).ok().or_else(|| {
        let start = trimmed.find('{')?;
        let end = trimmed.rfind('}')?;
        let span = trimmed[start..=end].replace(['\n', '\r', '\t'], " ");
        serde_json::from_str(&span).ok()
    });
    let v = parsed?;
    let reading = v.get("reading")?.as_str()?.trim();
    let reading = reading.split_whitespace().collect::<Vec<_>>().join(" ");
    if reading.is_empty() {
        return None;
    }
    let score = parse_crown_score(v.get("score")?)?;
    Some(CrownReply { reading, score })
}

/// count_sentences approximates the reading's sentence count for the eval budget checks: a
/// sentence ends at a run of `.` / `!` / `?` followed by whitespace or end-of-text. A decimal
/// point ("a 2.5 assist bump") is followed by a digit, so it never counts. (Folded from oracle.rs.)
pub fn count_sentences(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut n = 0;
    let mut i = 0;
    while i < chars.len() {
        if matches!(chars[i], '.' | '!' | '?') {
            let mut j = i + 1;
            while j < chars.len() && matches!(chars[j], '.' | '!' | '?') {
                j += 1;
            }
            if j >= chars.len() || chars[j].is_whitespace() {
                n += 1;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    n
}

/// CrownParser is the crown stage's `Parser` plug-in behind the `Parser<T>` seam. It never returns
/// the fail-closed `Ok(None)` — the crown's only fail-closed path is the pre-model no-pillar marker;
/// an unparseable reply (no reading or no score) is a genuine failure → `Err` → the item backs off.
pub struct CrownParser;

impl Parser<CrownReply> for CrownParser {
    fn parse(&self, raw: &str) -> Result<Option<CrownReply>> {
        match parse_crown_reply(raw) {
            Some(r) => {
                // The eval→guard migration (2026-08-19, DOCTRINE-directing.md): the reading's
                // global invariants fail closed in production — internal vocabulary, the verdict
                // formula, a peer roll call, product names, foreign script. Same lists as the
                // gate (`crate::guards`); the retry re-rolls for a discreet reading.
                if let Some(p) =
                    crate::guards::first_banned_phrase(&r.reading, crate::guards::ORACLE_READING_BANS)
                {
                    tracing::warn!(guard = "oracle_reading_ban", phrase = p, "reading rejected");
                    bail!("crown: reading carries banned vocabulary {p:?}");
                }
                let peers = crate::guards::count_named_peers(&r.reading);
                if peers > 1 {
                    tracing::warn!(guard = "peer_roll_call", peers, "reading rejected");
                    bail!("crown: reading names {peers} peer seats (max 1)");
                }
                if let Some(p) = crate::guards::first_product_name(&r.reading) {
                    tracing::warn!(guard = "product_name", name = p, "reading rejected");
                    bail!("crown: reading names product {p:?}");
                }
                if crate::util::has_foreign_script(&r.reading) {
                    tracing::warn!(guard = "foreign_script", "reading rejected");
                    bail!("crown: reading carries a foreign-script run");
                }
                Ok(Some(r))
            }
            None => bail!(
                "crown: could not parse reading+score from response (raw={:?})",
                truncate(raw, 200)
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// The core generate + the production handler.
// ---------------------------------------------------------------------------

// The old panel-core helpers were retired with the crown fold (2026-07-21). The
// handler below inlines the single OracleLogic call, and the crown eval task builds
// the prompt directly.

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
        "omen": out.omen,
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

/// persist_to_sigil_synthesis writes one crown row — the scored reading OR the no-pillar NULL
/// marker, which differ only in the bound values. One call now, so the crown's model/prompt IS
/// the voice's: voiced_score echoes the emitted score (the verdict IS the voiced score), and
/// voiced_at/voice_* are stamped only when a reading was drawn (NULL for the marker). trigger_type
/// 'periodic', trigger_payload `{}`. The moat fields route through the shared `Provenance` envelope.
async fn persist_to_sigil_synthesis(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    season: i32,
    out: &SigilOutput,
    previous_score: Option<i16>,
) -> Result<i64> {
    let prov = out.provenance();
    let entity_id = item.entity_id_i32()?;
    let score: Option<i16> = out.score.map(|n| n as i16);
    // Deterministic convergence (mig 143 nullable smallint) — None for a marker or a spread with
    // no directional pair; rides the same 1-100 shape as `score`.
    let convergence: Option<i16> = out.convergence.map(|n| n as i16);
    let row = sqlx::query(
        r#"
        INSERT INTO sigil_synthesis (
            entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            score, previous_score, input_components, input_hash,
            model_version, prompt_version, convergence,
            reading, omen, voiced_score,
            voiced_at, voice_model_version, voice_prompt_version
        ) VALUES ($1,$2,$3,$4,'periodic','{}'::jsonb, $5,$6,$7::jsonb,$8, $9,$10,$11,
            $12,$13,$14,
            CASE WHEN $12 IS NOT NULL THEN NOW() END,
            CASE WHEN $12 IS NOT NULL THEN $9 END,
            CASE WHEN $12 IS NOT NULL THEN $10 END)
        RETURNING id
        "#,
    )
    .bind(&item.entity_type) // $1
    .bind(entity_id) // $2
    .bind(sport) // $3
    .bind(season) // $4
    .bind(score) // $5
    .bind(previous_score) // $6
    .bind(out.input_components_json.as_str()) // $7
    .bind(prov.input_hash.as_deref()) // $8
    .bind(prov.model_version.as_str()) // $9  (also voice_model_version when reading present)
    .bind(prov.prompt_version) // $10 (also voice_prompt_version when reading present)
    .bind(convergence) // $11
    .bind(out.reading.as_deref()) // $12
    .bind(out.omen) // $13
    .bind(score) // $14  voiced_score = the emitted score (they reconcile)
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
            // Stage names WHERE the call ran, lens/role name WHAT ran: the crown is now the
            // single OracleLogic call at the sigil stage (the panel's SynthesisLogic row is gone).
            stage: "sigil".to_string(),
            lens: "oracle".to_string(),
            role: Role::OracleLogic.as_str().to_string(),
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
            output_contract_version: ORACLE_OUTPUT_CONTRACT_VERSION.to_string(),
            input_ids: Vec::new(),
            input_hash: out.input_hash.clone(),
            request_body: out.request_body.clone(),
            built_prompt: out.built_prompt.clone(),
            included_evidence: sigil_included_evidence(out),
            excluded_evidence: sigil_excluded_evidence(out),
            context_budget: serde_json::json!({
                "num_predict": ORACLE_NUM_PREDICT,
                "eval_count": out.eval_count,
                "wall_ms": out.wall_ms,
            }),
            parser_outcome: sigil_parser_outcome(out).to_string(),
        },
    )
    .await;
}

/// SigilHandler drains the durable `sigil` stage. It reads the pillars season-exact,
/// skips the model call when the pillar hash is unchanged (`debounce_unchanged`), otherwise
/// calls OracleLogic and persists one `sigil_synthesis` row carrying the reading and score.
/// Terminal stage: it enqueues nothing downstream.
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

        // No-pillar marker (no model call): no cards turned up, so the crown has nothing to read.
        if narratives.is_empty()
            && rating.is_none()
            && vibe.is_none()
            && momentum.empty()
            && transfers.is_empty()
        {
            let out = SigilOutput {
                score: None,
                reading: None,
                season,
                input_components_json: "{}".to_string(),
                input_hash: None,
                model: hx.router.for_role(Role::OracleLogic).model().to_string(),
                prompt_version: ORACLE_PROMPT_VERSION,
                convergence: None,
                omen: None,
                built_prompt: None,
                request_body: None,
                eval_count: None,
                wall_ms: None,
            };
            // The marker carries NULL reading/voice columns — serve-latest ignores markers, so the
            // last real reading keeps serving.
            let product_row_id =
                persist_to_sigil_synthesis(&hx.pool, item, &sport, season, &out, None).await?;
            write_sigil_ledger(&hx.pool, item, entity_id, &sport, &out, product_row_id).await;
            return Ok(());
        }

        // SkipUnchanged debounce: skip the crown call when the pillar input hash matches the
        // entity-season's latest synthesis. The pillar-inputs hash is byte-stable from the panel
        // era, so existing rows debounce exactly as before — the fold re-fires nothing.
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
        // One round-trip to the latest synthesis row for the debounce hash + the previous-score
        // baseline. (The prior blurb is gone with the panel; prior READINGS load below as memory.)
        let (prev_score_raw, latest_hash) = hx.latest_with_hash("sigil_synthesis", &key).await?;
        if latest_hash.as_deref() == Some(input_hash.as_str()) {
            return Ok(());
        }
        let prev = prev_score_raw.map(|v| v as i32).unwrap_or(0);

        // Deterministic convergence + omen, computed BEFORE the call and handed to the model as
        // decided cards (the PEAK ScoutingDecision discipline): the crown reads them, never infers.
        let comparisons =
            build_pillar_divergence(&narratives, rating.as_ref(), vibe.as_ref(), &momentum);
        let convergence = pillar_convergence(&comparisons);
        let (omen, omen_reason) = compute_omen(convergence, &momentum);

        // or9 (Scott, 2026-08-10 evening): the crown is BLIND TO MEMORIES — the prior-read and
        // relational-memory loads are gone with their prompt blocks (and `load_prior_read` is
        // deleted outright: its only caller was here). Both were prompt-only and outside the
        // input_hash, so removing them regenerates nothing by itself; the reading is the five
        // cards + the omen, whole.

        // The 4096 envelope (7.8): in a SMALL window every pillar body is capped and the
        // reservation shrinks, because the crown is the ONE seat that reads five cards at once
        // and, until now, truncated none of them. Keyed on the window rather than the rail
        // (Scott, 2026-08-06): the cards are the same size whichever corpus produced them, so it
        // is the room they have to fit in that decides. The Oracle itself reads no packet — §4
        // keeps it blind to evidence: five cards and its own verdict trail, nothing else.
        let small = crate::route::small_voice_window(hx.voice_num_ctx);
        let body_cap = small.then_some(prompt::CROWN_CARD_BODY_CAP);

        // The one crown call (OracleLogic): read the cards + the omen, then emit
        // {reading, score}. Fail-closed lives in CrownParser (unparseable → Err → the item backs off).
        let prompt = build_crown_prompt(
            &item.entity_type,
            &name,
            &item.sport,
            &narratives,
            rating.as_ref(),
            vibe.as_ref(),
            &momentum,
            &transfers,
            omen,
            &omen_reason,
            body_cap,
        );
        let opts = GenerateOptions {
            system: Some(ORACLE_SYSTEM_PROMPT.to_string()),
            temperature: Some(ORACLE_TEMPERATURE),
            num_predict: if small {
                SMALL_WINDOW_NUM_PREDICT
            } else {
                ORACLE_NUM_PREDICT
            },
            num_ctx: hx.voice_num_ctx,
            json_mode: false,
            format_schema: Some(oracle_format_schema()),
            format_schema_raw: None,
        };
        let extracted = hx
            .extract(Role::OracleLogic, &prompt, &opts, &CrownParser)
            .await?;
        let reply = extracted
            .value
            .ok_or_else(|| anyhow!("crown: parser returned no value"))?;

        let out = SigilOutput {
            score: Some(reply.score),
            reading: Some(reply.reading),
            season,
            input_components_json,
            input_hash: Some(input_hash),
            model: extracted.model,
            prompt_version: ORACLE_PROMPT_VERSION,
            convergence,
            omen: Some(omen),
            built_prompt: Some(extracted.built_prompt),
            request_body: Some(extracted.request_body),
            eval_count: Some(extracted.eval_count),
            wall_ms: Some(extracted.wall_ms),
        };
        let prev_score: Option<i16> = if prev > 0 { Some(prev as i16) } else { None };
        let product_row_id =
            persist_to_sigil_synthesis(&hx.pool, item, &sport, season, &out, prev_score).await?;
        write_sigil_ledger(&hx.pool, item, entity_id, &sport, &out, product_row_id).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
