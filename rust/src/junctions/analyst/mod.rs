//! Momentum stage: the Analyst narrates the Scout and Vibe rails over deterministic slopes.
//!
//! `momentum_scores` remains the numeric backbone for leaderboards and ranking. This stage adds the
//! client-surfaced read: a direction, a signed ±5 conviction, and the blurb with provenance,
//! persisted to `momentum_summaries` and consumed by the Oracle as the Momentum pillar.
//!
//! Direction and conviction are computed here; only the read and headline come from the model.

use crate::harness::{EntityKey, Generation, GenerationCall, Harness, Parser};
use crate::junctions::oracle::{self, SynthMomentum, SynthRating, SynthVibe};
use crate::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::util::{hash_components, round1};
use crate::work::{self, Item, Stage};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use tracing::debug;

mod inputs;
pub mod prompt;
pub use inputs::build_momentum_prompt;
pub use prompt::{MOMENTUM_PROMPT_VERSION, MOMENTUM_SYSTEM_PROMPT};

/// Output contract captured separately in the diagnostic ledger.
pub const MOMENTUM_OUTPUT_CONTRACT_VERSION: &str = "momentum-summary-v1";

const MOMENTUM_LEDGER: LedgerSpec = LedgerSpec {
    stage: "momentum",
    lens: "momentum",
    role: Role::MomentumLogic,
    product_table: "momentum_summaries",
    output_contract_version: MOMENTUM_OUTPUT_CONTRACT_VERSION,
};

/// Keep Momentum on the incumbent stats route until a broader fixture set proves a split.
pub const MOMENTUM_TEMPERATURE: f64 = 0.3;

// A single direction read — two rails and what they are doing to each other — is a handful of
// sentences on a card. Nothing here scales with story count.
pub const MOMENTUM_NUM_PREDICT: i32 = 300;

/// Scores within this band are steady; values at or beyond it rise or fall by sign.
pub const MOMENTUM_STEADY_BAND: f64 = 10.0;

const MOMENTUM_WORK_PREFIX: &str = "momentum:s";

#[derive(Clone, Debug)]
pub struct MomentumContext {
    pub season: i32,
    pub rating: Option<SynthRating>,
    pub vibe: Option<SynthVibe>,
    pub snapshot: SynthMomentum,
    pub input_components_json: String,
    pub input_hash: String,
}

impl MomentumContext {
    fn empty(&self) -> bool {
        self.rating.is_none() && self.vibe.is_none() && self.snapshot.empty()
    }
}

/// Parsed model prose. Direction and conviction are deterministic product fields.
#[derive(Clone, Debug)]
pub struct MomentumReply {
    /// The Analyst's read.
    pub blurb: String,
    /// Optional card title.
    pub headline: Option<String>,
}

#[derive(Clone, Debug)]
pub struct MomentumSummary {
    pub direction: String,
    pub score: i32,
    pub blurb: String,
    /// Model-emitted card title.
    pub headline: Option<String>,
    pub season: i32,
    pub input_components_json: String,
}

pub type MomentumOutput = Generation<MomentumSummary>;

pub struct MomentumParser;

impl Parser<MomentumReply> for MomentumParser {
    fn parse(&self, raw: &str) -> Result<Option<MomentumReply>> {
        // Keep a bounded raw excerpt so malformed output is diagnosable.
        let mut reply = parse_momentum_reply(raw).ok_or_else(|| {
            anyhow!(
                "momentum: invalid response (raw={:?})",
                crate::util::truncate_bytes(raw.trim(), 160)
            )
        })?;
        // Production guards live at the Parser seam; eval can still inspect the raw parse.
        if let Some(p) =
            crate::guards::first_banned_phrase(&reply.blurb, crate::guards::MOMENTUM_BANNED_PHRASES)
        {
            tracing::warn!(
                guard = "momentum_banned_phrase",
                phrase = p,
                "momentum READ rejected"
            );
            anyhow::bail!("momentum: READ carries banned phrase {p:?}");
        }
        if let Some(p) = crate::guards::first_product_name(&reply.blurb) {
            tracing::warn!(guard = "product_name", name = p, "momentum READ rejected");
            anyhow::bail!("momentum: READ names product {p:?}");
        }
        // Sporting numbers are evidence; internal field citations leak the input contract.
        if crate::guards::has_bookkeeping_citation(&reply.blurb) {
            tracing::warn!(guard = "bookkeeping_citation", "momentum READ rejected");
            anyhow::bail!("momentum: READ carries a bookkeeping citation");
        }
        // A bad optional title degrades to NULL without costing the read.
        reply.headline = crate::guards::settle_title("analyst", reply.headline.as_deref());
        Ok(Some(reply))
    }
}

pub async fn load_momentum_snapshot(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<SynthMomentum> {
    #[allow(clippy::type_complexity)]
    let row: Option<(Option<f64>, i32, Option<f64>, i32, Option<f64>)> = sqlx::query_as(
        r#"
        SELECT vibe_slope::float8, vibe_samples,
               rating_slope::float8, rating_samples,
               momentum_score::float8
        FROM public.latest_momentum_scores_per_entity
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
        LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load momentum snapshot {entity_type}/{entity_id}"))?;

    Ok(row
        .map(
            |(vibe_slope, vibe_samples, rating_slope, rating_samples, momentum_score)| {
                SynthMomentum {
                    vibe_slope,
                    vibe_samples,
                    rating_slope,
                    rating_samples,
                    momentum_score,
                    ..SynthMomentum::default()
                }
            },
        )
        .unwrap_or_default())
}

pub async fn load_momentum_context(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<MomentumContext> {
    let season = oracle::resolve_season(&hx.pool, sport, None).await?;
    let (rating, vibe, snapshot) = tokio::try_join!(
        oracle::load_rating_pillar(&hx.pool, entity_type, entity_id, sport, Some(season)),
        oracle::load_vibe_pillar(&hx.pool, entity_type, entity_id, sport),
        load_momentum_snapshot(&hx.pool, entity_type, entity_id, sport),
    )?;
    let input_components_json =
        build_momentum_input_components(rating.as_ref(), vibe.as_ref(), &snapshot);
    let input_hash = hash_components(&input_components_json);
    Ok(MomentumContext {
        season,
        rating,
        vibe,
        snapshot,
        input_components_json,
        input_hash,
    })
}

pub async fn enqueue_momentum_if_needed(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<bool> {
    let sport = sport.to_uppercase();
    let ctx = load_momentum_context(hx, entity_type, entity_id, &sport).await?;
    if ctx.empty() {
        return Ok(false);
    }
    let key = EntityKey {
        entity_type: entity_type.to_string(),
        entity_id,
        sport: sport.clone(),
        season: Some(ctx.season),
    };
    if hx
        .debounce_unchanged("momentum_summaries", &key, &ctx.input_hash)
        .await?
    {
        return Ok(false);
    }
    let it = Item {
        stage: Stage::Momentum,
        entity_type: entity_type.to_string(),
        entity_id: i64::from(entity_id),
        sport,
        input_version: Some(momentum_work_input_version(ctx.season, &ctx.input_hash)),
        attempts: 0,
    };
    work::enqueue(&hx.pool, &it).await?;
    Ok(true)
}

pub fn momentum_work_input_version(season: i32, input_hash: &str) -> String {
    format!("{MOMENTUM_WORK_PREFIX}{season}:{input_hash}")
}

/// Deterministic direction from the ±100-scale signed slope average. No snapshot means steady.
pub fn momentum_direction_from_score(momentum_score: Option<f64>) -> &'static str {
    match momentum_score {
        Some(s) if s >= MOMENTUM_STEADY_BAND => "rising",
        Some(s) if s <= -MOMENTUM_STEADY_BAND => "falling",
        _ => "steady",
    }
}

/// Deterministic ±5 conviction from `momentum_score`. The steady band maps to zero or a one-point
/// lean; larger absolute scores step through the remaining bands. No snapshot maps to zero.
pub fn momentum_conviction_from_score(momentum_score: Option<f64>) -> i32 {
    let Some(s) = momentum_score else { return 0 };
    let mag = s.abs();
    let sign = if s < 0.0 { -1 } else { 1 };
    let step = if mag < MOMENTUM_STEADY_BAND / 2.0 {
        return 0; // genuinely flat: no measured lean at all
    } else if mag < 20.0 {
        1 // covers the top half of the steady band AND the first rising/falling notch
    } else if mag < 35.0 {
        2
    } else if mag < 55.0 {
        3
    } else if mag < 80.0 {
        4
    } else {
        5
    };
    sign * step
}

fn build_momentum_input_components(
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
) -> String {
    let mut components = serde_json::Map::new();
    components.insert(
        "prompt_version".into(),
        serde_json::json!(MOMENTUM_PROMPT_VERSION),
    );
    if let Some(r) = rating {
        components.insert("notability".into(), serde_json::json!(r.notability));
        components.insert(
            "rating_trajectory".into(),
            serde_json::json!(r.rating_trajectory),
        );
        if !r.rating_trajectory_label.is_empty() {
            components.insert(
                "rating_trajectory_label".into(),
                serde_json::json!(r.rating_trajectory_label),
            );
        }
    }
    if let Some(v) = vibe {
        // Sentiment only — the vibe felt-read prose stays in the PROMPT but out of the hash
        // (F1, material-only debounce): vibe generates at temp 0.7, so its prose changes on
        // every re-run even when material is byte-identical; hashing it cascaded
        // momentum→sigil→oracle regenerations on zero material change.
        components.insert("vibe_sentiment".into(), serde_json::json!(v.sentiment));
    }
    if let Some(s) = mom.rating_slope {
        components.insert("momentum_rating_slope".into(), serde_json::json!(round1(s)));
        components.insert(
            "momentum_rating_samples".into(),
            serde_json::json!(mom.rating_samples),
        );
    }
    if let Some(s) = mom.vibe_slope {
        components.insert("momentum_vibe_slope".into(), serde_json::json!(round1(s)));
        components.insert(
            "momentum_vibe_samples".into(),
            serde_json::json!(mom.vibe_samples),
        );
    }
    if let Some(score) = mom.momentum_score {
        components.insert("momentum_score".into(), serde_json::json!(round1(score)));
    }
    serde_json::Value::Object(components).to_string()
}

pub fn parse_momentum_reply(raw: &str) -> Option<MomentumReply> {
    let rest = raw.trim().strip_prefix("READ:")?;
    let (body, headline) = match rest.split_once("\nHEADLINE:") {
        Some((body, title)) if !title.contains('\n') => {
            let title = title.trim();
            (body, (!title.is_empty()).then(|| title.to_string()))
        }
        Some(_) => return None,
        None => (rest, None),
    };

    let blurb = crate::guards::clean_served_prose(&crate::junctions::form::normalize_body(body));
    if blurb.is_empty() {
        return None;
    }
    // Reject a foreign-script generation so the work item retries.
    if crate::guards::has_foreign_script(&blurb) {
        return None;
    }
    Some(MomentumReply { blurb, headline })
}

async fn persist_momentum_summary(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    out: &MomentumOutput,
) -> Result<i64> {
    let trigger_payload = serde_json::json!({});
    let row = sqlx::query(
        r#"
        INSERT INTO public.momentum_summaries (
            entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            direction, score, blurb, headline, input_components, input_hash,
            model_version, prompt_version, generated_at
        ) VALUES ($1,$2,$3,$4,'periodic',$5::jsonb,$6,$7,$8,$9,$10::jsonb,$11,$12,$13,NOW())
        RETURNING id
        "#,
    )
    .bind(&item.entity_type)
    .bind(item.entity_id_i32()?)
    .bind(sport)
    .bind(out.season)
    .bind(&trigger_payload)
    .bind(&out.direction)
    .bind(out.score as i16)
    .bind(&out.blurb)
    .bind(&out.headline)
    .bind(&out.input_components_json)
    .bind(out.provenance.input_hash.as_deref())
    .bind(&out.provenance.model_version)
    .bind(out.provenance.prompt_version)
    .fetch_one(pool)
    .await
    .context("persist momentum summary")?;
    Ok(row.get("id"))
}

pub struct MomentumHandler;

impl MomentumHandler {
    pub fn new() -> Self {
        MomentumHandler
    }
}

impl Default for MomentumHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for MomentumHandler {
    fn stage(&self) -> Stage {
        Stage::Momentum
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let entity_id = item.entity_id_i32()?;
        let sport = item.sport.to_uppercase();
        let name =
            crate::corpus::lookup_entity_name(&hx.pool, &item.entity_type, entity_id, &item.sport)
                .await?;
        let ctx = load_momentum_context(hx, &item.entity_type, entity_id, &sport).await?;
        if ctx.empty() {
            debug!(
                entity_type = %item.entity_type,
                entity_id = item.entity_id,
                sport = %sport,
                "momentum: skipped empty context"
            );
            return Ok(());
        }
        // Enqueue performs the empty and debounce gates; the recomputed hash records provenance
        // for the row actually generated. The Analyst reads only the two numeric rails.

        // Identity card: house records, dated — degrades to absent like memory.
        let identity =
            crate::corpus::load_identity_card(&hx.pool, &item.entity_type, entity_id, &sport)
                .await
                .unwrap_or_default();
        let prompt = build_momentum_prompt(
            &item.entity_type,
            &name,
            &item.sport,
            ctx.rating.as_ref(),
            ctx.vibe.as_ref(),
            &ctx.snapshot,
            identity.as_deref(),
        );
        let opts = GenerateOptions {
            system: Some(MOMENTUM_SYSTEM_PROMPT.to_string()),
            temperature: Some(MOMENTUM_TEMPERATURE),
            num_predict: if crate::route::small_voice_window(hx.voice_num_ctx) {
                crate::junctions::oracle::SMALL_WINDOW_NUM_PREDICT
            } else {
                MOMENTUM_NUM_PREDICT
            },
            num_ctx: hx.voice_num_ctx,
            json_mode: false,
            format_schema: None,
            format_schema_raw: None,
        };
        let extracted = hx
            .extract(Role::MomentumLogic, &prompt, &opts, &MomentumParser)
            .await?;
        let call = GenerationCall::from(&extracted);
        let model = extracted.model.clone();
        let reply = extracted
            .value
            .ok_or_else(|| anyhow!("momentum: parser returned no value"))?;

        // Both numbers come from the same score, so they cannot disagree. The decided direction
        // was included in the prompt for the model to narrate.
        let direction = momentum_direction_from_score(ctx.snapshot.momentum_score);
        let score = momentum_conviction_from_score(ctx.snapshot.momentum_score);

        // A title that misses the entity degrades to no title without retrying the read.
        let headline = reply.headline.filter(|t| {
            let named = crate::guards::title_names_entity(t, &name);
            if !named {
                tracing::warn!(seat = "analyst", guard = "title_entity_absent",
                    entity = %name, title = %t,
                    "headline names no form of the entity; dropped");
            }
            named
        });

        let out = Generation::called(
            MomentumSummary {
                direction: direction.to_string(),
                score,
                blurb: reply.blurb,
                headline,
                season: ctx.season,
                input_components_json: ctx.input_components_json.clone(),
            },
            model,
            MOMENTUM_PROMPT_VERSION,
            Vec::new(),
            Some(ctx.input_hash.clone()),
            call,
        );
        let product_row_id = persist_momentum_summary(&hx.pool, item, &sport, &out).await?;
        insert_generation_ledger_best_effort(
            &hx.pool,
            &out,
            MOMENTUM_LEDGER,
            LedgerEvent {
                entity_type: &item.entity_type,
                entity_id,
                sport: &sport,
                pair_entity: None,
                trigger_type: "periodic",
                trigger_payload: serde_json::json!({}),
                product_row_ids: vec![product_row_id],
                included_evidence: serde_json::json!({
                    "input_components": serde_json::from_str::<serde_json::Value>(
                        &ctx.input_components_json
                    ).unwrap_or_else(|_| serde_json::json!({
                        "raw_input_components": ctx.input_components_json
                    })),
                    "has_rating": ctx.rating.is_some(),
                    "has_vibe": ctx.vibe.is_some(),
                    "has_momentum_snapshot": !ctx.snapshot.empty(),
                }),
                excluded_evidence: serde_json::json!({"empty_context": ctx.empty()}),
                context_budget: out.context_budget(serde_json::json!({
                    "num_predict": MOMENTUM_NUM_PREDICT,
                    "decided_direction": direction,
                    "steady_band": MOMENTUM_STEADY_BAND,
                    "computed_conviction": score,
                })),
                parser_outcome: "scored",
            },
        )
        .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
