//! Read-only tool pilot. No publication, preloaded article or silent prose fallback.
use super::{fresh, prompt, VibeParser, VOICE};
use crate::plugins::meta::EntityMeta;
use crate::studio::{
    model::{GenerateOptions, Inference},
    Parser,
};
use anyhow::{ensure, Result};
use serde_json::{json, Value};

pub async fn read(
    pool: &sqlx::PgPool,
    backend: &dyn Inference,
    subject: &EntityMeta,
    num_ctx: i32,
) -> Result<Value> {
    run(backend, subject, num_ctx, || fresh::read(pool, subject)).await
}

/// The function argument makes the actual database read observable in the protocol test.
async fn run<F, Fut>(
    backend: &dyn Inference,
    subject: &EntityMeta,
    num_ctx: i32,
    mut read_source: F,
) -> Result<Value>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<Value>>,
{
    let tools = [json!({"type":"function","function":{
        "name":"read_source",
        "description":"Read the assigned publisher source for this subject. Returns available source evidence or an explicit unavailable result.",
        "parameters":{"type":"object","properties":{},"required":[],"additionalProperties":false}
    }})];
    // Only identity is initially supplied as data. Tone and form remain instructions.
    let system = format!(
        "{}\nTone: {VOICE}\nOutput: {}",
        prompt::RESEARCH_TASK,
        serde_json::to_string(&crate::plugins::support::form::observation_form())?
    );
    let mut messages = vec![
        json!({"role":"system","content":system}),
        json!({"role":"user","content":serde_json::to_string(&subject.for_writing())?}),
    ];
    let options = GenerateOptions {
        temperature: Some(0.0),
        num_ctx,
        num_predict: super::VIBE_NUM_PREDICT,
        ..Default::default()
    };
    let mut turns = Vec::new();
    let mut evidence = None;
    // One evidence read, then one answer. More tools/turns require a demonstrated need.
    for _ in 0..2 {
        let offered = if evidence.is_some() {
            &[][..]
        } else {
            &tools[..]
        };
        let mut turn_options = options.clone();
        if evidence.is_some() {
            turn_options.format_schema = Some(crate::plugins::support::form::observation_schema());
        }
        let (reply, request, message) = backend.chat(&messages, offered, &turn_options).await?;
        let response: Value = serde_json::from_str(&reply.raw_response_body)?;
        turns.push(
            json!({"request":request,"raw_response_body":reply.raw_response_body,
            "response":response,"message":message}),
        );
        ensure!(
            message["role"] == "assistant",
            "expected assistant tool-chat turn"
        );
        let calls = message.get("tool_calls").and_then(Value::as_array);
        if let Some(calls) = calls.filter(|calls| !calls.is_empty()) {
            if evidence.is_some() || calls.len() != 1 {
                return Ok(
                    json!({"turns":turns,"accepted":false,"error":"one_source_read_budget"}),
                );
            }
            let call = &calls[0]["function"];
            if call["name"] != "read_source" || call["arguments"] != json!({}) {
                return Ok(
                    json!({"turns":turns,"accepted":false,"error":"undeclared_tool_or_arguments"}),
                );
            }
            // No substring/code-block interpretation: only native provider tool_calls dispatch.
            let result = read_source().await?;
            messages.push(message);
            messages.push(
                json!({"role":"tool","tool_name":"read_source","content":result.to_string()}),
            );
            turns.push(json!({"tool":"read_source","result":result}));
            evidence = Some(result);
        } else {
            let Some(evidence) = &evidence else {
                return Ok(json!({"turns":turns,"accepted":false,"error":"source_not_read"}));
            };
            let parsed = VibeParser.parse(&reply.response);
            return Ok(match parsed {
                Ok(value) if evidence["status"] == "available" || value.is_none() => {
                    json!({"turns":turns,"accepted":true,"body":value.and_then(|v|v.body),
                        "review":"Structural acceptance only; factual review required."})
                }
                Ok(_) => {
                    json!({"turns":turns,"accepted":false,"error":"answer_without_available_source"})
                }
                Err(error) => json!({"turns":turns,"accepted":false,"error":error.to_string()}),
            });
        }
    }
    Ok(json!({"turns":turns,"accepted":false,"error":"turn_budget"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studio::model::GenerateResult;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };
    struct Script(Mutex<Vec<Value>>);
    #[async_trait::async_trait]
    impl Inference for Script {
        async fn generate(&self, _: &str, _: &GenerateOptions) -> Result<(GenerateResult, Value)> {
            unreachable!()
        }
        fn model(&self) -> &str {
            "script"
        }
        fn request_body(&self, _: &str, _: &GenerateOptions) -> Value {
            unreachable!()
        }
        async fn chat(
            &self,
            messages: &[Value],
            tools: &[Value],
            opts: &GenerateOptions,
        ) -> Result<(GenerateResult, Value, Value)> {
            let response = self.0.lock().unwrap().remove(0);
            let message = response["message"].clone();
            Ok((
                GenerateResult {
                    response: response["message"]["content"].as_str().unwrap_or("").into(),
                    raw_response_body: response.to_string(),
                    model: "script".into(),
                    thinking: String::new(),
                    total_duration: Default::default(),
                    prompt_eval_count: 0,
                    eval_count: 0,
                    completion_reason: Some("stop".into()),
                },
                json!({"messages":messages,"tools":tools,"format":opts.format_schema}),
                message,
            ))
        }
    }
    #[tokio::test]
    async fn dispatch_is_scoped_and_evidence_precedes_answer() {
        let subject = EntityMeta {
            name: "Cedar".into(),
            entity_type: "team".into(),
            entity_id: 7,
            sport: "NBA".into(),
        };
        let call = |name: &str, args: Value| json!({"message":{"role":"assistant","tool_calls":[{"function":{"name":name,"arguments":args}}]}});
        let answer = |body: Value| json!({"message":{"role":"assistant","content":json!({"body":body}).to_string()}});
        for (script, result, expected_reads, error) in [
            (
                vec![
                    call("read_source", json!({})),
                    answer(json!("Training starts Tuesday.")),
                ],
                json!({"status":"available","source":{"publisher_excerpt":"Training starts Tuesday."}}),
                1,
                None,
            ),
            (
                vec![call("read_source", json!({})), answer(Value::Null)],
                json!({"status":"unavailable"}),
                1,
                None,
            ),
            (
                vec![
                    call("read_source", json!({})),
                    answer(json!("Invented report.")),
                ],
                json!({"status":"unavailable"}),
                1,
                Some("answer_without_available_source"),
            ),
            (
                vec![call("execute_sql", json!({}))],
                Value::Null,
                0,
                Some("undeclared_tool_or_arguments"),
            ),
            (
                vec![call("read_source", json!({"entity_id":8}))],
                Value::Null,
                0,
                Some("undeclared_tool_or_arguments"),
            ),
            (
                vec![answer(json!("No tool was called."))],
                Value::Null,
                0,
                Some("source_not_read"),
            ),
        ] {
            let reads = AtomicUsize::new(0);
            let capture = run(&Script(Mutex::new(script)), &subject, 4096, || {
                reads.fetch_add(1, Ordering::SeqCst);
                std::future::ready(Ok(result.clone()))
            })
            .await
            .unwrap();
            assert_eq!(reads.load(Ordering::SeqCst), expected_reads);
            assert_eq!(capture["accepted"], error.is_none());
            assert_eq!(capture["error"].as_str(), error);
            if result["status"] == "unavailable" && error.is_none() {
                assert!(capture["body"].is_null());
            }
            let initial = &capture["turns"][0]["request"]["messages"][1]["content"];
            assert!(!initial.as_str().unwrap().contains("publisher_excerpt"));
            if error.is_none() {
                assert!(capture["turns"][0]["request"]["format"].is_null());
                assert!(capture["turns"][2]["request"]["tools"]
                    .as_array()
                    .unwrap()
                    .is_empty());
                assert!(capture["turns"][2]["request"]["format"].is_object());
                let returned = &capture["turns"][2]["request"]["messages"][3];
                assert_eq!(returned["role"], "tool");
                assert_eq!(
                    serde_json::from_str::<Value>(returned["content"].as_str().unwrap()).unwrap(),
                    result
                );
            }
        }
    }
}
