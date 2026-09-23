//! The Insider vets transfer pairs and wraps a prepared wire.
//!
//! Studio owns prompts, parsers, deterministic verdict shaping, and model sessions. PostgreSQL
//! evidence, queue claims, partial-progress policy, publication, identity effects, and follow-up
//! delivery live in the application adapter.

use crate::studio::model::GenerateOptions;
use crate::studio::{Generation, GenerationCall, Parser, Studio};
use crate::util::truncate_bytes;
use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize};

mod brief;
mod inputs;
mod verification;

pub use crate::plugins::support::form::insider_score_format_schema;
pub use brief::{CHARACTER, INSIDER_SCORE_PROMPT_VERSION, INSIDER_SCORE_SYSTEM_PROMPT};
/// One active, vetted transfer rumor naming its counterparty.
#[derive(Clone, Debug)]
pub struct HeatItem {
    pub counterparty: String,
    pub heat: i32,
    pub stage: String,
    pub direction: String,
    pub summary: String,
    pub confidence: Option<f64>,
}

pub use inputs::build_insider_score_prompt;
pub use verification::{
    build_transfer_identity_adjudication_prompt, build_transfer_prompt,
    transfer_identity_adjudication_system_prompt, transfer_system_prompt,
    TRANSFER_IDENTITY_ADJUDICATION_PROMPT_VERSION, TRANSFER_PROMPT_VERSION,
    TRANSFER_PROMPT_VERSION_PERSON,
};

pub const TRANSFER_OUTPUT_CONTRACT_VERSION: &str = "transfer-verdict-v1";
pub const TRANSFER_TEMPERATURE: f64 = 0.3;
pub const TRANSFER_NUM_PREDICT: i32 = 900;
pub const TRANSFER_DEFAULT_MIN_ARTICLES: i32 = 2;
pub const INSIDER_SCORE_OUTPUT_CONTRACT_VERSION: &str = "insider-score-v1";
pub const INSIDER_SCORE_TEMPERATURE: f64 = 0.3;
pub const INSIDER_SCORE_NUM_PREDICT: i32 = 600;
pub const TRANSFER_IDENTITY_ADJUDICATION_SCHEMA_RAW: &str = r#"{
    "type": "object",
    "properties": {
        "decision": { "type": "string", "enum": ["apply", "reject"] },
        "event_type": { "type": "string", "enum": [
            "transfer", "trade", "loan", "signing", "extension", "rumor", "false_positive"
        ] },
        "old_team_id": { "type": ["integer", "null"] },
        "new_team_id": { "type": "integer" },
        "reason": { "type": "string", "maxLength": 500 },
        "evidence_spans": { "type": "array", "items": { "type": "string", "maxLength": 200 }, "maxItems": 6 }
    },
    "required": ["decision", "event_type", "old_team_id", "new_team_id", "reason", "evidence_spans"]
}"#;

const SUMMARY_TRUNCATE: usize = 240;
pub(crate) const DESC_TRUNCATE: usize = 160;
const VALID_STAGES: &[&str] = &[
    "speculation",
    "concrete_interest",
    "advanced_talks",
    "here_we_go",
];
const RETURN_SIGNALS: &[&str] = &[
    "return to",
    "returning to",
    "rejoin",
    "re-sign",
    "resign for",
    "back to",
    "back at",
    "comeback",
    "second spell",
    "reunite",
    "bring back",
    "brings back",
    "volver a",
    "regresar a",
    "vuelve a",
    "regresa a",
    "retour a",
    "retour à",
    "retour au",
    "revient a",
    "revient à",
    "rejoindre",
    "ruckkehr zu",
    "rückkehr zu",
    "kehrt zuruck",
    "kehrt zurück",
    "zuruck zu",
    "zurück zu",
    "ritorno a",
    "torna a",
    "tornare a",
    "regresso ao",
    "retorno ao",
    "volta ao",
    "voltar ao",
    "terugkeer naar",
    "keert terug",
    "terug naar",
];

#[derive(Clone, Debug)]
pub struct TransferCandidate {
    pub player_id: i32,
    pub player_name: String,
    pub nationality: String,
    pub current_club: String,
    pub position: String,
    pub subject_type: String,
    pub relationship_override: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewsItem {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub source: String,
}

#[derive(Clone, Debug, Default)]
pub struct TransferEvidence {
    pub total_articles: usize,
    pub distinct_sources: usize,
    pub best_source: String,
}

impl TransferEvidence {
    pub fn from_news(news: &[NewsItem], total_articles: usize, best: &str) -> Self {
        let distinct_sources = news
            .iter()
            .map(|item| item.source.to_lowercase())
            .filter(|source| !source.is_empty())
            .collect::<std::collections::HashSet<_>>()
            .len();
        Self {
            total_articles,
            distinct_sources,
            best_source: best.to_string(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct TransferVerdict {
    pub is_rumor: Option<bool>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub subject: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub direction: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub stage: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub summary: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub confidence: f64,
}

fn null_as_default<'de, D, T>(deserializer: D) -> std::result::Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

pub struct TransferParser;

impl Parser<TransferVerdict> for TransferParser {
    fn parse(&self, raw: &str) -> Result<Option<TransferVerdict>> {
        let (start, end) = match (raw.find('{'), raw.rfind('}')) {
            (Some(start), Some(end)) if end > start => (start, end),
            _ => return Ok(None),
        };
        Ok(serde_json::from_str(&raw[start..=end]).ok())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferIdentityAdjudication {
    pub decision: String,
    pub event_type: String,
    #[serde(default)]
    pub confidence: Option<f64>,
    pub old_team_id: Option<i32>,
    pub new_team_id: i32,
    pub reason: String,
    pub evidence_spans: Vec<String>,
}

pub struct TransferIdentityAdjudicationParser;

impl Parser<TransferIdentityAdjudication> for TransferIdentityAdjudicationParser {
    fn parse(&self, raw: &str) -> Result<Option<TransferIdentityAdjudication>> {
        let (start, end) = match (raw.find('{'), raw.rfind('}')) {
            (Some(start), Some(end)) if end > start => (start, end),
            _ => return Ok(None),
        };
        let value: serde_json::Value = match serde_json::from_str(&raw[start..=end]) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        let Some(object) = value.as_object() else {
            return Ok(None);
        };
        for key in [
            "decision",
            "event_type",
            "old_team_id",
            "new_team_id",
            "reason",
            "evidence_spans",
        ] {
            if !object.contains_key(key) {
                return Ok(None);
            }
        }
        let adjudication: TransferIdentityAdjudication = match serde_json::from_value(value) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        if !matches!(adjudication.decision.as_str(), "apply" | "reject")
            || !matches!(
                adjudication.event_type.as_str(),
                "transfer"
                    | "trade"
                    | "loan"
                    | "signing"
                    | "extension"
                    | "rumor"
                    | "false_positive"
            )
            || adjudication
                .confidence
                .is_some_and(|confidence| !(0.0..=1.0).contains(&confidence))
        {
            return Ok(None);
        }
        Ok(Some(adjudication))
    }
}

pub fn identity_options(sport: &str, num_ctx: i32) -> GenerateOptions {
    GenerateOptions {
        system: Some(transfer_identity_adjudication_system_prompt(sport)),
        temperature: Some(0.0),
        num_predict: 700,
        num_ctx,
        json_mode: false,
        format_schema: Some(
            serde_json::from_str(TRANSFER_IDENTITY_ADJUDICATION_SCHEMA_RAW)
                .expect("identity adjudication schema is valid JSON"),
        ),
        format_schema_raw: Some(TRANSFER_IDENTITY_ADJUDICATION_SCHEMA_RAW.to_string()),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Rumor,
    Cleared,
    Unknown,
    Skipped,
}

#[derive(Clone, Debug)]
pub struct TransferRow {
    pub is_rumor: Option<bool>,
    pub direction: Option<String>,
    pub stage: Option<String>,
    pub summary: Option<String>,
    pub attribution: Option<String>,
    pub confidence: Option<f64>,
    pub model: Option<String>,
    pub trigger_payload: String,
}

#[derive(Clone, Debug)]
pub struct TransferPairProduct {
    pub player_id: i32,
    pub subject_type: String,
    pub heat: Option<i16>,
    pub components: String,
    pub news_ids: Vec<i64>,
    pub prompted_news_ids: Vec<i64>,
    pub stale_news_ids: Vec<i64>,
    pub outcome: Outcome,
    pub row: Option<TransferRow>,
    pub identity_apply_news: Vec<NewsItem>,
}

pub type TransferPairOutput = Generation<TransferPairProduct>;

#[derive(Clone, Debug)]
pub struct PairAssignment {
    pub player_id: i32,
    pub subject_type: String,
    pub heat: i16,
    pub components: String,
    pub news_ids: Vec<i64>,
    pub prompted_news_ids: Vec<i64>,
    pub stale_news_ids: Vec<i64>,
    pub news: Vec<NewsItem>,
    pub relationship: String,
    pub attribution: String,
    pub prompt: String,
    pub options: GenerateOptions,
    pub model_configured: String,
    pub failed_request_body: serde_json::Value,
    pub input_hash: String,
}

pub fn skipped_pair(
    player_id: i32,
    subject_type: &str,
    components: String,
    news_ids: Vec<i64>,
    model: String,
) -> TransferPairOutput {
    Generation::uncalled(
        TransferPairProduct {
            player_id,
            subject_type: subject_type.to_string(),
            heat: None,
            components,
            news_ids,
            prompted_news_ids: Vec::new(),
            stale_news_ids: Vec::new(),
            outcome: Outcome::Skipped,
            row: None,
            identity_apply_news: Vec::new(),
        },
        model,
        prompt_version(subject_type),
        Vec::new(),
        None,
    )
}

pub async fn create_pair(studio: &Studio<'_>, assignment: PairAssignment) -> TransferPairOutput {
    let extracted = studio
        .extract(
            &assignment.prompt,
            &assignment.options,
            &TransferParser,
            crate::plugins::support::form::structured_correction,
        )
        .await;
    let (mut verdict, model, call) = match extracted {
        Ok(extracted) => {
            let call = GenerationCall::from(&extracted);
            (extracted.value, extracted.model.clone(), call)
        }
        Err(error) => {
            tracing::warn!(
                player = assignment.player_id,
                error = %error,
                "transfers: model generate failed; UNKNOWN (fail-closed)"
            );
            (
                None,
                assignment.model_configured.clone(),
                GenerationCall {
                    built_prompt: assignment.prompt.clone(),
                    request_body: assignment.failed_request_body.clone(),
                    eval_count: None,
                    wall_ms: None,
                },
            )
        }
    };
    if let Some(verdict) = verdict.as_mut() {
        if verdict.is_rumor == Some(true)
            && assignment.relationship == "former"
            && !has_return_signal(&assignment.news)
        {
            verdict.is_rumor = Some(false);
        }
    }
    let (row, outcome) = row_from_verdict(
        verdict.as_ref(),
        &assignment.relationship,
        (!assignment.attribution.is_empty()).then_some(assignment.attribution.as_str()),
        &assignment.model_configured,
    );
    let input_ids = assignment.news_ids.clone();
    Generation::called(
        TransferPairProduct {
            player_id: assignment.player_id,
            subject_type: assignment.subject_type.clone(),
            heat: Some(assignment.heat),
            components: assignment.components,
            news_ids: assignment.news_ids,
            prompted_news_ids: assignment.prompted_news_ids,
            stale_news_ids: assignment.stale_news_ids,
            outcome,
            row: Some(row),
            identity_apply_news: assignment.news,
        },
        model,
        prompt_version(&assignment.subject_type),
        input_ids,
        Some(assignment.input_hash),
        call,
    )
}

fn prompt_version(subject_type: &str) -> &'static str {
    if subject_type == "person" {
        TRANSFER_PROMPT_VERSION_PERSON
    } else {
        TRANSFER_PROMPT_VERSION
    }
}

pub fn direction_for(relationship: &str) -> &'static str {
    if relationship == "current" {
        "outgoing"
    } else {
        "incoming"
    }
}

fn has_return_signal(news: &[NewsItem]) -> bool {
    news.iter().any(|item| {
        let text = format!("{} {}", item.title, item.description).to_lowercase();
        RETURN_SIGNALS.iter().any(|signal| text.contains(signal))
    })
}

fn row_from_verdict(
    verdict: Option<&TransferVerdict>,
    relationship: &str,
    attribution: Option<&str>,
    model_configured: &str,
) -> (TransferRow, Outcome) {
    let attribution = attribution
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let trigger_payload = verdict
        .map(|value| value.subject.trim())
        .filter(|value| !value.is_empty())
        .map(|value| serde_json::json!({ "subject": value }).to_string())
        .unwrap_or_else(|| "{}".to_string());
    let unknown = || TransferRow {
        is_rumor: None,
        direction: Some(direction_for(relationship).to_string()),
        stage: None,
        summary: None,
        attribution: attribution.clone(),
        confidence: None,
        model: None,
        trigger_payload: trigger_payload.clone(),
    };
    match verdict.and_then(|value| value.is_rumor) {
        None => (unknown(), Outcome::Unknown),
        Some(false) => (
            TransferRow {
                is_rumor: Some(false),
                direction: None,
                stage: None,
                summary: None,
                attribution,
                confidence: None,
                model: Some(model_configured.to_string()),
                trigger_payload,
            },
            Outcome::Cleared,
        ),
        Some(true) => {
            let verdict = verdict.expect("committed verdict exists");
            let summary = crate::plugins::support::guards::clean_served_prose(&verdict.summary);
            (
                TransferRow {
                    is_rumor: Some(true),
                    direction: Some(direction_for(relationship).to_string()),
                    stage: Some(norm_stage(&verdict.stage)),
                    summary: (!summary.is_empty())
                        .then(|| truncate_bytes(&summary, SUMMARY_TRUNCATE)),
                    attribution,
                    confidence: Some(clamp_conf(verdict.confidence)),
                    model: Some(model_configured.to_string()),
                    trigger_payload,
                },
                Outcome::Rumor,
            )
        }
    }
}

fn norm_stage(value: &str) -> String {
    let normalized = value.trim().replace(' ', "_").to_lowercase();
    if VALID_STAGES.contains(&normalized.as_str()) {
        normalized
    } else {
        "speculation".to_string()
    }
}

fn clamp_conf(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

pub fn build_transfer_input_components(
    news_ids: &[i64],
    heat_components_json: &str,
    relationship: &str,
) -> String {
    let components: serde_json::Value =
        serde_json::from_str(heat_components_json).unwrap_or(serde_json::Value::Null);
    let distinct_sources = components
        .get("distinct_sources")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let mut ids = news_ids.to_vec();
    ids.sort_unstable();
    serde_json::json!({
        "distinct_sources": distinct_sources,
        "identity_adjudication_prompt_version": TRANSFER_IDENTITY_ADJUDICATION_PROMPT_VERSION,
        "news_ids": ids,
        "prompt_version": TRANSFER_PROMPT_VERSION,
        "relationship": relationship,
    })
    .to_string()
}

#[derive(Clone, Debug)]
pub struct InsiderScore {
    pub read: String,
    pub headline: Option<String>,
    pub score: i16,
}

pub type InsiderScoreReply = InsiderScore;

fn parse_insider_score_reply(raw: &str) -> Option<InsiderScoreReply> {
    let trimmed = raw.trim();
    let value: serde_json::Value = serde_json::from_str(trimmed).ok().or_else(|| {
        let start = trimmed.find('{')?;
        let end = trimmed.rfind('}')?;
        serde_json::from_str(&trimmed[start..=end]).ok()
    })?;
    let read = value.get("read")?.as_str()?.trim();
    let read = crate::plugins::support::guards::clean_served_prose(
        &crate::plugins::support::form::normalize_body(read),
    );
    if read.is_empty() {
        return None;
    }
    Some(InsiderScore {
        read,
        headline: crate::plugins::support::guards::settle_title(
            "insider",
            value.get("headline").and_then(|headline| headline.as_str()),
        ),
        score: parse_score(value.get("score")?)?,
    })
}

pub struct InsiderScoreParser;

impl Parser<InsiderScore> for InsiderScoreParser {
    fn parse(&self, raw: &str) -> Result<Option<InsiderScore>> {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
            crate::plugins::support::form::validate_hook(
                value.get("headline").and_then(|v| v.as_str()),
            )?;
        }
        let reply = parse_insider_score_reply(raw)
            .ok_or_else(|| anyhow::anyhow!("insider score: could not parse read+score"))?;
        crate::plugins::support::form::validate_body(&reply.read)?;
        Ok(Some(reply))
    }
}

fn parse_score(value: &serde_json::Value) -> Option<i16> {
    let value = if let Some(value) = value.as_i64() {
        value
    } else if let Some(value) = value.as_f64() {
        value.is_finite().then_some(value.round() as i64)?
    } else {
        value.as_str()?.split_whitespace().next()?.parse().ok()?
    };
    Some(value.clamp(1, 99) as i16)
}

pub fn score_options(num_ctx: i32) -> GenerateOptions {
    GenerateOptions {
        system: Some(INSIDER_SCORE_SYSTEM_PROMPT.to_string()),
        temperature: Some(INSIDER_SCORE_TEMPERATURE),
        num_predict: if crate::studio::model::small_voice_window(num_ctx) {
            crate::plugins::oracle::cognition::SMALL_WINDOW_NUM_PREDICT
        } else {
            INSIDER_SCORE_NUM_PREDICT
        },
        num_ctx,
        json_mode: false,
        format_schema: Some(insider_score_format_schema()),
        format_schema_raw: None,
    }
}

pub async fn create_score(
    studio: &Studio<'_>,
    prompt: &str,
    options: &GenerateOptions,
    input_hash: String,
) -> Result<Generation<InsiderScore>> {
    let extracted = studio
        .extract(
            prompt,
            options,
            &InsiderScoreParser,
            crate::plugins::support::form::publishing_correction,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let model = extracted.model.clone();
    let product = extracted
        .value
        .ok_or_else(|| anyhow::anyhow!("insider score parser abstained"))?;
    Ok(Generation::called(
        product,
        model,
        INSIDER_SCORE_PROMPT_VERSION,
        Vec::new(),
        Some(input_hash),
        call,
    ))
}

pub fn build_insider_score_input_components(heat: &[HeatItem]) -> String {
    let mut lines: Vec<String> = heat
        .iter()
        .map(|item| {
            format!(
                "{}:{}:{}:{}",
                item.counterparty, item.heat, item.direction, item.stage
            )
        })
        .collect();
    lines.sort();
    serde_json::json!({
        "board": lines,
        "prompt_version": INSIDER_SCORE_PROMPT_VERSION,
    })
    .to_string()
}

#[cfg(test)]
mod tests;
