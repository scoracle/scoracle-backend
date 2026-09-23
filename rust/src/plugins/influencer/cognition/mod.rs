//! Influencer creation from prepared story material and rendered continuity.
//! Retrieval, debounce, persistence and Momentum work belong to the application.

use crate::studio::model::GenerateOptions;
use crate::studio::{Generation, GenerationCall, Parser, Studio};
use crate::util::truncate;
use anyhow::{anyhow, bail, Context, Result};

mod brief;
mod inputs;
pub use brief::{CHARACTER, VIBE_PROMPT_VERSION, VIBE_SYSTEM_PROMPT};
pub use inputs::build_sentiment_prompt;

/// Production sentiment temperature.
pub const VIBE_TEMPERATURE: f64 = 0.7;

/// Standard output reservation; the application selects the smaller runtime envelope.
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
    /// Felt-read prose; `None` for the uncalled marker.
    pub vibe_prompt: Option<String>,
    /// Optional card title.
    pub hook: Option<String>,
    /// Canonical material-input JSON and hash pre-image.
    pub input_components_json: String,
}

pub type VibeOutput = Generation<VibeScore>;

/// One rendered packet as the Influencer reads it: the block, and the packet id that identifies
/// the snapshot it was rendered from.
#[derive(Clone, Debug)]
pub struct PacketBlock {
    pub packet_id: i64,
    pub text: String,
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
    if raw.trim_start().starts_with('{') {
        let card: crate::plugins::support::form::CardReply = serde_json::from_str(raw)?;
        let score = card
            .score
            .filter(|s| (1..=100).contains(s))
            .ok_or_else(|| anyhow!("card score must be 1–100"))?;
        return Ok((
            score,
            Some(card.headline),
            crate::plugins::support::form::normalize_body(&card.body),
        ));
    }
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
    let body = crate::plugins::support::form::normalize_body(body);
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
        crate::plugins::support::form::validate_hook(hook.as_deref())?;
        let hook = crate::plugins::support::guards::settle_title("influencer", hook.as_deref())
            .ok_or_else(|| anyhow!("vibe: missing or invalid HOOK line"))?;
        // Typography is scrubbed rather than treated as a content failure.
        let vibe_prompt = crate::plugins::support::guards::clean_served_prose(&vibe_prompt);
        // Keep prose before the first prompt-echo marker; all-echo output retries.
        crate::plugins::support::form::validate_body(&vibe_prompt)?;
        if vibe_prompt.is_empty() {
            tracing::warn!(guard = "prompt_echo", "vibe body rejected: all echo");
            bail!("vibe: body is prompt echo");
        }
        if let Some(p) = crate::plugins::support::guards::first_product_name(&vibe_prompt) {
            tracing::warn!(guard = "product_name", name = p, "vibe body rejected");
            bail!("vibe: body names product {p:?}");
        }
        if crate::plugins::support::guards::has_foreign_script(&vibe_prompt) {
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

/// The application supplies selected evidence and its material fingerprint separately from
/// continuity. A prior real score permits a closing read even with no live packets.
#[derive(Clone, Debug)]
pub struct Assignment {
    pub entity_type: String,
    pub entity_name: String,
    pub sport: String,
    pub packets: Vec<PacketBlock>,
    pub memory: Option<String>,
    pub previous_score: Option<i16>,
    pub input_components_json: String,
    pub input_hash: String,
    pub options: GenerateOptions,
}

pub async fn create(studio: &Studio<'_>, assignment: &Assignment) -> Result<VibeOutput> {
    if assignment.packets.is_empty() && assignment.previous_score.is_none() {
        return Ok(Generation::uncalled(
            VibeScore {
                sentiment: None,
                vibe_prompt: None,
                hook: None,
                input_components_json: assignment.input_components_json.clone(),
            },
            studio.model_name().to_string(),
            VIBE_PROMPT_VERSION,
            Vec::new(),
            Some(assignment.input_hash.clone()),
        ));
    }
    let prompt = build_sentiment_prompt(
        &assignment.entity_type,
        &assignment.entity_name,
        &assignment.sport,
        &assignment.packets,
        assignment.memory.as_deref(),
    );
    let extracted = studio
        .extract(
            &prompt,
            &assignment.options,
            &VibeParser,
            crate::plugins::support::form::publishing_correction,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let model = extracted.model.clone();
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
            input_components_json: assignment.input_components_json.clone(),
        },
        model,
        VIBE_PROMPT_VERSION,
        Vec::new(),
        Some(assignment.input_hash.clone()),
        call,
    ))
}

/// The caller chooses the reservation for its runtime envelope (production or eval).
pub fn generation_options(temperature: f64, num_ctx: i32, num_predict: i32) -> GenerateOptions {
    GenerateOptions {
        system: Some(VIBE_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict,
        num_ctx,
        json_mode: false,
        format_schema: Some(crate::plugins::support::form::card_schema(true)),
        format_schema_raw: None,
    }
}

#[cfg(test)]
mod tests;
