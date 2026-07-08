//! Vibe stage — the first ported derivation handler (Phase 1 beachhead).
//!
//! Rust implementation of the vibe stage. The Go source provided the original machinery spec:
//!   - `go/internal/ml/vibe.go`         — Generate, prompt assembly, parsing, persist
//!   - `go/internal/ml/transfer_heat.go` — the shared transfer-heat primitive
//!   - `go/internal/derive/derive.go`    — drainVibe: queue Item → request, sigil hand-off
//!
//! The deterministic loaders, prompt assembly, parser, and persist path live here so prompt changes
//! are versioned and inspectable.
//!
//! Fail-closed semantics reproduced verbatim: when an entity has NO narratives AND no
//! transfer heat, we skip the model and write a NULL-sentiment marker row (the read
//! path returns "no data"; the debounce stops re-running it). A completed vibe enqueues
//! its `sigil` convergence BEFORE the work row is completed, so a crash in between
//! re-runs vibe (idempotent) rather than dropping the sigil.

use crate::harness::{Harness, Parser, Provenance};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::util::{truncate, truncate_bytes};
use crate::work::{self, Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use sqlx::PgPool;

/// Prompt version for the Vibe sentiment + felt-read contract.
pub const VIBE_PROMPT_VERSION: &str = "v9";

/// Production sentiment temperature (vibe.go uses 0.7). The parity harness overrides
/// this with an explicit 0.
pub const VIBE_TEMPERATURE: f64 = 0.7;

/// Token cap for the two-line answer.
pub const VIBE_NUM_PREDICT: i32 = 512;

/// Bounds the transfer rumors shown to the model as the entity's "transfer temperature".
/// Mirrors `maxHeatItems` in transfer_heat.go.
const MAX_HEAT_ITEMS: i64 = 6;

/// Body truncation in the prompt — mirrors `truncate(n.body, 280)` in vibe.go.
const BODY_TRUNCATE: usize = 280;

/// System prompt for the Vibe sentiment + felt-read contract.
pub const VIBE_SYSTEM_PROMPT: &str = r#"Task: produce a sentiment score and a short felt read from the supplied narratives and transfer/trade activity.

Voice: direct, sports-literate, grounded. No hype, no melodrama, no invented drama.

SCORE (1-100):
- 1 = grim or in freefall.
- 50 = quiet, unclear, or genuinely mixed.
- 100 = euphoric or surging.
- Weigh narratives by impact.
- Transfer/trade activity is energy, not automatically good or bad.
- If little is happening, stay near 50.

VIBE:
- One or two sentences. Use three only for a truly major multi-strand moment.
- Name the actual players, clubs, moves, or numbers behind the dominant threads.
- Do not list every minor item.
- Do not use generic phrases when the signals give specifics.
- Ground every claim in the supplied signals.

Reply with exactly these two lines:
SCORE: <integer 1-100>
VIBE: <the felt read>"#;

/// One narrative from the entity's latest generation (news_summaries).
#[derive(Clone, Debug)]
pub struct Narrative {
    pub title: String,
    pub body: String,
    pub impact: i32,
    pub trajectory: String,
}

/// One active transfer/trade rumor naming the counterparty. Mirrors `heatItem`.
#[derive(Clone, Debug)]
pub struct HeatItem {
    pub counterparty: String,
    pub heat: i32,
    pub stage: String,
    pub direction: String,
}

/// The result of running the vibe core for one entity, before persistence. Captures
/// everything both the production handler (→ vibe_scores) and the parity harness
/// (→ vibe_scores_shadow) need to persist.
#[derive(Clone, Debug)]
pub struct VibeOutput {
    /// `None` ⇒ no-corpus NULL marker (no model call was made).
    pub sentiment: Option<i32>,
    /// The one-sentence felt read; `None` when empty (the column is nullable).
    pub vibe_prompt: Option<String>,
    /// Provenance: the narratives' source article ids, deduped in first-seen order.
    pub input_news_ids: Vec<i64>,
    /// no-corpus → the configured model name; scored → the model echoed in the response.
    pub model: String,
    pub prompt_version: &'static str,
    /// The exact user prompt sent to the model; `None` for the no-corpus marker.
    pub built_prompt: Option<String>,
    /// The exact Ollama request body that was POSTed — captured by `extract` from the same
    /// backend + opts the call used (single source of truth), so it can't drift from what was
    /// sent. `None` for the no-corpus marker (no model call). The production persist ignores
    /// it; the parity harness records it as the jsonb axis of the temp-0 diff.
    pub request_body: Option<serde_json::Value>,
    pub skipped_no_corpus: bool,
}

/// vibe_version fingerprints a vibe result for the sigil queue's input_version, exactly
/// as `vibeVersion` in derive.go: `s<sentiment>` (`s0` for the no-corpus marker). Coarse
/// on purpose — the SigilGenerator's own pillar input-hash is the real convergence gate;
/// this only keeps the queue row's reopen/dedupe sane.
pub fn vibe_version(out: &VibeOutput) -> String {
    format!("s{}", out.sentiment.unwrap_or(0))
}

impl VibeOutput {
    /// provenance lifts the moat fields into the shared `Provenance` envelope (Plan §1.6).
    /// Vibe does not debounce, so `input_hash` is `None`; the scored row and the no-corpus
    /// marker produce the same envelope shape, differing only in the values they carry.
    fn provenance(&self) -> Provenance {
        Provenance {
            model_version: self.model.clone(),
            prompt_version: self.prompt_version,
            input_ids: self.input_news_ids.clone(),
            input_hash: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Corpus loaders — byte-for-byte the same SQL the Go stage runs.
// ---------------------------------------------------------------------------

/// load_latest_narratives returns the narratives from the entity's most recent
/// generation (news_summaries), hottest first, plus the deduped union of their source
/// article ids. Empty when the latest generation was a no-narratives marker (body NULL)
/// or the entity has none yet. Mirrors VibeGenerator.loadLatestNarratives.
pub async fn load_latest_narratives(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<(Vec<Narrative>, Vec<i64>)> {
    // COALESCE(impact, 0): impact is int2 but the `0` literal is int4, so the result is
    // int4 → scan as i32 (matches Go scanning into `int`).
    let rows: Vec<(String, String, i32, Vec<i64>, String)> = sqlx::query_as(
        r#"
        SELECT narrative_title, body, COALESCE(impact, 0), input_news_ids,
               COALESCE(trajectory, 'developing_story')
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
    .with_context(|| format!("load narratives {entity_type}/{entity_id}"))?;

    let mut narratives = Vec::with_capacity(rows.len());
    let mut ids: Vec<i64> = Vec::new();
    for (title, body, impact, mut nids, trajectory) in rows {
        ids.append(&mut nids);
        narratives.push(Narrative {
            title,
            body,
            impact,
            trajectory,
        });
    }
    Ok((narratives, dedupe_i64(ids)))
}

/// dedupe_i64 removes duplicates preserving first-seen order. Mirrors `dedupeInt64`.
fn dedupe_i64(input: Vec<i64>) -> Vec<i64> {
    let mut out = Vec::with_capacity(input.len());
    let mut seen = std::collections::HashSet::with_capacity(input.len());
    for v in input {
        if seen.insert(v) {
            out.push(v);
        }
    }
    out
}

/// load_transfer_heat returns the entity's hottest active transfer/trade rumors (latest
/// per counterparty, heat > 0, model-vetted), naming the counterparty. The `is_rumor IS
/// TRUE` gate is applied AFTER picking the latest row per counterparty (FIRST-GPT-AUDIT
/// Session 10), so a newer cleared/unknown verdict supersedes an older TRUE. Mirrors
/// `loadTransferHeat`; branches on entity type exactly as the Go query does. The
/// current-week freshness gate plus the shared cooling-off retirement rule keeps very
/// old false positives from grounding prompts forever — mirrors the /transfers card
/// read path.
pub async fn load_transfer_heat(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<HeatItem>> {
    let query = if entity_type == "team" {
        r#"
        SELECT counterparty, heat, stage, direction FROM (
            SELECT DISTINCT ON (tr.player_id)
                   p.name AS counterparty, tr.team_id, pci.team_id AS current_team_id,
                   tr.heat, tr.is_rumor,
                   COALESCE(tr.stage,'') AS stage, COALESCE(tr.direction,'') AS direction,
                   tr.generated_at
            FROM transfer_rumors tr
            JOIN players p ON p.id = tr.player_id AND p.sport = tr.sport
            LEFT JOIN public.player_current_identity pci ON pci.player_id = tr.player_id AND pci.sport = tr.sport
            WHERE tr.team_id = $1 AND tr.sport = $2 AND tr.heat IS NOT NULL
              AND tr.generated_at > NOW() - INTERVAL '7 days'
              AND (COALESCE(tr.trajectory, 'developing_story') <> 'cooling_off'
                   OR COALESCE(tr.rumor_updated_at, tr.source_latest_at, tr.generated_at) > NOW() - INTERVAL '3 days')
            ORDER BY tr.player_id, tr.generated_at DESC
        ) latest
        WHERE heat > 0 AND is_rumor IS TRUE
          AND NOT (
              (current_team_id IS NOT NULL AND current_team_id = team_id AND direction = 'incoming')
              OR (current_team_id IS NOT NULL AND current_team_id <> team_id AND direction = 'outgoing')
          )
        ORDER BY heat DESC LIMIT $3
        "#
    } else {
        r#"
        SELECT counterparty, heat, stage, direction FROM (
            SELECT DISTINCT ON (tr.team_id)
                   t.name AS counterparty, tr.team_id, pci.team_id AS current_team_id,
                   tr.heat, tr.is_rumor,
                   COALESCE(tr.stage,'') AS stage, COALESCE(tr.direction,'') AS direction,
                   tr.generated_at
            FROM transfer_rumors tr
            JOIN teams t ON t.id = tr.team_id AND t.sport = tr.sport
            LEFT JOIN public.player_current_identity pci ON pci.player_id = tr.player_id AND pci.sport = tr.sport
            WHERE tr.player_id = $1 AND tr.sport = $2 AND tr.heat IS NOT NULL
              AND tr.generated_at > NOW() - INTERVAL '7 days'
              AND (COALESCE(tr.trajectory, 'developing_story') <> 'cooling_off'
                   OR COALESCE(tr.rumor_updated_at, tr.source_latest_at, tr.generated_at) > NOW() - INTERVAL '3 days')
            ORDER BY tr.team_id, tr.generated_at DESC
        ) latest
        WHERE heat > 0 AND is_rumor IS TRUE
          AND NOT (
              (current_team_id IS NOT NULL AND current_team_id = team_id AND direction = 'incoming')
              OR (current_team_id IS NOT NULL AND current_team_id <> team_id AND direction = 'outgoing')
          )
        ORDER BY heat DESC LIMIT $3
        "#
    };

    // heat is int2 → scan as i16 (matches Go scanning into `int`, widened below).
    let rows: Vec<(String, i16, String, String)> = sqlx::query_as(query)
        .bind(entity_id)
        .bind(sport)
        .bind(MAX_HEAT_ITEMS)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load transfer heat {entity_type}/{entity_id}"))?;

    Ok(rows
        .into_iter()
        .map(|(counterparty, heat, stage, direction)| HeatItem {
            counterparty,
            heat: heat as i32,
            stage,
            direction,
        })
        .collect())
}

/// lookup_entity_name resolves the display name for the prompt. Mirrors
/// `corpus.LookupEntityName` + drainVibe's `nameOf` (an empty/missing name is an error,
/// so the work item fails and retries rather than generating against a blank prompt).
pub async fn lookup_entity_name(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<String> {
    let query = if entity_type == "player" {
        "SELECT name FROM players WHERE id = $1 AND sport = $2"
    } else {
        "SELECT name FROM teams WHERE id = $1 AND sport = $2"
    };
    let name: String = sqlx::query_scalar(query)
        .bind(entity_id)
        .bind(sport)
        .fetch_one(pool)
        .await
        .with_context(|| format!("lookup {entity_type}/{entity_id} ({sport})"))?;
    if name.is_empty() {
        bail!("empty name for {entity_type}/{entity_id} ({sport})");
    }
    Ok(name)
}

// ---------------------------------------------------------------------------
// Prompt assembly.
// ---------------------------------------------------------------------------

/// build_sentiment_prompt assembles the user prompt. `sport` is the original-case value used in
/// the prompt; the SQL reads use the upper-cased form.
pub fn build_sentiment_prompt(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    narratives: &[Narrative],
    heat: &[HeatItem],
) -> String {
    let mut b = String::new();

    b.push_str(&format!(
        "Entity: {} {} ({})\n",
        title_first(entity_type),
        entity_name,
        sport
    ));

    b.push_str("\nNarratives forming around them (impact in brackets):\n");
    if narratives.is_empty() {
        b.push_str("- (none this cycle)\n");
    } else {
        for n in narratives {
            b.push_str(&format!(
                "- [{}, {}] {}: {}\n",
                n.impact,
                trajectory_label(&n.trajectory),
                n.title,
                truncate_bytes(&n.body, BODY_TRUNCATE)
            ));
        }
    }

    b.push_str("\nCurrent transfer/trade activity (heat 0-100):\n");
    if heat.is_empty() {
        b.push_str("- (none)\n");
    } else {
        write_heat_lines(&mut b, heat);
    }

    b.push_str("\nRespond now (SCORE line, then VIBE line).");
    b
}

/// write_heat_lines renders heat bullets exactly as `writeHeatLines`:
///   `- <counterparty> — heat <heat>[, <direction>][, <stage>]`
///
/// `pub` because it is the SHARED transfer-heat line format — Go homes it in `transfer_heat.go`
/// and BOTH vibe and narratives render heat through it; the Rust single-home is here (alongside
/// [`load_transfer_heat`] / [`HeatItem`]), so `narratives::build_narratives_prompt` reuses it
/// rather than carrying a second copy (the L11/L12 single-home discipline).
pub fn write_heat_lines(b: &mut String, heat: &[HeatItem]) {
    for h in heat {
        let mut line = format!("- {} — heat {}", h.counterparty, h.heat);
        if !h.direction.is_empty() {
            line.push_str(", ");
            line.push_str(&h.direction);
        }
        if !h.stage.is_empty() {
            line.push_str(", ");
            line.push_str(&h.stage);
        }
        b.push_str(&line);
        b.push('\n');
    }
}

/// title_first upper-cases the first character, mirroring `strings.Title` for the
/// single-word entity types ("player" → "Player", "team" → "Team").
fn title_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn trajectory_label(raw: &str) -> &'static str {
    match raw {
        "heating_up" => "Heating up",
        "cooling_off" => "Cooling off",
        _ => "Developing story...",
    }
}

// ---------------------------------------------------------------------------
// Output parsing — mirrors parseSentimentAndPrompt + parseSentiment.
// ---------------------------------------------------------------------------

/// parse_sentiment_and_prompt extracts the SCORE (1-100) and the one-line VIBE from the
/// model's two-line v6 reply. The score falls back to the first integer anywhere
/// (format drift); the prompt is "" when absent. Mirrors `parseSentimentAndPrompt`.
pub fn parse_sentiment_and_prompt(raw: &str) -> Result<(i32, String)> {
    let mut score: i32 = 0;
    let mut prompt = String::new();
    let lines: Vec<&str> = raw.trim().split('\n').collect();

    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        let up = t.to_uppercase();
        if score == 0 && up.starts_with("SCORE:") {
            // "SCORE:" is ASCII (6 bytes) regardless of case, so t[6..] is a boundary.
            let rest = t[6..].trim();
            if let Ok(n) = rest.parse::<i64>() {
                score = n.clamp(1, 100) as i32;
            }
        } else if prompt.is_empty() && up.starts_with("VIBE:") {
            prompt = t[5..].trim().to_string();
            for extra in &lines[i + 1..] {
                let e = extra.trim();
                if !e.is_empty() {
                    prompt.push(' ');
                    prompt.push_str(e);
                }
            }
        }
    }

    if score == 0 {
        // No SCORE: label parsed — fall back to the first integer anywhere.
        score = parse_sentiment(raw)?;
    }
    Ok((score, prompt))
}

/// parse_sentiment pulls the first run of ASCII digits out of the response and clamps it
/// into 1-100; errors only when there are no digits at all (or the run overflows, as
/// Go's strconv.Atoi does). Mirrors `parseSentiment` (the `\d+` regex path).
fn parse_sentiment(raw: &str) -> Result<i32> {
    let bytes = raw.as_bytes();
    let start = match bytes.iter().position(|b| b.is_ascii_digit()) {
        Some(i) => i,
        None => return Err(anyhow!("no digit in response")),
    };
    let end = bytes[start..]
        .iter()
        .position(|b| !b.is_ascii_digit())
        .map(|off| start + off)
        .unwrap_or(bytes.len());
    let digits = &raw[start..end];
    match digits.parse::<i64>() {
        Ok(n) => Ok(n.clamp(1, 100) as i32),
        Err(_) => Err(anyhow!("parse digit {digits:?}")),
    }
}

/// VibeReply is the validated two-line answer — the SCORE (1-100) and the one-line felt
/// read. The vibe Extract output shape (the `T` in `Parser<T>` / `Extracted<T>`).
#[derive(Clone, Debug)]
pub struct VibeReply {
    pub sentiment: i32,
    pub vibe_prompt: String,
}

/// VibeParser is the vibe stage's `Parser` plug-in: it wraps `parse_sentiment_and_prompt`
/// behind the capability library's `Parser<T>` seam.
/// It never returns the fail-closed `Ok(None)` — vibe's only fail-closed path is the
/// no-corpus short-circuit *before* the model call (a NULL marker), so an unparseable reply
/// is a genuine failure → `Err` → the work item backs off, exactly as the Go stage does.
pub struct VibeParser;

impl Parser<VibeReply> for VibeParser {
    fn parse(&self, raw: &str) -> Result<Option<VibeReply>> {
        let (sentiment, vibe_prompt) = parse_sentiment_and_prompt(raw)
            .with_context(|| format!("parse sentiment (raw={:?})", truncate(raw, 120)))?;
        Ok(Some(VibeReply {
            sentiment,
            vibe_prompt,
        }))
    }
}

// ---------------------------------------------------------------------------
// The core generate + the production handler.
// ---------------------------------------------------------------------------

/// generate_vibe runs the full vibe derivation for one entity at the given temperature and
/// returns the un-persisted result. Shared by the production handler (temp 0.7 → it writes
/// vibe_scores + enqueues sigil) and the parity harness (temp 0 → it writes the shadow
/// table). This is the L1 composition — `route(EmotionalNews) + extract(VibeParser)` — over
/// the same loaders + prompt: validate, read narratives + heat, short-circuit
/// to the no-corpus marker when both are empty, else build the prompt and `extract`.
pub async fn generate_vibe(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport_raw: &str,
    temperature: f64,
) -> Result<VibeOutput> {
    if entity_id <= 0 || entity_name.is_empty() || sport_raw.is_empty() || entity_type.is_empty() {
        bail!("vibe: entity context incomplete");
    }
    // Reads use the upper-cased sport; the prompt uses the original-case value (req.Sport).
    let sport = sport_raw.to_uppercase();

    let (narratives, news_ids) =
        load_latest_narratives(&hx.pool, entity_type, entity_id, &sport).await?;
    let heat = load_transfer_heat(&hx.pool, entity_type, entity_id, &sport).await?;

    // No derived signal (no narratives AND no transfer heat) → no rating. Persist a
    // NULL-sentiment marker (handled by the caller); the read path returns "no data". No
    // model call is made, so the marker's model_version is the role's configured model.
    if narratives.is_empty() && heat.is_empty() {
        return Ok(VibeOutput {
            sentiment: None,
            vibe_prompt: None,
            input_news_ids: Vec::new(),
            model: hx.router.for_role(Role::EmotionalNews).model().to_string(),
            prompt_version: VIBE_PROMPT_VERSION,
            built_prompt: None,
            request_body: None,
            skipped_no_corpus: true,
        });
    }

    let prompt = build_sentiment_prompt(entity_type, entity_name, sport_raw, &narratives, &heat);
    let opts = GenerateOptions {
        system: Some(VIBE_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict: VIBE_NUM_PREDICT,
        json_mode: false,
    };

    // vibe = route(EmotionalNews) + extract(VibeParser). The fail-closed contract lives in
    // the parser: an unparseable reply surfaces as its `Err` (item fails + backs off), and
    // `extract` records the exact wire body it sent.
    let extracted = hx
        .extract(Role::EmotionalNews, &prompt, &opts, &VibeParser)
        .await?;

    // VibeParser only ever returns `Ok(Some)` on success (vibe's no-corpus marker is the
    // pre-model short-circuit above, not a parser fail-closed), so a `None` here would be a
    // contract violation — fail the item rather than fabricate a row.
    let reply = extracted
        .value
        .ok_or_else(|| anyhow!("vibe: parser returned no value"))?;

    Ok(VibeOutput {
        sentiment: Some(reply.sentiment),
        vibe_prompt: if reply.vibe_prompt.is_empty() {
            None
        } else {
            Some(reply.vibe_prompt)
        },
        input_news_ids: news_ids,
        model: extracted.model,
        prompt_version: VIBE_PROMPT_VERSION,
        built_prompt: Some(extracted.built_prompt),
        request_body: Some(extracted.request_body),
        skipped_no_corpus: false,
    })
}

/// persist_to_vibe_scores writes one row to the LIVE vibe_scores table — both the scored
/// row and the no-corpus NULL marker, which differ only in the bound values. Mirrors
/// persistSentiment / persistNoCorpus: trigger_type 'periodic', trigger_payload the JSON
/// `null` (marshal of a nil trigger map), empty felt-read stored as NULL.
async fn persist_to_vibe_scores(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    out: &VibeOutput,
) -> Result<()> {
    // Route the moat fields through the shared Provenance envelope (vibe does not debounce,
    // so input_hash is None); the typed INSERT stays the stage's own (Postgres-as-serializer).
    let entity_id = item.entity_id_i32()?;
    let prov = out.provenance();
    let sentiment: Option<i16> = out.sentiment.map(|n| n as i16);
    sqlx::query(
        r#"
        INSERT INTO vibe_scores (
            entity_type, entity_id, sport,
            trigger_type, trigger_payload,
            sentiment, prompt, input_news_ids,
            model_version, prompt_version
        ) VALUES ($1,$2,$3,'periodic','null'::jsonb,$4,$5,$6,$7,$8)
        "#,
    )
    .bind(&item.entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(sentiment)
    .bind(out.vibe_prompt.as_deref())
    .bind(prov.input_ids.as_slice())
    .bind(prov.model_version.as_str())
    .bind(prov.prompt_version)
    .execute(pool)
    .await
    .context("persist vibe")?;
    Ok(())
}

/// VibeHandler drains the durable `vibe` stage: read the fresh narratives + heat, score
/// with the model, persist to vibe_scores, and enqueue the `sigil` convergence before
/// completing. This is the production path for the Phase 2 cutover (registered in
/// main.rs); the Phase 1 parity harness reuses the same core but writes the shadow table.
pub struct VibeHandler;

impl VibeHandler {
    pub fn new() -> Self {
        VibeHandler
    }
}

impl Default for VibeHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for VibeHandler {
    fn stage(&self) -> Stage {
        Stage::Vibe
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let entity_id = item.entity_id_i32()?;
        // nameOf: the name lookup uses the queue's raw sport value (drainVibe).
        let name = lookup_entity_name(&hx.pool, &item.entity_type, entity_id, &item.sport).await?;

        let out = generate_vibe(
            hx,
            &item.entity_type,
            entity_id,
            &name,
            &item.sport,
            VIBE_TEMPERATURE,
        )
        .await?;

        let sport = item.sport.to_uppercase();
        persist_to_vibe_scores(&hx.pool, item, &sport, &out).await?;

        // Enqueue the sigil convergence BEFORE completing (the work row is completed by
        // the worker only after this returns Ok). A crash or enqueue failure after persist
        // fails this vibe item, so the queue retries instead of silently dropping sigil.
        let sig = Item {
            stage: Stage::Sigil,
            entity_type: item.entity_type.clone(),
            entity_id: item.entity_id,
            sport: item.sport.clone(),
            input_version: Some(vibe_version(&out)),
            attempts: 0,
        };
        work::enqueue(&hx.pool, &sig).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_line_reply() {
        let (score, vibe) =
            parse_sentiment_and_prompt("SCORE: 73\nVIBE: Quietly surging into the playoff race.")
                .unwrap();
        assert_eq!(score, 73);
        assert_eq!(vibe, "Quietly surging into the playoff race.");
    }

    #[test]
    fn clamps_and_joins_trailing_vibe_lines() {
        let (score, vibe) =
            parse_sentiment_and_prompt("SCORE: 250\nVIBE: line one\nline two").unwrap();
        assert_eq!(score, 100);
        assert_eq!(vibe, "line one line two");
    }

    #[test]
    fn falls_back_to_first_integer() {
        let (score, vibe) = parse_sentiment_and_prompt("the vibe is about 64 today").unwrap();
        assert_eq!(score, 64);
        assert_eq!(vibe, "");
    }

    #[test]
    fn errors_without_digits() {
        assert!(parse_sentiment_and_prompt("no number here").is_err());
    }

    #[test]
    fn builds_prompt_with_empty_sections() {
        let p = build_sentiment_prompt("player", "Test Player", "NBA", &[], &[]);
        assert_eq!(
            p,
            "Entity: Player Test Player (NBA)\n\nNarratives forming around them (impact in brackets):\n- (none this cycle)\n\nCurrent transfer/trade activity (heat 0-100):\n- (none)\n\nRespond now (SCORE line, then VIBE line)."
        );
    }

    #[test]
    fn dedupes_preserving_order() {
        assert_eq!(dedupe_i64(vec![3, 1, 3, 2, 1]), vec![3, 1, 2]);
    }

    #[test]
    fn vibe_parser_wraps_valid_reply_as_some() {
        let reply = VibeParser
            .parse("SCORE: 73\nVIBE: Quietly surging into the playoff race.")
            .unwrap()
            .expect("a valid reply is Some, never the fail-closed None");
        assert_eq!(reply.sentiment, 73);
        assert_eq!(reply.vibe_prompt, "Quietly surging into the playoff race.");
    }

    #[test]
    fn vibe_parser_errors_without_digits() {
        // No digit anywhere ⇒ Err (retry/back-off), NOT Ok(None): vibe's only fail-closed
        // path is the pre-model no-corpus marker, never an unparseable reply.
        assert!(VibeParser.parse("no number here").is_err());
    }
}
