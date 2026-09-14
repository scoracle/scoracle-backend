//! Build reproducible Scout memory-ablation requests; never calls a model or DB.
//! cargo run --example memory_probe > /tmp/memory-probe-requests.json
//! Runtime defaults mirror the recorded September 14 Scout configuration.

use anyhow::{Context, Result};
use scoracle_cognition::{
    composition::{compose_card, form, memories::Package},
    runtime::config::{Backend, RouteConfig},
    runtime::providers::ollama::{GenerateOptions, OllamaClient},
    runtime::route::{resolve_voice_num_ctx, Role},
};
use serde_json::{json, Value};
use std::time::Duration;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if let [flag, path] = args.as_slice() {
        if flag == "--review" {
            return review(path);
        }
    }
    let path = match args.as_slice() {
        [] => "fixtures/memories/packages-v2-2026-09-14.json",
        [flag, path] if flag == "--packages" => path.as_str(),
        _ => anyhow::bail!(
            "usage: memory_probe [--packages PACKAGES_JSON | --review RESPONSES_JSONL]"
        ),
    };
    let captured: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let source = captured
        .iter()
        .find(|c| c["package"]["entity"]["id"] == 4592198)
        .context("Rogers package missing")?;
    let full: Package = serde_json::from_value(source["package"].clone())?;
    let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "granite4.2:3b".into());
    let base = std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".into());
    let config = RouteConfig::from_env(&model, &base);
    let spec = &config.roles[&Role::StatsLogic];
    anyhow::ensure!(
        spec.backend == Backend::Ollama,
        "this raw-metrics probe supports Ollama only"
    );
    let client = OllamaClient::with_think(
        &spec.base_url,
        &spec.model,
        Duration::from_secs(300),
        spec.think,
    )?;
    let mut requests = Vec::new();
    for seed in [17, 43] {
        for condition in ["full_memories", "without_performance_baseline"] {
            let mut package = full.clone();
            if condition == "without_performance_baseline" {
                package
                    .groups
                    .retain(|g| g.id != "Historical performance baseline");
                package.diagnostics.push("Controlled ablation: prior performance group removed; all remaining evidence is identical.".into());
            }
            let composed = compose_card(&package, "")?;
            let opts = GenerateOptions {
                system: Some(composed.system),
                temperature: Some(0.6),
                num_predict: scoracle_cognition::junctions::scout::RATING_NUM_PREDICT,
                num_ctx: resolve_voice_num_ctx(std::env::var("VOICE_NUM_CTX").ok().as_deref()),
                json_mode: false,
                format_schema: Some(form::card_schema(false)),
                format_schema_raw: None,
            };
            let mut body = client.request_body(&composed.prompt, &opts);
            // The only diagnostic setting added beyond the production builder.
            body["options"]["seed"] = json!(seed);
            requests.push(json!({"condition":condition,"seed":seed,"memory_fingerprint":package.fingerprint()?,"context_bytes":composed.prompt.len(),"request":body}));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "endpoint":format!("{}/api/chat",spec.base_url),
            "notes":"First-pass offline experiment, no surface rewrite, no production persistence. Matched seeds are diagnostic controls, not a guarantee of deterministic GPU output.",
            "requests":requests
        }))?
    );
    Ok(())
}

fn review(path: &str) -> Result<()> {
    use scoracle_cognition::junctions::scout::RatingParser;
    use scoracle_cognition::runtime::harness::Parser;
    let mut results = Vec::new();
    for line in std::fs::read_to_string(path)?.lines() {
        let run: Value = serde_json::from_str(line)?;
        let response = &run["response"];
        let text = response["message"]["content"].as_str().unwrap_or_default();
        let decoded: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        let complete = response["done"] == true && response["done_reason"] == "stop";
        let validation = RatingParser.parse(text);
        results.push(json!({
            "condition":run["condition"],"seed":run["seed"],"wall_seconds":run["wall_seconds"],
            "request_error":run["error"],"complete":complete,"done_reason":response["done_reason"],
            "parser_pass":validation.is_ok(),"parser_error":validation.err().map(|e|e.to_string()),
            "headline":decoded["headline"],"body":decoded["body"],
            "body_chars":decoded["body"].as_str().map(|b|b.chars().count()),
            "prompt_tokens":response["prompt_eval_count"],"output_tokens":response["eval_count"],
            "prompt_eval_ns":response["prompt_eval_duration"],"output_eval_ns":response["eval_duration"],
            "note":"Parser/completion checks do not establish factual or editorial quality."
        }));
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
