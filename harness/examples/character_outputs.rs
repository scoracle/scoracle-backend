//! Sequential, file-backed character trials using current fixture assembly and validation.
//! character_outputs OUTPUT_DIR MODEL_URL MODEL FIXTURE.json [FIXTURE.json ...]
use anyhow::{ensure, Context, Result};
use scoracle_cognition::evaluation::tasks::{resolve_task, Fixture};
use scoracle_cognition::harness::model::{Inference, ResponseFailure};
use scoracle_cognition::harness::providers::llama::LlamaClient;
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn save(path: impl AsRef<Path>, value: &Value) -> Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(value)? + "\n")?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        args.len() >= 4,
        "usage: character_outputs OUTPUT_DIR MODEL_URL MODEL FIXTURE.json [...]"
    );
    let num_ctx = scoracle_cognition::harness::route::resolve_voice_num_ctx(
        std::env::var("VOICE_NUM_CTX").ok().as_deref(),
    );
    let root = Path::new(&args[0]);
    std::fs::create_dir(root).context("output directory must be new")?;
    let backend = LlamaClient::new(&args[1], &args[2], Duration::from_secs(300), Some(false))?;
    save(
        root.join("run.json"),
        &json!({
            "started_epoch":SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            "model":args[2], "base_url":args[1], "context":num_ctx,
            "concurrency":1, "thinking":false, "retries":0,
            "shared_server":true, "database_access":false, "publishes":false,
            "fixtures":args[3..], "qualification":"first-pass fixture outputs; human review required"
        }),
    )?;
    let mut ledger = std::fs::File::create(root.join("results.jsonl"))?;
    let mut readings = std::fs::File::create(root.join("outputs.txt"))?;
    writeln!(
        readings,
        "First-pass fixture outputs. Status records mechanical checks, not factual quality.
Model: {}. Shared server; one request at a time; no retries or publication.
",
        args[2]
    )?;
    let mut results = Vec::new();
    for (index, path) in args[3..].iter().enumerate() {
        let captured = std::fs::read_to_string(path)?;
        let fixture: Fixture = serde_json::from_str(&captured)?;
        let task = resolve_task(&fixture.task).context("unknown task")?;
        ensure!(
            fixture.prompt_version == task.prompt_version(),
            "stale fixture: {path}"
        );
        let mut assignment = task.prepare_fixture(&fixture)?;
        assignment.options.num_ctx = num_ctx;
        let case = root.join(format!("{:02}", index + 1));
        std::fs::create_dir(&case)?;
        std::fs::write(case.join("fixture.json"), captured)?;
        std::fs::write(case.join("prompt.json"), &assignment.user_prompt)?;
        let mut record = json!({"case":index+1,"task":fixture.task,"name":fixture.name,
            "prompt_version":fixture.prompt_version,"fixture":path,
            "request_bytes":assignment.user_prompt.len(),"temperature":assignment.options.temperature,
            "output_limit":assignment.options.num_predict,"review":fixture.review});
        eprintln!("case {}: {} / {}", index + 1, fixture.task, fixture.name);
        let started = Instant::now();
        let result: Result<()> = async {
            if !assignment.should_call {
                record["status"] = json!("no_call");
                return Ok(());
            }
            let prepared = backend
                .prepare(&assignment.user_prompt, &assignment.options)
                .await?;
            save(
                case.join("prepared.json"),
                &serde_json::to_value(&prepared)?,
            )?;
            prepared.verify_input(&assignment.user_prompt, &assignment.options)?;
            save(case.join("request.json"), &prepared.request)?;
            let (generated, _) = backend
                .generate_prepared(&assignment.user_prompt, &assignment.options, &prepared)
                .await?;
            std::fs::write(case.join("response.json"), &generated.raw_response_body)?;
            std::fs::write(case.join("output.json"), &generated.response)?;
            let verdict = task.evaluate_prepared(
                &assignment,
                &generated.response,
                None,
                Some(&fixture.expect),
            );
            record["status"] = json!(if verdict.parsed && verdict.all_checks_pass() {
                "accepted"
            } else {
                "rejected"
            });
            record["parsed"] = json!(verdict.parsed);
            record["checks"] = json!(verdict
                .checks
                .iter()
                .map(|c| json!({"name":c.name,"pass":c.pass,"detail":c.detail}))
                .collect::<Vec<_>>());
            record["reading"] = json!(verdict.display);
            record["input_tokens"] = json!(generated.prompt_eval_count);
            record["output_tokens"] = json!(generated.eval_count);
            record["generation_wall_ms"] = json!(generated.total_duration.as_millis());
            record["completion_reason"] = json!(generated.completion_reason);
            Ok(())
        }
        .await;
        if let Err(error) = result {
            record["status"] = json!("error");
            record["error"] = json!(format!("{error:#}"));
            if let Some(failure) = error.downcast_ref::<ResponseFailure>() {
                std::fs::write(case.join("response.json"), &failure.raw_response_body)?;
            }
        }
        record["wall_ms"] = json!(started.elapsed().as_millis());
        save(case.join("result.json"), &record)?;
        writeln!(ledger, "{}", record)?;
        ledger.flush()?;
        writeln!(
            readings,
            "{} — {} / {} [{}]\n{}\n",
            index + 1,
            fixture.task,
            fixture.name,
            record["status"].as_str().unwrap(),
            record["reading"]
                .as_str()
                .or(record["error"].as_str())
                .unwrap_or("No model call")
        )?;
        readings.flush()?;
        eprintln!(
            "  {} ({:.1}s)",
            record["status"],
            started.elapsed().as_secs_f64()
        );
        results.push(record);
        save(
            root.join("summary.json"),
            &json!({"completed":results.len(),"cases":results}),
        )?;
    }
    Ok(())
}
