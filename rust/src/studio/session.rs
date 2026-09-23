//! Bounded creation and rewrite mechanics shared by all plugins.

use super::model::GenerateOptions;
use super::{Extracted, Parser};
use anyhow::{Context, Result};
#[cfg(test)]
use std::time::Duration;

pub(super) async fn extract_with_backend<T, P: Parser<T>>(
    backend: &dyn crate::studio::model::Inference,
    prompt: &str,
    opts: &GenerateOptions,
    parser: &P,
    correction: fn(&anyhow::Error) -> Option<String>,
) -> Result<Extracted<T>> {
    let mut built_prompt = prompt.to_string();
    for attempt in 0..3 {
        let result = async {
            let (gen, request_body) = backend
                .generate(&built_prompt, opts)
                .await
                .context("model generate")?;
            let value = parser.parse(&gen.response)?;
            Ok::<_, anyhow::Error>((gen, request_body, value))
        }
        .await;
        let (gen, request_body, value) = match result {
            Ok(result) => result,
            Err(error) if attempt < 2 => {
                let Some(instruction) = correction(&error) else {
                    return Err(error);
                };
                tracing::warn!(%error, "plugin-requested output correction");
                built_prompt.push_str("\nOutput correction: ");
                built_prompt.push_str(&instruction);
                continue;
            }
            Err(error) => return Err(error),
        };
        return Ok(Extracted {
            value,
            raw_response: gen.response,
            model: gen.model,
            built_prompt,
            request_body,
            eval_count: gen.eval_count,
            wall_ms: gen.total_duration.as_millis() as u64,
        });
    }
    unreachable!("bounded rewrite returns on its last attempt")
}

#[cfg(test)]
mod surface_tests {
    use super::super::model::GenerateResult;
    use super::*;
    use std::sync::Mutex;

    struct Backend(Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl crate::studio::model::Inference for Backend {
        async fn generate(
            &self,
            prompt: &str,
            opts: &GenerateOptions,
        ) -> Result<(GenerateResult, serde_json::Value)> {
            let reply = self.0.lock().unwrap().remove(0);
            if reply == "length" {
                return Err(crate::studio::model::IncompleteOutput("length".into()).into());
            }
            Ok((
                GenerateResult {
                    response: reply,
                    thinking: String::new(),
                    model: "test".into(),
                    total_duration: Duration::ZERO,
                    prompt_eval_count: 1,
                    eval_count: 1,
                    completion_reason: Some("stop".into()),
                    raw_response_body: String::new(),
                },
                self.request_body(prompt, opts),
            ))
        }
        fn model(&self) -> &str {
            "test"
        }
        fn request_body(&self, prompt: &str, opts: &GenerateOptions) -> serde_json::Value {
            serde_json::json!({"prompt":prompt,"num_ctx":opts.num_ctx,"num_predict":opts.num_predict})
        }
    }
    struct BodyParser;
    impl Parser<String> for BodyParser {
        fn parse(&self, raw: &str) -> Result<Option<String>> {
            if raw == "null" {
                return Ok(None);
            }
            crate::plugins::support::form::validate_body(raw)?;
            Ok(Some(raw.to_string()))
        }
    }

    #[tokio::test]
    async fn a_rewrite_can_end_in_an_explicit_pass() {
        let backend = Backend(Mutex::new(vec![
            "length".into(),
            "null".into(),
            "unused".into(),
        ]));
        let result = extract_with_backend(
            &backend,
            "Evidence",
            &GenerateOptions::default(),
            &BodyParser,
            crate::plugins::support::form::publishing_correction,
        )
        .await
        .unwrap();
        assert!(result.value.is_none());
        assert_eq!(result.raw_response, "null");
        assert_eq!(backend.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn rewrite_is_bounded_and_preserves_evidence_and_capacity() {
        let opts = GenerateOptions {
            num_ctx: 4096,
            num_predict: 700,
            ..Default::default()
        };
        for first in ["x".repeat(1201), "length".into()] {
            let backend = Backend(Mutex::new(vec![
                first,
                "The measured creation is strong.".into(),
            ]));
            let result = extract_with_backend(
                &backend,
                "Original evidence",
                &opts,
                &BodyParser,
                crate::plugins::support::form::publishing_correction,
            )
            .await
            .unwrap();
            assert!(backend.0.lock().unwrap().is_empty());
            assert!(result
                .built_prompt
                .starts_with("Original evidence\nOutput correction:"));
            assert!(result
                .built_prompt
                .contains("Target at most 500 body characters"));
            assert_eq!(result.request_body["num_ctx"], 4096);
            assert_eq!(result.request_body["num_predict"], 700);
        }
        let backend = Backend(Mutex::new(vec![
            "x".repeat(1201),
            "x".repeat(1201),
            "x".repeat(1201),
            "unused".into(),
        ]));
        assert!(extract_with_backend(
            &backend,
            "Evidence",
            &opts,
            &BodyParser,
            crate::plugins::support::form::publishing_correction,
        )
        .await
        .is_err());
        assert_eq!(backend.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_second_surface_failure_gets_one_final_bounded_rewrite() {
        let opts = GenerateOptions {
            num_ctx: 4096,
            num_predict: 700,
            ..Default::default()
        };
        let backend = Backend(Mutex::new(vec![
            "x".repeat(1201),
            "x".repeat(1201),
            "The measured creation is strong.".into(),
        ]));
        let result = extract_with_backend(
            &backend,
            "Original evidence",
            &opts,
            &BodyParser,
            crate::plugins::support::form::publishing_correction,
        )
        .await
        .unwrap();
        assert!(backend.0.lock().unwrap().is_empty());
        assert_eq!(result.built_prompt.matches("Output correction:").count(), 2);
        assert_eq!(result.raw_response, "The measured creation is strong.");
    }

    #[tokio::test]
    async fn structured_policy_retries_truncation_without_card_instructions() {
        let backend = Backend(Mutex::new(vec!["length".into(), "structured".into()]));
        struct Structured;
        impl Parser<String> for Structured {
            fn parse(&self, raw: &str) -> Result<Option<String>> {
                Ok(Some(raw.to_string()))
            }
        }
        let result = extract_with_backend(
            &backend,
            "Evidence",
            &GenerateOptions::default(),
            &Structured,
            crate::plugins::support::form::structured_correction,
        )
        .await
        .unwrap();
        assert!(result
            .built_prompt
            .contains("complete requested JSON object"));
        assert!(!result.built_prompt.contains("compact paragraph"));
    }
}
