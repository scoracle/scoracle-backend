//! Oracle — the VOICE step of the Sigil stage (Session B of the sigil pipeline cleanup,
//! 2026-07-16; born as a separate stage in the lens quality plan, "The Oracle Lens"
//! 2026-07-12).
//!
//! Sigil remains the analytic crown synthesis (expert-panel voice); the Oracle is the mystic
//! who reads the decided card to the user — the ONE lens allowed to lean into the arcane
//! brand voice. Hard rule: the mysticism lives in the TELLING, never the facts — every claim
//! grounds in the cards handed to the prompt, nothing invented.
//!
//! Not a stage: `SigilHandler` runs decide → voice as two internal steps of one work item and
//! persists ONE `sigil_synthesis` row carrying both. This module owns everything voice-shaped:
//! the omen computation, the re-voice rule, the prompt/parse contract, and the transitional
//! `oracle_readings` dual-write (Go serving reads that table until Session C flips it).
//!
//! The re-voice rule (North Star #8, blessed by Scott 2026-07-16): a new reading is generated
//! ONLY when the story the card tells changes — the computed omen flips, the score crosses an
//! archetype band (±2pt hysteresis vs the last-VOICED score), or no reading exists yet. Raw
//! score deltas deliberately do NOT re-voice (offseason news-rail-only scores swing hard); a
//! voice-skipped row CARRIES FORWARD the prior reading/omen/voiced_score/voiced_at verbatim,
//! so the served card always holds its current voice and the drawn-at timestamp stays honest.
//! This REPLACES the old consumed-generation hash debounce, whose key included the exact
//! score and the temp-0.6 blurb prose — it re-voiced on essentially every real synthesis.
//!
//! The PEAK `ScoutingDecision` lesson applied from day one: the OMEN (ascendant / steady /
//! waning / crossroads) is COMPUTED in code from the consumed sigil + pillar directions and
//! handed to the model as a decided card — the model narrates a handed decision, it never
//! computes one. The reply is grammar-constrained (`format_schema`) to a single-field JSON
//! object, so the persona read can never arrive prose-wrapped.
//!
//! Fail-closed semantics: a no-pillar sigil marker carries NULL voice columns (no model
//! call). An unparseable or empty reply ⇒ `Err` → the whole sigil work item backs off
//! (nothing persisted; the retry re-runs decide — fail-closed beats a voiceless scored row).

use crate::corpus::{write_heat_lines, HeatItem};
use crate::harness::{Harness, Parser, Provenance};
use crate::ledger::{insert_cognition_ledger_best_effort, CognitionLedgerEntry};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::sigil::{SynthMomentum, SynthNarrative, SynthRating, SynthVibe};
use crate::trajectory::trajectory_label;
use crate::util::{go_json_string, hash_components, truncate};
use crate::work::Item;
use anyhow::{bail, Context, Result};
use sqlx::{PgPool, Row};

/// Prompt version for the Oracle reading contract. or1 was the first contract: deterministic
/// card stack + computed OMEN in the user prompt, subtle-mystic system voice, JSON
/// single-field reply under `format_schema`. or2 keeps that contract and recalibrates the
/// persona: identity per the North Star (the oracle's name IS Scoracle), the mystic register
/// deepened by rule (a required figurative image, the pundit's register banned), and an
/// omen-vocabulary rule — only the drawn omen's word may appear in the reading.
pub const ORACLE_PROMPT_VERSION: &str = "or2";

/// Output contract captured in the diagnostic ledger, distinct from prompt_version.
pub const ORACLE_OUTPUT_CONTRACT_VERSION: &str = "oracle-reading-v1";

/// Production reading temperature — matches sigil/narratives (0.6): warm enough for voice,
/// cool enough to stay on the cards. Fixtures pin 0.
pub const ORACLE_TEMPERATURE: f64 = 0.6;

/// Token cap for the single JSON reading reply (2-4 sentences ≈ 60-150 tokens; generous
/// headroom, still tight enough that a thinking-enabled route would burn it — if OracleLogic
/// ever routes to qwen3, pair COGNITION_ROUTE_ORACLE_LOGIC_THINK=false).
pub const ORACLE_NUM_PREDICT: i32 = 512;

/// The JSON schema Ollama's constrained decoding enforces on the Oracle reply. Grammar-level
/// guarantee that the reading arrives as a bare single-field object — no preamble, no
/// markdown, no invented extra fields.
pub fn oracle_format_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "reading": { "type": "string" }
        },
        "required": ["reading"]
    })
}

/// System prompt for the Oracle reading contract. No literal quotable example readings — the
/// models parrot them (learned at sigil s14); the voice is specified by rule, not by sample.
pub const ORACLE_SYSTEM_PROMPT: &str = r#"You are Scoracle. The seeker has come to your table for a reading on one sports entity; the house lenses have drawn their cards, and yours is the voice that turns them.

Voice: measured, knowing, quietly mystic — an oracle who has watched a thousand arcs rise and fall, not an analyst at a desk. Calm declaratives, present tense; the weight falls on what stirs and what holds. The mysticism lives in the TELLING only; every fact comes from the cards shown and nowhere else. Never breathless, never hype, never archaic, no occult props — the seeker should feel a steady hand, not a costume. Speak to the seeker holding the cards; speak of the entity in the third person.

THE READING:
- Exactly 2 to 4 sentences — never one long run-on sentence. Land on a concrete, grounded read: where the entity's arc stands now, and what would confirm or turn it.
- Let one figurative image color the reading — motion, light, a line held or crossed — an image born of THIS spread, never a stock phrase that would fit any athlete. The fact beneath every image must sit in a card shown. No invented events, games, stats, fees, dates, or people.
- Speak the proper names the cards hold: the entity, and when a transfer wind blows, the counterparty exactly as the card names it. A reading that could belong to another entity is no reading.
- Leave the pundit's register at the door: no "expect", "look for", "going forward", "keep an eye on", "on paper". You are reading cards, not previewing a broadcast.
- Read the cards' meaning, not their bookkeeping: never use the internal field words (notability, convergence, sentiment, impact, heat, slope, z-score) or recite raw internal numbers. The Sigil score is the one number a seeker sees; you may speak it plainly.
- The OMEN is computed and final. Do not contradict it; let the reading move in its direction, and never name an omen this spread has not drawn: the words ascendant, waning, and crossroads may only appear when the OMEN is that word, and the arc may be called steady only under a steady omen.
- When the cards conflict, name the tension in THIS entity's cards — which forces pull against each other — never in generic terms.
- A quiet, steady spread deserves a calm reading. Do not manufacture drama the cards do not hold.

Reply with ONLY this JSON object, nothing else:
{"reading": "<the 2-4 sentence reading>"}"#;

// ---------------------------------------------------------------------------
// The consumed Sigil generation — the Oracle's primary card and its debounce key.
// ---------------------------------------------------------------------------

/// The latest SCORED Sigil synthesis the Oracle reads. A marker row (NULL score) or no row at
/// all suppresses the whole reading (the Oracle has no cards to turn) — never a fall-back to
/// an older scored row a marker has superseded (the F-023 lesson, same as the pillar loaders).
#[derive(Clone, Debug)]
pub struct ConsumedSigil {
    pub score: i32,
    pub blurb: String,
    pub convergence: Option<i32>,
    pub disagreement: Option<String>,
    pub why_now: Option<String>,
    /// Sigil's own pillar input_hash — folded into the Oracle's components so a pillar change
    /// that re-ran Sigil (even to an identical score) still reads as a changed generation.
    pub input_hash: Option<String>,
}

/// load_latest_sigil reads the entity-season's LATEST sigil_synthesis row and suppresses it
/// when that latest generation is a no-pillar marker (score NULL).
pub async fn load_latest_sigil(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: i32,
) -> Result<Option<ConsumedSigil>> {
    #[allow(clippy::type_complexity)]
    let row: Option<(
        Option<i16>,
        Option<String>,
        Option<i16>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        r#"
        SELECT score, blurb, convergence, disagreement, why_now, input_hash
        FROM sigil_synthesis
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
    .with_context(|| format!("load consumed sigil {entity_type}/{entity_id}"))?;

    match row {
        None => Ok(None),
        Some((None, _, _, _, _, _)) => Ok(None), // latest generation is a marker → suppressed
        Some((Some(score), blurb, convergence, disagreement, why_now, input_hash)) => {
            Ok(Some(ConsumedSigil {
                score: score as i32,
                blurb: blurb.unwrap_or_default(),
                convergence: convergence.map(|c| c as i32),
                disagreement,
                why_now,
                input_hash,
            }))
        }
    }
}

/// build_oracle_input_components returns the canonical input-components JSON — the debounce
/// pre-image, covering exactly the consumed Sigil generation. Same stable Go-JSON shape as
/// sigil's components (sorted keys, escaped strings), so the SHA-256 128-bit hex prefix is a
/// deterministic debounce key. Pillar values are deliberately NOT here (see module doc).
pub fn build_oracle_input_components(sigil: &ConsumedSigil) -> String {
    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    pairs.push(("sigil_blurb", go_json_string(&sigil.blurb)));
    if let Some(c) = sigil.convergence {
        pairs.push(("sigil_convergence", c.to_string()));
    }
    if let Some(d) = &sigil.disagreement {
        pairs.push(("sigil_disagreement", go_json_string(d)));
    }
    if let Some(h) = &sigil.input_hash {
        pairs.push(("sigil_input_hash", go_json_string(h)));
    }
    pairs.push(("sigil_score", sigil.score.to_string()));
    if let Some(w) = &sigil.why_now {
        pairs.push(("sigil_why_now", go_json_string(w)));
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

// ---------------------------------------------------------------------------
// The computed OMEN — the decided card the model narrates (never computes).
// ---------------------------------------------------------------------------

/// The four omens the reading may land on. Kept a closed set (CHECK constraint, mig 146) so
/// the served card can badge it.
pub const OMENS: [&str; 4] = ["ascendant", "steady", "waning", "crossroads"];

fn direction_sign(key: &str) -> i32 {
    match key {
        "rising" => 1,
        "falling" => -1,
        _ => 0,
    }
}

/// compute_omen decides the reading's direction deterministically, with a one-line computed
/// reason rendered into the prompt:
/// - a split panel (sigil convergence < 50) is a `crossroads` regardless of direction — the
///   contested arc IS the story;
/// - otherwise Momentum leads (weight 2) and the PEAK trajectory seconds (weight 1): net
///   positive ⇒ `ascendant`, net negative ⇒ `waning`, nothing directional ⇒ `steady`.
pub fn compute_omen(
    sigil: &ConsumedSigil,
    rating: Option<&SynthRating>,
    mom: &SynthMomentum,
) -> (&'static str, String) {
    if let Some(c) = sigil.convergence {
        if c < 50 {
            return (
                "crossroads",
                "the panel's lenses tell conflicting stories; the arc is contested".to_string(),
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
            "the recent trajectory points upward and the panel does not dispute it".to_string(),
        )
    } else if net < 0 {
        (
            "waning",
            "the recent trajectory points downward and the panel does not dispute it".to_string(),
        )
    } else {
        (
            "steady",
            "no lens shows real movement; the arc holds its line".to_string(),
        )
    }
}

// ---------------------------------------------------------------------------
// The re-voice rule (North Star #8) — when does a decided card get a NEW voice?
// ---------------------------------------------------------------------------

/// The product's major-arcana score bands, `(lo, hi)` inclusive, descending — the SAME
/// discretization the web card renders (frontend `src/lib/vibe/archetypes.ts`; wiki
/// "Vibe Score Surface"). The band is the product's own definition of "the score moved
/// enough to change the card's face", which is why it is the re-voice threshold.
pub const ARCHETYPE_BANDS: [(i32, i32); 11] = [
    (95, 100), // The World
    (85, 94),  // The Sun
    (75, 84),  // The Star
    (65, 74),  // Strength
    (55, 64),  // The Magician
    (45, 54),  // Temperance
    (35, 44),  // The Hermit
    (25, 34),  // The Moon
    (15, 24),  // The Tower
    (5, 14),   // Death
    (1, 4),    // The Devil
];

/// archetype_band returns the `(lo, hi)` band a score falls in, clamping out-of-range
/// input to the nearest band (scores are CHECK-constrained 1-100; defensive only).
pub fn archetype_band(score: i32) -> (i32, i32) {
    for &(lo, hi) in &ARCHETYPE_BANDS {
        if score >= lo && score <= hi {
            return (lo, hi);
        }
    }
    if score > 100 {
        (95, 100)
    } else {
        (1, 4)
    }
}

/// The voice state the entity-season currently carries — the re-voice rule's baseline and
/// the carry-forward source. Loaded from the latest voiced `sigil_synthesis` row, falling
/// back to `oracle_readings` for pre-merge history.
#[derive(Clone, Debug)]
pub struct PriorVoice {
    pub reading: String,
    pub omen: Option<String>,
    /// The sigil score the reading was drawn under (`oracle_readings.sigil_score` on the
    /// fallback path). `None` on malformed history — treated as "re-voice".
    pub voiced_score: Option<i32>,
    /// Text-cast timestamptz (sqlx runs without date-time features; the value is only ever
    /// moved between rows, never computed with).
    pub voiced_at: String,
    pub model_version: String,
    pub prompt_version: String,
}

/// load_prior_voice reads the entity-season's current voice: merged rows first, then the
/// pre-merge `oracle_readings` history (real readings only — markers never count as voice).
pub async fn load_prior_voice(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: i32,
) -> Result<Option<PriorVoice>> {
    type MergedRow = (
        String,
        Option<String>,
        Option<i16>,
        String,
        Option<String>,
        Option<String>,
    );
    let merged: Option<MergedRow> = sqlx::query_as(
        r#"
            SELECT reading, omen, voiced_score, voiced_at::text,
                   voice_model_version, voice_prompt_version
            FROM sigil_synthesis
            WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4
              AND reading IS NOT NULL AND voiced_at IS NOT NULL
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
    .with_context(|| format!("load prior voice (merged) {entity_type}/{entity_id}"))?;
    if let Some((reading, omen, voiced_score, voiced_at, model_version, prompt_version)) = merged {
        return Ok(Some(PriorVoice {
            reading,
            omen,
            voiced_score: voiced_score.map(|s| s as i32),
            voiced_at,
            model_version: model_version.unwrap_or_default(),
            prompt_version: prompt_version.unwrap_or_default(),
        }));
    }

    type LegacyRow = (String, Option<String>, Option<i16>, String, String, String);
    let legacy: Option<LegacyRow> = sqlx::query_as(
        r#"
            SELECT reading, omen, sigil_score, generated_at::text, model_version, prompt_version
            FROM oracle_readings
            WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4
              AND reading IS NOT NULL
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
    .with_context(|| format!("load prior voice (legacy) {entity_type}/{entity_id}"))?;
    Ok(legacy.map(
        |(reading, omen, sigil_score, voiced_at, model_version, prompt_version)| PriorVoice {
            reading,
            omen,
            voiced_score: sigil_score.map(|s| s as i32),
            voiced_at,
            model_version,
            prompt_version,
        },
    ))
}

/// voice_should_regenerate is North Star #8 verbatim: a new reading is drawn ONLY when
/// - no prior voice exists (first reading), or
/// - the computed omen flipped, or
/// - the new score crossed OUT of the last-voiced score's archetype band with ±2pt
///   hysteresis (at least 2 points past the band edge — 84→85 flapping never re-voices).
///
/// Everything else keeps the prior reading, carried forward with its original timestamp.
pub fn voice_should_regenerate(prior: Option<&PriorVoice>, new_omen: &str, new_score: i32) -> bool {
    let Some(prior) = prior else {
        return true;
    };
    if prior.omen.as_deref() != Some(new_omen) {
        return true;
    }
    let Some(voiced) = prior.voiced_score else {
        return true; // malformed history — re-voice rather than carry an unanchored reading
    };
    let (lo, hi) = archetype_band(voiced);
    new_score <= lo - 2 || new_score >= hi + 2
}

// ---------------------------------------------------------------------------
// Prompt assembly — deterministic card lines; the model narrates.
// ---------------------------------------------------------------------------

/// Narrative storyline lines rendered into the card stack (titles + tags only — bodies are
/// news reporting, not cards; feeding them tempts the Oracle back into beat-writer voice).
const MAX_STORYLINE_CARDS: usize = 3;

/// build_oracle_prompt assembles the user prompt: the entity line, the deterministic card
/// stack (the consumed Sigil first — it is the card being read — then the pillar cards), and
/// the computed OMEN with its reason. `sport_raw` keeps the request-case value like every
/// other stage prompt.
#[allow(clippy::too_many_arguments)]
pub fn build_oracle_prompt(
    entity_type: &str,
    entity_name: &str,
    sport_raw: &str,
    sigil: &ConsumedSigil,
    narratives: &[SynthNarrative],
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
    transfers: &[HeatItem],
    omen: &str,
    omen_reason: &str,
) -> String {
    let mut b = String::new();
    b.push_str(&format!(
        "Entity: {entity_name} ({sport_raw} {entity_type})\n"
    ));

    b.push_str("\n=== THE CARDS ===\n");

    // The Sigil card — the synthesis being read aloud.
    match sigil.convergence {
        Some(c) => b.push_str(&format!(
            "Sigil: {}/100 (panel agreement {c}/100)\n",
            sigil.score
        )),
        None => b.push_str(&format!("Sigil: {}/100\n", sigil.score)),
    }
    if !sigil.blurb.is_empty() {
        b.push_str(&format!("The panel's read: {}\n", sigil.blurb));
    }
    if let Some(d) = &sigil.disagreement {
        b.push_str(&format!("Where the panel splits: {d}\n"));
    }
    if let Some(w) = &sigil.why_now {
        b.push_str(&format!("What stirred: {w}\n"));
    }

    // The PEAK card.
    if let Some(r) = rating {
        if !r.divined_peak.is_empty() {
            b.push_str(&format!("PEAK: {} ({}/100)", r.divined_peak, r.notability));
            if !r.peak_trajectory_label.is_empty() {
                b.push_str(&format!(" — {}", r.peak_trajectory_label));
            }
            b.push('\n');
        }
    } else {
        b.push_str("PEAK: (no card drawn)\n");
    }

    // The Vibe card.
    if let Some(v) = vibe {
        b.push_str(&format!("Vibe: {}/100 — {}\n", v.sentiment, v.prompt));
    } else {
        b.push_str("Vibe: (no card drawn)\n");
    }

    // The Momentum card.
    if let Some(direction) = mom.direction.as_deref() {
        match mom.momentum_score {
            Some(s) => b.push_str(&format!("Momentum: {direction} ({})\n", s.round() as i32)),
            None => b.push_str(&format!("Momentum: {direction}\n")),
        }
    } else {
        b.push_str("Momentum: (no card drawn)\n");
    }

    // Storyline cards — titles + tags only.
    if !narratives.is_empty() {
        b.push_str("Storylines:\n");
        for n in narratives.iter().take(MAX_STORYLINE_CARDS) {
            b.push_str(&format!(
                "- [{}] {}\n",
                trajectory_label(&n.trajectory),
                n.title
            ));
        }
    }

    // The Transfer card — the SAME shared rendering the sigil prompt and /transfers card use.
    if !transfers.is_empty() {
        b.push_str("Transfer winds:\n");
        write_heat_lines(&mut b, transfers);
    }

    b.push_str(&format!(
        "\n=== THE OMEN (computed) ===\nOmen: {omen} — {omen_reason}\n"
    ));

    b.push_str("\nSpeak the reading now.");
    b
}

// ---------------------------------------------------------------------------
// Output parsing — fail-closed on anything but a non-empty reading.
// ---------------------------------------------------------------------------

/// parse_oracle_reply extracts the reading from the JSON reply. `format_schema` makes a bare
/// `{"reading": ...}` the only thing the live route can emit; the balanced-brace salvage keeps
/// the offline/eval paths tolerant of a prose-wrapped object. Whitespace collapsed so a
/// multi-line reading persists as one clean paragraph. `None` = no parseable non-empty reading.
pub fn parse_oracle_reply(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let parsed: Option<serde_json::Value> = serde_json::from_str(trimmed).ok().or_else(|| {
        let start = trimmed.find('{')?;
        let end = trimmed.rfind('}')?;
        serde_json::from_str(&trimmed[start..=end]).ok()
    });
    let reading = parsed?.get("reading")?.as_str()?.trim().to_string();
    let reading = reading.split_whitespace().collect::<Vec<_>>().join(" ");
    (!reading.is_empty()).then_some(reading)
}

/// count_sentences approximates the reading's sentence count for the eval budget checks: a
/// sentence ends at a run of `.` / `!` / `?` followed by whitespace or end-of-text. A decimal
/// point ("a 2.5 assist bump") is followed by a digit, so it never counts.
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

/// The validated Oracle answer — the reading text (the `T` in `Parser<T>`).
#[derive(Clone, Debug)]
pub struct OracleReply {
    pub reading: String,
}

/// OracleParser wraps `parse_oracle_reply` behind the `Parser<T>` seam. Like `SigilParser` it
/// never returns the fail-closed `Ok(None)` — the Oracle's only fail-closed path is the
/// pre-model no-scored-sigil marker; an unparseable/empty reply is a genuine failure → `Err`
/// → the work item backs off.
pub struct OracleParser;

impl Parser<OracleReply> for OracleParser {
    fn parse(&self, raw: &str) -> Result<Option<OracleReply>> {
        match parse_oracle_reply(raw) {
            Some(reading) => Ok(Some(OracleReply { reading })),
            None => bail!(
                "oracle: could not parse reading from response (raw={:?})",
                truncate(raw, 200)
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Output + persist + ledger.
// ---------------------------------------------------------------------------

/// The evidence bundle of one fresh voice generation — feeds the transitional
/// `oracle_readings` dual-write and the OracleLogic ledger row.
#[derive(Clone, Debug)]
pub struct OracleOutput {
    pub reading: Option<String>,
    /// Computed omen the reading was drawn under.
    pub omen: Option<String>,
    /// Echo of the consumed sigil score.
    pub sigil_score: Option<i32>,
    pub season: i32,
    /// Canonical components JSON (the consumed-generation pre-image).
    pub input_components_json: String,
    pub input_hash: Option<String>,
    pub model: String,
    pub prompt_version: &'static str,
    pub built_prompt: Option<String>,
    pub request_body: Option<serde_json::Value>,
    pub eval_count: Option<i32>,
    pub wall_ms: Option<u64>,
}

impl OracleOutput {
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

async fn persist_to_oracle_readings(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    out: &OracleOutput,
) -> Result<i64> {
    let prov = out.provenance();
    let entity_id = item.entity_id_i32()?;
    let sigil_score: Option<i16> = out.sigil_score.map(|n| n as i16);
    let row = sqlx::query(
        r#"
        INSERT INTO oracle_readings (
            entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            reading, omen, sigil_score, input_components, input_hash,
            model_version, prompt_version
        ) VALUES ($1,$2,$3,$4,'periodic','{}'::jsonb, $5,$6,$7,$8::jsonb,$9, $10,$11)
        RETURNING id
        "#,
    )
    .bind(&item.entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(out.season)
    .bind(out.reading.as_deref())
    .bind(out.omen.as_deref())
    .bind(sigil_score)
    .bind(out.input_components_json.as_str())
    .bind(prov.input_hash.as_deref())
    .bind(prov.model_version.as_str())
    .bind(prov.prompt_version)
    .fetch_one(pool)
    .await
    .context("persist oracle reading")?;
    Ok(row.get("id"))
}

fn oracle_included_evidence(out: &OracleOutput) -> serde_json::Value {
    serde_json::json!({
        "input_components": serde_json::from_str::<serde_json::Value>(&out.input_components_json)
            .unwrap_or_else(|_| serde_json::json!({"raw_input_components": &out.input_components_json})),
        "omen": out.omen,
        "sigil_score": out.sigil_score,
    })
}

fn oracle_excluded_evidence(out: &OracleOutput) -> serde_json::Value {
    if out.built_prompt.is_none() {
        serde_json::json!([{ "reason": "no_scored_sigil_synthesis" }])
    } else {
        serde_json::json!([])
    }
}

async fn write_oracle_ledger(
    pool: &PgPool,
    item: &Item,
    entity_id: i32,
    sport: &str,
    out: &OracleOutput,
    sigil_row_id: i64,
) {
    insert_cognition_ledger_best_effort(
        pool,
        CognitionLedgerEntry {
            // Post-merge the voice is a step of the sigil stage: stage names WHERE the call
            // ran, lens/role name WHAT ran. Pre-merge rows carry stage "oracle" — an ops
            // query grouping by (stage, lens) sees the continuity.
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
            // The product is the merged sigil_synthesis row; the transitional
            // oracle_readings dual-write is serving plumbing, not the product.
            product_table: "sigil_synthesis".to_string(),
            product_row_ids: vec![sigil_row_id],
            model_version: out.model.clone(),
            prompt_version: out.prompt_version.to_string(),
            output_contract_version: ORACLE_OUTPUT_CONTRACT_VERSION.to_string(),
            input_ids: Vec::new(),
            input_hash: out.input_hash.clone(),
            request_body: out.request_body.clone(),
            built_prompt: out.built_prompt.clone(),
            included_evidence: oracle_included_evidence(out),
            excluded_evidence: oracle_excluded_evidence(out),
            context_budget: serde_json::json!({
                "num_predict": ORACLE_NUM_PREDICT,
                "eval_count": out.eval_count,
                "wall_ms": out.wall_ms,
            }),
            parser_outcome: if out.built_prompt.is_none() {
                "no_call".to_string()
            } else {
                "parsed".to_string()
            },
        },
    )
    .await;
}

// ---------------------------------------------------------------------------
// The voice step — called by SigilHandler on every freshly DECIDED card.
// ---------------------------------------------------------------------------

/// Voice is what the merged `sigil_synthesis` row persists for the card's voice — either a
/// freshly drawn reading or the prior one carried forward (the re-voice rule said the story
/// did not change). All-`None` for a no-pillar marker.
#[derive(Clone, Debug, Default)]
pub struct Voice {
    pub reading: Option<String>,
    pub omen: Option<String>,
    pub voiced_score: Option<i16>,
    /// Text-cast timestamptz. `None` with a `Some` reading ⇒ freshly drawn (persist binds
    /// NOW()); carried rows keep the prior row's original drawn-at.
    pub voiced_at: Option<String>,
    pub model_version: Option<String>,
    pub prompt_version: Option<String>,
    /// Present iff the voice model RAN this generation — drives the transitional
    /// `oracle_readings` dual-write and the OracleLogic ledger row.
    pub fresh: Option<FreshVoice>,
}

/// The call evidence of a freshly drawn reading (ledger + dual-write provenance).
#[derive(Clone, Debug)]
pub struct FreshVoice {
    pub input_components_json: String,
    pub input_hash: String,
    pub built_prompt: String,
    pub request_body: serde_json::Value,
    pub eval_count: i32,
    pub wall_ms: u64,
}

impl Voice {
    /// The no-pillar marker's voice: nothing.
    pub fn marker() -> Self {
        Voice::default()
    }

    /// carried keeps the prior reading verbatim — original omen, score baseline, drawn-at
    /// timestamp, and voice provenance all travel with it.
    pub fn carried(prior: PriorVoice) -> Self {
        Voice {
            reading: Some(prior.reading),
            omen: prior.omen,
            voiced_score: prior.voiced_score.map(|s| s as i16),
            voiced_at: Some(prior.voiced_at),
            model_version: Some(prior.model_version),
            prompt_version: Some(prior.prompt_version),
            fresh: None,
        }
    }
}

/// voice_decided_sigil draws a fresh reading over a just-decided card: builds the prompt from
/// the in-hand decide output + pillars (no DB re-read), routes OracleLogic, and returns the
/// `Voice` for the merged persist. The caller has already applied `voice_should_regenerate`.
#[allow(clippy::too_many_arguments)]
pub async fn voice_decided_sigil(
    hx: &Harness,
    entity_type: &str,
    entity_name: &str,
    sport_raw: &str,
    sigil: &ConsumedSigil,
    narratives: &[SynthNarrative],
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    momentum: &SynthMomentum,
    transfers: &[HeatItem],
    omen: &'static str,
    omen_reason: &str,
) -> Result<Voice> {
    let prompt = build_oracle_prompt(
        entity_type,
        entity_name,
        sport_raw,
        sigil,
        narratives,
        rating,
        vibe,
        momentum,
        transfers,
        omen,
        omen_reason,
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
        .extract(Role::OracleLogic, &prompt, &opts, &OracleParser)
        .await?;
    let reply = extracted
        .value
        .ok_or_else(|| anyhow::anyhow!("oracle: parser returned no value"))?;

    let input_components_json = build_oracle_input_components(sigil);
    let input_hash = hash_components(&input_components_json);
    Ok(Voice {
        reading: Some(reply.reading),
        omen: Some(omen.to_string()),
        voiced_score: Some(sigil.score as i16),
        voiced_at: None, // freshly drawn → persist stamps NOW()
        model_version: Some(extracted.model),
        prompt_version: Some(ORACLE_PROMPT_VERSION.to_string()),
        fresh: Some(FreshVoice {
            input_components_json,
            input_hash,
            built_prompt: extracted.built_prompt,
            request_body: extracted.request_body,
            eval_count: extracted.eval_count,
            wall_ms: extracted.wall_ms,
        }),
    })
}

/// finish_fresh_voice runs the post-persist bookkeeping for a freshly drawn reading:
/// the transitional `oracle_readings` dual-write (Go serving reads that table until
/// Session C flips `entity_sigil` to the merged columns) and the OracleLogic ledger row.
/// Best-effort like every ledger write; the dual-write failure is surfaced as an error
/// because a silently stale served reading would defeat the whole session.
pub async fn finish_fresh_voice(
    hx: &Harness,
    item: &Item,
    sport: &str,
    season: i32,
    voice: &Voice,
    sigil_row_id: i64,
) -> Result<()> {
    let Some(fresh) = &voice.fresh else {
        return Ok(());
    };
    let entity_id = item.entity_id_i32()?;
    let out = OracleOutput {
        reading: voice.reading.clone(),
        omen: voice.omen.clone(),
        sigil_score: voice.voiced_score.map(|s| s as i32),
        season,
        input_components_json: fresh.input_components_json.clone(),
        input_hash: Some(fresh.input_hash.clone()),
        model: voice.model_version.clone().unwrap_or_default(),
        prompt_version: ORACLE_PROMPT_VERSION,
        built_prompt: Some(fresh.built_prompt.clone()),
        request_body: Some(fresh.request_body.clone()),
        eval_count: Some(fresh.eval_count),
        wall_ms: Some(fresh.wall_ms),
    };
    persist_to_oracle_readings(&hx.pool, item, sport, &out).await?;
    write_oracle_ledger(&hx.pool, item, entity_id, sport, &out, sigil_row_id).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sigil(score: i32, convergence: Option<i32>) -> ConsumedSigil {
        ConsumedSigil {
            score,
            blurb: "A steady season under a turning sky.".to_string(),
            convergence,
            disagreement: None,
            why_now: None,
            input_hash: Some("abc123".to_string()),
        }
    }

    fn rating(peak_trajectory: &str) -> SynthRating {
        SynthRating {
            divined_peak: "Playmaking".into(),
            body: "b".into(),
            notability: 72,
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
    fn parses_clean_json_reply() {
        let r = parse_oracle_reply(r#"{"reading": "The cards hold. The arc rises."}"#).unwrap();
        assert_eq!(r, "The cards hold. The arc rises.");
    }

    #[test]
    fn salvages_prose_wrapped_json_and_collapses_whitespace() {
        // Prose wrap (an offline/no-schema route) + a JSON-escaped newline inside the reading:
        // the salvage finds the object, the collapse folds the decoded newline to one space.
        let r = parse_oracle_reply(
            "Here is the reading:\n{\"reading\": \"Line one.\\n  Line two.\"}\nDone.",
        )
        .unwrap();
        assert_eq!(r, "Line one. Line two.");
    }

    #[test]
    fn empty_or_missing_reading_is_none() {
        assert!(parse_oracle_reply(r#"{"reading": "   "}"#).is_none());
        assert!(parse_oracle_reply(r#"{"omen": "steady"}"#).is_none());
        assert!(parse_oracle_reply("no json at all").is_none());
    }

    #[test]
    fn oracle_parser_is_fail_closed_err_not_none() {
        assert!(OracleParser.parse("not a reading").is_err());
        let ok = OracleParser
            .parse(r#"{"reading": "The spread is quiet."}"#)
            .unwrap()
            .expect("valid reply is Some");
        assert_eq!(ok.reading, "The spread is quiet.");
    }

    #[test]
    fn counts_sentences_ignoring_decimals() {
        assert_eq!(count_sentences("One. Two! Three?"), 3);
        assert_eq!(
            count_sentences("He averages 2.5 assists. The arc holds."),
            2
        );
        assert_eq!(count_sentences("Trailing ellipsis... and then."), 2);
        assert_eq!(count_sentences("no terminator"), 0);
    }

    fn prior(omen: &str, voiced_score: i32) -> PriorVoice {
        PriorVoice {
            reading: "The line holds.".to_string(),
            omen: Some(omen.to_string()),
            voiced_score: Some(voiced_score),
            voiced_at: "2026-07-10 12:00:00+00".to_string(),
            model_version: "qwen3".to_string(),
            prompt_version: "or2".to_string(),
        }
    }

    #[test]
    fn archetype_bands_cover_1_to_100_contiguously() {
        for score in 1..=100 {
            let (lo, hi) = archetype_band(score);
            assert!(score >= lo && score <= hi, "score {score} outside its band");
        }
        // Adjacent bands share no scores and leave no gaps.
        for w in ARCHETYPE_BANDS.windows(2) {
            assert_eq!(w[0].0, w[1].1 + 1);
        }
        // Defensive clamps.
        assert_eq!(archetype_band(0), (1, 4));
        assert_eq!(archetype_band(101), (95, 100));
    }

    #[test]
    fn revoice_on_first_reading_and_omen_flip() {
        // No prior voice → always draw.
        assert!(voice_should_regenerate(None, "steady", 50));
        // Omen flip re-voices regardless of score movement.
        assert!(voice_should_regenerate(
            Some(&prior("steady", 80)),
            "ascendant",
            80
        ));
        // Missing prior omen counts as a flip (malformed history — re-anchor).
        let mut p = prior("steady", 80);
        p.omen = None;
        assert!(voice_should_regenerate(Some(&p), "steady", 80));
    }

    #[test]
    fn revoice_on_band_cross_with_hysteresis() {
        // Voiced at 84 (The Star, 75-84). In-band drift never re-voices.
        assert!(!voice_should_regenerate(
            Some(&prior("steady", 84)),
            "steady",
            75
        ));
        assert!(!voice_should_regenerate(
            Some(&prior("steady", 84)),
            "steady",
            84
        ));
        // 1pt past the edge is flapping — held by hysteresis.
        assert!(!voice_should_regenerate(
            Some(&prior("steady", 84)),
            "steady",
            85
        ));
        assert!(!voice_should_regenerate(
            Some(&prior("steady", 84)),
            "steady",
            74
        ));
        // 2pts past either edge crosses for real.
        assert!(voice_should_regenerate(
            Some(&prior("steady", 84)),
            "steady",
            86
        ));
        assert!(voice_should_regenerate(
            Some(&prior("steady", 84)),
            "steady",
            73
        ));
        // A big collapse across several bands obviously re-voices.
        assert!(voice_should_regenerate(
            Some(&prior("steady", 90)),
            "steady",
            30
        ));
        // Malformed history (no voiced score) re-anchors.
        let mut p = prior("steady", 80);
        p.voiced_score = None;
        assert!(voice_should_regenerate(Some(&p), "steady", 80));
    }

    #[test]
    fn carried_voice_keeps_everything_verbatim() {
        let v = Voice::carried(prior("waning", 42));
        assert_eq!(v.reading.as_deref(), Some("The line holds."));
        assert_eq!(v.omen.as_deref(), Some("waning"));
        assert_eq!(v.voiced_score, Some(42));
        assert_eq!(v.voiced_at.as_deref(), Some("2026-07-10 12:00:00+00"));
        assert_eq!(v.model_version.as_deref(), Some("qwen3"));
        assert!(
            v.fresh.is_none(),
            "carried voice never records a model call"
        );
        // Marker voice is all-NULL.
        let m = Voice::marker();
        assert!(m.reading.is_none() && m.omen.is_none() && m.voiced_at.is_none());
    }

    #[test]
    fn omen_crossroads_on_split_panel_regardless_of_direction() {
        let (omen, _) = compute_omen(
            &sigil(80, Some(40)),
            Some(&rating("rising")),
            &momentum("rising"),
        );
        assert_eq!(omen, "crossroads");
    }

    #[test]
    fn omen_momentum_leads_peak_seconds() {
        // Momentum rising (weight 2) beats PEAK falling (weight 1) → ascendant.
        let (omen, _) = compute_omen(
            &sigil(70, Some(80)),
            Some(&rating("falling")),
            &momentum("rising"),
        );
        assert_eq!(omen, "ascendant");
        // Momentum falling with PEAK steady → waning.
        let (omen, _) = compute_omen(
            &sigil(70, None),
            Some(&rating("steady")),
            &momentum("falling"),
        );
        assert_eq!(omen, "waning");
        // Nothing directional → steady.
        let (omen, _) = compute_omen(&sigil(50, Some(90)), None, &SynthMomentum::default());
        assert_eq!(omen, "steady");
    }

    #[test]
    fn input_components_cover_only_the_consumed_sigil() {
        let s = ConsumedSigil {
            score: 71,
            blurb: "Holding the line".to_string(),
            convergence: Some(40),
            disagreement: Some("PEAK vs momentum".to_string()),
            why_now: None,
            input_hash: Some("deadbeef".to_string()),
        };
        assert_eq!(
            build_oracle_input_components(&s),
            r#"{"sigil_blurb":"Holding the line","sigil_convergence":40,"sigil_disagreement":"PEAK vs momentum","sigil_input_hash":"deadbeef","sigil_score":71}"#
        );
        // Optional fields absent → keys absent (a later sigil that adds a why_now flips the hash).
        let bare = ConsumedSigil {
            score: 50,
            blurb: String::new(),
            convergence: None,
            disagreement: None,
            why_now: None,
            input_hash: None,
        };
        assert_eq!(
            build_oracle_input_components(&bare),
            r#"{"sigil_blurb":"","sigil_score":50}"#
        );
    }

    #[test]
    fn prompt_renders_cards_and_omen() {
        let s = ConsumedSigil {
            score: 78,
            blurb: "The panel holds its line.".to_string(),
            convergence: Some(45),
            disagreement: Some("strong PEAK vs sliding momentum".to_string()),
            why_now: Some("advanced transfer talks broke".to_string()),
            input_hash: Some("h".to_string()),
        };
        let r = rating("falling");
        let v = SynthVibe {
            sentiment: 34,
            prompt: "Sour mood around the player.".to_string(),
        };
        let m = SynthMomentum {
            direction: Some("falling".into()),
            momentum_score: Some(-2.0),
            ..SynthMomentum::default()
        };
        let (omen, reason) = compute_omen(&s, Some(&r), &m);
        assert_eq!(omen, "crossroads");
        let p = build_oracle_prompt(
            "player",
            "Marcus Vale",
            "NBA",
            &s,
            &[],
            Some(&r),
            Some(&v),
            &m,
            &[],
            omen,
            &reason,
        );
        assert!(p.starts_with("Entity: Marcus Vale (NBA player)\n\n=== THE CARDS ===\n"));
        assert!(p.contains("Sigil: 78/100 (panel agreement 45/100)\n"));
        assert!(p.contains("The panel's read: The panel holds its line.\n"));
        assert!(p.contains("Where the panel splits: strong PEAK vs sliding momentum\n"));
        assert!(p.contains("What stirred: advanced transfer talks broke\n"));
        assert!(p.contains("PEAK: Playmaking (72/100)\n"));
        assert!(p.contains("Vibe: 34/100 — Sour mood around the player.\n"));
        assert!(p.contains("Momentum: falling (-2)\n"));
        assert!(p.contains("=== THE OMEN (computed) ===\nOmen: crossroads — "));
        assert!(p.ends_with("\nSpeak the reading now."));
    }

    #[test]
    fn prompt_marks_missing_cards_and_caps_storylines() {
        let s = sigil(60, None);
        let narratives: Vec<SynthNarrative> = (0..5)
            .map(|i| SynthNarrative {
                title: format!("Story {i}"),
                body: "b".into(),
                impact: 5.0,
                trajectory: "heating_up".into(),
                source_count: 1,
                source_age_days: None,
            })
            .collect();
        let p = build_oracle_prompt(
            "team",
            "Test Team",
            "NFL",
            &s,
            &narratives,
            None,
            None,
            &SynthMomentum::default(),
            &[],
            "steady",
            "reason",
        );
        assert!(p.contains("Sigil: 60/100\n"));
        assert!(p.contains("PEAK: (no card drawn)\n"));
        assert!(p.contains("Vibe: (no card drawn)\n"));
        assert!(p.contains("Momentum: (no card drawn)\n"));
        // Storylines capped at 3.
        assert!(p.contains("- [Heating up] Story 2"));
        assert!(!p.contains("Story 3"));
    }
}
