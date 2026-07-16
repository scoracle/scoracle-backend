//! Embed — the CPU-bound embedding primitive (Plan §1.4), candle-backed.
//!
//! This is the "genuine compute win" the conception names: a sentence-embedding model
//! (default BGE-small-en-v1.5, a BERT-arch model) loaded once and run **on the CPU**
//! (candle's `gemm` backend → AVX2/FMA on Archbox's i7-7700; NO CUDA feature). Running on
//! the CPU is the whole point — embeddings never contend with the generation GPU, and because the
//! embedding-backed Resolve hybrid (§1.3) uses them to *pre-filter* candidates before the
//! expensive local model call, every embedding computed here is GPU time SAVED.
//!
//! The model is addressed by config (`COGNITION_EMBED_*`), never hard-coded in stage code —
//! the same "models by role, never by name" discipline the router holds for generation.
//!
//! Validation is by QUALITY, not byte-parity (the L4 pivot): the embeddings feed a measured
//! gate against the model-vetted `news_article_entities.vetted` labels (the L4–L6 measurement bins that
//! settled the bands are removed post-Step-3; their findings live on in `ResolveConfig` defaults).

use crate::config::EmbedConfig;
use crate::harness::Vector;
use anyhow::{anyhow, Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig, DTYPE};
use hf_hub::api::sync::Api;
use hf_hub::{Repo, RepoType};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

/// Pooling is how a model collapses per-token hidden states into one sentence vector. It is a
/// property OF the model, not a free choice: BGE/E5 use `Cls` (the `[CLS]` token), MiniLM/nomic
/// use `Mean` (masked mean over tokens). Picking the wrong one silently degrades quality, so it
/// travels in [`EmbedConfig`] alongside the model id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pooling {
    Cls,
    Mean,
}

impl Pooling {
    /// from_str maps the `COGNITION_EMBED_POOLING` value to a strategy, defaulting to `Cls`
    /// (the default model BGE-small-en-v1.5's correct pooling).
    pub fn from_str_or_cls(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "mean" => Pooling::Mean,
            _ => Pooling::Cls,
        }
    }
}

/// Embedder owns the candle model + tokenizer for one embedding model. Built once (boot, or the
/// experiment harness) — `load` downloads + mmaps the weights, then `embed_batch` is pure CPU
/// compute. `Send + Sync` (candle tensors are `Arc`-backed, `Tokenizer` is thread-safe), so it
/// lives behind `&Harness` shared across stages.
pub struct Embedder {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
    pooling: Pooling,
    /// The embedding dimensionality (BGE-small = 384) — exposed for callers sizing buffers.
    pub dim: usize,
    /// Everything that determines the output vectors (repo@revision|pooling|max_tokens) in one
    /// string — the cache key for persisted embeddings (mig 151): any change invalidates them.
    pub identity: String,
}

impl Embedder {
    /// from_config loads the model named by [`EmbedConfig`] (the configured default is
    /// BGE-small-en-v1.5 / CLS / 256 tokens).
    pub fn from_config(cfg: &EmbedConfig) -> Result<Self> {
        Self::load(&cfg.model_repo, &cfg.revision, cfg.pooling, cfg.max_tokens)
    }

    /// load fetches `config.json` + `tokenizer.json` + `model.safetensors` from the HF hub
    /// (cached under `~/.cache/huggingface` after the first run), builds the BERT model on the
    /// **CPU**, and configures the tokenizer to pad-to-longest + truncate at `max_tokens`. The
    /// network fetch is the only blocking I/O; it runs once at construction.
    pub fn load(
        model_repo: &str,
        revision: &str,
        pooling: Pooling,
        max_tokens: usize,
    ) -> Result<Self> {
        let device = Device::Cpu;
        let api = Api::new().context("init hf-hub api")?;
        let repo = api.repo(Repo::with_revision(
            model_repo.to_string(),
            RepoType::Model,
            revision.to_string(),
        ));
        let config_path = repo.get("config.json").context("fetch config.json")?;
        let tokenizer_path = repo.get("tokenizer.json").context("fetch tokenizer.json")?;
        let weights_path = repo
            .get("model.safetensors")
            .context("fetch model.safetensors")?;

        let bert_config: BertConfig = serde_json::from_str(&std::fs::read_to_string(&config_path)?)
            .context("parse bert config.json")?;

        let mut tokenizer =
            Tokenizer::from_file(&tokenizer_path).map_err(|e| anyhow!("load tokenizer: {e}"))?;
        // Pad every row in a batch to the batch's longest sequence (so they stack into one
        // tensor) and truncate at max_tokens (a transfer/news title+blurb is short — 256 is
        // ample, and it bounds the CPU matmul cost).
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            ..Default::default()
        }));
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: max_tokens,
                ..Default::default()
            }))
            .map_err(|e| anyhow!("set truncation: {e}"))?;

        // SAFETY: from_mmaped_safetensors mmaps the weights file; standard candle pattern. The
        // file is read-only model data we just downloaded.
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[weights_path], DTYPE, &device)? };
        let model = BertModel::load(vb, &bert_config).context("load bert weights")?;

        Ok(Self {
            model,
            tokenizer,
            device,
            pooling,
            dim: bert_config.hidden_size,
            identity: format!("{model_repo}@{revision}|{pooling:?}|{max_tokens}"),
        })
    }

    /// embed_batch turns N texts into N L2-normalized vectors (so cosine == dot). Pure CPU
    /// compute — no async, no network. Call it from a blocking context (the experiment runs it
    /// directly; [`crate::harness::Harness::embed`] wraps it in `block_in_place`).
    ///
    /// Empty input → empty output (no model call). One tokenize → one forward → pool → normalize.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vector>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| anyhow!("tokenize batch: {e}"))?;

        let batch = encodings.len();
        // Uniform after BatchLongest padding, so the rows stack into one [batch, seq] tensor.
        let seq = encodings[0].get_ids().len();
        let mut ids = Vec::with_capacity(batch * seq);
        let mut mask = Vec::with_capacity(batch * seq);
        for enc in &encodings {
            ids.extend_from_slice(enc.get_ids());
            mask.extend_from_slice(enc.get_attention_mask());
        }

        let input_ids = Tensor::from_vec(ids, (batch, seq), &self.device)?;
        let attention_mask = Tensor::from_vec(mask, (batch, seq), &self.device)?;
        let token_type_ids = input_ids.zeros_like()?;

        // [batch, seq, hidden]
        let hidden = self
            .model
            .forward(&input_ids, &token_type_ids, Some(&attention_mask))?;

        let pooled = match self.pooling {
            // CLS: the first token's hidden state (BGE/E5).
            Pooling::Cls => hidden.narrow(1, 0, 1)?.squeeze(1)?,
            // Mean: masked mean over real tokens (MiniLM/nomic). Weight by the attention mask so
            // pad tokens don't dilute the average.
            Pooling::Mean => {
                let mask_f = attention_mask.to_dtype(DType::F32)?; // [batch, seq]
                let summed = hidden.broadcast_mul(&mask_f.unsqueeze(2)?)?.sum(1)?; // [batch, hidden]
                let counts = mask_f.sum(1)?.unsqueeze(1)?; // [batch, 1]
                summed.broadcast_div(&counts)?
            }
        };

        // L2-normalize so a downstream cosine is a plain dot product.
        let norm = pooled.sqr()?.sum_keepdim(1)?.sqrt()?; // [batch, 1]
        let normalized = pooled.broadcast_div(&norm)?;
        Ok(normalized.to_vec2::<f32>()?)
    }

    /// embed_one is the single-text convenience over [`Self::embed_batch`].
    pub fn embed_one(&self, text: &str) -> Result<Vector> {
        Ok(self
            .embed_batch(std::slice::from_ref(&text.to_string()))?
            .pop()
            .expect("embed_batch of one returns one"))
    }
}

/// cosine_similarity is the similarity of two embedding vectors in [-1, 1]. Defensive on
/// magnitude (a zero vector → 0), so callers needn't pre-normalize — though `embed_batch`
/// already returns unit vectors, making this a dot product in practice.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    (dot / (na.sqrt() * nb.sqrt())).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The pure-math helper is offline-testable; the real model run is #[ignore]'d (network +
    // ~130MB download + CPU inference), so `cargo test --lib` stays fast and offline (the gate).

    #[test]
    fn cosine_of_vectors() {
        let i = [1.0_f32, 0.0];
        let j = [0.0_f32, 1.0];
        let neg = [-1.0_f32, 0.0];
        let scaled = [3.0_f32, 0.0]; // non-unit → still cosine 1 with i
        let zero = [0.0_f32, 0.0];
        assert!((cosine_similarity(&i, &i) - 1.0).abs() < 1e-6);
        assert!(cosine_similarity(&i, &j).abs() < 1e-6);
        assert!((cosine_similarity(&i, &neg) + 1.0).abs() < 1e-6);
        assert!((cosine_similarity(&i, &scaled) - 1.0).abs() < 1e-6);
        assert_eq!(cosine_similarity(&i, &zero), 0.0); // zero-magnitude guard
    }

    #[test]
    fn pooling_parses_case_insensitively() {
        assert_eq!(Pooling::from_str_or_cls("MEAN"), Pooling::Mean);
        assert_eq!(Pooling::from_str_or_cls(" mean "), Pooling::Mean);
        assert_eq!(Pooling::from_str_or_cls("cls"), Pooling::Cls);
        assert_eq!(Pooling::from_str_or_cls("anything-else"), Pooling::Cls);
    }

    #[test]
    #[ignore = "downloads BGE-small (~130MB) + runs CPU inference; run manually with --ignored"]
    fn paraphrase_beats_unrelated() {
        let e = Embedder::load("BAAI/bge-small-en-v1.5", "main", Pooling::Cls, 256).unwrap();
        let v = e
            .embed_batch(&[
                "The midfielder is set to join Chelsea in a big-money transfer.".to_string(),
                "Chelsea agree a deal to sign the midfielder this summer.".to_string(),
                "The stock market fell sharply on Tuesday amid inflation fears.".to_string(),
            ])
            .unwrap();
        assert_eq!(v[0].len(), e.dim);
        let para = cosine_similarity(&v[0], &v[1]);
        let unrel = cosine_similarity(&v[0], &v[2]);
        assert!(
            para > unrel,
            "paraphrase {para} should exceed unrelated {unrel}"
        );
    }
}
