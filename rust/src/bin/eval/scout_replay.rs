//! DB-free replay of captured Scout assignments through production guards and
//! bounded correction. Records every returned response, including rejected ones.
use anyhow::{anyhow, Result};
use scoracle_cognition::plugins::scout::parser::RatingRequestParser;
use scoracle_cognition::plugins::scout::prompt::Parts;
use scoracle_cognition::runtime::{config::Config, route::Router};
use scoracle_cognition::studio::{
    model::{GenerateOptions, GenerateResult, Inference},
    Parser, Studio,
};
use serde_json::{json, Value};
use std::{path::Path, sync::Mutex};

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
        let started = std::time::Instant::now();
        let result = self.backend.generate(prompt, opts).await;
        let mut record = match &result {
            Ok((g, request)) => {
                json!({"request":request,"raw_response":g.response,"raw_response_body":g.raw_response_body,"latency_ms":g.total_duration.as_millis(),"request_bytes":prompt.len()+opts.system.as_ref().map_or(0,String::len),"prompt_eval_count":g.prompt_eval_count,"eval_count":g.eval_count,"completion_reason":g.completion_reason,"guard_error":self.parser.parse(&g.response).err().map(|e|format!("{e:#}"))})
            }
            Err(e) => {
                json!({"request":self.backend.request_body(prompt,opts),"provider_error":format!("{e:#}"),"raw_response_unavailable":true})
            }
        };
        record["wall_ms"] = json!(started.elapsed().as_millis());
        record["request_bytes"] = json!(prompt.len() + opts.system.as_ref().map_or(0, String::len));
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
    let backend = router.for_route(scoracle_cognition::plugins::scout::manifest::ROUTE);
    for line in std::fs::read_to_string(path)?
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let capture: Value = serde_json::from_str(line)?;
        anyhow::ensure!(
            capture["capture_version"] == 2,
            "capture must include current prepared parts; recapture this assignment"
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
        let parts: Parts = serde_json::from_value(a["parts"].clone())?;
        if !parts.has_measured_profile() {
            println!(
                "{}",
                json!({"capture":capture,"outcome":"no_stats","attempts":[]})
            );
            continue;
        }
        anyhow::ensure!(
            parts.render() == prompt,
            "captured request differs from current plugin assembly"
        );
        let directions = parts.comparison_directions();
        let bands = parts.measurement_bands();
        let opts = parts.generation_options(
            serde_json::from_value(a["options"]["num_ctx"].clone())?,
            serde_json::from_value(a["options"]["temperature"].clone())?,
        );
        let recording = Recording {
            backend: backend.as_ref(),
            parser: RatingRequestParser::new(prompt, &directions, &bands),
            attempts: Mutex::new(vec![]),
        };
        let result = Studio::new(&recording)
            .extract(
                prompt,
                &opts,
                &recording.parser,
                scoracle_cognition::plugins::scout::prompt::correction,
            )
            .await;
        let outcome = match result {
            Ok(r) => {
                json!({"status":if r.value.is_some() { "accepted" } else { "abstained" },"raw_response":r.raw_response,"built_prompt":r.built_prompt})
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
    use std::collections::BTreeMap;
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
        // The Scout's declared surface is the shared keyed prose map, and its
        // recorded non-participation on the paragraph rule means an over-long
        // body is refused for its TOTAL length.
        let backend = Backend(Mutex::new(vec![
            json!({"body":"x".repeat(1201)}).to_string(),
            json!({"body":"The measured rebounding is strong."}).to_string(),
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
            .extract(
                "Evidence",
                &opts,
                &recording.parser,
                scoracle_cognition::plugins::scout::prompt::correction,
            )
            .await
            .unwrap();
        let attempts = recording.attempts.lock().unwrap();
        assert_eq!(attempts.len(), 2);
        // The rejection is the shared body ceiling. The Scout records a
        // non-participation on the paragraph rule, so an over-long body is
        // refused for its total length, not for a paragraph.
        assert!(
            attempts[0]["guard_error"]
                .as_str()
                .unwrap()
                .contains("Prose totals 1201 characters"),
            "guard error was {:?}",
            attempts[0]["guard_error"]
        );
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
