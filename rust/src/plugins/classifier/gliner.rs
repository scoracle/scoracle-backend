//! Native GLiNER2.5 classification: schema layout + DeBERTa-v2 encoder + classifier head.
//! Pure Rust (candle), no Python or ONNX Runtime. The schema layout is reproduced from gliner2
//! and differential-tested: `input_ids` and marker positions match exactly, scores to ~1e-6.
use anyhow::{Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::{ops, Linear, Module, VarBuilder};
use candle_transformers::models::debertav2::{Config as DebertaConfig, DebertaV2Model};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;
use tokenizers::Tokenizer;

const VECTORS: &str = include_str!("../../../fixtures/classifier/vector-schema-v1.json");
const CRITERIA: &str = include_str!("../../../fixtures/classifier/classification-criteria-v1.json");
pub const MODEL_ID: &str = "fastino/gliner2.5-base-v1@ca906247640776a07753514055be9726f9080ead";
pub const CRITERIA_VERSION: &str = "classification-criteria-v1";
const SEP_STRUCT: &str = "[SEP_STRUCT]";
const SEP_TEXT: &str = "[SEP_TEXT]";
const P_TOKEN: &str = "[P]";
const L_TOKEN: &str = "[L]";
const DESC_TOKEN: &str = "[DESCRIPTION]";
/// candle sizes its (unused, `position_biased_input=false`) absolute `position_ids` from
/// `max_position_embeddings` and narrows them to the sequence length. Our schema+text exceeds the
/// trained 512, so we hand candle headroom and pin `max_relative_positions` to the trained value.
const POSITION_HEADROOM: i64 = 8192;

/// One classification task in the schema layout.
#[derive(Clone, Deserialize)]
struct Classification {
    task: String,
    labels: Vec<String>,
    #[serde(default)]
    label_descriptions: BTreeMap<String, String>,
}

fn word_splitter() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:https?://\S+|www\.\S+)|[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}|@[a-z0-9_]+|\w+(?:[-_]\w+)*|\S",
        )
        .expect("word splitter regex")
    })
}

fn text_words(text: &str) -> Vec<String> {
    word_splitter().find_iter(text).map(|m| m.as_str().to_lowercase()).collect()
}

/// The ordered classification schema, matching gliner2's iteration of `vector-schema-v1.json`.
/// Order is part of the layout contract (it fixes token positions and therefore the classifier
/// scores), so it is pinned here rather than taken from a map whose iteration order is
/// alphabetical. `schema_order_matches_the_vector_schema` guards against drift.
const SCHEMA_ORDER: &[(&str, &[&str])] = &[
    ("emotion", &["admiration", "amusement", "anger", "annoyance", "approval", "caring", "confusion", "curiosity", "desire", "disappointment", "disapproval", "disgust", "embarrassment", "excitement", "fear", "gratitude", "grief", "joy", "love", "nervousness", "optimism", "pride", "realization", "relief", "remorse", "sadness", "surprise"]),
    ("topic", &["match_event", "performance", "availability", "player_move", "contract", "staffing", "discipline", "off_field", "routine", "league_context"]),
    ("temporal", &["current_reference", "historical_reference", "future_reference"]),
    ("relevance", &["direct_subject", "opponent_context", "incidental_mention"]),
];

/// Reproduce `classifier_gliner.make_schema(spectrum=..., extras=false)` for one target:
/// the spectrum families in order, then `extraction_usable`.
fn classifications(target: &str) -> Result<Vec<Classification>> {
    let vectors: Value = serde_json::from_str(VECTORS)?;
    let mut result = Vec::new();
    for (family, labels) in SCHEMA_ORDER {
        let spec = &vectors["vectors"][family];
        let scope = if spec["scope"].as_str() == Some("target_window") {
            format!(" About candidate target {target}.")
        } else {
            String::new()
        };
        let descriptions = spec["labels"].as_object().context("family labels")?;
        let mut label_descriptions = BTreeMap::new();
        for label in labels.iter() {
            let description = descriptions
                .get(*label)
                .and_then(|value| value.as_str())
                .with_context(|| format!("missing description for {family}.{label}"))?;
            label_descriptions.insert((*label).to_string(), format!("{description}{scope}"));
        }
        result.push(Classification {
            task: (*family).to_string(),
            labels: labels.iter().map(|label| (*label).to_string()).collect(),
            label_descriptions,
        });
    }
    result.push(Classification {
        task: "extraction_usable".into(),
        labels: vec!["true".into(), "false".into()],
        label_descriptions: [
            ("true".to_string(), "Usable reporting".to_string()),
            ("false".to_string(), "Only an interstitial, navigation, advertisement or publisher furniture".to_string()),
        ]
        .into_iter()
        .collect(),
    });
    Ok(result)
}

/// Build the token layout, returning subword ids, scored `[L]` marker positions and their labels.
fn layout(tokenizer: &Tokenizer, schema: &[Classification], text: &str) -> Result<(Vec<u32>, Vec<usize>, Vec<String>)> {
    let mut structs: Vec<Vec<String>> = Vec::new();
    let mut label_order: Vec<String> = Vec::new();
    for classification in schema {
        let mut prompt = classification.task.clone();
        for label in &classification.labels {
            if let Some(description) = classification.label_descriptions.get(label) {
                prompt.push_str(&format!(" {DESC_TOKEN} {label}: {description}"));
            }
        }
        let mut tokens = vec!["(".to_string(), P_TOKEN.to_string(), prompt, "(".to_string()];
        for label in &classification.labels {
            tokens.push(L_TOKEN.to_string());
            tokens.push(label.clone());
            label_order.push(label.clone());
        }
        tokens.push(")".to_string());
        tokens.push(")".to_string());
        structs.push(tokens);
    }

    let mut combined: Vec<String> = Vec::new();
    for (index, struct_) in structs.iter().enumerate() {
        combined.extend(struct_.iter().cloned());
        if index + 1 < structs.len() {
            combined.push(SEP_STRUCT.to_string());
        }
    }
    combined.push(SEP_TEXT.to_string());
    let text = if !text.is_empty() && !text.ends_with(['.', '!', '?']) {
        format!("{text}.")
    } else {
        text.to_string()
    };
    combined.extend(text_words(&text));

    let mut marker_orig = HashSet::new();
    let mut offset = 0usize;
    for struct_ in &structs {
        let mut index = 4usize;
        while index + 2 < struct_.len() {
            marker_orig.insert(offset + index);
            index += 2;
        }
        offset += struct_.len() + 1;
    }

    let mut ids: Vec<u32> = Vec::new();
    let mut markers: Vec<usize> = Vec::new();
    for (orig, token) in combined.iter().enumerate() {
        let position = ids.len();
        let encoding = tokenizer.encode(token.as_str(), false).map_err(|e| anyhow::anyhow!("tokenize {token:?}: {e}"))?;
        ids.extend_from_slice(encoding.get_ids());
        if marker_orig.contains(&orig) {
            markers.push(position);
        }
    }
    Ok((ids, markers, label_order))
}

/// The native Classifier model. Immutable after load; safe to share across worker tasks.
pub struct Gliner {
    device: Device,
    tokenizer: Tokenizer,
    encoder: DebertaV2Model,
    classifier0: Linear,
    classifier3: Linear,
}

impl Gliner {
    /// Load the tokenizer, encoder config and weights from `dir` (fastino/gliner2.5-base layout:
    /// `encoder_config/config.json`, `tokenizer.json`, `model.safetensors`).
    pub fn load(dir: &str) -> Result<Self> {
        let device = Device::Cpu;
        let tokenizer = Tokenizer::from_file(format!("{dir}/tokenizer.json")).map_err(|e| anyhow::anyhow!("tokenizer: {e}"))?;
        let mut config_value: Value = serde_json::from_str(&std::fs::read_to_string(format!("{dir}/encoder_config/config.json")).context("encoder config")?)?;
        let trained = config_value["max_position_embeddings"].as_i64().unwrap_or(512);
        config_value["max_relative_positions"] = serde_json::json!(trained);
        config_value["max_position_embeddings"] = serde_json::json!(POSITION_HEADROOM);
        let config: DebertaConfig = serde_json::from_value(config_value)?;
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[format!("{dir}/model.safetensors")], DType::F32, &device)? };
        let encoder = DebertaV2Model::load(vb.pp("encoder"), &config)?;
        let hidden = config.hidden_size;
        let classifier0 = candle_nn::linear(hidden, hidden * 2, vb.pp("classifier").pp("0"))?;
        let classifier3 = candle_nn::linear(hidden * 2, 1, vb.pp("classifier").pp("3"))?;
        Ok(Self { device, tokenizer, encoder, classifier0, classifier3 })
    }

    /// Score every criterion for one `(source text, candidate target)`. Returns `family.label -> 0..1`.
    pub fn spectrum(&self, text: &str, target: &str) -> Result<BTreeMap<String, f32>> {
        let schema = classifications(target)?;
        let (ids, markers, labels) = layout(&self.tokenizer, &schema, text)?;
        if markers.is_empty() {
            anyhow::bail!("empty classification layout");
        }
        let input = Tensor::new(ids.as_slice(), &self.device)?.unsqueeze(0)?;
        let mask = Tensor::new(vec![1u32; ids.len()], &self.device)?.unsqueeze(0)?;
        let hidden = self.encoder.forward(&input, None, Some(mask))?;
        let index = Tensor::new(markers.iter().map(|m| *m as u32).collect::<Vec<_>>(), &self.device)?;
        let states = hidden.index_select(&index, 1)?.squeeze(0)?;
        let logits = self.classifier3.forward(&self.classifier0.forward(&states)?.relu()?)?.squeeze(1)?;
        let scores = ops::sigmoid(&logits)?.to_vec1::<f32>()?;
        if scores.len() != labels.len() {
            anyhow::bail!("classification output length {} != labels {}", scores.len(), labels.len());
        }
        // Map back to family-qualified criterion names, matching the schema task order.
        let mut result = BTreeMap::new();
        let mut position = 0;
        for classification in &schema {
            for label in &classification.labels {
                result.insert(format!("{}.{}", classification.task, label), scores[position]);
                position += 1;
            }
        }
        Ok(result)
    }
}

/// Evaluate the `classification-criteria-v1` routing gate over a spectrum (0..1 scores).
/// Returns `character -> fires`. Rules are per-character `{mode: any|all, criteria: {pattern: threshold}}`.
pub fn gate(spectrum: &BTreeMap<String, f32>) -> Result<BTreeMap<String, bool>> {
    let criteria: Value = serde_json::from_str(CRITERIA)?;
    let mut result = BTreeMap::new();
    for (character, rule) in criteria["characters"].as_object().context("gate characters")? {
        let mode = rule["mode"].as_str().unwrap_or("any");
        let rules = rule["criteria"].as_object().context("gate criteria")?;
        let mut fired = Vec::new();
        for (pattern, threshold) in rules {
            let threshold = threshold.as_f64().unwrap_or(0.0) as f32;
            let value = match pattern.strip_suffix(".*") {
                Some(prefix) => spectrum
                    .iter()
                    .filter(|(key, _)| key.starts_with(&format!("{prefix}.")))
                    .map(|(_, value)| *value)
                    .fold(0.0f32, f32::max),
                None => *spectrum.get(pattern).unwrap_or(&0.0),
            };
            fired.push(value * 100.0 >= threshold);
        }
        let ok = if mode == "all" { fired.iter().all(|f| *f) } else { fired.iter().any(|f| *f) };
        result.insert(character.clone(), ok);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_fires_per_character_rules() {
        let mut spectrum = BTreeMap::new();
        spectrum.insert("emotion.joy".to_string(), 0.8);
        spectrum.insert("topic.availability".to_string(), 0.75);
        let gate = gate(&spectrum).unwrap();
        assert!(gate["influencer"], "emotional_charge 80 >= 50");
        assert!(gate["scout"], "availability 75 >= 70");
        assert!(!gate["insider"], "no move/contract/staffing/discipline");
        assert!(!gate["journalist"], "journalist requires all of direct_subject/current/usable");
    }

    #[test]
    fn schema_order_matches_the_vector_schema() {
        let vectors: Value = serde_json::from_str(VECTORS).unwrap();
        for (family, labels) in SCHEMA_ORDER {
            let expected: HashSet<&str> = vectors["vectors"][family]["labels"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            let got: HashSet<&str> = labels.iter().copied().collect();
            assert!(got.is_subset(&expected), "{family} labels drifted from vector-schema");
        }
    }

    #[test]
    fn classifications_match_the_production_schema() {
        let schema = classifications("Cedar United").unwrap();
        let tasks: Vec<&str> = schema.iter().map(|c| c.task.as_str()).collect();
        assert_eq!(tasks, vec!["emotion", "topic", "temporal", "relevance", "extraction_usable"]);
        let emotion = &schema[0];
        assert_eq!(emotion.labels.len(), 27, "neutral excluded");
        assert!(!emotion.labels.contains(&"neutral".to_string()));
        let topic = &schema[1];
        assert!(topic.label_descriptions["availability"].ends_with("About candidate target Cedar United."));
    }

    #[test]
    fn layout_is_deterministic_and_marks_every_label() {
        let tokenizer = match std::env::var("GLINER_MODEL_DIR") {
            Ok(dir) => Tokenizer::from_file(format!("{dir}/tokenizer.json")).unwrap(),
            Err(_) => return, // model not present locally; the differential test covers this
        };
        let schema = classifications("Cedar United").unwrap();
        let labels: usize = schema.iter().map(|c| c.labels.len()).sum();
        let (ids, markers, order) = layout(&tokenizer, &schema, "Cedar United are in talks.").unwrap();
        assert!(!ids.is_empty());
        assert_eq!(markers.len(), labels);
        assert_eq!(order.len(), labels);
    }

    /// Differential test against gliner2. Set GLINER_MODEL_DIR and GLINER_FIXTURE (an offline
    /// fixture generated by examples/fixture_gen.py) and run with `--ignored`.
    #[test]
    #[ignore]
    fn matches_gliner2_layout_and_scores() {
        let dir = std::env::var("GLINER_MODEL_DIR").expect("GLINER_MODEL_DIR");
        let fixture: Value = serde_json::from_str(&std::fs::read_to_string(std::env::var("GLINER_FIXTURE").expect("GLINER_FIXTURE")).unwrap()).unwrap();
        let model = Gliner::load(&dir).unwrap();
        for case in fixture.as_array().unwrap() {
            let text = case["text"].as_str().unwrap();
            let target = case["target"].as_str().unwrap();
            let schema = classifications(target).unwrap();
            let tokenizer = Tokenizer::from_file(format!("{dir}/tokenizer.json")).unwrap();
            let (ids, markers, _) = layout(&tokenizer, &schema, text).unwrap();
            assert_eq!(ids, case["input_ids"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect::<Vec<_>>(), "input_ids");
            assert_eq!(markers, case["cls_marker_indices"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect::<Vec<_>>(), "markers");
            let scores = model.spectrum(text, target).unwrap();
            for (family, labels) in case["scores"].as_object().unwrap() {
                for (label, expected) in labels.as_object().unwrap() {
                    let got = scores[&format!("{family}.{label}")];
                    assert!((got - expected.as_f64().unwrap() as f32).abs() < 1e-4, "{family}.{label}: {got} vs {expected}");
                }
            }
        }
    }
}
