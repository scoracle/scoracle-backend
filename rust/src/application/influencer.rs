//! Influencer evidence, debounce, publication and Momentum coordination.

use crate::composition::memories::{self, MemoryRequest, Mission};

use crate::evidence::corpus::lookup_entity_name;
use crate::runtime::harness::{EntityKey, Harness};
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::runtime::route::Role;
use crate::runtime::stage::StageHandler;
use crate::runtime::util::hash_components;
use crate::runtime::work::{Item, Stage};
use crate::studio::influencer::{
    self, Assignment, PacketBlock, VibeOutput, VibeScore, VIBE_NUM_PREDICT, VIBE_PROMPT_VERSION,
    VIBE_TEMPERATURE,
};
use crate::studio::{Publisher, Studio};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use tracing::debug;

/// Output contract captured separately in the diagnostic ledger.
pub const VIBE_OUTPUT_CONTRACT_VERSION: &str = "vibe-score-v1";

const VIBE_LEDGER: LedgerSpec = LedgerSpec {
    stage: "vibe",
    lens: "vibe",
    role: Role::VibeLogic,
    product_table: "vibe_scores",
    output_contract_version: VIBE_OUTPUT_CONTRACT_VERSION,
};

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
    pub memories: memories::Package,
    /// The entity's live packets rendered for the Influencer.
    pub packets: Vec<PacketBlock>,
    pub input_components_json: String,
    pub input_hash: String,
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
    let memories = memories::load(
        &hx.pool,
        MemoryRequest::new(Mission::Influencer, entity_type, entity_id, &sport),
    )
    .await?;
    let input_components_json = memories.with_input_components(&input_components_json)?;
    let input_hash = hash_components(&input_components_json);

    Ok(VibeContext {
        memories,
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
/// actually moved since the last `vibe_scores` row — `Ok(false)` when unchanged
/// (nothing enqueued), `Ok(true)` on enqueue. Idempotent: `work::enqueue`'s ON CONFLICT
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
        claim_token: None,
    };
    crate::runtime::work::enqueue(&hx.pool, &it).await?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// Prompt assembly.
// ---------------------------------------------------------------------------

/// Read the latest marker/score and debounce hash. Prior interpretation belongs to memories.
async fn load_latest_vibe_row(
    pool: &PgPool,
    key: &EntityKey,
) -> Result<(Option<i16>, Option<String>)> {
    let row = sqlx::query_as("SELECT sentiment,input_hash FROM vibe_scores WHERE entity_type=$1 AND entity_id=$2 AND sport=$3 ORDER BY generated_at DESC,id DESC LIMIT 1")
        .bind(&key.entity_type).bind(key.entity_id).bind(&key.sport).fetch_optional(pool).await?;
    Ok(row.unwrap_or((None, None)))
}

/// Runtime-selected capacity; the small envelope still reserves 700 output tokens.
fn production_options(
    temperature: f64,
    voice_num_ctx: i32,
) -> crate::studio::model::GenerateOptions {
    influencer::generation_options(
        temperature,
        voice_num_ctx,
        if crate::runtime::route::small_voice_window(voice_num_ctx) {
            700
        } else {
            VIBE_NUM_PREDICT
        },
    )
}

struct Request<'a> {
    entity_type: &'a str,
    entity_name: &'a str,
    sport: &'a str,
    temperature: f64,
    voice_num_ctx: i32,
}

impl Request<'_> {
    fn assignment(&self, ctx: &VibeContext) -> Result<Assignment> {
        Ok(Assignment {
            entity_type: self.entity_type.into(),
            entity_name: self.entity_name.into(),
            sport: self.sport.into(),
            packets: ctx.packets.clone(),
            // Preserve the early marker path: it never renders memory.
            memory: if ctx.empty() && ctx.memories.previous_score.is_none() {
                None
            } else {
                Some(ctx.memories.render_for_model()?)
            },
            previous_score: ctx.memories.previous_score,
            input_components_json: ctx.input_components_json.clone(),
            input_hash: ctx.input_hash.clone(),
            options: production_options(self.temperature, self.voice_num_ctx),
        })
    }
}

/// Undebounced standalone generation uses the same preparation and Studio creation as production.
pub async fn generate_vibe(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport_raw: &str,
    temperature: f64,
) -> Result<VibeOutput> {
    let ctx = load_vibe_context(hx, entity_type, entity_id, entity_name, sport_raw).await?;
    let request = Request {
        entity_type,
        entity_name,
        sport: sport_raw,
        temperature,
        voice_num_ctx: hx.voice_num_ctx,
    };
    let model = hx.router.for_role(Role::VibeLogic);
    influencer::create(&Studio::new(model.as_ref()), &request.assignment(&ctx)?).await
}

#[async_trait]
trait MomentumHandoff: Sync {
    async fn offer(&self) -> Result<()>;
}

#[derive(Debug, PartialEq, Eq)]
enum Outcome<R> {
    Debounced,
    Published(R),
}

/// Coordinate a prepared drain. Both successful paths offer Momentum; failed creation or
/// publication stops before handoff. This is application policy, not a Studio abstention.
async fn run_prepared<P: Publisher<VibeScore>, F: MomentumHandoff>(
    studio: &Studio<'_>,
    request: &Request<'_>,
    ctx: &VibeContext,
    latest: &(Option<i16>, Option<String>),
    publisher: &P,
    follow_up: &F,
) -> Result<Outcome<P::Receipt>> {
    let buried = latest.0.is_none() && ctx.memories.previous_score.is_some();
    let outcome = if latest.1.as_deref() == Some(ctx.input_hash.as_str()) && !buried {
        debug!("vibe: debounce-skip, material inputs unchanged");
        Outcome::Debounced
    } else {
        Outcome::Published(influencer::run(studio, &request.assignment(ctx)?, publisher).await?)
    };
    follow_up.offer().await?;
    Ok(outcome)
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

struct Publication<'a> {
    hx: &'a Harness,
    item: &'a Item,
    sport: &'a str,
}

#[async_trait]
impl Publisher<VibeScore> for Publication<'_> {
    type Receipt = i64;
    async fn publish(&self, out: &VibeOutput) -> Result<i64> {
        let Self { hx, item, sport } = *self;
        let entity_id = item.entity_id_i32()?;
        let product_row_id = persist_to_vibe_scores(&hx.pool, item, sport, out).await?;
        insert_generation_ledger_best_effort(
            &hx.pool,
            out,
            VIBE_LEDGER,
            LedgerEvent {
                entity_type: &item.entity_type,
                entity_id,
                sport,
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

        Ok(product_row_id)
    }
}

#[async_trait]
impl MomentumHandoff for Publication<'_> {
    async fn offer(&self) -> Result<()> {
        if !crate::junctions::analyst::enqueue_momentum_if_needed(
            self.hx,
            &self.item.entity_type,
            self.item.entity_id_i32()?,
            self.sport,
        )
        .await?
        {
            debug!(entity_type = %self.item.entity_type, entity_id = self.item.entity_id,
                sport = %self.sport, "vibe: momentum enqueue skipped unchanged/empty context");
        }
        Ok(())
    }
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
        Some(crate::runtime::stage::MAC_SLOTS)
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
        let latest = load_latest_vibe_row(&hx.pool, &key).await?;
        let request = Request {
            entity_type: &item.entity_type,
            entity_name: &name,
            sport: &item.sport,
            temperature: VIBE_TEMPERATURE,
            voice_num_ctx: hx.voice_num_ctx,
        };
        let model = hx.router.for_role(Role::VibeLogic);
        let publication = Publication {
            hx,
            item,
            sport: &sport,
        };
        run_prepared(
            &Studio::new(model.as_ref()),
            &request,
            &ctx,
            &latest,
            &publication,
            &publication,
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
