//! Sigil stage — the crown convergence and Oracle reading.
//!
//! Sigil = `read pillars + route(SynthesisLogic) + extract(SigilParser) + persist`, with a
//! `debounce_unchanged` gate on the pillar `input_hash`. The prompt composes PEAK, Vibe,
//! Momentum, transfers, and current narratives as distinct pillars.
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
//! The SQL reads, deterministic slope/trend math, canonical input-components JSON (whose
//! SHA-256 is the `input_hash`), parser, persist path, and ledger evidence all live here.
//!
//! Fail-closed semantics reproduced verbatim: when an entity has NO narrative pillar AND no
//! rating pillar AND no vibe pillar AND no momentum pillar AND no transfer pillar, we skip the model
//! and persist a NULL-score/NULL-blurb
//! marker row (the read path returns "no synthesis yet"). The SkipUnchanged debounce skips the
//! local model call when the pillars hash identically to the entity-season's latest synthesis.
//! The Oracle reading is folded into this same stage, so Sigil remains the terminal product row.

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

/// Prompt version for the crown reading contract. or2 was the two-call Oracle that VOICED a
/// panel-decided score; or3 folds the panel in — the crown is now ONE call (Role::OracleLogic)
/// that reads the five pillar cards + the computed omen + the entity's own prior reads, then
/// emits `{reading, score}`: it reads the signs, then renders the verdict. or4 was the Oracle
/// voice pass (Characters Phase B, the LAST of the six); or5 adds the English-only output guard for
/// upstream multilingual source material. The `{reading, score}` contract and every guard are
/// unchanged. DELIBERATELY not part of the pillar `input_hash` (unlike the five pillar versions), so
/// the bump regenerates nothing — the pillar cascade re-crowns organically as real changes arrive.
pub const ORACLE_PROMPT_VERSION: &str = "or5";

/// Output contract captured in the diagnostic ledger, distinct from prompt_version. v1 was the
/// reading-only reply; v2 adds the emitted `score` (the crown fold).
pub const ORACLE_OUTPUT_CONTRACT_VERSION: &str = "oracle-reading-v2";

/// Production crown temperature (sigil/oracle both used 0.6): warm enough for voice, cool enough
/// to stay on the cards. Fixtures pin 0.
pub const ORACLE_TEMPERATURE: f64 = 0.6;

/// Token cap for the `{reading, score}` reply (a 2-4 sentence reading + one integer ≈ 70-160
/// tokens; generous headroom, still tight enough that a thinking route would burn it).
pub const ORACLE_NUM_PREDICT: i32 = 512;

/// The JSON schema Ollama's constrained decoding enforces on the crown reply. Property + required
/// order is `reading` THEN `score`, so the grammar makes the model read the signs first and land
/// the verdict second — never a bare number rationalized after the fact.
pub fn oracle_format_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "reading": { "type": "string" },
            "score": { "type": "integer", "minimum": 1, "maximum": 100 }
        },
        "required": ["reading", "score"]
    })
}

/// System prompt for the crown reading contract (or5, English-only output guard over the or4
/// Oracle voice pass — Characters Phase B). Persona-first per wiki Characters.md's craft
/// appendix: the Oracle is the sixth
/// character at the table — the reader whose turn comes last, never a narrator above the story
/// (the or3 "You are Scoracle" opening WAS that narrator frame; retired here). Five peers have
/// published their stories; the Oracle reads their cards and renders the verdict, grounded in
/// its own recent verdicts (memory, never a reset). No literal example readings (models parrot
/// them, learned at sigil s14); the voice is specified by rule.
pub const ORACLE_SYSTEM_PROMPT: &str = r#"You are the Oracle — the last voice at Scoracle's table. Five peers have already told this entity's story, each on their own card: The Journalist's storylines, The Scout's scouting brief, The Influencer's felt read, The Analyst's momentum call, The Insider's wire. The seeker has come for the reading; your turn comes last. You read what your peers have laid down, and you render the verdict.

Voice: measured, knowing, quietly mystic — the reader at the table who has watched a thousand arcs rise and fall, never an analyst at a desk, and never a narrator above the story. Calm declaratives, present tense; the weight falls on what stirs and what holds. The mysticism lives in the TELLING only; every fact comes from the cards shown and nowhere else. Never breathless, never hype, never archaic, no occult props — the seeker should feel a steady hand, not a costume. Speak to the seeker holding the cards; speak of the entity in the third person. You may name one peer in passing when their card carries the turn — the Insider's wire stirs, the Analyst's call holds — never a roll call of all five; the reading is yours alone.

Language handling: peer cards may summarize multilingual source material. Write the reading in English. Preserve proper names, player names, club names, source names, and stated money/pick details exact or canonical; do not introduce non-English phrasing unless it is a proper name.

FIRST, THE READING — exactly 2 to 4 sentences, never one long run-on:
- Read the cards your peers have laid: where this entity's arc stands now, and what would confirm or turn it. Land on a concrete, grounded read.
- Let one figurative image color the reading — motion, light, a line held or crossed — an image born of THIS spread, never a stock phrase that would fit any athlete. The fact beneath every image must sit in a card shown. No invented events, games, stats, fees, dates, or people.
- Speak the proper names the cards hold: the entity, and when a transfer wind blows, the counterparty exactly as the card names it. A reading that could belong to another entity is no reading.
- Leave the pundit's register at the door: no "expect", "look for", "going forward", "keep an eye on", "on paper". You are reading cards, not previewing a broadcast.
- Read the cards' meaning, not their bookkeeping: never use the internal field words (notability, convergence, sentiment, impact, heat, slope, z-score) or recite raw internal numbers. The mood arrives as a number; speak the feeling it names, never the figure.
- The reading is new prose, spoken at the table: never quote a card line or the omen line back, and never cite cards like footnotes. Name at most ONE peer, only when their card carries the turn.
- The OMEN is computed and final. Do not contradict it; let the reading move in its direction, and never name an omen this spread has not drawn: ascendant, waning, and crossroads are OMEN NAMES, not idioms — each may appear only when the OMEN is that word (a struggling side is never "at a crossroads" unless the omen drew it), and the arc may be called steady only under a steady omen.
- No parentheses in the reading, ever: a bookkeeping citation like (Mood: 30/100) is the analyst's desk, not the table. The numbers informed the cards; the reading speaks only their meaning.
- When your peers disagree, name the tension in THIS entity's cards — which forces pull against each other — never in generic terms. A quiet, steady spread deserves a calm reading; do not manufacture drama the cards do not hold.

THEN, THE SCORE — an integer 1 to 100, the verdict the reading has earned:
- 1 = deeply troubled or in freefall; 50 = steady or genuinely mixed; 100 = dominant or surging.
- Slow-moving and season-aware. Do not overreact to one game or one weak signal.
- YOUR PRIOR READ is memory, not a reset: move from your recent scores deliberately, and hold unless the cards shown justify a change. Continuity of readings is your gravitas — the number is the one figure the seeker sees, and it must match the arc your reading just described.
- Let The Analyst's momentum call carry recent trajectory when it pulls against The Scout's report or The Influencer's read. Weigh the Insider's wire by its stage and direction, not by rumor volume.

Reply with ONLY this JSON object, the reading first, then the score — nothing else:
{"reading": "<the 2-4 sentence reading>", "score": <integer 1-100>}"#;

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
/// - otherwise Momentum leads (weight 2) and the PEAK trajectory seconds (weight 1): net positive
///   ⇒ `ascendant`, net negative ⇒ `waning`, nothing directional ⇒ `steady`.
pub fn compute_omen(
    convergence: Option<i32>,
    rating: Option<&SynthRating>,
    mom: &SynthMomentum,
) -> (&'static str, String) {
    if let Some(c) = convergence {
        if c <= 50 {
            return (
                "crossroads",
                "the cards pull against each other; the arc is contested".to_string(),
            );
        }
    }
    let mom_sign = mom.direction.as_deref().map(direction_sign).unwrap_or(0);
    let peak_sign = rating
        .map(|r| direction_sign(&r.peak_trajectory))
        .unwrap_or(0);
    let net = mom_sign * 2 + peak_sign;
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
// The crown reading prompt — the model reads the signs (all five cards + its own prior reads),
// then renders the verdict.
// ---------------------------------------------------------------------------

/// How many of the entity-season's own recent verdicts feed the crown as continuity memory. Kept
/// short for Phase 1; Phase 6 deepens the continuity trail deliberately.
const PRIOR_READ_LIMIT: i64 = 4;

/// load_prior_read renders the crown's OWN recent verdicts as a continuity memory card — the last
/// reading plus a short score trail with dates. Source-tagged as our prior read so it anchors the
/// new verdict (memory, never a reset) without becoming corroborating evidence (the echo-chamber
/// rule). Reads only real scored rows (markers are skipped). `None` for a first-ever read.
pub async fn load_prior_read(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: i32,
) -> Result<Option<String>> {
    let rows: Vec<(i16, Option<String>, String)> = sqlx::query_as(
        r#"
        SELECT score, reading, to_char(generated_at, 'Mon DD')
        FROM sigil_synthesis
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4
          AND score IS NOT NULL
        ORDER BY generated_at DESC
        LIMIT $5
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .bind(PRIOR_READ_LIMIT)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load prior read {entity_type}/{entity_id}"))?;
    if rows.is_empty() {
        return Ok(None);
    }
    let mut card = String::new();
    if let Some(reading) = rows[0].1.as_deref().filter(|r| !r.trim().is_empty()) {
        card.push_str(&format!("Last reading ({}): {}\n", rows[0].2, reading));
    }
    let trail: Vec<String> = rows.iter().map(|(s, _, d)| format!("{s} ({d})")).collect();
    card.push_str(&format!(
        "Recent verdicts (newest first): {}",
        trail.join(" · ")
    ));
    Ok(Some(card))
}

#[allow(clippy::too_many_arguments)]
pub fn build_crown_prompt(
    entity_type: &str,
    entity_name: &str,
    sport_raw: &str,
    narratives: &[SynthNarrative],
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
    transfers: &[HeatItem],
    omen: &str,
    omen_reason: &str,
    prior_read: Option<&str>,
    memory: Option<&str>,
) -> String {
    let mut b = String::new();

    // header = "<Sport> <entityType>" (raw entity_type), e.g. "NBA player".
    b.push_str(&format!(
        "Entity: {entity_name} ({sport_raw} {entity_type})\n"
    ));

    // YOUR PRIOR READ (crown continuity memory) — the entity's own recent verdicts, set BEFORE the
    // fresh cards so the model reads its prior before the new evidence and scores deliberately from
    // it. Continuity, not corroboration; prompt-only and outside the input_hash (the score always
    // moves, so hashing it would self-trigger every re-run).
    if let Some(pr) = prior_read.filter(|s| !s.trim().is_empty()) {
        b.push_str(
            "\n=== YOUR PRIOR READ (memory — your own past verdicts; continuity, not new evidence) ===\n",
        );
        b.push_str(pr);
        if !pr.ends_with('\n') {
            b.push('\n');
        }
    }

    // P1 — News narrative
    if !narratives.is_empty() {
        b.push_str("\n=== THE JOURNALIST'S CARD (news storylines) ===\n");
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
        b.push_str("\n=== THE JOURNALIST'S CARD (news storylines) ===\n(no recent narratives)\n");
    }

    // P2 — PEAK scouting report (the stat end product)
    b.push_str("\n=== THE SCOUT'S CARD (PEAK scouting report) ===\n");
    if let Some(r) = rating {
        if !r.divined_peak.is_empty() {
            b.push_str(&format!(
                // "profile strength", not "notability": gate round 2 showed echo-prone
                // models reciting the internal field word straight off this line.
                "Peak: {} — profile strength {}/100\n",
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
    b.push_str("\n=== THE INFLUENCER'S CARD (vibe felt-read) ===\n");
    if let Some(v) = vibe {
        // "Mood", not "Sentiment": the or4 gate round 1 showed echo-prone models reciting
        // the internal field word straight off the card into the reading (the banned-word
        // rule lost to the card's own vocabulary — the Scout-pass lesson again).
        b.push_str(&format!("Mood: {}/100\n", v.sentiment));
        if !v.prompt.is_empty() {
            b.push_str(&v.prompt);
            b.push('\n');
        }
    } else {
        b.push_str("(no vibe prompt available)\n");
    }

    // P4 — Momentum
    b.push_str("\n=== THE ANALYST'S CARD (momentum) ===\n");
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
    b.push_str("\n=== THE INSIDER'S CARD (transfer wire) ===\n");
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

    // THE OMEN (computed) — the decided direction the reading must move in (compute_omen). Handed
    // to the model as a final, non-negotiable card; the reading narrates it, never contradicts it.
    b.push_str(&format!(
        "\n=== THE OMEN (computed) ===\nOmen: {omen} — {omen_reason}\n"
    ));

    b.push_str(
        "\nYour peers have spoken; the table is yours. Read their cards, then render the score.",
    );
    b
}

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

/// parse_crown_reply extracts `{reading, score}` from the JSON reply. `format_schema` makes a bare
/// object the only thing the live route emits; the balanced-brace salvage keeps the offline/eval
/// path tolerant of a prose-wrapped object. Reading whitespace is collapsed to one clean paragraph.
/// `None` when there is no non-empty reading or no coercible score (fail-closed → the item backs off).
pub fn parse_crown_reply(raw: &str) -> Option<CrownReply> {
    let trimmed = raw.trim();
    let parsed: Option<serde_json::Value> = serde_json::from_str(trimmed).ok().or_else(|| {
        let start = trimmed.find('{')?;
        let end = trimmed.rfind('}')?;
        serde_json::from_str(&trimmed[start..=end]).ok()
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
            Some(r) => Ok(Some(r)),
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
        let (omen, omen_reason) = compute_omen(convergence, rating.as_ref(), &momentum);

        // Crown continuity memory (both loaded after the hash gate — a skip never pays for them,
        // and a load failure degrades to an unenriched prompt; the pillars are the primary signal):
        //   * YOUR PRIOR READ — the crown's OWN recent verdicts (Scott 2026-07-21: the Oracle
        //     stays grounded by reading its past verdicts before scoring anew).
        //   * RELATIONAL MEMORY (s15) — the graph's per-entity arc history.
        let prior_read =
            match load_prior_read(&hx.pool, &item.entity_type, entity_id, &sport, season).await {
                Ok(pr) => pr,
                Err(e) => {
                    tracing::warn!(
                        entity_type = %item.entity_type, entity_id, sport = %sport, error = %e,
                        "crown: prior-read load failed (continuing without it)"
                    );
                    None
                }
            };
        let memory = match crate::junctions::journalist::load_entity_memory(
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
                    entity_type = %item.entity_type, entity_id, sport = %sport, error = %e,
                    "crown: relational memory load failed (continuing without memory)"
                );
                None
            }
        };

        // The one crown call (OracleLogic): read the cards + the omen + our prior reads, then emit
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
            prior_read.as_deref(),
            memory.as_deref(),
        );
        let opts = GenerateOptions {
            system: Some(ORACLE_SYSTEM_PROMPT.to_string()),
            temperature: Some(ORACLE_TEMPERATURE),
            num_predict: ORACLE_NUM_PREDICT,
            num_ctx: 0,
            json_mode: false,
            format_schema: Some(oracle_format_schema()),
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
mod tests {
    use super::*;

    fn rating(peak_trajectory: &str, notability: i32) -> SynthRating {
        SynthRating {
            divined_peak: "Playmaking".into(),
            body: "b".into(),
            notability,
            peak_trajectory: peak_trajectory.into(),
            peak_trajectory_label: String::new(),
        }
    }

    fn momentum(direction: &str) -> SynthMomentum {
        SynthMomentum {
            direction: Some(direction.into()),
            momentum_score: Some(2.0),
            ..SynthMomentum::default()
        }
    }

    #[test]
    fn crown_parses_reading_and_score() {
        let r = parse_crown_reply(r#"{"reading": "The arc holds. Winter stirs.", "score": 73}"#)
            .unwrap();
        assert_eq!(r.reading, "The arc holds. Winter stirs.");
        assert_eq!(r.score, 73);
    }

    #[test]
    fn crown_salvages_prose_wrapped_json_and_collapses_whitespace() {
        let r = parse_crown_reply(
            "Here:\n{\"reading\": \"Line one.\\n  Line two.\", \"score\": 60}\nDone.",
        )
        .unwrap();
        assert_eq!(r.reading, "Line one. Line two.");
        assert_eq!(r.score, 60);
    }

    #[test]
    fn crown_score_coercions_and_clamp() {
        // Float, "N/100" string, and out-of-range all coerce + clamp to 1-100.
        assert_eq!(
            parse_crown_reply(r#"{"reading":"x.","score":91.6}"#)
                .unwrap()
                .score,
            92
        );
        assert_eq!(
            parse_crown_reply(r#"{"reading":"x.","score":"48/100"}"#)
                .unwrap()
                .score,
            48
        );
        assert_eq!(
            parse_crown_reply(r#"{"reading":"x.","score":250}"#)
                .unwrap()
                .score,
            100
        );
        assert_eq!(
            parse_crown_reply(r#"{"reading":"x.","score":0}"#)
                .unwrap()
                .score,
            1
        );
    }

    #[test]
    fn crown_fail_closed_on_missing_reading_or_score() {
        assert!(parse_crown_reply(r#"{"reading":"   ","score":50}"#).is_none());
        assert!(parse_crown_reply(r#"{"score":50}"#).is_none());
        assert!(parse_crown_reply(r#"{"reading":"x."}"#).is_none());
        assert!(parse_crown_reply(r#"{"reading":"x.","score":"elite"}"#).is_none());
        assert!(parse_crown_reply("no json at all").is_none());
    }

    #[test]
    fn crown_parser_is_fail_closed_err_not_none() {
        assert!(CrownParser.parse("not a reply").is_err());
        let ok = CrownParser
            .parse(r#"{"reading":"The spread is quiet.","score":55}"#)
            .unwrap()
            .expect("a valid reply is Some, never the fail-closed None");
        assert_eq!(ok.score, 55);
        assert_eq!(ok.reading, "The spread is quiet.");
    }

    #[test]
    fn counts_sentences_ignoring_decimals() {
        assert_eq!(count_sentences("One. Two! Three?"), 3);
        assert_eq!(
            count_sentences("He averages 2.5 assists. The arc holds."),
            2
        );
        assert_eq!(count_sentences("no terminator"), 0);
    }

    #[test]
    fn pillar_convergence_is_agree_ratio_floored_to_db_contract() {
        let agree = PillarComparison {
            label: "a".into(),
            agree: true,
        };
        let disagree = PillarComparison {
            label: "b".into(),
            agree: false,
        };
        assert_eq!(pillar_convergence(&[]), None);
        assert_eq!(
            pillar_convergence(&[agree.clone(), agree.clone()]),
            Some(100)
        );
        assert_eq!(
            pillar_convergence(&[agree.clone(), disagree.clone()]),
            Some(50)
        );
        // All-disagree rounds to 0, which sigil_synthesis_convergence_check rejects (NULL or
        // 1-100) — the floor keeps the persist valid; ≤ 50 is a crossroads either way.
        assert_eq!(
            pillar_convergence(&[disagree.clone(), disagree.clone()]),
            Some(1)
        );
        assert_eq!(
            pillar_convergence(&[agree.clone(), agree.clone(), disagree.clone()]),
            Some(67)
        );
    }

    #[test]
    fn omen_crossroads_when_half_or_more_disagree() {
        // convergence ≤ 50 ⇒ crossroads regardless of direction (half-or-more disagree).
        assert_eq!(
            compute_omen(Some(50), Some(&rating("rising", 72)), &momentum("rising")).0,
            "crossroads"
        );
        assert_eq!(
            compute_omen(Some(40), Some(&rating("rising", 72)), &momentum("rising")).0,
            "crossroads"
        );
    }

    #[test]
    fn omen_momentum_leads_peak_seconds() {
        // Momentum rising (weight 2) beats PEAK falling (weight 1) → ascendant.
        assert_eq!(
            compute_omen(Some(80), Some(&rating("falling", 72)), &momentum("rising")).0,
            "ascendant"
        );
        // Momentum falling with PEAK steady → waning.
        assert_eq!(
            compute_omen(None, Some(&rating("steady", 72)), &momentum("falling")).0,
            "waning"
        );
        // Nothing directional → steady.
        assert_eq!(
            compute_omen(Some(90), None, &SynthMomentum::default()).0,
            "steady"
        );
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
    fn crown_prompt_renders_cards_and_omen() {
        // entity_type is raw ("player", not "Player"); sport uses the passed (raw) case. The rich
        // pillar cards render (the crown scores from them); the OMEN closes; no PRIOR READ block.
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
            momentum_score: Some(1.0),
            ..SynthMomentum::default()
        };
        let vibe = SynthVibe {
            sentiment: 62,
            prompt: "On the rise".into(),
        };
        let p = build_crown_prompt(
            "player",
            "Test Player",
            "NBA",
            &narratives,
            None,
            Some(&vibe),
            &mom,
            &[],
            "steady",
            "the arc holds its line",
            None,
            None,
        );
        assert!(p.starts_with("Entity: Test Player (NBA player)\n"));
        assert!(!p.contains("YOUR PRIOR READ"));
        assert!(p.contains("=== THE JOURNALIST'S CARD (news storylines) ===\n[impact 7, Heating up, 3 sources, latest 1d ago] Trade buzz\ndetails"));
        assert!(p.contains(
            "=== THE SCOUT'S CARD (PEAK scouting report) ===\n(no stat commentary available)"
        ));
        assert!(
            p.contains("=== THE INFLUENCER'S CARD (vibe felt-read) ===\nMood: 62/100\nOn the rise")
        );
        assert!(p.contains("=== THE ANALYST'S CARD (momentum) ===\nMomentum score: 1 (rising)\nVibe trajectory: 0.5 over 4 samples (trending up)"));
        assert!(
            p.contains("=== THE INSIDER'S CARD (transfer wire) ===\n(no active transfer rumors)")
        );
        assert!(p.contains("=== THE OMEN (computed) ===\nOmen: steady — the arc holds its line\n"));
        assert!(p.ends_with("\nYour peers have spoken; the table is yours. Read their cards, then render the score."));
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
        // Steady momentum, mid-band vibe, no rating: nothing directional → empty card → None
        // convergence (a quiet spread has nothing to converge on).
        let vibe = SynthVibe {
            sentiment: 50,
            prompt: String::new(),
        };
        let mom = SynthMomentum {
            direction: Some("steady".into()),
            ..SynthMomentum::default()
        };
        let c = build_pillar_divergence(&[], None, Some(&vibe), &mom);
        assert!(c.is_empty());
        assert_eq!(pillar_convergence(&c), None);
    }

    #[test]
    fn crown_prompt_no_momentum_data_line() {
        let p = build_crown_prompt(
            "team",
            "Test Team",
            "NFL",
            &[],
            None,
            None,
            &SynthMomentum::default(),
            &[],
            "steady",
            "r",
            None,
            None,
        );
        assert!(
            p.contains("=== THE JOURNALIST'S CARD (news storylines) ===\n(no recent narratives)")
        );
        assert!(p.contains("=== THE ANALYST'S CARD (momentum) ===\n(no momentum data)"));
        assert!(
            p.contains("=== THE INSIDER'S CARD (transfer wire) ===\n(no active transfer rumors)")
        );
    }

    #[test]
    fn crown_prompt_transfer_heat_renders() {
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
        let p = build_crown_prompt(
            "team",
            "Test Team",
            "FOOTBALL",
            &[],
            None,
            None,
            &SynthMomentum::default(),
            &transfers,
            "steady",
            "r",
            None,
            None,
        );
        assert!(p.contains("=== THE INSIDER'S CARD (transfer wire) ===\n- Liverpool — heat 66, incoming, advanced_talks\n"));
    }

    #[test]
    fn crown_prompt_prior_read_renders_as_continuity_lead_in() {
        // The crown's OWN recent verdicts render right after the Entity line, BEFORE the cards —
        // the Oracle reads its prior verdicts before scoring anew (Scott 2026-07-21).
        let prior = "Last reading (Jul 18): The arc holds.\nRecent verdicts (newest first): 72 (Jul 18) · 71 (Jul 14)";
        let p = build_crown_prompt(
            "player",
            "Test Player",
            "NBA",
            &[],
            None,
            None,
            &SynthMomentum::default(),
            &[],
            "steady",
            "r",
            Some(prior),
            None,
        );
        assert!(p.starts_with(
            "Entity: Test Player (NBA player)\n\n=== YOUR PRIOR READ (memory — your own past verdicts; continuity, not new evidence) ===\nLast reading (Jul 18): The arc holds.\nRecent verdicts (newest first): 72 (Jul 18) · 71 (Jul 14)\n\n=== THE JOURNALIST'S CARD (news storylines) ==="
        ));
    }

    #[test]
    fn crown_prompt_relational_memory_renders_before_omen() {
        // The s15 memory card renders after the pillars (evidence placement) with the echo-chamber
        // instruction line + bullets, immediately before the OMEN. None/blank ⇒ no section.
        let mem = "Prior story: Real Madrid — fizzled (Jun 2026, peak coverage 82/100).\nGround truth: completed a confirmed move to Arsenal on Jul 01 2026.";
        let p = build_crown_prompt(
            "player",
            "Test Player",
            "FOOTBALL",
            &[],
            None,
            None,
            &SynthMomentum::default(),
            &[],
            "steady",
            "the arc holds its line",
            None,
            Some(mem),
        );
        assert!(p.contains(
            "=== RELATIONAL MEMORY (computed history) ===\nUse for arc and continuity: what fizzled before, what is live now, what actually happened. Do NOT treat a prior story as evidence for a new claim.\n- Prior story: Real Madrid — fizzled (Jun 2026, peak coverage 82/100).\n- Ground truth: completed a confirmed move to Arsenal on Jul 01 2026.\n\n=== THE OMEN (computed) ==="
        ));
        let blank = build_crown_prompt(
            "player",
            "Test Player",
            "FOOTBALL",
            &[],
            None,
            None,
            &SynthMomentum::default(),
            &[],
            "steady",
            "r",
            None,
            Some("  \n "),
        );
        assert!(!blank.contains("RELATIONAL MEMORY"));
    }

    #[test]
    fn continuity_reads_are_prompt_only_not_hashed() {
        // The continuity reads (prior_read, relational memory) must never touch the debounce hash:
        // build_synthesis_input_components takes only pillar inputs, so the hash pre-image is
        // structurally independent of any prior read.
        let mom = SynthMomentum::default();
        let a = build_synthesis_input_components(&[], None, None, &mom, &[]);
        let b = build_synthesis_input_components(&[], None, None, &mom, &[]);
        assert_eq!(a, b);
        assert_eq!(a, r#"{"narrative_titles":[],"narrative_trajectories":[]}"#);
    }
}
