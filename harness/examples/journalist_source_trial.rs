//! Read-only source-scoped claim preparation. SOURCES.jsonl SYSTEM.txt NEW_OUTPUT_DIR URL MODEL
use anyhow::{ensure, Context, Result};
use scoracle_cognition::harness::{
    model::{GenerateOptions, Inference, ResponseFailure},
    providers::llama::LlamaClient,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Source {
    name: String,
    user_prompt: String,
    schema_raw: String,
    provenance: Value,
}

// Exact spans protect copying, not interpretation. Semantic fidelity is reviewed separately.
fn validate(raw: &str, source: &str) -> Result<()> {
    let value: Value = serde_json::from_str(raw)?;
    let statements = value["statements"].as_array().context("statements")?;
    ensure!(
        (1..=3).contains(&statements.len()),
        "invalid statement count"
    );
    for statement in statements {
        let exact = |v: &Value| {
            v.as_str()
                .is_some_and(|s| !s.trim().is_empty() && source.contains(s))
        };
        if let Some(actor) = statement.get("actor") {
            ensure!(
                actor.is_null() || exact(actor),
                "actor is not an exact source span"
            );
        } else {
            let about = statement["about"].as_array().context("about")?;
            ensure!(
                about.len() <= 4 && about.iter().all(exact),
                "about is not an exact source span"
            );
            let speaker = statement.get("speaker").context("speaker")?;
            ensure!(
                speaker.is_null() || exact(speaker),
                "speaker is not an exact source span"
            );
        }
        ensure!(
            ["assertion", "opinion", "question", "listing"]
                .contains(&statement["kind"].as_str().unwrap_or("")),
            "invalid kind"
        );
        ensure!(
            statement["claim"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty()),
            "empty claim"
        );
        let qualifications = statement["qualifications"]
            .as_array()
            .context("qualifications")?;
        ensure!(
            qualifications.len() <= 5
                && qualifications.iter().all(|v| v
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty() && source.contains(s))),
            "qualification is not an exact source span"
        );
    }
    let unspecified = value["unspecified"].as_array().context("unspecified")?;
    ensure!(
        unspecified.len() <= 4
            && unspecified
                .iter()
                .all(|v| v.as_str().is_some_and(|s| !s.trim().is_empty())),
        "invalid unspecified details"
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 6,
        "SOURCES.jsonl SYSTEM.txt NEW_OUTPUT_DIR URL MODEL"
    );
    let cases: Vec<Source> = std::fs::read_to_string(&args[1])?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    ensure!(
        !cases.is_empty() && cases.len() <= 100,
        "requires 1–100 sources"
    );
    let system = std::fs::read_to_string(&args[2])?;
    let root = PathBuf::from(&args[3]);
    std::fs::create_dir(&root)?;
    let mut ledger = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(root.join("results.jsonl"))?;
    let model = LlamaClient::new(&args[4], &args[5], Duration::from_secs(180), Some(false))?;
    for case in cases {
        ensure!(
            !case.name.is_empty()
                && case
                    .name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-'),
            "unsafe source name"
        );
        let packet: Value = serde_json::from_str(&case.user_prompt)?;
        let evidence = packet["headline"].as_str().context("headline")?;
        let options = GenerateOptions {
            system: Some(system.clone()),
            temperature: Some(0.0),
            num_predict: 800,
            num_ctx: 4096,
            format_schema: Some(serde_json::from_str(&case.schema_raw)?),
            format_schema_raw: Some(case.schema_raw.clone()),
            ..Default::default()
        };
        let path = root.join(&case.name);
        std::fs::create_dir(&path)?;
        std::fs::write(
            path.join("input.json"),
            serde_json::to_vec_pretty(
                &json!({"user_prompt":case.user_prompt,"system":system,"schema_raw":case.schema_raw,"provenance":case.provenance,"num_ctx":4096,"num_predict":800,"temperature":0,"thinking":false,"model":model.model()}),
            )?,
        )?;
        let start = Instant::now();
        let mut record = json!({"name":case.name,"production_writes":false});
        match model.generate(&case.user_prompt, &options).await {
            Ok((response, receipt)) => {
                std::fs::write(path.join("output.json"), &response.response)?;
                std::fs::write(
                    path.join("receipt.json"),
                    serde_json::to_vec(
                        &json!({"request":receipt,"raw_response":response.raw_response_body}),
                    )?,
                )?;
                record["input_tokens"] = json!(response.prompt_eval_count);
                record["output_tokens"] = json!(response.eval_count);
                match validate(&response.response, evidence) {
                    Ok(()) => record["spans_valid"] = json!(true),
                    Err(error) => {
                        record["spans_valid"] = json!(false);
                        record["validation_error"] = json!(format!("{error:#}"));
                    }
                }
            }
            Err(error) => {
                record["error"] = json!(format!("{error:#}"));
                if let Some(failure) = error.downcast_ref::<ResponseFailure>() {
                    std::fs::write(path.join("failed-response.txt"), &failure.raw_response_body)?;
                }
            }
        }
        record["wall_ms"] = json!(start.elapsed().as_millis());
        writeln!(ledger, "{record}")?;
        ledger.flush()?;
        println!(
            "{}: spans_valid={} error={}",
            case.name, record["spans_valid"], record["error"]
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copied_spans_reject_invention_without_claiming_semantic_truth() {
        let source = "Avery may return; two approvals needed";
        let value = json!({"statements":[{"actor":"Avery","kind":"assertion","claim":"Avery may return.","qualifications":["may","two approvals needed"]}],"unspecified":["what the approvals are"]});
        assert!(validate(&value.to_string(), source).is_ok());
        let mut changed = value.clone();
        changed["statements"][0]["actor"] = json!("Coach Avery");
        assert!(validate(&changed.to_string(), source).is_err());
        changed = value.clone();
        changed["statements"][0]["qualifications"] = json!(["two clearances needed"]);
        assert!(validate(&changed.to_string(), source).is_err());
        changed = value.clone();
        changed["statements"][0]["actor"] = Value::Null;
        assert!(validate(&changed.to_string(), source).is_ok());
        changed = value;
        changed["statements"][0]["claim"] = json!("Avery has returned.");
        assert!(validate(&changed.to_string(), source).is_ok());
        assert!(validate(r#"{"statements":[],"unspecified":[]}"#, source).is_err());
    }

    #[test]
    fn speaker_and_subject_copy_checks_do_not_establish_role_truth() {
        let source = "Avery says Harbor may improve";
        let mut value = json!({"statements":[{"about":["Harbor"],"speaker":"Avery","kind":"opinion","claim":"Avery says Harbor may improve.","qualifications":["may"]}],"unspecified":[]});
        assert!(validate(&value.to_string(), source).is_ok());
        value["statements"][0]["speaker"] = json!("Coach Avery");
        assert!(validate(&value.to_string(), source).is_err());
        value["statements"][0]["speaker"] = json!("Harbor");
        assert!(validate(&value.to_string(), source).is_ok());
        value["statements"][0]["about"] = json!(["Metro"]);
        assert!(validate(&value.to_string(), source).is_err());
    }
}
