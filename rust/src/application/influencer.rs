//! Influencer evidence, debounce, publication and Momentum coordination.

use crate::evidence::memories::{self, MemoryRequest, Mission};

use crate::application::models::Models;
use crate::application::products::EntityKey;
use crate::application::queue::work::Item;
use crate::evidence::corpus::lookup_entity_name;
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::runtime::route::Role;
use crate::studio::influencer::{
    self, Assignment, PacketBlock, VibeOutput, VIBE_NUM_PREDICT, VIBE_PROMPT_VERSION,
    VIBE_TEMPERATURE,
};
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::studio::Studio;
use crate::util::hash_components;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction};
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
    pool: &sqlx::PgPool,
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

    let packets = load_vibe_packets(pool, entity_type, entity_id, entity_name, &sport).await?;
    let input_components_json = build_vibe_input_components(&packets);
    let memories = memories::load(
        pool,
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
    use crate::evidence::news::render::Voice;

    Ok(crate::evidence::news::packet::render_packets_for_entity(
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
        if crate::studio::model::small_voice_window(voice_num_ctx) {
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

enum Prepared {
    Debounced,
    Product(Box<VibeOutput>),
}

async fn prepare(
    studio: &Studio<'_>,
    request: &Request<'_>,
    ctx: &VibeContext,
    latest: &(Option<i16>, Option<String>),
) -> Result<Prepared> {
    let buried = latest.0.is_none() && ctx.memories.previous_score.is_some();
    if latest.1.as_deref() == Some(ctx.input_hash.as_str()) && !buried {
        debug!("vibe: debounce-skip, material inputs unchanged");
        return Ok(Prepared::Debounced);
    }
    Ok(Prepared::Product(Box::new(
        influencer::create(studio, &request.assignment(ctx)?).await?,
    )))
}

/// persist_to_vibe_scores writes one row to the LIVE vibe_scores table — both the scored
/// row and the no-corpus NULL marker, which differ only in the bound values. Mirrors
/// persistSentiment / persistNoCorpus: trigger_type 'periodic', trigger_payload the JSON
/// `null` (marshal of a nil trigger map), empty felt-read stored as NULL.
async fn persist_to_vibe_scores(
    tx: &mut Transaction<'_, Postgres>,
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
    .fetch_one(&mut **tx)
    .await
    .context("persist vibe")?;
    Ok(row.get("id"))
}

async fn record_ledger(
    pool: &sqlx::PgPool,
    item: &Item,
    sport: &str,
    product_row_id: i64,
    out: &VibeOutput,
) -> Result<()> {
    insert_generation_ledger_best_effort(
        pool,
        out,
        VIBE_LEDGER,
        LedgerEvent {
            entity_type: &item.entity_type,
            entity_id: item.entity_id_i32()?,
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
    Ok(())
}

async fn commit_claimed(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    prepared: &Prepared,
) -> Result<(PluginOutcome, Option<i64>)> {
    let mut tx = pool.begin().await.context("begin vibe publication")?;
    if !crate::application::queue::work::lock_claim(&mut tx, item).await? {
        tx.rollback()
            .await
            .context("close superseded vibe publication")?;
        return Ok((PluginOutcome::Superseded, None));
    }

    let product_row_id = match prepared {
        Prepared::Debounced => None,
        Prepared::Product(output) => {
            Some(persist_to_vibe_scores(&mut tx, item, sport, output).await?)
        }
    };
    crate::application::queue::outbox::record_vibe_completed(&mut tx, item).await?;
    if !crate::application::queue::work::complete_in_transaction(&mut tx, item).await? {
        bail!("vibe claim changed while its publication transaction held the row lock");
    }
    tx.commit().await.context("commit vibe publication")?;
    Ok((PluginOutcome::Committed, product_row_id))
}

/// VibeHandler drains the durable `vibe` stage: read the current packets, score
/// with the model, persist to vibe_scores, and enqueue the Momentum gate before completing.
/// This is the production path registered in `main.rs`.
pub struct VibeHandler {
    pool: sqlx::PgPool,
    models: std::sync::Arc<Models>,
}

impl VibeHandler {
    pub fn new(pool: sqlx::PgPool, models: std::sync::Arc<Models>) -> Self {
        Self { pool, models }
    }
}

#[async_trait]
impl StudioPlugin for VibeHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &crate::studio::fleet::INFLUENCER
    }

    // One slot leaves room in the shared voice group for the terminal Oracle.

    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        let pool = &self.pool;
        let models = &self.models;
        let entity_id = item.entity_id_i32()?;
        // The name lookup uses the queue's raw sport value; sport normalization happens below.
        let name = lookup_entity_name(pool, &item.entity_type, entity_id, &item.sport).await?;
        let sport = item.sport.to_uppercase();

        // Gate on the entity-scoped material hash before the model call.
        let ctx = load_vibe_context(pool, &item.entity_type, entity_id, &name, &item.sport).await?;
        let key = EntityKey {
            entity_type: item.entity_type.clone(),
            entity_id,
            sport: sport.clone(),
            season: None,
        };
        let latest = load_latest_vibe_row(pool, &key).await?;
        let request = Request {
            entity_type: &item.entity_type,
            entity_name: &name,
            sport: &item.sport,
            temperature: VIBE_TEMPERATURE,
            voice_num_ctx: models.voice_num_ctx,
        };
        let model = models.router.for_role(Role::VibeLogic);
        let prepared = prepare(&Studio::new(model.as_ref()), &request, &ctx, &latest).await?;
        let (outcome, product_row_id) = commit_claimed(pool, item, &sport, &prepared).await?;
        if let (Some(product_row_id), Prepared::Product(output)) = (product_row_id, &prepared) {
            record_ledger(pool, item, &sport, product_row_id, output).await?;
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests;
