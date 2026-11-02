//! Offline prompt comparisons using production preparation, coverage checks and parsers.
//! character_prompt_trial CASES.jsonl NEW_OUTPUT_DIR MODEL_URL MODEL
use anyhow::{ensure, Context, Result};
use scoracle_cognition::{
    evaluation::tasks::{resolve_task, Fixture},
    harness::{model::Inference, providers::llama::LlamaClient},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    arm: String,
    fixture: Fixture,
    system: String,
    user_prompt: String,
    #[serde(default)]
    schema_raw: Option<String>,
}

fn save(path: impl AsRef<Path>, value: &impl serde::Serialize) -> Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(value)? + "\n")?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--validate") {
        ensure!(args.len() == 3, "--validate FIXTURE.json OUTPUT.json");
        let fixture: Fixture = serde_json::from_str(&std::fs::read_to_string(&args[1])?)?;
        let task = resolve_task(&fixture.task).context("unknown task")?;
        let prepared = task.prepare_fixture(&fixture)?;
        let raw = std::fs::read_to_string(&args[2])?;
        let verdict = task.evaluate_prepared(&prepared, &raw, None, Some(&fixture.expect));
        println!(
            "{}",
            json!({"accepted":verdict.all_checks_pass(),"parser_detail":verdict.display})
        );
        return Ok(());
    }
    ensure!(
        args.len() == 4,
        "CASES.jsonl NEW_OUTPUT_DIR MODEL_URL MODEL"
    );
    let cases = std::fs::read_to_string(&args[0])?
        .lines()
        .map(serde_json::from_str::<Case>)
        .collect::<Result<Vec<_>, _>>()?;
    let root = Path::new(&args[1]);
    std::fs::create_dir(root).context("output directory must be new")?;
    let backend = LlamaClient::new(&args[2], &args[3], Duration::from_secs(300), Some(false))?;
    save(
        root.join("run.json"),
        &json!({"cases":args[0],"model":args[3],"url":args[2],
        "thinking":false,"concurrency":1,"retries":0,"publishes":false,"database_access":false,
        "changes":"system/user messages and optional schema property order; production schema semantics, preparation and parser"}),
    )?;
    let mut ledger = std::fs::File::create(root.join("results.jsonl"))?;
    for (index, case) in cases.into_iter().enumerate() {
        let task = resolve_task(&case.fixture.task).context("unknown task")?;
        ensure!(
            case.fixture.prompt_version == task.prompt_version(),
            "stale fixture"
        );
        let mut assignment = task.prepare_fixture(&case.fixture)?;
        ensure!(
            assignment.should_call,
            "trial case has no callable evidence"
        );
        assignment.options.system = Some(case.system.trim().to_owned());
        assignment.user_prompt = case.user_prompt.trim().to_owned();
        if let Some(raw) = case.schema_raw {
            ensure!(
                Some(serde_json::from_str::<Value>(&raw)?) == assignment.options.format_schema,
                "trial may reorder the schema but cannot change its contract"
            );
            assignment.options.format_schema_raw = Some(raw);
        }
        let dir = root.join(format!("{:02}-{}-{}", index + 1, case.name, case.arm));
        std::fs::create_dir(&dir)?;
        save(dir.join("fixture.json"), &case.fixture)?;
        std::fs::write(
            dir.join("system.txt"),
            assignment.options.system.as_ref().unwrap(),
        )?;
        std::fs::write(dir.join("user.txt"), &assignment.user_prompt)?;
        let started = Instant::now();
        let mut row = json!({"name":case.name,"arm":case.arm,"task":case.fixture.task,
            "receipt":dir.file_name().unwrap().to_string_lossy(),"temperature":assignment.options.temperature});
        let result: Result<()> = async {
            let prepared = backend
                .prepare(&assignment.user_prompt, &assignment.options)
                .await?;
            save(dir.join("prepared.json"), &prepared)?;
            prepared.verify_input(&assignment.user_prompt, &assignment.options)?;
            let (generated, receipt) = backend
                .generate_prepared(&assignment.user_prompt, &assignment.options, &prepared)
                .await?;
            save(dir.join("request.json"), &receipt)?;
            std::fs::write(dir.join("response.json"), &generated.raw_response_body)?;
            std::fs::write(dir.join("output.json"), &generated.response)?;
            let verdict = task.evaluate_prepared(
                &assignment,
                &generated.response,
                None,
                Some(&case.fixture.expect),
            );
            row["status"] = json!(if verdict.all_checks_pass() {
                "accepted"
            } else {
                "rejected"
            });
            row["parser_detail"] = json!(verdict.display);
            row["checks"] = json!(verdict
                .checks
                .iter()
                .map(|c| json!({"name":c.name,"pass":c.pass,"detail":c.detail}))
                .collect::<Vec<_>>());
            row["input_tokens"] = json!(generated.prompt_eval_count);
            row["output_tokens"] = json!(generated.eval_count);
            Ok(())
        }
        .await;
        if let Err(error) = result {
            row["status"] = Value::from("error");
            row["error"] = Value::from(format!("{error:#}"));
            if let Some(failure) =
                error.downcast_ref::<scoracle_cognition::harness::model::ResponseFailure>()
            {
                std::fs::write(dir.join("response.json"), &failure.raw_response_body)?;
            }
        }
        row["wall_ms"] = json!(started.elapsed().as_millis());
        save(dir.join("result.json"), &row)?;
        writeln!(ledger, "{row}")?;
        ledger.flush()?;
        eprintln!("{} {}: {}", case.name, case.arm, row["status"]);
    }
    Ok(())
}
