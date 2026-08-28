//! Vibe stage — the emotional/news rail end product.
//!
//! The deterministic loaders, prompt assembly, parser, and persist path live here so prompt changes
//! are versioned and inspectable. SQL supplies the persisted narrative/transfer context; Rust owns
//! the transient prompt shaping, model call, parsing, fail-closed marker, debounce, and downstream
//! queue hand-off.
//!
//! Fail-closed semantics reproduced verbatim: when an entity has NO narratives AND no
//! transfer heat, we skip the model and write a NULL-sentiment marker row (the read
//! path returns "no data"). A completed vibe enqueues Momentum before the terminal Sigil
//! convergence.
//!
//! F2 (2026-07-12) gives vibe a real debounce: the handler hashes the MATERIAL inputs only —
//! latest narrative titles/impacts/trajectories + transfer-heat facts, no prose, no
//! timestamps (mirroring narratives' key discipline) — and skips the GPU call when the
//! latest `vibe_scores` row carries the same `input_hash` (mig 147). Marker rows carry the
//! empty-material hash too, so quiet entities debounce instead of re-marking every cycle.
//! On a debounce-skip the handler still enqueues Momentum (hash-gated and cheap), so a prior
//! missed hand-off self-heals without spending a model call.
//!
//! v12 (junction memory rollout step 2, 2026-07-19): the prompt gains the previous vibe read
//! as a continuity anchor (the proven Sigil Phase-5.2 shape — prompt-only, never hashed) and
//! the per-entity relational memory card (`narrative_context_for_entity`, mig 163 — same
//! not-in-input-hash decision as n8/t8). Continuity discipline is the whiplash killer: the
//! felt read moves like a belief, not a readout of the day's headlines.

use crate::corpus::{
    dedupe_i64, load_transfer_heat, lookup_entity_name, HeatItem,
};
use crate::harness::{EntityKey, Harness, Parser, Provenance};
use crate::ledger::{insert_cognition_ledger_best_effort, CognitionLedgerEntry};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::trajectory::DEFAULT_TRAJECTORY;
use crate::util::{go_json_string, hash_components, truncate};
use crate::work::{Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use tracing::{debug, warn};

// This junction's contract with its model — system prompt, contract version, and prompt
// builder — lives in `prompt.rs`, so a change to what this character is asked is a one-file
// diff. Re-exported here so call sites and the ledger keep reading it from the stage module.
pub mod prompt;
pub use prompt::{VIBE_PROMPT_VERSION, VIBE_SYSTEM_PROMPT, build_sentiment_prompt};

/// Output contract captured separately in the Phase 2 diagnostic ledger.
pub const VIBE_OUTPUT_CONTRACT_VERSION: &str = "vibe-score-v1";

/// Production sentiment temperature.
pub const VIBE_TEMPERATURE: f64 = 0.7;

/// Token cap for the two-line answer.
// Second-largest, and for the same reason as The Journalist's: she voices each developing
// emotional story, so multiple stories means multiple reads. Nuance is her product.
pub const VIBE_NUM_PREDICT: i32 = 800;

/// Body truncation in the prompt.
const BODY_TRUNCATE: usize = 280;

/// v19: one story block's rendered allowance in the vibe prompt, in chars (~750 tokens).
/// MAX_VIBE_PACKETS bounds how many stories she reads; this bounds how DEEP each one runs —
/// a mega-storyline's single block reached ~2k tokens of claims and kept the seat's p95 over
/// the 4,096 window after the v18 diet. Two blocks at this cap spend ~1.5k tokens, which the
/// window arithmetic (system ~700 + narratives + heat + memory + reply room) actually affords.
///
/// 3,000 → 2,400 (2026-08-28): v24 composed the form blocks into the system prompt (~200 tok,
/// the same growth 26ce39f repaid for the Journalist) and the fat tail started 400ing over
/// the window — 20 busy entities stuck failed on "request (4099..4361 tokens) exceeds the
/// available context size (4096)", among them Chelsea and the Bengals, the entities a vibe
/// card matters most for. Two blocks now spend ~1.2k tokens; the ~300 the cut frees covers
/// the measured worst overflow (+265) with margin. Same treatment as
/// PACKET_NEWS_BUDGET_CHARS 6,000 → 5,200; keep rule unchanged.
const PACKET_BLOCK_TRUNCATE: usize = 2_400;

/// One narrative from the entity's latest generation (news_summaries).
#[derive(Clone, Debug)]
pub struct Narrative {
    pub title: String,
    pub body: String,
    pub impact: i32,
    pub trajectory: String,
    pub topic_heat: i32,
    /// Corroboration weight from the narratives stage (Phase 1): how many distinct sources
    /// backed the storyline. Was persisted from day one and read by nothing downstream.
    pub source_count: i32,
    /// Whole days since the storyline's freshest source (`source_latest_at`), when known.
    pub source_age_days: Option<i32>,
}

/// The result of running the vibe core for one entity, before persistence. Captures
/// the production row payload for `vibe_scores`.
#[derive(Clone, Debug)]
pub struct VibeOutput {
    /// `None` ⇒ no-corpus NULL marker (no model call was made).
    pub sentiment: Option<i32>,
    /// The one-sentence felt read; `None` when empty (the column is nullable).
    pub vibe_prompt: Option<String>,
    /// The Influencer's card title (v13 HOOK line); `None` for markers and hook-less replies.
    pub hook: Option<String>,
    /// Provenance: the narratives' source article ids, deduped in first-seen order.
    pub input_news_ids: Vec<i64>,
    /// The canonical material-inputs JSON — the pre-image of `input_hash` (F2).
    pub input_components_json: String,
    /// SHA-256 (128-bit hex prefix) of `input_components_json` — the debounce key. Always
    /// real: the no-corpus marker carries the empty-material hash so quiet entities debounce
    /// too (the narratives precedent).
    pub input_hash: String,
    /// no-corpus → the configured model name; scored → the model echoed in the response.
    pub model: String,
    pub prompt_version: &'static str,
    pub built_prompt: Option<String>,
    pub request_body: Option<serde_json::Value>,
    pub eval_count: Option<i32>,
    pub wall_ms: Option<u64>,
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
    /// Vibe debounces since F2 (mig 147), so `input_hash` is carried; the scored row and the
    /// no-corpus marker produce the same envelope shape, differing only in the values they carry.
    fn provenance(&self) -> Provenance {
        Provenance {
            model_version: self.model.clone(),
            prompt_version: self.prompt_version,
            input_ids: self.input_news_ids.clone(),
            input_hash: Some(self.input_hash.clone()),
            trigger_payload: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Corpus loaders.
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
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, i32, Vec<i64>, String, i32, i32, Option<i32>)> = sqlx::query_as(
        r#"
        SELECT narrative_title, body, COALESCE(impact, 0), input_news_ids,
               COALESCE(trajectory, $4),
               COALESCE((
                   SELECT max(a.topic_heat)::int
                   FROM unnest(input_news_ids) AS nid(article_id)
                   JOIN news_articles a ON a.id = nid.article_id
               ), 1) AS topic_heat,
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
    .with_context(|| format!("load narratives {entity_type}/{entity_id}"))?;

    let mut narratives = Vec::with_capacity(rows.len());
    let mut ids: Vec<i64> = Vec::new();
    for (title, body, impact, mut nids, trajectory, topic_heat, source_count, source_age_days) in
        rows
    {
        ids.append(&mut nids);
        narratives.push(Narrative {
            title,
            body,
            impact,
            trajectory,
            topic_heat,
            source_count,
            source_age_days,
        });
    }
    Ok((narratives, dedupe_i64(ids)))
}

/// order_narratives orders the storylines for the prompt: topic heat → impact → title.
///
/// Phase 2 dropped the identity re-load + candle embed that used to compute a per-narrative
/// relevance ordering key: its only product was this sort's primary axis, the offline parity
/// bins (no embedder) never ran it, and every debounce-skipped wake paid it for nothing.
/// Topic heat is the durable relevance proxy (candle-clustered upstream, and already in both
/// the prompt tags and the debounce pre-image), so live ordering now matches what the parity
/// bins always produced.
fn order_narratives(narratives: &mut [Narrative]) {
    narratives.sort_by(|a, b| {
        b.topic_heat
            .cmp(&a.topic_heat)
            .then_with(|| b.impact.cmp(&a.impact))
            .then_with(|| a.title.cmp(&b.title))
    });
}

// ---------------------------------------------------------------------------
// Input components + hash — the debounce key (F2, mig 147).
// ---------------------------------------------------------------------------

/// build_vibe_input_components is the canonical debounce pre-image: the `prompt_version`
/// (leading, the narratives M4 pattern — a v-bump forces exactly one regen per entity as its
/// pipeline next wakes), the latest narrative titles/impacts/trajectories, plus the
/// transfer-heat facts in sigil's `counterparty:heat:direction:stage` convention — the
/// material signals behind the sentiment. Same canonical-JSON discipline as
/// `narratives::build_narratives_input_components`.
///
/// Deliberately EXCLUDED: narrative bodies (model prose — the F1 rule), `topic_heat` and
/// `relevance` (derived ordering signals that tick without the storylines moving),
/// `source_count`/`source_age_days` (corroboration/freshness are prompt-only, sigil's
/// precedent — an age ticking over a day boundary must not re-run the GPU), and the heat
/// summary/confidence (derived commentary, the narratives precedent). The three narrative
/// keys are ALWAYS present (even `[]`) so the no-corpus marker has a stable pre-image;
/// `transfer_heat` is conditional, matching the sigil/narratives convention.
///
/// `packets` (7.6) is CONDITIONAL in exactly the way `transfer_heat` is: it is empty on the legacy
/// rail by construction, so the legacy pre-image — and therefore every legacy `input_hash` — is
/// byte-identical to what shipped before Phase 7. On the packet rail it carries the packet IDs,
/// which is the whole of what "her material moved" means: packets are append-only snapshots, so a
/// recompiled storyline is a NEW id and a story that has not moved keeps its own. Hashing the ids
/// rather than the rendered prose also keeps the register's phrasing out of the key — the phrase
/// is the Editor's copy of the article's words, and re-reading the same story because a synonym
/// changed is the churn this discipline exists to prevent.
pub fn build_vibe_input_components(
    narratives: &[Narrative],
    heat: &[HeatItem],
    packets: &[PacketBlock],
) -> String {
    fn push_sorted_lines(out: &mut String, mut lines: Vec<String>) {
        lines.sort();
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&go_json_string(line));
        }
    }

    let mut out = format!(
        "{{\"prompt_version\":{},\"narrative_impacts\":[",
        go_json_string(VIBE_PROMPT_VERSION)
    );
    push_sorted_lines(
        &mut out,
        narratives
            .iter()
            .map(|n| format!("{}:{}", n.title, n.impact))
            .collect(),
    );
    out.push_str("],\"narrative_titles\":[");
    push_sorted_lines(
        &mut out,
        narratives.iter().map(|n| n.title.clone()).collect(),
    );
    out.push_str("],\"narrative_trajectories\":[");
    push_sorted_lines(
        &mut out,
        narratives
            .iter()
            .map(|n| format!("{}:{}", n.title, n.trajectory))
            .collect(),
    );
    out.push(']');
    if !heat.is_empty() {
        out.push_str(",\"transfer_heat\":[");
        push_sorted_lines(
            &mut out,
            heat.iter()
                .map(|t| format!("{}:{}:{}:{}", t.counterparty, t.heat, t.direction, t.stage))
                .collect(),
        );
        out.push(']');
    }
    if !packets.is_empty() {
        out.push_str(",\"packets\":[");
        push_sorted_lines(
            &mut out,
            packets.iter().map(|p| p.packet_id.to_string()).collect(),
        );
        out.push(']');
    }
    out.push('}');
    out
}

/// The loaded-and-weighted vibe context: everything the prompt needs plus the debounce key.
/// Splitting the load from the model call lets the handler gate on `input_hash`
/// before paying for the GPU.
pub struct VibeContext {
    /// Ordered for the prompt (topic heat → impact → title).
    pub narratives: Vec<Narrative>,
    pub heat: Vec<HeatItem>,
    /// Deduped union of the narratives' source article ids (provenance).
    pub news_ids: Vec<i64>,
    /// The entity's live packets, rendered for HER (7.6) — empty on the legacy rail, always.
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
    /// No derived signal at all → the no-corpus marker path (no model call).
    ///
    /// **E3, the first-voice fix (7.6).** Until the packet rail, "empty" meant no NARRATIVES and
    /// no heat — which made the Influencer structurally incapable of speaking before The
    /// Journalist, because her only material was his output. On the packet rail she is woken by
    /// the packet's `charged` tag and the packet is material in its own right: the register and
    /// its phrase are HERS (§1c), and nobody else is shown them. A packet therefore counts, and
    /// she can file first.
    pub fn empty(&self) -> bool {
        self.narratives.is_empty() && self.heat.is_empty() && self.packets.is_empty()
    }
}

/// load_vibe_context runs the deterministic prefix: validate, load narratives + heat
/// concurrently, weight/order the narratives for the prompt, and compute the material-only
/// debounce key. NO model call.
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

    // Independent reads (news_summaries vs transfer_rumors vs packets, no data dependency) —
    // run them concurrently (plan A3).
    let ((mut narratives, news_ids), heat, packets) = tokio::try_join!(
        load_latest_narratives(&hx.pool, entity_type, entity_id, &sport),
        load_transfer_heat(&hx.pool, entity_type, entity_id, &sport),
        load_vibe_packets(&hx.pool, entity_type, entity_id, entity_name, &sport),
    )?;
    order_narratives(&mut narratives);

    // Hash BEFORE any model involvement: the pre-image is order-insensitive (sorted lines),
    // so the weighting above cannot perturb it.
    let input_components_json = build_vibe_input_components(&narratives, &heat, &packets);
    let input_hash = hash_components(&input_components_json);

    Ok(VibeContext {
        narratives,
        heat,
        news_ids,
        packets,
        input_components_json,
        input_hash,
    })
}

/// Packets read per entity per vibe run. The felt read is about the entity's MOMENT, so a
/// handful of live storylines is the whole of it; the rest are archive.
///
/// **4→2 at v18 (the D-T54 diet).** The exact census read vibe as the FATTEST seat — p50 3,315
/// tokens, max 7,808, 24 of 25 sampled prompts over 2,000 — and each packet renders under a
/// ~2,000-token budget, so the 4-packet allowance was most of the tail. That tail is what trips
/// the oMLX prefill guard ~1/min under sustained drain (D-T56): the guard prices CURRENT pool
/// pressure + the new prefill, so the fattest prompts park and retry. Two packets keep her
/// first-voice-capable on the two liveliest stories (D-T52 set momentum's to 1; hers stays 2
/// because packets are her PRIMARY material, not corroboration).
const MAX_VIBE_PACKETS: i64 = 2;

/// load_vibe_packets renders the entity's live packets for the Influencer (7.6).
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

/// enqueue_vibe_if_needed is the Phase-3 hand-off the narratives handler makes after it persists:
/// narratives now feeds vibe (mirroring vibe → momentum), replacing the scrub `vetted` trigger's
/// old parallel vibe fan-out (mig 174 removed it). It loads the material vibe context (this
/// entity's latest narratives + transfer heat) and enqueues the Vibe stage ONLY when that context
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
    if ctx.empty() {
        return Ok(false);
    }
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

/// parse_vibe_reply extracts the SCORE (1-100), the optional one-line HOOK (v13 — The
/// Influencer's card title), and the VIBE prose from the model's labeled reply. Parse-compat
/// with the two-line v6..v12 shape: HOOK is optional (`None` when absent), the score falls
/// back to the first integer anywhere (format drift), and the vibe joins trailing lines —
/// skipping a drifted post-VIBE HOOK line rather than swallowing it into the prose.
pub fn parse_vibe_reply(raw: &str) -> Result<(i32, Option<String>, String)> {
    let mut score: i32 = 0;
    let mut hook: Option<String> = None;
    let mut prompt = String::new();
    let lines: Vec<&str> = raw.trim().split('\n').collect();

    for (i, line) in lines.iter().enumerate() {
        // Strip Markdown decoration before matching: a larger model may emit `**SCORE: 62**`,
        // which does not start with `SCORE:` and would sink the whole reply. See
        // `util::strip_markdown_emphasis` for the 2026-07-26 production break.
        let t_owned = crate::util::strip_markdown_emphasis(line);
        let t = t_owned.as_str();
        let up = t.to_uppercase();
        if score == 0 && up.starts_with("SCORE:") {
            // "SCORE:" is ASCII (6 bytes) regardless of case, so t[6..] is a boundary.
            let rest = t[6..].trim();
            if let Ok(n) = rest.parse::<i64>() {
                score = n.clamp(1, 100) as i32;
            }
        } else if hook.is_none() && up.starts_with("HOOK:") {
            let h = t[5..].trim();
            if !h.is_empty() {
                hook = Some(h.to_string());
            }
        } else if prompt.is_empty() && up.starts_with("VIBE:") {
            prompt = t[5..].trim().to_string();
            for extra in &lines[i + 1..] {
                let e = extra.trim();
                if e.to_uppercase().starts_with("HOOK:") {
                    continue;
                }
                // THE STORY FORM (2026-08-25): a blank line is a paragraph break and part of
                // the card — the old join flattened every trailing line into one blurb, which
                // would silently undo the form at the last step. Runs of blanks collapse to
                // one break; a leading or trailing break never survives the trims.
                if e.is_empty() {
                    if !prompt.is_empty() && !prompt.ends_with("\n\n") {
                        prompt.push_str("\n\n");
                    }
                    continue;
                }
                if !prompt.is_empty() && !prompt.ends_with("\n\n") {
                    prompt.push(' ');
                }
                prompt.push_str(e);
            }
            prompt = prompt.trim_end().to_string();
        }
    }

    if score == 0 {
        // No SCORE: label parsed — fall back to the first integer anywhere.
        score = parse_sentiment(raw)?;
    }
    Ok((score, hook, prompt))
}

/// parse_sentiment_and_prompt is the two-line compat view (score + felt read) over
/// [`parse_vibe_reply`] — kept for the callers that predate the v13 HOOK line. Mirrors
/// `parseSentimentAndPrompt`.
pub fn parse_sentiment_and_prompt(raw: &str) -> Result<(i32, String)> {
    parse_vibe_reply(raw).map(|(score, _hook, prompt)| (score, prompt))
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

/// VibeReply is the validated answer — the SCORE (1-100), the optional HOOK (v13 — The
/// Influencer's card title), and the felt read. The vibe Extract output shape (the `T` in
/// `Parser<T>` / `Extracted<T>`).
#[derive(Clone, Debug)]
pub struct VibeReply {
    pub sentiment: i32,
    /// The Influencer's card title; `None` on a v12-shape reply without a HOOK line.
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
        let (sentiment, mut hook, vibe_prompt) = parse_vibe_reply(raw)
            .with_context(|| format!("parse sentiment (raw={:?})", truncate(raw, 120)))?;
        // The eval→guard migration (2026-08-19, DOCTRINE-directing.md): the HOOK contract and
        // the body's global invariants fail closed in production — same rules as the gate
        // (`crate::guards`); a violation re-rolls through the queue for a clean read.
        // v21 salvage first (the fail-rate session): a two-beat title deterministically
        // trimmed to its first beat ships NOW instead of burning a retry — the trim IS the
        // one-clause rule the prompt states, applied in code. Unsalvageable violations
        // still re-roll: the board needs a real hook, and the retry usually lands one.
        hook = crate::guards::settle_title("influencer", hook.as_deref());
        // Emphasis is STRIPPED from the body, not banned (2026-08-23) — the fourth and last
        // seat to take this treatment, after the Scout's clean_commentary and the Insider's is4.
        //
        // MEASURED: 89 vibe items dead-lettered on `body carries banned "**"`, the largest
        // failure bucket on the rail. parse_vibe_reply already strips emphasis per LINE to find
        // the SCORE/HOOK/VIBE labels, but the assembled body kept its asterisks and then hit a
        // hard ban — so a good felt read, and the SCORE that momentum depends on, died over
        // typography. Stripping is lossless and the stripped body is exactly the body intended.
        let vibe_prompt = crate::guards::clean_served_prose(&vibe_prompt);
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
            hook,
            vibe_prompt,
        }))
    }
}

// ---------------------------------------------------------------------------
// The core generate + the production handler.
// ---------------------------------------------------------------------------

/// generate_vibe runs the full vibe derivation for one entity at the given temperature and
/// returns the un-persisted result. This is the L1 composition — `route(VibeLogic) +
/// extract(VibeParser)` — over the same loaders + prompt: load the context, short-circuit
/// to the no-corpus marker when it is empty, else build the prompt and `extract`. The
/// production handler calls `load_vibe_context` itself (to debounce before the model call)
/// and then `generate_vibe_from_context`; this convenience wrapper is the undebounced
/// load-and-generate composition. Skips the v12 enrichment riders (continuity prior +
/// relational memory) — those are the production handler's loads.
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
    temperature: f64,
) -> Result<VibeOutput> {
    // No derived signal (no narratives AND no transfer heat) → no rating. Persist a
    // NULL-sentiment marker (handled by the caller); the read path returns "no data". No
    // model call is made, so the marker's model_version is the role's configured model.
    // The marker still carries the (empty-material) hash so quiet entities debounce.
    if ctx.empty() {
        return Ok(VibeOutput {
            sentiment: None,
            vibe_prompt: None,
            hook: None,
            input_news_ids: Vec::new(),
            input_components_json: ctx.input_components_json,
            input_hash: ctx.input_hash,
            model: hx.router.for_role(Role::VibeLogic).model().to_string(),
            prompt_version: VIBE_PROMPT_VERSION,
            built_prompt: None,
            request_body: None,
            eval_count: None,
            wall_ms: None,
        });
    }

    let prompt = build_sentiment_prompt(
        entity_type,
        entity_name,
        sport_raw,
        &ctx.narratives,
        &ctx.heat,
        &ctx.packets,
        previous,
        memory,
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

    // VibeParser only ever returns `Ok(Some)` on success (vibe's no-corpus marker is the
    // pre-model short-circuit above, not a parser fail-closed), so a `None` here would be a
    // contract violation — fail the item rather than fabricate a row.
    let reply = extracted
        .value
        .ok_or_else(|| anyhow!("vibe: parser returned no value"))?;

    let built_prompt = extracted.built_prompt;
    let request_body = extracted.request_body;
    let eval_count = extracted.eval_count;
    let wall_ms = extracted.wall_ms;

    Ok(VibeOutput {
        sentiment: Some(reply.sentiment),
        vibe_prompt: if reply.vibe_prompt.is_empty() {
            None
        } else {
            Some(reply.vibe_prompt)
        },
        hook: reply.hook,
        input_news_ids: ctx.news_ids,
        input_components_json: ctx.input_components_json,
        input_hash: ctx.input_hash,
        model: extracted.model,
        prompt_version: VIBE_PROMPT_VERSION,
        built_prompt: Some(built_prompt),
        request_body: Some(request_body),
        eval_count: Some(eval_count),
        wall_ms: Some(wall_ms),
    })
}

fn vibe_included_evidence(out: &VibeOutput) -> serde_json::Value {
    serde_json::json!({
        "input_components": serde_json::from_str::<serde_json::Value>(&out.input_components_json)
            .unwrap_or_else(|_| serde_json::json!({"raw_input_components": out.input_components_json})),
        "input_news_ids": &out.input_news_ids,
        "sentiment": out.sentiment,
        "vibe_prompt": &out.vibe_prompt,
        "hook": &out.hook,
    })
}

fn vibe_excluded_evidence(out: &VibeOutput) -> serde_json::Value {
    if out.built_prompt.is_none() {
        serde_json::json!([{
            "reason": "no_latest_narratives_or_transfer_heat",
        }])
    } else {
        serde_json::json!([])
    }
}

fn vibe_parser_outcome(out: &VibeOutput) -> &'static str {
    if out.built_prompt.is_none() {
        "no_call"
    } else {
        "parsed"
    }
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
    let prov = out.provenance();
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

/// VibeHandler drains the durable `vibe` stage: read the fresh narratives + heat, score
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

    // Two-host split (2026-08-23): the Influencer's model runs on the Mac
    // (`COGNITION_ROUTE_VIBE_LOGIC_BASE_URL`), so she budgets against `MAC_SLOTS`, not the
    // archbox card — see that constant for the measured starvation the wrong group caused.
    //
    // Capped at 1, not 2, and the cap is the Mac group's FAIRNESS ARITHMETIC: top-up claims in
    // VOICE_ORDER with no rotation, so with the Journalist at 2 and this seat at 2 the two of
    // them fill all four Mac slots on every pass while their queues run deep, and the Oracle —
    // last in order — claims zero. Measured within minutes of the split deploy (2026-08-23):
    // sigil stayed at zero WITH the right group, because 2+2 left it no room. 2(Journalist)
    // + 1(here) ≤ 3 guarantees the Oracle a slot whenever the group saturates; the vibe queue
    // is the shallowest of the three, so hers is the cheapest slot to give back.
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

        // F2: gate on the material-only input_hash BEFORE the model call. Vibe is
        // entity-scoped — no season in its key.
        let ctx = load_vibe_context(hx, &item.entity_type, entity_id, &name, &item.sport).await?;
        let key = EntityKey {
            entity_type: item.entity_type.clone(),
            entity_id,
            sport: sport.clone(),
            season: None,
        };
        // One round-trip for the debounce hash AND the continuity prior (the sigil plan-A1
        // non-torn read). F2 semantics preserved: no row / NULL hash → run.
        let (prev_sentiment, prev_prompt, latest_hash) =
            load_latest_vibe_row(&hx.pool, &key).await?;
        if latest_hash.as_deref() == Some(ctx.input_hash.as_str()) {
            debug!(
                entity_type = %item.entity_type,
                entity_id = item.entity_id,
                sport = %sport,
                "vibe: debounce-skip, material inputs unchanged"
            );
            // Still hand off: the momentum enqueue is hash-gated and cheap, so a previously
            // lost hand-off self-heals as a no-op — the same shape as sigil's skip-path
            // oracle enqueue.
            crate::junctions::analyst::enqueue_momentum_if_needed(hx, &item.entity_type, entity_id, &sport)
                .await?;
            return Ok(());
        }

        // Previous vibe as prompt-only continuity (v12): built only for a real prior read
        // (a scored row — a NULL-sentiment marker anchors nothing). Deliberately NOT folded
        // into input_hash — the read always moves, so hashing it would self-trigger every
        // re-run; the sigil Phase-5.2 discipline.
        let previous = prev_sentiment.map(|s| PrevVibe {
            sentiment: s as i32,
            vibe_prompt: prev_prompt.unwrap_or_default(),
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

        let out = generate_vibe_from_context(
            hx,
            &item.entity_type,
            &name,
            &item.sport,
            ctx,
            previous.as_ref(),
            memory.as_deref(),
            VIBE_TEMPERATURE,
        )
        .await?;
        let product_row_id = persist_to_vibe_scores(&hx.pool, item, &sport, &out).await?;
        insert_cognition_ledger_best_effort(
            &hx.pool,
            CognitionLedgerEntry {
                stage: "vibe".to_string(),
                lens: "vibe".to_string(),
                role: Role::VibeLogic.as_str().to_string(),
                entity_type: item.entity_type.clone(),
                entity_id,
                sport: sport.clone(),
                pair_entity_type: None,
                pair_entity_id: None,
                trigger_type: "periodic".to_string(),
                trigger_payload: serde_json::Value::Null,
                product_table: "vibe_scores".to_string(),
                product_row_ids: vec![product_row_id],
                model_version: out.model.clone(),
                prompt_version: out.prompt_version.to_string(),
                output_contract_version: VIBE_OUTPUT_CONTRACT_VERSION.to_string(),
                input_ids: out.input_news_ids.clone(),
                input_hash: Some(out.input_hash.clone()),
                request_body: out.request_body.clone(),
                built_prompt: out.built_prompt.clone(),
                included_evidence: vibe_included_evidence(&out),
                excluded_evidence: vibe_excluded_evidence(&out),
                context_budget: serde_json::json!({
                    "num_predict": VIBE_NUM_PREDICT,
                    "eval_count": out.eval_count,
                    "wall_ms": out.wall_ms,
                }),
                parser_outcome: vibe_parser_outcome(&out).to_string(),
            },
        )
        .await;

        // Vibe now feeds Momentum first; Momentum persists the generated trajectory card and then
        // enqueues Sigil if the Momentum context actually moved.
        if !crate::junctions::analyst::enqueue_momentum_if_needed(hx, &item.entity_type, entity_id, &sport)
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
