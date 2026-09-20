//! DB-free replay of captured Scout assignments through production guards and
//! bounded correction. Records every returned response, including rejected ones.
use anyhow::{anyhow, Result};
use scoracle_cognition::runtime::{
    config::Config,
    route::{Role, Router},
};
use scoracle_cognition::studio::{
    model::{GenerateOptions, GenerateResult, Inference},
    scout::{RatingRequestParser, RelativeDirection},
    Parser, Studio,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, sync::Mutex};

struct Recording<'a> {
    backend: &'a dyn Inference,
    parser: RatingRequestParser<'a>,
    attempts: Mutex<Vec<Value>>,
}

#[async_trait::async_trait]
impl Inference for Recording<'_> {
    async fn generate(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, Value)> {
        let result = self.backend.generate(prompt, opts).await;
        let record = match &result {
            Ok((g, request)) => {
                json!({"request":request,"raw_response":g.response,"raw_response_body":g.raw_response_body,"prompt_eval_count":g.prompt_eval_count,"eval_count":g.eval_count,"completion_reason":g.completion_reason,"guard_error":self.parser.parse(&g.response).err().map(|e|format!("{e:#}"))})
            }
            Err(e) => {
                json!({"request":self.backend.request_body(prompt,opts),"provider_error":format!("{e:#}"),"raw_response_unavailable":true})
            }
        };
        self.attempts.lock().unwrap().push(record);
        result
    }
    fn model(&self) -> &str {
        self.backend.model()
    }
    fn request_body(&self, prompt: &str, opts: &GenerateOptions) -> Value {
        self.backend.request_body(prompt, opts)
    }
}

pub async fn run(cfg: &Config, path: &Path) -> Result<()> {
    let router = Router::from_config(&cfg.route, cfg.ollama_timeout, 1)?;
    let backend = router.for_role(Role::StatsLogic);
    for line in std::fs::read_to_string(path)?
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let capture: Value = serde_json::from_str(line)?;
        anyhow::ensure!(
            capture["capture_version"] == 1,
            "unsupported capture version"
        );
        let a = &capture["assignment"];
        if a["status"] == "no_stats" {
            println!(
                "{}",
                json!({"capture":capture,"outcome":"no_stats","attempts":[]})
            );
            continue;
        }
        anyhow::ensure!(a["status"] == "ready", "capture has no ready assignment");
        let prompt = a["built_prompt"]
            .as_str()
            .ok_or_else(|| anyhow!("missing prompt"))?;
        let directions =
            serde_json::from_value::<BTreeMap<String, String>>(a["comparison_directions"].clone())?
                .into_iter()
                .map(|(k, v)| {
                    Ok((
                        k,
                        match v.as_str() {
                            "Rose" => RelativeDirection::Rose,
                            "Fell" => RelativeDirection::Fell,
                            "Held" => RelativeDirection::Held,
                            _ => return Err(anyhow!("invalid direction {v}")),
                        },
                    ))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
        let bands = serde_json::from_value(a["measurement_bands"].clone())?;
        let o = &a["options"];
        let opts = GenerateOptions {
            system: serde_json::from_value(o["system"].clone())?,
            temperature: serde_json::from_value(o["temperature"].clone())?,
            num_ctx: serde_json::from_value(o["num_ctx"].clone())?,
            num_predict: serde_json::from_value(o["num_predict"].clone())?,
            json_mode: serde_json::from_value(o["json_mode"].clone())?,
            format_schema: serde_json::from_value(o["format_schema"].clone())?,
            format_schema_raw: serde_json::from_value(o["format_schema_raw"].clone())?,
        };
        let recording = Recording {
            backend: backend.as_ref(),
            parser: RatingRequestParser::new(prompt, &directions, &bands),
            attempts: Mutex::new(vec![]),
        };
        let result = Studio::new(&recording)
            .extract(prompt, &opts, &recording.parser)
            .await;
        let outcome = match result {
            Ok(r) => {
                json!({"status":"accepted","raw_response":r.raw_response,"built_prompt":r.built_prompt})
            }
            Err(e) => json!({"status":"rejected","error":format!("{e:#}")}),
        };
        println!(
            "{}",
            json!({"capture":capture,"model":backend.model(),"outcome":outcome,"attempts":*recording.attempts.lock().unwrap()})
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Backend(Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl Inference for Backend {
        async fn generate(
            &self,
            prompt: &str,
            opts: &GenerateOptions,
        ) -> Result<(GenerateResult, Value)> {
            Ok((
                GenerateResult {
                    response: self.0.lock().unwrap().remove(0),
                    thinking: String::new(),
                    model: "test".into(),
                    total_duration: std::time::Duration::ZERO,
                    prompt_eval_count: 31,
                    eval_count: 17,
                    completion_reason: Some("stop".into()),
                    raw_response_body: "retained HTTP body".into(),
                },
                self.request_body(prompt, opts),
            ))
        }
        fn model(&self) -> &str {
            "test"
        }
        fn request_body(&self, prompt: &str, opts: &GenerateOptions) -> Value {
            json!({"prompt":prompt,"num_ctx":opts.num_ctx})
        }
    }

    #[tokio::test]
    async fn replay_retains_rejected_response_and_bounded_correction() {
        let backend = Backend(Mutex::new(vec![
            json!({"body":"x".repeat(1201),"headline":"Measured profile"}).to_string(),
            json!({"body":"The measured rebounding is strong.","headline":"Measured profile"})
                .to_string(),
        ]));
        let directions = BTreeMap::new();
        let bands = BTreeMap::new();
        let recording = Recording {
            backend: &backend,
            parser: RatingRequestParser::new("Evidence", &directions, &bands),
            attempts: Mutex::new(vec![]),
        };
        let opts = GenerateOptions {
            num_ctx: 4096,
            num_predict: 700,
            ..Default::default()
        };
        Studio::new(&recording)
            .extract("Evidence", &opts, &recording.parser)
            .await
            .unwrap();
        let attempts = recording.attempts.lock().unwrap();
        assert_eq!(attempts.len(), 2);
        assert!(attempts[0]["guard_error"]
            .as_str()
            .unwrap()
            .contains("1200"));
        assert!(attempts[1]["guard_error"].is_null());
        assert_eq!(attempts[0]["raw_response_body"], "retained HTTP body");
        assert_eq!(attempts[0]["prompt_eval_count"], 31);
        assert_eq!(attempts[1]["request"]["num_ctx"], 4096);
        assert!(attempts[1]["request"]["prompt"]
            .as_str()
            .unwrap()
            .contains("Output correction:"));
    }
}
