//! Read-only frozen-input comparison: baseline, reviewed groups, and 4B-produced groups.
//! CASES.jsonl PROMPT_DIR NEW_OUTPUT_DIR MODEL_URL MODEL [selected]
use anyhow::{ensure, Context, Result};
use scoracle_cognition::{
    evaluation::tasks::{LensTask, NarrativesTask, Prepared},
    harness::{
        model::{GenerateOptions, Inference, ResponseFailure},
        providers::llama::LlamaClient,
    },
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    parts: Value,
    reference_groups: Vec<Vec<usize>>,
}

// IDs may support multiple topics; no report may disappear or acquire a new identity.
fn validate_groups(mut groups: Vec<Vec<usize>>, count: usize) -> Result<Vec<Vec<usize>>> {
    ensure!(
        !groups.is_empty() && groups.len() <= count,
        "invalid topic count"
    );
    let mut coverage = BTreeSet::new();
    let mut anchors = BTreeSet::new();
    for group in &groups {
        ensure!(!group.is_empty(), "empty topic");
        ensure!(anchors.insert(group[0]), "duplicate topic anchor");
        let mut seen = BTreeSet::new();
        for &id in group {
            ensure!(
                id > 0 && id <= count && seen.insert(id),
                "invalid or duplicate report ID"
            );
            coverage.insert(id);
        }
    }
    ensure!(coverage.len() == count, "grouping omitted reports");
    groups.sort_by_key(|g| g[0]);
    Ok(groups)
}

fn grouped_packet(packet: &Value, groups: &[Vec<usize>], selected_only: bool) -> Result<String> {
    let reports = packet["fresh"].as_array().context("fresh")?;
    let groups = validate_groups(groups.to_vec(), reports.len())?;
    let mut grouped = packet.clone();
    grouped["fresh"] = json!(groups
        .iter()
        .enumerate()
        .map(|(i, g)| json!({
            "topic_key":format!("topic_{}",i+1),
            "anchor_id":reports[g[0]-1]["id"],
            "reports":g.iter().map(|id| reports[id-1].clone()).collect::<Vec<_>>()
        }))
        .collect::<Vec<_>>());
    if selected_only {
        grouped["fresh"].as_array_mut().unwrap().truncate(3);
    }
    Ok(serde_json::to_string(&grouped)?)
}

async fn generate(
    model: &LlamaClient,
    prepared: &Prepared,
    path: &Path,
) -> Result<(Option<String>, Value)> {
    std::fs::create_dir(path)?;
    std::fs::write(
        path.join("input.json"),
        serde_json::to_vec_pretty(&json!({
            "user_prompt":prepared.user_prompt,"system":prepared.options.system,
            "schema":prepared.options.format_schema,"num_ctx":prepared.options.num_ctx,
            "num_predict":prepared.options.num_predict,"model":model.model(),"temperature":0,"thinking":false
        }))?,
    )?;
    let start = Instant::now();
    let mut record = json!({"production_writes":false});
    let output = match model
        .generate(&prepared.user_prompt, &prepared.options)
        .await
    {
        Ok((result, request)) => {
            record["input_tokens"] = json!(result.prompt_eval_count);
            record["output_tokens"] = json!(result.eval_count);
            std::fs::write(path.join("output.json"), &result.response)?;
            std::fs::write(
                path.join("receipt.json"),
                serde_json::to_vec(&json!({
                    "request":request,"raw_response":result.raw_response_body
                }))?,
            )?;
            Some(result.response)
        }
        Err(error) => {
            record["error"] = json!(format!("{error:#}"));
            if let Some(failure) = error.downcast_ref::<ResponseFailure>() {
                std::fs::write(path.join("failed-response.txt"), &failure.raw_response_body)?;
            }
            None
        }
    };
    record["wall_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
    std::fs::write(
        path.join("result.json"),
        serde_json::to_vec_pretty(&record)?,
    )?;
    Ok((output, record))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 6 || (args.len() == 7 && args[6] == "selected"),
        "CASES.jsonl PROMPT_DIR NEW_OUTPUT_DIR MODEL_URL MODEL [selected]"
    );
    let selected_only = args.len() == 7;
    let cases: Vec<Case> = std::fs::read_to_string(&args[1])?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    ensure!(!cases.is_empty() && cases.len() <= 8, "requires 1–8 cases");
    let prompts = Path::new(&args[2]);
    let flat_system = std::fs::read_to_string(prompts.join("baseline.txt"))?;
    let grouped_system = std::fs::read_to_string(prompts.join("grouped.txt"))?;
    let grouping_system = std::fs::read_to_string(prompts.join("grouper.txt"))?;
    let root = Path::new(&args[3]);
    std::fs::create_dir(root)?;
    let mut ledger = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(root.join("results.jsonl"))?;
    let model = LlamaClient::new(&args[4], &args[5], Duration::from_secs(180), Some(false))?;
    for pass in 1..=2 {
        for case in &cases {
            let original = NarrativesTask.assemble(&case.parts)?;
            ensure!(original.options.format_schema.as_ref().is_some_and(|s| s["properties"]["narratives"].is_object()),
                "archived multi-story experiment; use character_outputs for the current one-hook recap contract");
            let packet: Value = serde_json::from_str(&original.user_prompt)?;
            let count = packet["fresh"].as_array().context("fresh")?.len();
            let reference = validate_groups(case.reference_groups.clone(), count)?;
            let case_root = root.join(format!("{}-pass-{pass}", case.name));
            std::fs::create_dir(&case_root)?;
            std::fs::write(
                case_root.join("parts.json"),
                serde_json::to_vec_pretty(&case.parts)?,
            )?;
            let order = if selected_only {
                vec!["selected"]
            } else if pass == 1 {
                vec!["baseline", "reference", "automatic"]
            } else {
                vec!["automatic", "reference", "baseline"]
            };
            for mode in order {
                let start = Instant::now();
                let mut record =
                    json!({"name":case.name,"pass":pass,"mode":mode,"production_writes":false});
                let mut groups = reference.clone();
                if mode == "automatic" {
                    let mut options = GenerateOptions {
                        system: Some(grouping_system.clone()),
                        temperature: Some(0.0),
                        num_predict: 1200,
                        num_ctx: 10000,
                        ..Default::default()
                    };
                    options.format_schema = Some(
                        json!({"type":"object","additionalProperties":false,"required":["groups"],"properties":{"groups":{"type":"array","minItems":1,"maxItems":count,"items":{"type":"array","minItems":1,"maxItems":count,"uniqueItems":true,"items":{"type":"integer","minimum":1,"maximum":count}}}}}),
                    );
                    let grouping = Prepared::captured(
                        serde_json::to_string(
                            &json!({"meta":packet["meta"],"reports":packet["fresh"]}),
                        )?,
                        options,
                    );
                    let (raw, receipt) =
                        generate(&model, &grouping, &case_root.join("grouper")).await?;
                    record["grouping"] = receipt;
                    let parsed = raw.context("grouping generation failed").and_then(|raw| {
                        let value: Value = serde_json::from_str(&raw)?;
                        validate_groups(serde_json::from_value(value["groups"].clone())?, count)
                    });
                    match parsed {
                        Ok(value) => groups = value,
                        Err(error) => {
                            record["error"] = json!(format!("{error:#}"));
                            record["total_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
                            writeln!(ledger, "{record}")?;
                            ledger.flush()?;
                            println!(
                                "{} pass {pass} automatic: grouping failed: {error:#}",
                                case.name
                            );
                            continue;
                        }
                    }
                }
                let mut prepared = original.clone();
                // All arms get the same physical-safe budget; added group markers must not clip evidence.
                prepared.options.num_ctx = 10000;
                prepared.options.system = Some(if mode == "baseline" {
                    flat_system.clone()
                } else {
                    grouped_system.clone()
                });
                if mode != "baseline" {
                    prepared.user_prompt = grouped_packet(&packet, &groups, selected_only)?;
                    if selected_only {
                        groups.truncate(3);
                        let keys: BTreeSet<_> = groups.iter().flatten().copied().collect();
                        let references = &mut prepared.options.format_schema.as_mut().unwrap()
                            ["properties"]["narratives"]["items"]["properties"]["report_ids"];
                        references["items"]["enum"] = json!(keys);
                        references["maxItems"] = json!(keys.len());
                        record["visible_reports"] = json!(keys.len());
                        record["selection_is_reviewed_not_automatic"] = json!(true);
                    }
                    record["groups"] = json!(groups);
                }
                let (raw, receipt) = generate(&model, &prepared, &case_root.join(mode)).await?;
                record["writing"] = receipt;
                if let Some(raw) = raw {
                    let verdict = NarrativesTask.evaluate_prepared(&prepared, &raw, None, None);
                    record["accepted"] = json!(verdict.all_checks_pass());
                    record["verdict"] = json!(verdict.display);
                    if mode != "baseline" {
                        let reply: Value = serde_json::from_str(&raw)?;
                        record["citations_within_assigned_topics"] = json!(reply["narratives"]
                            .as_array()
                            .context("narratives")?
                            .iter()
                            .enumerate()
                            .all(|(i, n)| {
                                groups.get(i).is_some_and(|g| {
                                    n["report_ids"].as_array().is_some_and(|keys| {
                                        !keys.is_empty()
                                            && keys
                                                .iter()
                                                .all(|key| g.iter().any(|id| key == &json!(*id)))
                                    })
                                })
                            }));
                    }
                }
                record["total_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
                writeln!(ledger, "{record}")?;
                ledger.flush()?;
                println!(
                    "{} pass {pass} {mode}: accepted={} total_ms={}",
                    case.name, record["accepted"], record["total_ms"]
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grouping_keeps_source_objects_and_rejects_missing_or_fabricated_ids() {
        let packet = json!({"meta":{"name":"A"},"tone":"Direct","fresh":[
            {"id":1,"publisher":"Wire","headline":"Possible move; no agreement."},
            {"id":2,"publisher":"Ledger","headline":"Club denies the agreement."},
            {"id":3,"publisher":"Wire","headline":"Separate discussion."}]});
        let grouped: Value =
            serde_json::from_str(&grouped_packet(&packet, &[vec![3], vec![1, 2]], false).unwrap())
                .unwrap();
        assert_eq!(
            grouped["fresh"][0]["reports"],
            json!([packet["fresh"][0], packet["fresh"][1]])
        );
        assert_eq!(grouped["fresh"][1]["reports"][0], packet["fresh"][2]);
        assert_eq!(grouped["meta"], packet["meta"]);
        assert_eq!(grouped["tone"], packet["tone"]);
        for bad in [
            vec![vec![1, 2]],
            vec![vec![1, 2, 4]],
            vec![vec![1, 1, 2, 3]],
            vec![vec![1, 2], vec![1, 3]],
            vec![vec![], vec![1, 2, 3]],
        ] {
            assert!(validate_groups(bad, 3).is_err());
        }
        assert!(validate_groups(vec![vec![1, 2], vec![3, 2]], 3).is_ok());
        let mut four = packet.clone();
        four["fresh"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":4,"publisher":"Other","headline":"Unrelated"}));
        let selected: Value = serde_json::from_str(
            &grouped_packet(&four, &[vec![1, 2], vec![3], vec![4]], true).unwrap(),
        )
        .unwrap();
        assert_eq!(
            selected["fresh"][0]["reports"],
            grouped["fresh"][0]["reports"]
        );
        let singleton_groups = vec![vec![1], vec![2], vec![3], vec![4]];
        let selected: Value =
            serde_json::from_str(&grouped_packet(&four, &singleton_groups, true).unwrap()).unwrap();
        assert_eq!(selected["fresh"].as_array().unwrap().len(), 3);
        assert_eq!(selected["fresh"][2]["reports"][0], four["fresh"][2]);
    }
}
