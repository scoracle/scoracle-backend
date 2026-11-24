//! Bounded creation and rewrite mechanics shared by all plugins.

use super::model::GenerateOptions;
use super::{Extracted, Parser};
use crate::tools::form::SurfaceError;
use anyhow::Result;
#[cfg(test)]
use std::time::Duration;

/// Daily characters make one generation attempt; rejection remains a visible failed product.
pub fn no_rewrite(_: &anyhow::Error) -> Option<String> {
    None
}

pub fn publishing_correction(error: &anyhow::Error) -> Option<String> {
    if let Some(detail) = error.downcast_ref::<SurfaceError>() {
        return Some(format!("{detail} Return the complete requested JSON with nonblank prose in concise paragraphs, preserving the supplied evidence and schema."));
    }
    structured_correction(error)
}

pub fn structured_correction(error: &anyhow::Error) -> Option<String> {
    crate::harness::model::is_incomplete_output(error)
        .then(|| "the response was truncated. Return the complete requested JSON object from scratch, preserving the supplied evidence and schema.".to_string())
}

/// A provider may return a usable final answer along with a truncated completion.
pub(crate) async fn generate_character(
    backend: &dyn crate::harness::model::Inference,
    prompt: &str,
    opts: &GenerateOptions,
) -> Result<(super::model::GenerateResult, serde_json::Value)> {
    let started = std::time::Instant::now();
    match backend.generate(prompt, opts).await {
        Err(error)
            if super::rejections::character_claim()
                && super::model::is_incomplete_output(&error) =>
        {
            let Some(failure) = error.downcast_ref::<super::model::ResponseFailure>() else {
                return Err(error);
            };
            let envelope: serde_json::Value = serde_json::from_str(&failure.raw_response_body)?;
            let Some(answer) = envelope["content"]
                .as_str()
                .or_else(|| {
                    envelope
                        .pointer("/choices/0/message/content")
                        .and_then(|v| v.as_str())
                })
                .or_else(|| envelope["response"].as_str())
            else {
                return Err(error);
            };
            let answer = crate::tools::form::answer_text(answer);
            let request = backend.request_body(prompt, opts);
            super::rejections::record_decision(
                backend.model(),
                prompt,
                &request,
                Some(answer),
                &failure.raw_response_body,
                &error,
                1,
                "flagged",
            )
            .await?;
            Ok((
                super::model::GenerateResult {
                    response: answer.into(),
                    thinking: String::new(),
                    model: backend.model().into(),
                    total_duration: started.elapsed(),
                    prompt_eval_count: envelope["tokens_evaluated"].as_i64().unwrap_or(0) as i32,
                    eval_count: envelope["tokens_predicted"].as_i64().unwrap_or(0) as i32,
                    completion_reason: Some("incomplete".into()),
                    raw_response_body: failure.raw_response_body.clone(),
                },
                request,
            ))
        }
        result => result,
    }
}

pub(super) async fn extract_with_backend<T, P: Parser<T>>(
    backend: &dyn crate::harness::model::Inference,
    prompt: &str,
    opts: &GenerateOptions,
    parser: &P,
    correction: fn(&anyhow::Error) -> Option<String>,
) -> Result<Extracted<T>> {
    let mut built_prompt = prompt.to_string();
    for attempt in 0..3 {
        let result = async {
            let (gen, request_body) = match generate_character(backend, &built_prompt, opts).await {
                Ok(result) => result,
                Err(error) => {
                    if let Some(failure) = error.downcast_ref::<super::model::ResponseFailure>() {
                        super::rejections::record(
                            backend.model(),
                            &built_prompt,
                            &backend.request_body(&built_prompt, opts),
                            None,
                            &failure.raw_response_body,
                            &error,
                            attempt + 1,
                        )
                        .await?;
                    }
                    return Err(error.context("model generate"));
                }
            };
            let value = match parser.parse(&gen.response) {
                Ok(value) => value,
                Err(error) if super::rejections::character_claim() => {
                    super::rejections::record_decision(&gen.model,&built_prompt,&request_body,Some(&gen.response),
                        &gen.raw_response_body,&error,attempt+1,"flagged").await?;
                    parser.flagged_output(&gen.response)?
                }
                Err(error) => {
                    let candidate = super::rejections::observe_content()
                        .then(|| parser.parse_candidate(&gen.response));
                    let allowed = candidate.as_ref().is_some_and(Result::is_ok);
                    let error = match &candidate {
                        Some(Err(structural)) => error.context(format!("observation decoding failed: {structural:#}")),
                        _ => error,
                    };
                    super::rejections::record_decision(
                        &gen.model, &built_prompt, &request_body, Some(&gen.response),
                        &gen.raw_response_body, &error, attempt + 1,
                        if allowed { "observed" } else { "rejected" },
                    ).await?;
                    if let Some(Ok(value)) = candidate {
                        tracing::warn!(%error, "content validation observed; candidate allowed to publish");
                        value
                    } else {
                        return Err(error.context(super::model::RejectedReply {
                            raw_response_body: if gen.raw_response_body.is_empty() {
                                gen.response.clone()
                            } else {
                                gen.raw_response_body.clone()
                            },
                            request_body: request_body.clone(),
                            built_prompt: built_prompt.clone(),
                        }));
                    }
                }
            };
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
                match serde_json::from_str::<serde_json::Value>(&built_prompt) {
                    Ok(mut package) if package["task"].is_string() => {
                        package["task"] = serde_json::json!(format!(
                            "{}\nOutput correction: {instruction}",
                            package["task"].as_str().unwrap()
                        ));
                        built_prompt = package.to_string();
                    }
                    _ => {
                        built_prompt.push_str("\nOutput correction: ");
                        built_prompt.push_str(&instruction);
                    }
                }
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
pub(crate) mod surface_tests {
    use super::super::model::GenerateResult;
    use super::*;
    use std::sync::Mutex;

    struct Backend(Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl crate::harness::model::Inference for Backend {
        async fn generate(
            &self,
            prompt: &str,
            opts: &GenerateOptions,
        ) -> Result<(GenerateResult, serde_json::Value)> {
            let reply = self.0.lock().unwrap().remove(0);
            if reply == "partial provider reply" {
                return Err(super::super::model::ResponseFailure {
                    raw_response_body:
                        r#"{"content":"<think>private</think>First half","stop_type":"limit"}"#
                            .into(),
                    error: super::super::model::IncompleteOutput("length".into()).into(),
                }
                .into());
            }
            if reply == "length" {
                return Err(crate::harness::model::IncompleteOutput("length".into()).into());
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
    pub(crate) async fn truncated_analyst_blurb() -> Result<String> {
        let backend = Backend(Mutex::new(vec!["partial provider reply".into()]));
        let output = extract_with_backend(
            &backend,
            "Evidence",
            &GenerateOptions::default(),
            &crate::harness::skills::analyst::parser::MomentumParser,
            no_rewrite,
        )
        .await?;
        Ok(output.value.unwrap().blurb)
    }

    struct BodyParser;
    impl Parser<String> for BodyParser {
        fn parse(&self, raw: &str) -> Result<Option<String>> {
            if raw == "null" {
                return Ok(None);
            }
            crate::tools::form::validate_body(raw)?;
            Ok(Some(raw.to_string()))
        }
    }

    struct ReviewParser;
    impl Parser<String> for ReviewParser {
        fn parse(&self, _: &str) -> Result<Option<String>> {
            anyhow::bail!("unsupported named claim")
        }
        fn parse_candidate(&self, raw: &str) -> Result<Option<String>> {
            let text: String = serde_json::from_str(raw)?;
            Ok(Some(text))
        }
    }

    #[tokio::test]
    async fn observation_archives_content_failures_without_rewriting_or_accepting_bad_json() {
        for (observe, reply, allowed) in [
            (false, r#""Invented news""#, false),
            (true, r#""Invented news""#, true),
            (true, "malformed", false),
        ] {
            let path = std::env::temp_dir().join(format!(
                "observation-{}-{}-{}.jsonl",
                std::process::id(),
                observe,
                allowed
            ));
            let _ = std::fs::remove_file(&path);
            let backend = Backend(Mutex::new(vec![reply.into(), "unused".into()]));
            let result = super::super::rejections::TEST_PATH
                .scope(
                    path.clone(),
                    super::super::rejections::TEST_OBSERVE.scope(
                        observe,
                        extract_with_backend(
                            &backend,
                            "Exact evidence",
                            &GenerateOptions::default(),
                            &ReviewParser,
                            no_rewrite,
                        ),
                    ),
                )
                .await;
            assert_eq!(result.is_ok(), allowed);
            if allowed {
                assert_eq!(result.unwrap().value.as_deref(), Some("Invented news"));
            }
            assert_eq!(backend.0.lock().unwrap().len(), 1);
            let records = std::fs::read_to_string(&path).unwrap();
            assert_eq!(records.lines().count(), 1);
            let record: serde_json::Value = serde_json::from_str(records.trim()).unwrap();
            assert_eq!(record["response"], reply);
            assert_eq!(
                record["disposition"],
                if allowed { "observed" } else { "rejected" }
            );
            std::fs::remove_file(path).unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "requires migrated disposable coverage database"]
    async fn truncated_character_reply_is_returned_with_a_durable_reason() {
        let pool = sqlx::PgPool::connect(&std::env::var("TEST_DATABASE_URL").unwrap())
            .await
            .unwrap();
        let database: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(database.starts_with("classifier_test_coverage"));
        let token: String = sqlx::query_scalar("SELECT gen_random_uuid()::text")
            .fetch_one(&pool)
            .await
            .unwrap();
        let item = crate::harness::queue::work::Item {
            stage: crate::harness::queue::work::TaskKey::new("rating"),
            entity_type: "team".into(),
            entity_id: 7,
            sport: "ZZ_TEST".into(),
            input_version: Some("fixture".into()),
            claim_token: Some(token.clone()),
            attempts: 0,
        };
        let backend = Backend(Mutex::new(vec!["partial provider reply".into()]));
        let path = std::env::temp_dir().join(format!("review-{token}.jsonl"));
        let output = super::super::rejections::TEST_PATH
            .scope(
                path.clone(),
                super::super::rejections::with_claim(
                    &pool,
                    &item,
                    extract_with_backend(
                        &backend,
                        "Evidence",
                        &GenerateOptions::default(),
                        &BodyParser,
                        no_rewrite,
                    ),
                ),
            )
            .await
            .unwrap();
        assert_eq!(output.value.as_deref(), Some("First half"));
        super::super::rejections::deliver_flags(&pool, &path)
            .await
            .unwrap();
        super::super::rejections::deliver_flags(&pool, &path)
            .await
            .unwrap();
        let (answer, reason): (String, String) = sqlx::query_as(
            "DELETE FROM character_output_flags WHERE claim_token=$1::uuid RETURNING answer,reason",
        )
        .bind(token)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(answer, "First half");
        assert!(reason.contains("length"));
        std::fs::remove_dir(path.with_extension("watchdog-pending")).unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn every_correction_is_archived_with_claim_and_useful_error_feedback() {
        let path = std::env::temp_dir().join(format!(
            "session-rejections-{}.jsonl",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let backend = Backend(Mutex::new(vec![
            " ".into(),
            " ".into(),
            "Grounded prose.".into(),
        ]));
        let item = crate::harness::queue::work::Item {
            stage: crate::harness::queue::work::TaskKey::new("classifier"),
            entity_type: "team".into(),
            entity_id: 7,
            sport: "NBA".into(),
            input_version: Some("revision".into()),
            claim_token: Some("token".into()),
            attempts: 2,
        };
        let output = super::super::rejections::TEST_PATH
            .scope(
                path.clone(),
                super::super::rejections::with_claim(
                    &sqlx::postgres::PgPoolOptions::new()
                        .connect_lazy("postgres:///unused")
                        .unwrap(),
                    &item,
                    extract_with_backend(
                        &backend,
                        "Evidence",
                        &GenerateOptions::default(),
                        &BodyParser,
                        publishing_correction,
                    ),
                ),
            )
            .await
            .unwrap();
        let records: Vec<serde_json::Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(records.len(), 2);
        for (i, record) in records.iter().enumerate() {
            assert_eq!(record["generation_attempt"], i + 1);
            assert_eq!(record["response"], " ");
            assert_eq!(record["claim"]["job_attempt"], 3);
            assert_eq!(record["claim"]["claim_token"], "token");
            assert_eq!(record["request"]["prompt"], record["built_prompt"]);
        }
        let detail = BodyParser.parse(" ").unwrap_err().to_string();
        assert!(output.built_prompt.contains(&detail));
        assert!(!output
            .built_prompt
            .contains("Output correction: model reply rejected"));
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn archive_failure_stops_before_another_generation() {
        let backend = Backend(Mutex::new(vec![" ".into(), "unused".into()]));
        let error = super::super::rejections::TEST_PATH
            .scope(
                "/dev/null/rejections.jsonl".into(),
                extract_with_backend(
                    &backend,
                    "Evidence",
                    &GenerateOptions::default(),
                    &BodyParser,
                    publishing_correction,
                ),
            )
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("persist rejected model output"));
        assert_eq!(backend.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn character_rewrites_remain_one_json_document_with_unchanged_evidence() {
        let package = serde_json::json!({
            "meta":{"name":"Club"}, "task":"Read the fresh evidence.",
            "fresh":{"report":"Full reporting, including the qualification."}, "voice":"Restrained"
        });
        let backend = Backend(Mutex::new(vec![
            " ".into(),
            "length".into(),
            "A grounded reading.".into(),
        ]));
        let result = extract_with_backend(
            &backend,
            &package.to_string(),
            &GenerateOptions::default(),
            &BodyParser,
            publishing_correction,
        )
        .await
        .unwrap();
        let rewritten: serde_json::Value = serde_json::from_str(&result.built_prompt).unwrap();
        assert_eq!(rewritten.as_object().unwrap().len(), 4);
        for key in ["meta", "fresh", "voice"] {
            assert_eq!(rewritten[key], package[key]);
        }
        let task = rewritten["task"].as_str().unwrap();
        assert!(task.starts_with(package["task"].as_str().unwrap()));
        assert_eq!(task.matches("Output correction:").count(), 2);
        assert_eq!(result.request_body["prompt"], result.built_prompt);
        assert!(backend.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn repeated_rejection_stops_after_the_existing_three_attempt_limit() {
        let backend = Backend(Mutex::new(vec![
            " ".into(),
            " ".into(),
            " ".into(),
            "unused".into(),
        ]));
        let error = extract_with_backend(
            &backend,
            "Complete evidence",
            &GenerateOptions::default(),
            &BodyParser,
            publishing_correction,
        )
        .await
        .err()
        .unwrap();
        assert!(error.is::<SurfaceError>());
        assert_eq!(backend.0.lock().unwrap().len(), 1);
        let rejected = error
            .downcast_ref::<super::super::model::RejectedReply>()
            .unwrap();
        assert_eq!(
            rejected.built_prompt.matches("Output correction:").count(),
            2
        );
    }

    #[tokio::test]
    async fn a_rejected_complete_reply_retains_its_attempt_and_original_error_type() {
        let backend = Backend(Mutex::new(vec![" ".into(), "unused".into()]));
        let error = extract_with_backend(
            &backend,
            "Complete evidence",
            &GenerateOptions::default(),
            &BodyParser,
            |_| None,
        )
        .await
        .err()
        .unwrap();
        assert!(error.is::<SurfaceError>());
        let rejected = error
            .downcast_ref::<super::super::model::RejectedReply>()
            .unwrap();
        assert_eq!(rejected.raw_response_body, " ");
        assert_eq!(rejected.built_prompt, "Complete evidence");
        assert_eq!(rejected.request_body["prompt"], "Complete evidence");
        assert_eq!(
            backend.0.lock().unwrap().len(),
            1,
            "parser rejection must not add a model call"
        );
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
            crate::harness::session::publishing_correction,
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
        for first in [" ".into(), "length".into()] {
            let backend = Backend(Mutex::new(vec![
                first,
                "The measured creation is strong.".into(),
            ]));
            let result = extract_with_backend(
                &backend,
                "Original evidence",
                &opts,
                &BodyParser,
                crate::harness::session::publishing_correction,
            )
            .await
            .unwrap();
            assert!(backend.0.lock().unwrap().is_empty());
            assert!(result
                .built_prompt
                .starts_with("Original evidence\nOutput correction:"));
            assert!(result
                .built_prompt
                .contains("preserving the supplied evidence and schema"));
            assert_eq!(result.request_body["num_ctx"], 4096);
            assert_eq!(result.request_body["num_predict"], 700);
        }
        let backend = Backend(Mutex::new(vec![
            " ".into(),
            " ".into(),
            " ".into(),
            "unused".into(),
        ]));
        assert!(extract_with_backend(
            &backend,
            "Evidence",
            &opts,
            &BodyParser,
            crate::harness::session::publishing_correction,
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
            " ".into(),
            " ".into(),
            "The measured creation is strong.".into(),
        ]));
        let result = extract_with_backend(
            &backend,
            "Original evidence",
            &opts,
            &BodyParser,
            crate::harness::session::publishing_correction,
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
            crate::harness::session::structured_correction,
        )
        .await
        .unwrap();
        assert!(result
            .built_prompt
            .contains("complete requested JSON object"));
        assert!(!result.built_prompt.contains("compact paragraph"));
    }
}
