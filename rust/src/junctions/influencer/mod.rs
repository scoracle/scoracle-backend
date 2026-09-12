//! Vibe stage — The Influencer's emotional/news card.
//!
//! The deterministic loaders, prompt assembly, parser, and persist path live here so prompt changes
//! are versioned and inspectable. SQL supplies the persisted narrative/transfer context; Rust owns
//! the transient prompt shaping, model call, parsing, fail-closed marker, debounce, and downstream
//! queue hand-off.
//!
//! Packet snapshots drive the debounce; generated prose, timestamps, and continuity memory do not.
//! Empty material after a real read gets one closing quiet read; a never-scored entity gets a
//! NULL marker. Every completed or skipped item offers the hash-gated Momentum hand-off.

use crate::corpus::lookup_entity_name;
use crate::harness::{EntityKey, Generation, GenerationCall, Harness, Parser};
use crate::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::util::{hash_components, truncate};
use crate::work::{Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use tracing::{debug, warn};

mod inputs;
pub mod prompt;
pub use inputs::build_sentiment_prompt;
pub use prompt::{VIBE_PROMPT_VERSION, VIBE_SYSTEM_PROMPT};

/// Output contract captured separately in the diagnostic ledger.
pub const VIBE_OUTPUT_CONTRACT_VERSION: &str = "vibe-score-v1";

const VIBE_LEDGER: LedgerSpec = LedgerSpec {
    stage: "vibe",
    lens: "vibe",
    role: Role::VibeLogic,
    product_table: "vibe_scores",
    output_contract_version: VIBE_OUTPUT_CONTRACT_VERSION,
};

/// Production sentiment temperature.
pub const VIBE_TEMPERATURE: f64 = 0.7;

/// Token cap for the two-line answer.
// Second-largest, and for the same reason as The Journalist's: she voices each developing
// emotional story, so multiple stories means multiple reads. Nuance is her product.
pub const VIBE_NUM_PREDICT: i32 = 800;

/// Per-packet character cap sized for two packets in the small context window.
const PACKET_BLOCK_TRUNCATE: usize = 2_400;

/// The result of running the vibe core for one entity, before persistence. Captures
/// the production row payload for `vibe_scores`.
#[derive(Clone, Debug)]
pub struct VibeScore {
    /// `None` ⇒ no-corpus NULL marker (no model call was made).
    pub sentiment: Option<i32>,
    /// The one-sentence felt read; `None` when empty (the column is nullable).
    pub vibe_prompt: Option<String>,
    /// Optional card title.
    pub hook: Option<String>,
    /// Canonical material-input JSON and hash pre-image.
    pub input_components_json: String,
}

pub type VibeOutput = Generation<VibeScore>;

/// vibe_version fingerprints a vibe result for the sigil queue's input_version, exactly
/// as `vibeVersion` in derive.go: `s<sentiment>` (`s0` for the no-corpus marker). Coarse
/// on purpose — the SigilGenerator's own pillar input-hash is the real convergence gate;
/// this only keeps the queue row's reopen/dedupe sane.
pub fn vibe_version(out: &VibeOutput) -> String {
    format!("s{}", out.sentiment.unwrap_or(0))
}

// ---------------------------------------------------------------------------
// Input components and debounce hash.
// ---------------------------------------------------------------------------

/// Build the canonical debounce pre-image from the prompt version and packet snapshots the model
/// actually sees. Packet text is editorial output; append-only packet IDs are their identity.
pub fn build_vibe_input_components(packets: &[PacketBlock]) -> String {
    let mut packet_ids: Vec<i64> = packets.iter().map(|packet| packet.packet_id).collect();
    packet_ids.sort_unstable();
    serde_json::json!({
        "packets": packet_ids,
        "prompt_version": VIBE_PROMPT_VERSION,
    })
    .to_string()
}

/// The loaded-and-weighted vibe context: everything the prompt needs plus the debounce key.
/// Splitting the load from the model call lets the handler gate on `input_hash`
/// before paying for the GPU.
pub struct VibeContext {
    /// The entity's live packets rendered for the Influencer.
    pub packets: Vec<PacketBlock>,
    pub input_components_json: String,
    pub input_hash: String,
}

/// One rendered packet as the Influencer reads it: the block, and the packet id that identifies
/// the snapshot it was rendered from.
#[derive(Clone, Debug)]
pub struct PacketBlock {
    pub packet_id: i64,
    pub text: String,
}

impl VibeContext {
    pub fn empty(&self) -> bool {
        self.packets.is_empty()
    }
}

/// Load the packet evidence and compute its material-only debounce key. No model call.
pub async fn load_vibe_context(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport_raw: &str,
) -> Result<VibeContext> {
    if entity_id <= 0 || entity_name.is_empty() || sport_raw.is_empty() || entity_type.is_empty() {
        bail!("vibe: entity context incomplete");
    }
    // Reads use the upper-cased sport; the prompt uses the original-case value (req.Sport).
    let sport = sport_raw.to_uppercase();

    let packets = load_vibe_packets(&hx.pool, entity_type, entity_id, entity_name, &sport).await?;
    let input_components_json = build_vibe_input_components(&packets);
    let input_hash = hash_components(&input_components_json);

    Ok(VibeContext {
        packets,
        input_components_json,
        input_hash,
    })
}

/// Maximum live storylines per Influencer prompt.
const MAX_VIBE_PACKETS: i64 = 2;

/// Render the entity's live packets for the Influencer.
///
/// Hers is the only render that carries `MOOD:` — the register and its phrase (§1c, and pinned by
/// a test in `render.rs`). Handing the same charged phrase to The Journalist would leak her
/// judgment into his copy, which is why the renderer keys it on the voice rather than on a flag
/// the caller could get wrong.
async fn load_vibe_packets(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport: &str,
) -> Result<Vec<PacketBlock>> {
    use crate::junctions::editor::render::Voice;

    Ok(crate::junctions::editor::packet::render_packets_for_entity(
        pool,
        entity_type,
        entity_id,
        entity_name,
        sport,
        Voice::Influencer,
        MAX_VIBE_PACKETS,
    )
    .await?
    .into_iter()
    .map(|(packet_id, text)| PacketBlock { packet_id, text })
    .collect())
}

/// VIBE_WORK_PREFIX namespaces the vibe queue row's `input_version` (mirrors momentum's
/// `momentum:s` prefix). Vibe is entity-scoped, so there is no season segment.
const VIBE_WORK_PREFIX: &str = "vibe:";

/// vibe_work_input_version derives the vibe queue row's reopen key from the material input hash, so
/// the enqueue gate (`debounce_unchanged`) and the `pipeline_work` reopen gate agree — a stale
/// re-enqueue with the same material is a no-op.
pub fn vibe_work_input_version(input_hash: &str) -> String {
    format!("{VIBE_WORK_PREFIX}{input_hash}")
}

/// Load the material vibe context and enqueue the Vibe stage only when it
/// actually moved since the last `vibe_scores` row — `Ok(false)` when the context is empty or
/// unchanged (nothing enqueued), `Ok(true)` on enqueue. Idempotent: `work::enqueue`'s ON CONFLICT
/// reopens the row only when the `input_version` changed, so a redundant call is harmless.
pub async fn enqueue_vibe_if_needed(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport: &str,
) -> Result<bool> {
    let sport = sport.to_uppercase();
    let ctx = load_vibe_context(hx, entity_type, entity_id, entity_name, &sport).await?;
    // Empty material must reach the handler once so a previously vivid room can close quietly.
    let key = EntityKey {
        entity_type: entity_type.to_string(),
        entity_id,
        sport: sport.clone(),
        season: None,
    };
    if hx
        .debounce_unchanged("vibe_scores", &key, &ctx.input_hash)
        .await?
    {
        return Ok(false);
    }
    let it = Item {
        stage: Stage::Vibe,
        entity_type: entity_type.to_string(),
        entity_id: i64::from(entity_id),
        sport,
        input_version: Some(vibe_work_input_version(&ctx.input_hash)),
        attempts: 0,
    };
    crate::work::enqueue(&hx.pool, &it).await?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// Prompt assembly.
// ---------------------------------------------------------------------------

/// The previous vibe read fed back into the prompt for continuity (v12 — the Sigil
/// Phase-5.2 shape). Prompt-only: it is NOT part of `build_vibe_input_components` / the
/// `input_hash` — the read always moves, so hashing it would self-trigger every re-run.
/// Constructed only for a real prior read (latest row scored, not a NULL-sentiment marker).
#[derive(Clone, Debug)]
pub struct PrevVibe {
    pub sentiment: i32,
    /// The prior felt read; may be empty (the column is nullable) — then only the Score
    /// line renders.
    pub vibe_prompt: String,
}

/// load_latest_vibe_row fetches the entity's LATEST vibe_scores row in ONE query:
/// sentiment + felt read (the continuity prior) and input_hash (the debounce gate) as a
/// consistent, non-torn read — the sigil plan-A1 consolidation. Vibe owns the SQL because
/// `Harness::latest_with_hash` is shaped to sigil's score/blurb columns.
async fn load_latest_vibe_row(
    pool: &PgPool,
    key: &EntityKey,
) -> Result<(Option<i16>, Option<String>, Option<String>)> {
    let row: Option<(Option<i16>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT sentiment, prompt, input_hash FROM vibe_scores \
         WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 \
         ORDER BY generated_at DESC LIMIT 1",
    )
    .bind(&key.entity_type)
    .bind(key.entity_id)
    .bind(&key.sport)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("latest vibe row {}/{}", key.entity_type, key.entity_id))?;
    Ok(row.unwrap_or((None, None, None)))
}

/// Latest scored row, matching the serving view's `sentiment IS NOT NULL` filter. This may sit
/// below a newer marker. `None` when the entity has never been scored.
async fn load_latest_scored_vibe_row(
    pool: &PgPool,
    key: &EntityKey,
) -> Result<Option<(i16, Option<String>)>> {
    sqlx::query_as(
        "SELECT sentiment, prompt FROM vibe_scores \
         WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 \
           AND sentiment IS NOT NULL \
         ORDER BY generated_at DESC LIMIT 1",
    )
    .bind(&key.entity_type)
    .bind(key.entity_id)
    .bind(&key.sport)
    .fetch_optional(pool)
    .await
    .with_context(|| {
        format!(
            "latest scored vibe row {}/{}",
            key.entity_type, key.entity_id
        )
    })
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

// ---------------------------------------------------------------------------
// Output parsing — mirrors parseSentimentAndPrompt + parseSentiment.
// ---------------------------------------------------------------------------

/// Extract SCORE, HOOK, and VIBE prose from the current labeled reply. Trailing VIBE lines
/// preserve paragraph breaks.
pub fn parse_vibe_reply(raw: &str) -> Result<(i32, Option<String>, String)> {
    let (score_line, rest) = raw
        .trim()
        .split_once('\n')
        .ok_or_else(|| anyhow!("vibe: missing labeled body"))?;
    let score = score_line
        .strip_prefix("SCORE:")
        .and_then(|s| s.trim().parse::<i64>().ok())
        .map(|n| n.clamp(1, 100) as i32)
        .ok_or_else(|| anyhow!("vibe: missing or invalid SCORE line"))?;

    let (hook, vibe) = if let Some(hook_and_vibe) = rest.strip_prefix("HOOK:") {
        let (hook, vibe) = hook_and_vibe
            .split_once('\n')
            .ok_or_else(|| anyhow!("vibe: missing VIBE line"))?;
        let hook = hook.trim();
        ((!hook.is_empty()).then(|| hook.to_string()), vibe)
    } else {
        (None, rest)
    };
    let body = vibe
        .strip_prefix("VIBE:")
        .ok_or_else(|| anyhow!("vibe: missing VIBE line"))?;
    if body
        .lines()
        .skip(1)
        .any(|line| line.trim().starts_with("HOOK:"))
    {
        bail!("vibe: HOOK must precede VIBE");
    }
    let body = crate::junctions::form::normalize_body(body);
    if body.is_empty() {
        bail!("vibe: empty VIBE body");
    }
    Ok((score, hook, body))
}

/// Validated Influencer reply.
#[derive(Clone, Debug)]
pub struct VibeReply {
    pub sentiment: i32,
    /// Optional card title.
    pub hook: Option<String>,
    pub vibe_prompt: String,
}

/// VibeParser is the vibe stage's `Parser` plug-in: it wraps `parse_vibe_reply`
/// behind the capability library's `Parser<T>` seam.
/// It never returns the fail-closed `Ok(None)` — vibe's only fail-closed path is the
/// no-corpus short-circuit *before* the model call (a NULL marker), so an unparseable reply
/// is a genuine failure -> `Err` -> the work item backs off.
pub struct VibeParser;

impl Parser<VibeReply> for VibeParser {
    fn parse(&self, raw: &str) -> Result<Option<VibeReply>> {
        let (sentiment, hook, vibe_prompt) = parse_vibe_reply(raw)
            .with_context(|| format!("parse sentiment (raw={:?})", truncate(raw, 120)))?;
        let hook = crate::guards::settle_title("influencer", hook.as_deref())
            .ok_or_else(|| anyhow!("vibe: missing or invalid HOOK line"))?;
        // Typography is scrubbed rather than treated as a content failure.
        let vibe_prompt = crate::guards::clean_served_prose(&vibe_prompt);
        // Keep prose before the first prompt-echo marker; all-echo output retries.
        let vibe_prompt = crate::guards::truncate_prompt_echo(&vibe_prompt).to_string();
        if vibe_prompt.is_empty() {
            tracing::warn!(guard = "prompt_echo", "vibe body rejected: all echo");
            bail!("vibe: body is prompt echo");
        }
        if let Some(p) = crate::guards::first_product_name(&vibe_prompt) {
            tracing::warn!(guard = "product_name", name = p, "vibe body rejected");
            bail!("vibe: body names product {p:?}");
        }
        if crate::guards::has_foreign_script(&vibe_prompt) {
            tracing::warn!(guard = "foreign_script", "vibe body rejected");
            bail!("vibe: body carries a foreign-script run");
        }
        Ok(Some(VibeReply {
            sentiment,
            hook: Some(hook),
            vibe_prompt,
        }))
    }
}

// ---------------------------------------------------------------------------
// The core generate + the production handler.
// ---------------------------------------------------------------------------

/// Undebounced load-and-generate composition used outside the production handler. Continuity
/// and relational memory are supplied only by the handler.
pub async fn generate_vibe(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport_raw: &str,
    temperature: f64,
) -> Result<VibeOutput> {
    let ctx = load_vibe_context(hx, entity_type, entity_id, entity_name, sport_raw).await?;
    let out = generate_vibe_from_context(
        hx,
        entity_type,
        entity_name,
        sport_raw,
        ctx,
        None,
        None,
        None,
        temperature,
    )
    .await?;
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
async fn generate_vibe_from_context(
    hx: &Harness,
    entity_type: &str,
    entity_name: &str,
    sport_raw: &str,
    ctx: VibeContext,
    previous: Option<&PrevVibe>,
    memory: Option<&str>,
    identity: Option<&str>,
    temperature: f64,
) -> Result<VibeOutput> {
    // Never-scored empty context becomes a NULL marker. Empty context with a prior real read
    // generates one closing quiet card, then the empty-material hash debounces future drains.
    if ctx.empty() && previous.is_none() {
        return Ok(Generation::uncalled(
            VibeScore {
                sentiment: None,
                vibe_prompt: None,
                hook: None,
                input_components_json: ctx.input_components_json,
            },
            hx.router.for_role(Role::VibeLogic).model().to_string(),
            VIBE_PROMPT_VERSION,
            Vec::new(),
            Some(ctx.input_hash),
        ));
    }

    let prompt = build_sentiment_prompt(
        entity_type,
        entity_name,
        sport_raw,
        &ctx.packets,
        previous,
        memory,
        identity,
    );
    let opts = GenerateOptions {
        system: Some(VIBE_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict: if crate::route::small_voice_window(hx.voice_num_ctx) {
            crate::junctions::oracle::SMALL_WINDOW_NUM_PREDICT
        } else {
            VIBE_NUM_PREDICT
        },
        num_ctx: hx.voice_num_ctx,
        json_mode: false,
        format_schema: None,
        format_schema_raw: None,
    };

    // vibe = route(VibeLogic) + extract(VibeParser). The fail-closed contract lives in
    // the parser: an unparseable reply surfaces as its `Err` (item fails + backs off), and
    // `extract` records the exact wire body it sent.
    let extracted = hx
        .extract(Role::VibeLogic, &prompt, &opts, &VibeParser)
        .await?;
    let call = GenerationCall::from(&extracted);
    let model = extracted.model.clone();

    // VibeParser only ever returns `Ok(Some)` on success (vibe's no-corpus marker is the
    // pre-model short-circuit above, not a parser fail-closed), so a `None` here would be a
    // contract violation — fail the item rather than fabricate a row.
    let reply = extracted
        .value
        .ok_or_else(|| anyhow!("vibe: parser returned no value"))?;

    Ok(Generation::called(
        VibeScore {
            sentiment: Some(reply.sentiment),
            vibe_prompt: if reply.vibe_prompt.is_empty() {
                None
            } else {
                Some(reply.vibe_prompt)
            },
            hook: reply.hook,
            input_components_json: ctx.input_components_json,
        },
        model,
        VIBE_PROMPT_VERSION,
        Vec::new(),
        Some(ctx.input_hash),
        call,
    ))
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
) -> Result<i64> {
    // Route the moat fields through the shared Provenance envelope — input_hash included
    // since F2 (mig 147); the typed INSERT stays the stage's own (Postgres-as-serializer).
    let entity_id = item.entity_id_i32()?;
    let prov = &out.provenance;
    let sentiment: Option<i16> = out.sentiment.map(|n| n as i16);
    let row = sqlx::query(
        r#"
        INSERT INTO vibe_scores (
            entity_type, entity_id, sport,
            trigger_type, trigger_payload,
            sentiment, prompt, hook, input_news_ids,
            model_version, prompt_version, input_hash
        ) VALUES ($1,$2,$3,'periodic','null'::jsonb,$4,$5,$6,$7,$8,$9,$10)
        RETURNING id
        "#,
    )
    .bind(&item.entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(sentiment)
    .bind(out.vibe_prompt.as_deref())
    .bind(out.hook.as_deref())
    .bind(prov.input_ids.as_slice())
    .bind(prov.model_version.as_str())
    .bind(prov.prompt_version)
    .bind(prov.input_hash.as_deref())
    .fetch_one(pool)
    .await
    .context("persist vibe")?;
    Ok(row.get("id"))
}

/// VibeHandler drains the durable `vibe` stage: read the current packets, score
/// with the model, persist to vibe_scores, and enqueue the Momentum gate before completing.
/// This is the production path registered in `main.rs`.
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

    // One slot leaves room in the shared voice group for the terminal Oracle.
    fn max_in_flight(&self) -> usize {
        1
    }
    fn slot_group(&self) -> Option<(&'static str, usize)> {
        Some(crate::stage::MAC_SLOTS)
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let entity_id = item.entity_id_i32()?;
        // The name lookup uses the queue's raw sport value; sport normalization happens below.
        let name = lookup_entity_name(&hx.pool, &item.entity_type, entity_id, &item.sport).await?;
        let sport = item.sport.to_uppercase();

        // Gate on the entity-scoped material hash before the model call.
        let ctx = load_vibe_context(hx, &item.entity_type, entity_id, &name, &item.sport).await?;
        let key = EntityKey {
            entity_type: item.entity_type.clone(),
            entity_id,
            sport: sport.clone(),
            season: None,
        };
        // Load the debounce hash and latest continuity candidate in one round trip.
        let (latest_sentiment, latest_prompt, latest_hash) =
            load_latest_vibe_row(&hx.pool, &key).await?;
        // Continuity uses the latest scored row. If the newest row is a marker, look beneath it.
        let scored = match latest_sentiment {
            Some(s) => Some((s, latest_prompt.clone())),
            None if latest_hash.is_some() => load_latest_scored_vibe_row(&hx.pool, &key).await?,
            None => None,
        };
        // A marker above a scored row bypasses debounce once to file the closing quiet card.
        let buried = latest_sentiment.is_none() && scored.is_some();
        if latest_hash.as_deref() == Some(ctx.input_hash.as_str()) && !buried {
            debug!(
                entity_type = %item.entity_type,
                entity_id = item.entity_id,
                sport = %sport,
                "vibe: debounce-skip, material inputs unchanged"
            );
            // Still hand off: the momentum enqueue is hash-gated and cheap, so a previously
            // lost hand-off self-heals as a no-op — the same shape as sigil's skip-path
            // oracle enqueue.
            crate::junctions::analyst::enqueue_momentum_if_needed(
                hx,
                &item.entity_type,
                entity_id,
                &sport,
            )
            .await?;
            return Ok(());
        }

        // Previous scored prose is prompt-only continuity and never enters the material hash.
        let previous = scored.map(|(s, p)| PrevVibe {
            sentiment: s as i32,
            vibe_prompt: p.unwrap_or_default(),
        });
        // Memory-load failure degrades to an unenriched prompt (the n8 discipline): the
        // corpus is the primary signal, memory is enrichment.
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
                warn!(
                    entity_type = %item.entity_type,
                    entity_id = item.entity_id,
                    sport = %sport,
                    error = %e,
                    "vibe: relational memory load failed (continuing without memory)"
                );
                None
            }
        };

        // Identity card: house records, dated — degrades to absent like memory.
        let identity =
            crate::corpus::load_identity_card(&hx.pool, &item.entity_type, entity_id, &sport)
                .await
                .unwrap_or_default();

        let out = generate_vibe_from_context(
            hx,
            &item.entity_type,
            &name,
            &item.sport,
            ctx,
            previous.as_ref(),
            memory.as_deref(),
            identity.as_deref(),
            VIBE_TEMPERATURE,
        )
        .await?;
        let product_row_id = persist_to_vibe_scores(&hx.pool, item, &sport, &out).await?;
        insert_generation_ledger_best_effort(
            &hx.pool,
            &out,
            VIBE_LEDGER,
            LedgerEvent {
                entity_type: &item.entity_type,
                entity_id,
                sport: &sport,
                pair_entity: None,
                trigger_type: "periodic",
                trigger_payload: serde_json::Value::Null,
                product_row_ids: vec![product_row_id],
                included_evidence: serde_json::json!({
                    "input_components": serde_json::from_str::<serde_json::Value>(
                        &out.input_components_json
                    ).unwrap_or_else(|_| serde_json::json!({
                        "raw_input_components": out.input_components_json
                    })),
                    "sentiment": out.sentiment,
                    "vibe_prompt": &out.vibe_prompt,
                    "hook": &out.hook,
                }),
                excluded_evidence: if out.was_called() {
                    serde_json::json!([])
                } else {
                    serde_json::json!([{"reason": "no_live_packets"}])
                },
                context_budget: out.context_budget(serde_json::json!({
                    "num_predict": VIBE_NUM_PREDICT,
                })),
                parser_outcome: if out.was_called() {
                    "parsed"
                } else {
                    "no_call"
                },
            },
        )
        .await;

        // Vibe now feeds Momentum first; Momentum persists the generated trajectory card and then
        // enqueues Sigil if the Momentum context actually moved.
        if !crate::junctions::analyst::enqueue_momentum_if_needed(
            hx,
            &item.entity_type,
            entity_id,
            &sport,
        )
        .await?
        {
            debug!(
                entity_type = %item.entity_type,
                entity_id = item.entity_id,
                sport = %sport,
                "vibe: momentum enqueue skipped unchanged/empty context"
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
