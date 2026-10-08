//! Native llama.cpp template/tokenization/completion protocol. No source clipping.
use crate::harness::model::{
    GenerateOptions, GenerateResult, IncompleteOutput, Inference, InputCoverage, PreparedRequest,
    ResponseFailure,
};
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub struct LlamaClient {
    base_url: String,
    model: String,
    http: reqwest::Client,
    think: Option<bool>,
}
impl LlamaClient {
    pub fn new(
        base_url: &str,
        model: &str,
        timeout: Duration,
        think: Option<bool>,
    ) -> Result<Self> {
        Ok(Self {
            base_url: base_url.trim_end_matches('/').into(),
            model: model.into(),
            http: reqwest::Client::builder()
                .timeout(if timeout.is_zero() {
                    Duration::from_secs(60)
                } else {
                    timeout
                })
                .build()?,
            think,
        })
    }
    async fn json(&self, path: &str, payload: Option<&Value>) -> Result<(Value, String)> {
        let url = format!("{}{path}", self.base_url);
        let request = match payload {
            Some(body) => self.http.post(url).json(body),
            None => self.http.get(url),
        };
        let response = request.send().await.context("llama.cpp request")?;
        let status = response.status();
        let raw = response.text().await.context("llama.cpp response")?;
        let decoded = (|| {
            ensure!(status.is_success(), "llama.cpp HTTP {status}");
            let parsed: Value = serde_json::from_str(&raw)?;
            ensure!(parsed.get("error").is_none(), "llama.cpp error response");
            Ok(parsed)
        })()
        .map_err(|error| ResponseFailure {
            raw_response_body: raw.clone(),
            error,
        })?;
        Ok((decoded, raw))
    }
    pub async fn ping(&self) -> Result<()> {
        self.json("/health", None).await.map(|_| ())
    }
    // Alias/path/template metadata identifies the serving process but is not an immutable
    // weights digest. revision() remains None, so mutable tags are never reused.
    async fn metadata(&self) -> Result<Value> {
        let (props, _) = self.json("/props", None).await?;
        let (models, _) = self.json("/v1/models", None).await?;
        let models = models["data"]
            .as_array()
            .context("llama.cpp model inventory")?;
        ensure!(
            models.len() == 1 && models[0]["id"] == self.model,
            "llama.cpp requires one loaded model with the configured alias"
        );
        let path = props["model_path"]
            .as_str()
            .filter(|path| !path.is_empty())
            .context("llama.cpp model path")?;
        let template = props["chat_template"]
            .as_str()
            .filter(|template| !template.is_empty())
            .context("llama.cpp chat template")?;
        let (slots, _) = self.json("/slots", None).await?;
        let slots = slots
            .as_array()
            .filter(|slots| !slots.is_empty())
            .context("llama.cpp slot inventory")?;
        let contexts = slots
            .iter()
            .map(|slot| {
                slot["n_ctx"]
                    .as_i64()
                    .filter(|n| *n > 0)
                    .context("llama.cpp slot context")
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(
            json!({"model": {"id":models[0]["id"],"aliases":models[0]["aliases"],"meta":models[0]["meta"],"architecture":models[0]["architecture"]},"model_path":path,"chat_template":template,"context":contexts.iter().min(),"build_info":props["build_info"]}),
        )
    }
    fn completion(&self, tokens: &[i32], opts: &GenerateOptions) -> Result<Value> {
        ensure!(
            opts.format_schema_raw.is_none(),
            "llama.cpp adapter does not support raw schema property ordering"
        );
        let mut payload = json!({"model":self.model,"prompt":tokens,"stream":false,"cache_prompt":false,
            "n_predict":opts.num_predict,"n_keep":-1});
        if let Some(temperature) = opts.temperature {
            payload["temperature"] = json!(temperature);
        }
        if let Some(schema) = &opts.format_schema {
            payload["json_schema"] = schema.clone();
        } else if opts.json_mode {
            payload["json_schema"] = json!({"type":"object"});
        }
        Ok(payload)
    }
}

#[async_trait]
impl Inference for LlamaClient {
    fn model(&self) -> &str {
        &self.model
    }
    fn request_body(&self, prompt: &str, opts: &GenerateOptions) -> Value {
        let mut messages = Vec::new();
        if let Some(system) = &opts.system {
            messages.push(json!({"role":"system","content":system}));
        }
        messages.push(json!({"role":"user","content":prompt}));
        json!({"model":self.model,"messages":messages,"add_generation_prompt":true,
            "chat_template_kwargs":{"enable_thinking":self.think.unwrap_or(false)}})
    }
    async fn prepare(&self, prompt: &str, opts: &GenerateOptions) -> Result<PreparedRequest> {
        let provider = self.metadata().await?;
        let description = self.request_body(prompt, opts);
        let (rendered, _) = self.json("/apply-template", Some(&description)).await?;
        let rendered = rendered["prompt"]
            .as_str()
            .context("rendered llama.cpp prompt")?;
        ensure!(
            rendered.contains(prompt)
                && opts
                    .system
                    .as_ref()
                    .is_none_or(|system| rendered.contains(system)),
            "llama.cpp template omitted or changed supplied input"
        );
        let (tokens, _) = self
            .json(
                "/tokenize",
                Some(&json!({"content":rendered,"add_special":true,"parse_special":true})),
            )
            .await?;
        let token_ids: Vec<i32> =
            serde_json::from_value(tokens["tokens"].clone()).context("llama.cpp token IDs")?;
        ensure!(
            self.metadata().await? == provider,
            "llama.cpp model or template changed during preparation"
        );
        let context: i32 = provider["context"]
            .as_i64()
            .context("llama.cpp context")?
            .try_into()?;
        Ok(PreparedRequest {
            request: self.completion(&token_ids, opts)?,
            coverage: Some(InputCoverage {
                rendered_prompt: rendered.into(),
                token_ids,
                context,
                provider,
            }),
        })
    }
    async fn generate(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, Value)> {
        let prepared = self.prepare(prompt, opts).await?;
        self.generate_prepared(prompt, opts, &prepared).await
    }
    async fn generate_prepared(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
        prepared: &PreparedRequest,
    ) -> Result<(GenerateResult, Value)> {
        let coverage = prepared
            .coverage
            .as_ref()
            .context("llama.cpp exact tokenizer evidence required")?;
        prepared.verify_input(prompt, opts)?;
        ensure!(
            self.completion(&coverage.token_ids, opts)? == prepared.request,
            "llama.cpp prepared request changed"
        );
        ensure!(
            self.metadata().await? == coverage.provider,
            "llama.cpp serving identity changed before inference"
        );
        let started = Instant::now();
        let (response, raw) = self.json("/completion", Some(&prepared.request)).await?;
        let result = async {
            ensure!(
                self.metadata().await? == coverage.provider,
                "llama.cpp serving identity changed during inference"
            );
            decode(
                &self.model,
                &response,
                &raw,
                coverage.token_ids.len(),
                started.elapsed(),
            )
        }
        .await
        .map_err(|error| ResponseFailure {
            raw_response_body: raw,
            error,
        })?;
        Ok((result, prepared.request.clone()))
    }
}

fn decode(
    model: &str,
    response: &Value,
    raw: &str,
    input_tokens: usize,
    elapsed: Duration,
) -> Result<GenerateResult> {
    if !(response["stop"] == true
        && response["stop_type"] == "eos"
        && response["truncated"] == false)
    {
        return Err(IncompleteOutput(format!(
            "llama.cpp {model}: output incomplete or context truncated ({})",
            response["stop_type"]
        ))
        .into());
    }
    ensure!(
        response["tokens_evaluated"].as_u64() == Some(input_tokens as u64),
        "llama.cpp input token count differs from exact preflight"
    );
    let output = response["tokens_predicted"]
        .as_i64()
        .filter(|count| *count >= 0)
        .context("llama.cpp output token count")?;
    Ok(GenerateResult {
        response: response["content"]
            .as_str()
            .context("llama.cpp content")?
            .into(),
        thinking: String::new(),
        model: model.into(),
        total_duration: elapsed,
        prompt_eval_count: input_tokens.try_into()?,
        eval_count: output.try_into()?,
        completion_reason: Some("eos".into()),
        raw_response_body: raw.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_admission_and_completion_guards() {
        let opts = GenerateOptions {
            system: Some("system".into()),
            num_ctx: 8,
            num_predict: 2,
            ..Default::default()
        };
        let mut prepared = PreparedRequest {
            request: json!({"n_predict":2}),
            coverage: Some(InputCoverage {
                rendered_prompt: "system source".into(),
                token_ids: vec![1; 6],
                context: 8,
                provider: Value::Null,
            }),
        };
        prepared.verify_input("source", &opts).unwrap();
        prepared.request["n_predict"] = json!(3);
        assert!(prepared.verify_input("source", &opts).is_err());
        prepared.request["n_predict"] = json!(1);
        assert!(prepared.verify_input("source", &opts).is_err());
        prepared.request["n_predict"] = json!(2);
        prepared.coverage.as_mut().unwrap().token_ids.push(1);
        assert!(prepared.verify_input("source", &opts).is_err());
        prepared.coverage.as_mut().unwrap().token_ids.pop();
        prepared.coverage.as_mut().unwrap().rendered_prompt = "source".into();
        assert!(prepared.verify_input("source", &opts).is_err());
        let response = json!({"stop":true,"stop_type":"eos","truncated":false,
            "tokens_evaluated":6,"tokens_predicted":2,"content":"{}"});
        let raw = response.to_string();
        assert_eq!(
            decode("model", &response, &raw, 6, Duration::ZERO)
                .unwrap()
                .raw_response_body,
            raw
        );
        assert!(decode("model", &response, &raw, 7, Duration::ZERO).is_err());
        for (field, value) in [
            ("stop", json!(false)),
            ("stop_type", json!("limit")),
            ("truncated", json!(true)),
            ("truncated", Value::Null),
        ] {
            let mut invalid = response.clone();
            invalid[field] = value;
            assert!(decode("model", &invalid, &invalid.to_string(), 6, Duration::ZERO).is_err());
        }
        assert_eq!(
            crate::harness::config::Backend::from_env_str("llama.cpp"),
            crate::harness::config::Backend::LlamaCpp
        );
    }
}
