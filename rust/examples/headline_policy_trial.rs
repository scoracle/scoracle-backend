//! Paired frozen-input trial through current factories and guards; no database or publication.
//! Usage: headline_policy_trial CASES.jsonl NEW_OUTPUT_DIR [MODEL_URL MODEL]
use anyhow::{ensure, Context, Result};
use scoracle_cognition::{
    evaluation::tasks::{resolve_task, Expect, Fixture},
    harness::{model::Inference, providers::llama::LlamaClient},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{io::Write, path::PathBuf, time::Duration};

#[derive(Deserialize)]
struct Case {
    name: String,
    task: String,
    before: Value,
    after: Value,
    counts: Value,
    expect: Expect,
    review: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--screen") {
        ensure!(args.len() == 4, "--screen SOURCES.json NEW_OUTPUT.json");
        let mut cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&args[2])?)?;
        for case in &mut cases {
            for source in case["sources"].as_array_mut().context("source array")? {
                source["spam"] = json!(scoracle_cognition::tools::source::is_headline_spam(
                    source["source"]["title"].as_str().context("headline")?
                ));
            }
        }
        let mut output = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&args[3])?;
        output.write_all(&serde_json::to_vec(&cases)?)?;
        return Ok(());
    }
    ensure!(
        args.len() == 3 || args.len() == 5,
        "CASES.jsonl NEW_OUTPUT_DIR [MODEL_URL MODEL]"
    );
    let cases: Vec<Case> = std::fs::read_to_string(&args[1])?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        !cases.is_empty() && cases.len() <= 8,
        "trial requires 1–8 fixed cases"
    );
    let mut names = std::collections::HashSet::new();
    ensure!(
        cases.iter().all(|c| names.insert(&c.name)),
        "duplicate trial case"
    );
    let root = PathBuf::from(&args[2]);
    std::fs::create_dir(&root)?;
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(root.join("results.jsonl"))?;
    let model = if args.len() == 5 {
        Some(LlamaClient::new(
            &args[3],
            &args[4],
            Duration::from_secs(180),
            Some(false),
        )?)
    } else {
        None
    };
    for (index, case) in cases.iter().enumerate() {
        let task = resolve_task(&case.task).context("unknown trial task")?;
        // Alternate order; inference remains sequential with one first attempt per variant.
        let variants = if index % 2 == 0 {
            ["before", "after"]
        } else {
            ["after", "before"]
        };
        for variant in variants {
            let parts = if variant == "before" {
                &case.before
            } else {
                &case.after
            };
            let prepared = task.assemble(parts)?;
            let path = root.join(format!("{}-{variant}", case.name));
            std::fs::create_dir(&path)?;
            let fixture = Fixture {
                name: format!("{}-{variant}", case.name),
                task: case.task.clone(),
                sport: parts["subject"]["sport"]
                    .as_str()
                    .unwrap_or_default()
                    .into(),
                prompt_version: task.prompt_version().into(),
                system: String::new(),
                user_prompt: prepared.user_prompt.clone(),
                parts: Some(parts.clone()),
                temperature: 0.0,
                expect: case.expect.clone(),
                review: case.review.clone(),
            };
            std::fs::write(
                path.join("fixture.json"),
                serde_json::to_vec_pretty(&fixture)?,
            )?;
            let mut record = json!({"name":case.name,"variant":variant,"task":case.task,
                "counts":case.counts,"should_call":prepared.should_call,"production_writes":false,
                "prompt_bytes":prepared.user_prompt.len(),"attempt_limit":1,"human_factual_approval":false});
            if let Some(model) = &model {
                if prepared.should_call {
                    let mut options = prepared.options.clone();
                    options.num_ctx = 10000;
                    options.temperature = Some(0.0);
                    record["reserved_output"] = json!(options.num_predict);
                    let started = std::time::Instant::now();
                    match model.generate(&prepared.user_prompt, &options).await {
                        Ok((response, receipt)) => {
                            let verdict = task.evaluate_prepared(
                                &prepared,
                                &response.response,
                                None,
                                Some(&case.expect),
                            );
                            record["accepted"] = json!(verdict.all_checks_pass());
                            record["verdict"] = json!(verdict.display);
                            record["checks"] = json!(verdict
                                .checks
                                .iter()
                                .map(|c| json!({"name":c.name,"pass":c.pass,"detail":c.detail}))
                                .collect::<Vec<_>>());
                            record["input_tokens"] = json!(response.prompt_eval_count);
                            record["output_tokens"] = json!(response.eval_count);
                            std::fs::write(path.join("output.json"), &response.response)?;
                            std::fs::write(
                                path.join("receipt.json"),
                                serde_json::to_vec(
                                    &json!({"request":receipt,"raw_response":response.raw_response_body}),
                                )?,
                            )?;
                        }
                        Err(error) => record["error"] = json!(format!("{error:#}")),
                    }
                    record["wall_ms"] = json!(started.elapsed().as_millis());
                }
            }
            writeln!(output, "{record}")?;
            output.flush()?;
            println!(
                "{} {variant}: accepted={} input={} output={} error={}",
                case.name,
                record["accepted"],
                record["input_tokens"],
                record["output_tokens"],
                record["error"].as_str().unwrap_or("none")
            );
        }
    }
    Ok(())
}
