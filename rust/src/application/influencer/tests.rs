//! Service-free acceptance of the production coordinator and its actual Studio session.
use super::*;
use crate::studio::influencer::build_sentiment_prompt;
use crate::studio::model::{GenerateOptions, GenerateResult, IncompleteOutput, Inference};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

const VALID: &str = r#"{"score":62,"headline":"Test Team finds a quieter room","body":"The early excitement has cooled. The latest reporting carries little reaction."}"#;

#[derive(Default)]
struct Adapters {
    replies: Mutex<VecDeque<String>>,
    events: Mutex<Vec<&'static str>>,
    requests: Mutex<Vec<(String, GenerateOptions)>>,
    outputs: Mutex<Vec<VibeOutput>>,
    publication_fails: bool,
    handoff_fails: bool,
}

impl Adapters {
    fn with_replies(replies: &[&str]) -> Self {
        Self {
            replies: Mutex::new(replies.iter().map(|r| r.to_string()).collect()),
            ..Self::default()
        }
    }
}

#[async_trait]
impl Inference for Adapters {
    async fn generate(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)> {
        self.events.lock().unwrap().push("model");
        self.requests
            .lock()
            .unwrap()
            .push((prompt.into(), opts.clone()));
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected model call");
        match reply.as_str() {
            "transport" => bail!("transport unavailable"),
            "length" => return Err(IncompleteOutput("length".into()).into()),
            _ => (),
        }
        Ok((
            GenerateResult {
                response: reply,
                thinking: "private reasoning must never become a card".into(),
                model: "responding-model".into(),
                total_duration: Duration::from_millis(123),
                prompt_eval_count: 100,
                eval_count: 42,
                completion_reason: Some("stop".into()),
                raw_response_body: "{}".into(),
            },
            serde_json::json!({"sent_prompt": prompt, "wire": "actual"}),
        ))
    }
    fn model(&self) -> &str {
        "configured-model"
    }
    fn request_body(&self, _: &str, _: &GenerateOptions) -> serde_json::Value {
        panic!("use the actual sent request")
    }
}

#[async_trait]
impl Publisher<VibeScore> for Adapters {
    type Receipt = usize;
    async fn publish(&self, output: &VibeOutput) -> Result<usize> {
        self.events.lock().unwrap().push("publish");
        if self.publication_fails {
            bail!("publication unavailable");
        }
        let mut outputs = self.outputs.lock().unwrap();
        outputs.push(output.clone());
        Ok(outputs.len())
    }
}

#[async_trait]
impl MomentumHandoff for Adapters {
    async fn offer(&self) -> Result<()> {
        self.events.lock().unwrap().push("momentum");
        if self.handoff_fails {
            bail!("momentum unavailable");
        }
        Ok(())
    }
}

fn context(live: bool, previous: Option<i16>) -> VibeContext {
    let mut memories = memories::test_package();
    memories.mission = Mission::Influencer;
    memories.previous_score = previous;
    let packets = if live {
        vec![PacketBlock {
            packet_id: 9,
            text: "STORY: A quiet spell\nMOOD: mixed — the reporter sees little reaction".into(),
        }]
    } else {
        vec![]
    };
    let input_components_json = memories
        .with_input_components(&build_vibe_input_components(&packets))
        .unwrap();
    let input_hash = hash_components(&input_components_json);
    VibeContext {
        memories,
        packets,
        input_components_json,
        input_hash,
    }
}

fn request() -> Request<'static> {
    Request {
        entity_type: "team",
        entity_name: "Test Team",
        sport: "nba",
        temperature: VIBE_TEMPERATURE,
        voice_num_ctx: 4096,
    }
}

async fn drain(
    adapters: &Adapters,
    ctx: &VibeContext,
    latest: (Option<i16>, Option<String>),
) -> Result<Outcome<usize>> {
    run_prepared(
        &Studio::new(adapters),
        &request(),
        ctx,
        &latest,
        adapters,
        adapters,
    )
    .await
}

#[tokio::test]
async fn never_scored_empty_material_publishes_marker_once_with_provenance() {
    let ctx = context(false, None);
    let adapters = Adapters::default();
    assert_eq!(
        drain(&adapters, &ctx, (None, None)).await.unwrap(),
        Outcome::Published(1)
    );
    {
        let outputs = adapters.outputs.lock().unwrap();
        let marker = &outputs[0];
        assert!(!marker.was_called());
        assert_eq!(marker.sentiment, None);
        assert_eq!(marker.vibe_prompt, None);
        assert_eq!(marker.hook, None);
        assert_eq!(marker.input_components_json, ctx.input_components_json);
        assert_eq!(
            marker.provenance.input_hash.as_deref(),
            Some(ctx.input_hash.as_str())
        );
        assert_eq!(marker.provenance.model_version, "configured-model");
        assert_eq!(marker.provenance.prompt_version, "v32");
        assert!(marker.provenance.input_ids.is_empty());
    }
    assert_eq!(
        drain(&adapters, &ctx, (None, Some(ctx.input_hash.clone())))
            .await
            .unwrap(),
        Outcome::Debounced
    );
    assert_eq!(
        *adapters.events.lock().unwrap(),
        ["publish", "momentum", "momentum"]
    );
}

#[tokio::test]
async fn live_read_then_one_closing_read_then_empty_debounce() {
    let adapters = Adapters::with_replies(&[VALID, VALID]);
    let live = context(true, None);
    drain(&adapters, &live, (None, None)).await.unwrap();
    let closing = context(false, Some(62));
    assert_ne!(live.input_hash, closing.input_hash);
    drain(&adapters, &closing, (Some(62), Some(live.input_hash)))
        .await
        .unwrap();
    assert_eq!(
        drain(
            &adapters,
            &closing,
            (Some(62), Some(closing.input_hash.clone()))
        )
        .await
        .unwrap(),
        Outcome::Debounced
    );
    assert_eq!(
        *adapters.events.lock().unwrap(),
        ["model", "publish", "momentum", "model", "publish", "momentum", "momentum"]
    );
    let requests = adapters.requests.lock().unwrap();
    assert!(requests[0].0.contains("STORY: A quiet spell"));
    assert!(!requests[1].0.contains("The stories running"));
    assert!(requests[1].0.starts_with("Entity: Team Test Team (nba)\n"));
    let outputs = adapters.outputs.lock().unwrap();
    assert!(outputs
        .iter()
        .all(|out| out.was_called() && out.sentiment == Some(62)));
    let call = outputs[1].call.as_ref().unwrap();
    assert_eq!(call.built_prompt, requests[1].0);
    assert_eq!(call.request_body["sent_prompt"], call.built_prompt);
    assert_eq!(call.request_body["wire"], "actual");
    assert_eq!(call.eval_count, Some(42));
    assert_eq!(call.wall_ms, Some(123));
    assert_eq!(outputs[1].provenance.model_version, "responding-model");
    assert!(!outputs[1]
        .vibe_prompt
        .as_ref()
        .unwrap()
        .contains("private reasoning"));
}

#[tokio::test]
async fn latest_null_cannot_bury_prior_real_memory_even_with_the_same_hash() {
    for live in [false, true] {
        let ctx = context(live, Some(70));
        let adapters = Adapters::with_replies(&[VALID]);
        assert_eq!(
            drain(&adapters, &ctx, (None, Some(ctx.input_hash.clone())))
                .await
                .unwrap(),
            Outcome::Published(1)
        );
        assert_eq!(
            *adapters.events.lock().unwrap(),
            ["model", "publish", "momentum"]
        );
    }
    // Conversely, latest-row sentiment alone does not authorize a closing read.
    let ctx = context(false, None);
    let adapters = Adapters::default();
    drain(&adapters, &ctx, (Some(70), Some("old-material".into())))
        .await
        .unwrap();
    assert!(!adapters.outputs.lock().unwrap()[0].was_called());
}

#[tokio::test]
async fn debounce_and_marker_precede_memory_rendering() {
    let mut ctx = context(true, Some(70));
    // The loader normally validates this. Invalid rendering here detects accidentally eager work.
    ctx.memories.version = "invalid".into();
    assert!(ctx.memories.render_for_model().is_err());
    let adapters = Adapters::default();
    assert_eq!(
        drain(&adapters, &ctx, (Some(70), Some(ctx.input_hash.clone())))
            .await
            .unwrap(),
        Outcome::Debounced
    );
    assert_eq!(*adapters.events.lock().unwrap(), ["momentum"]);
    ctx.packets.clear();
    ctx.memories.previous_score = None;
    drain(&adapters, &ctx, (None, None)).await.unwrap();
    assert_eq!(
        *adapters.events.lock().unwrap(),
        ["momentum", "publish", "momentum"]
    );
    ctx.memories.previous_score = Some(70);
    assert!(drain(&adapters, &ctx, (None, None)).await.is_err());
    assert_eq!(
        *adapters.events.lock().unwrap(),
        ["momentum", "publish", "momentum"]
    );
}

#[tokio::test]
async fn failures_stop_at_the_failed_boundary_and_reach_the_caller() {
    for raw in [
        "transport",
        "malformed",
        r#"{"score":0,"headline":"Test Team","body":"Quiet."}"#,
    ] {
        let adapters = Adapters::with_replies(&[raw]);
        assert!(drain(&adapters, &context(true, None), (None, None))
            .await
            .is_err());
        assert_eq!(*adapters.events.lock().unwrap(), ["model"]);
    }
    for live in [false, true] {
        let adapters = Adapters {
            publication_fails: true,
            ..Adapters::with_replies(&[VALID])
        };
        assert!(drain(&adapters, &context(live, None), (None, None))
            .await
            .unwrap_err()
            .to_string()
            .contains("publication"));
        assert!(!adapters.events.lock().unwrap().contains(&"momentum"));
        assert!(adapters.outputs.lock().unwrap().is_empty());
    }
    for skipped in [false, true] {
        let adapters = Adapters {
            handoff_fails: true,
            ..Adapters::with_replies(&[VALID])
        };
        let ctx = context(true, None);
        let latest = if skipped {
            (Some(62), Some(ctx.input_hash.clone()))
        } else {
            (None, None)
        };
        assert!(drain(&adapters, &ctx, latest)
            .await
            .unwrap_err()
            .to_string()
            .contains("momentum"));
        assert_eq!(
            adapters.outputs.lock().unwrap().len(),
            usize::from(!skipped)
        );
    }
}

#[tokio::test]
async fn influencer_rewrites_are_bounded_and_publish_only_the_final_valid_request() {
    let long = serde_json::json!({"score":62,"headline":"Test Team waits","body":"x".repeat(1201)})
        .to_string();
    for failure in [long.as_str(), "length"] {
        for failures in [1, 2, 3] {
            let mut replies = vec![failure; failures];
            replies.push(VALID);
            let adapters = Adapters::with_replies(&replies);
            let result = drain(&adapters, &context(true, None), (None, None)).await;
            let requests = adapters.requests.lock().unwrap();
            assert_eq!(requests.len(), (failures + 1).min(3));
            for (prompt, opts) in requests.iter() {
                assert!(prompt.starts_with(&requests[0].0));
                assert_eq!(opts.num_ctx, 4096);
                assert_eq!(opts.num_predict, 700);
                assert_eq!(opts.temperature, Some(0.7));
            }
            if failures < 3 {
                assert!(result.is_ok());
                let outputs = adapters.outputs.lock().unwrap();
                assert_eq!(outputs.len(), 1);
                let call = outputs[0].call.as_ref().unwrap();
                assert_eq!(
                    call.built_prompt.matches("Output correction:").count(),
                    failures
                );
                assert_eq!(call.request_body["sent_prompt"], call.built_prompt);
            } else {
                assert!(result.is_err());
                assert!(adapters.outputs.lock().unwrap().is_empty());
                assert!(!adapters.events.lock().unwrap().contains(&"momentum"));
            }
        }
    }
}

#[test]
fn production_and_eval_keep_their_existing_options_and_capacity() {
    use crate::evaluation::tasks::{LensTask, VibeTask};
    for (window, reservation) in [(2048, 700), (4096, 700), (4097, 800), (16384, 800)] {
        for temperature in [0.0, 0.7] {
            let opts = production_options(temperature, window);
            assert_eq!(opts.num_ctx, window);
            assert_eq!(opts.num_predict, reservation);
            assert_eq!(opts.temperature, Some(temperature));
            assert_eq!(
                opts.system.as_deref(),
                Some(influencer::VIBE_SYSTEM_PROMPT.as_str())
            );
            assert!(!opts.json_mode);
            assert!(opts.format_schema_raw.is_none());
            assert_eq!(
                opts.format_schema,
                Some(crate::studio::form::card_schema(true))
            );
        }
    }
    let eval = VibeTask.gen_options(0.0);
    assert_eq!(eval.num_ctx, 0);
    assert_eq!(eval.num_predict, 800);
    assert_eq!(eval.temperature, Some(0.0));
    assert_eq!(
        eval.format_schema,
        production_options(0.0, 4096).format_schema
    );
    assert_eq!(VibeTask.role(), Role::VibeLogic);
    assert_eq!(VibeHandler.stage(), Stage::Vibe);
    assert_eq!(VibeHandler.max_in_flight(), 1);
    assert_eq!(
        VibeHandler.slot_group(),
        Some(crate::runtime::stage::MAC_SLOTS)
    );
}

#[test]
fn continuity_does_not_invalidate_but_selected_evidence_does() {
    let first = context(true, None);
    let prior = context(true, Some(85));
    assert_eq!(first.input_hash, prior.input_hash);
    let mut changed = prior.memories.clone();
    changed.groups[0].records[0].data["name"] = serde_json::json!("Corrected identity");
    let components = changed
        .with_input_components(&build_vibe_input_components(&prior.packets))
        .unwrap();
    assert_ne!(hash_components(&components), prior.input_hash);
    assert_eq!(
        vibe_work_input_version(&prior.input_hash),
        format!("vibe:{}", prior.input_hash)
    );
}

#[test]
fn input_components_empty_material_is_stable() {
    assert_eq!(
        build_vibe_input_components(&[]),
        format!(r#"{{"packets":[],"prompt_version":"{VIBE_PROMPT_VERSION}"}}"#)
    );
}

#[test]
fn packet_ids_enter_the_pre_image_sorted() {
    let out = build_vibe_input_components(&[packet_block(22), packet_block(9)]);
    assert_eq!(
        out,
        format!(r#"{{"packets":[9,22],"prompt_version":"{VIBE_PROMPT_VERSION}"}}"#)
    );
    let reversed = build_vibe_input_components(&[packet_block(9), packet_block(22)]);
    assert_eq!(hash_components(&out), hash_components(&reversed));
}

#[test]
fn a_packet_alone_is_material_enough_to_wake_her() {
    let with_packet = VibeContext {
        memories: memories::test_package(),
        packets: vec![packet_block(1)],
        input_components_json: String::new(),
        input_hash: String::new(),
    };
    assert!(
        !with_packet.empty(),
        "a charged packet is her material — she files first"
    );
    let nothing = VibeContext {
        packets: Vec::new(),
        ..with_packet
    };
    assert!(nothing.empty(), "no packet ⇒ marker path");
}

#[tokio::test]
async fn frozen_creation_contract() {
    let packages: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/memories/packages-v2-2026-09-14.json"
    ))
    .unwrap();
    let mut snapshots = Vec::new();
    for entry in packages.as_array().unwrap() {
        let mut memory: memories::Package =
            serde_json::from_value(entry["package"].clone()).unwrap();
        memory.mission = Mission::Influencer;
        for previous in [None, Some(70)] {
            memory.previous_score = previous;
            for packets in [vec![], vec![packet_block(22), packet_block(9)]] {
                let components = memory
                    .with_input_components(&build_vibe_input_components(&packets))
                    .unwrap();
                let rendered = memory.render_for_model().unwrap();
                let prompt = build_sentiment_prompt(
                    "player",
                    &memory.entity.name,
                    "football",
                    &packets,
                    Some(&rendered),
                );
                let ctx = VibeContext {
                    memories: memory.clone(),
                    packets: packets.clone(),
                    input_components_json: components.clone(),
                    input_hash: hash_components(&components),
                };
                let request = Request {
                    entity_type: "player",
                    entity_name: &memory.entity.name,
                    sport: "football",
                    ..request()
                };
                let adapters = Adapters::with_replies(&[VALID]);
                let out =
                    influencer::create(&Studio::new(&adapters), &request.assignment(&ctx).unwrap())
                        .await
                        .unwrap();
                assert_eq!(
                    out.was_called(),
                    !(packets.is_empty() && previous.is_none())
                );
                assert_eq!(
                    out.provenance.input_hash.as_deref(),
                    Some(ctx.input_hash.as_str())
                );
                if let Some(call) = &out.call {
                    assert_eq!(call.built_prompt, prompt);
                }
                snapshots.push(serde_json::json!({
                    "name": memory.entity.name, "previous": previous, "packets": packets.len(),
                    "components": components, "hash": hash_components(&components),
                    "prompt_hash": hash_components(&prompt),
                    "marker": packets.is_empty() && previous.is_none(),
                }));
            }
        }
    }
    let snapshot = serde_json::json!({"system": influencer::VIBE_SYSTEM_PROMPT.as_str(), "version": VIBE_PROMPT_VERSION, "cases": snapshots});
    let baseline: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/studio/influencer-baseline.json"
    ))
    .unwrap();
    assert_eq!(snapshot, baseline);
}

fn packet_block(id: i64) -> PacketBlock {
    PacketBlock {
        packet_id: id,
        text: "STORY: Arsenal close on Vinicius Junior\nMOOD: anticipation — \"the whole of north London is holding its breath\"\nREPORTED (newest first):\n- Football365: Arsenal have reached an agreement in principle\n".into(),
    }
}
