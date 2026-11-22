//! Controlled assembly comparison on one model; no database or publication.
//! CASES.jsonl SYSTEM.txt NEW_OUTPUT_DIR MODEL_URL MODEL
use anyhow::{ensure, Context, Result};
use scoracle_cognition::{
    evaluation::tasks::{Expect, LensTask, NarrativesTask},
    harness::{
        model::{Inference, ResponseFailure},
        providers::llama::LlamaClient,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    parts: Value,
    expect: Expect,
    review: Vec<String>,
    #[serde(default)]
    user_prompt: Option<String>,
    #[serde(default)]
    schema: Option<Value>,
    #[serde(default)]
    context_layout: Option<ContextLayout>,
    #[serde(default)]
    output_budget: Option<i32>,
}

fn output_budget(requested: Option<i32>, default: i32) -> Result<i32> {
    let budget = requested.unwrap_or(default);
    ensure!(
        budget > 0 && budget <= default,
        "invalid reduced output budget"
    );
    Ok(budget)
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum ContextLayout {
    JsonContextFirst,
    SectionsSourceFirst,
    SectionsContextFirst,
}

// Preserve each context's exact JSON bytes; only ordering and section boundaries change.
fn context_layout(original: &str, layout: ContextLayout) -> Result<String> {
    let fields: std::collections::BTreeMap<String, Box<serde_json::value::RawValue>> =
        serde_json::from_str(original)?;
    ensure!(
        fields.len() == 3
            && ["meta", "fresh", "tone"]
                .iter()
                .all(|k| fields.contains_key(*k)),
        "expected exactly meta, fresh and tone"
    );
    let meta = &fields["meta"];
    let fresh = &fields["fresh"];
    let tone = &fields["tone"];
    Ok(match layout {
        ContextLayout::JsonContextFirst => {
            format!(r#"{{"meta":{meta},"tone":{tone},"fresh":{fresh}}}"#)
        }
        ContextLayout::SectionsSourceFirst | ContextLayout::SectionsContextFirst => {
            let meta =
                format!("meta — database identity background, not publisher reporting:\n{meta}");
            let fresh = format!("fresh — publisher reporting evidence:\n{fresh}");
            let tone = format!("tone — writing style only:\n{tone}");
            if matches!(layout, ContextLayout::SectionsSourceFirst) {
                format!("{fresh}\n\n{meta}\n\n{tone}")
            } else {
                format!("{meta}\n\n{tone}\n\n{fresh}")
            }
        }
    })
}

fn statement_schema_order(schema: &Value) -> Result<String> {
    let item = &schema["properties"]["narratives"]["items"]["properties"];
    let statement = &item["statements"]["items"]["properties"];
    let gaps = statement
        .get("unspecified")
        .map(|value| format!(r#", "unspecified":{value}"#))
        .unwrap_or_default();
    let ordered_statement = format!(
        r#"{{"id":{}{gaps},"attribution":{},"clause":{}}}"#,
        statement["id"], statement["attribution"], statement["clause"]
    );
    let ordered_item = format!(
        r#"{{"statements":{},"headline":{}}}"#,
        item["statements"], item["headline"]
    );
    let raw = serde_json::to_string(schema)?
        .replacen(&serde_json::to_string(item)?, &ordered_item, 1)
        .replacen(&serde_json::to_string(statement)?, &ordered_statement, 1);
    ensure!(
        serde_json::from_str::<Value>(&raw)? == *schema,
        "statement schema changed"
    );
    Ok(raw)
}

// Rendering owns publisher spelling; it does not verify that a clause follows from its source.
fn render_statements(raw: &str, packet: &Value) -> Result<String> {
    let reply: Value = serde_json::from_str(raw)?;
    let entries = reply["narratives"].as_array().context("narratives")?;
    let groups = packet["fresh"].as_array().context("topic groups")?;
    ensure!(
        entries.len() == groups.len().min(3),
        "missing assigned topics"
    );
    let mut rendered = Vec::new();
    for (entry, group) in entries.iter().zip(groups) {
        let statements = entry["statements"].as_array().context("statements")?;
        ensure!(
            !statements.is_empty() && statements.len() <= 3,
            "invalid statement count"
        );
        let reports = group["reports"].as_array().context("reports")?;
        let mut keys = Vec::new();
        let mut sentences = Vec::new();
        for statement in statements {
            if let Some(gaps) = statement.get("unspecified") {
                let gaps = gaps.as_array().context("unspecified details")?;
                ensure!(
                    gaps.len() <= 3
                        && gaps
                            .iter()
                            .all(|v| v.as_str().is_some_and(|s| !s.trim().is_empty())),
                    "invalid unspecified details"
                );
            }
            let key = statement["id"].as_u64().context("report ID")?;
            let report = reports
                .iter()
                .find(|r| r["id"] == key)
                .context("statement cites outside its topic")?;
            let attribution = statement["attribution"].as_str().context("attribution")?;
            ensure!(
                [
                    "reports that",
                    "notes that",
                    "discusses",
                    "asks whether",
                    "lists",
                    "quotes"
                ]
                .contains(&attribution),
                "invalid attribution form"
            );
            let clause = statement["clause"].as_str().context("clause")?;
            ensure!(!clause.trim().is_empty(), "empty clause");
            sentences.push(format!(
                "{} {attribution} {clause}",
                report["publisher"].as_str().context("publisher")?
            ));
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        ensure!(
            keys.iter().any(|k| group["anchor_id"] == *k),
            "missing topic anchor"
        );
        rendered.push(
            json!({"headline":entry["headline"],"body":sentences.join(" "),"report_ids":keys}),
        );
    }
    Ok(serde_json::to_string(&json!({"narratives":rendered}))?)
}

// Change only property emission order; keep the complete schema semantically identical.
fn citations_first(schema: &Value) -> Result<String> {
    let props = &schema["properties"]["narratives"]["items"]["properties"];
    let sorted = serde_json::to_string(props)?;
    let ordered = format!(
        r#"{{"report_ids":{},"body":{},"headline":{}}}"#,
        props["report_ids"], props["body"], props["headline"]
    );
    let raw = serde_json::to_string(schema)?.replacen(&sorted, &ordered, 1);
    ensure!(
        serde_json::from_str::<Value>(&raw)? == *schema,
        "schema changed"
    );
    Ok(raw)
}

// Legacy-style numbered source blocks, retaining every value in the current packet.
fn source_blocks(packet: &Value) -> Result<String> {
    let mut text = format!("meta:\n{}\n\nfresh:\n", packet["meta"]);
    for report in packet["fresh"].as_array().context("fresh reports")? {
        text.push_str(&format!(
            "\n{} [{}] scope={} published_at={}\n{}\n",
            report["id"].as_u64().context("report ID")?,
            report["publisher"].as_str().context("publisher")?,
            report["evidence_scope"],
            report["published_at"],
            report
                .get("headline")
                .or(report.get("publisher_excerpt"))
                .context("evidence")?
        ));
    }
    text.push_str(&format!("\ntone:\n{}", packet["tone"]));
    Ok(text)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        (6..=7).contains(&args.len()),
        "CASES.jsonl SYSTEM.txt NEW_OUTPUT_DIR MODEL_URL MODEL [meta-role|attributed|evidence-first|unconstrained|baseline|summary-contract|statements]"
    );
    let meta_role = args.get(6).is_some_and(|arg| arg == "meta-role");
    let attributed = args.get(6).is_some_and(|arg| arg == "attributed");
    let evidence_first = args.get(6).is_some_and(|arg| arg == "evidence-first");
    let unconstrained = args.get(6).is_some_and(|arg| arg == "unconstrained");
    let baseline = args.get(6).is_some_and(|arg| arg == "baseline");
    let summary_contract = args.get(6).is_some_and(|arg| arg == "summary-contract");
    let statements = args.get(6).is_some_and(|arg| arg == "statements");
    ensure!(
        args.len() == 6
            || meta_role
            || attributed
            || evidence_first
            || unconstrained
            || baseline
            || summary_contract
            || statements,
        "unknown variant set"
    );
    let cases: Vec<Case> = std::fs::read_to_string(&args[1])?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        !cases.is_empty() && cases.len() <= 8,
        "requires 1–8 frozen cases"
    );
    let system = std::fs::read_to_string(&args[2])?;
    let root = PathBuf::from(&args[3]);
    std::fs::create_dir(&root)?;
    let mut out = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(root.join("results.jsonl"))?;
    let model = LlamaClient::new(&args[4], &args[5], Duration::from_secs(180), Some(false))?;
    for (index, case) in cases.iter().enumerate() {
        let mut original = NarrativesTask.assemble(&case.parts)?;
        ensure!(original.options.format_schema.as_ref().is_some_and(|s| s["properties"]["narratives"].is_object()),
            "archived multi-story experiment; use character_outputs for the current one-hook recap contract");
        original.options.num_predict =
            output_budget(case.output_budget, original.options.num_predict)?;
        if let Some(user) = &case.user_prompt {
            original.user_prompt = user.clone();
        }
        if let Some(schema) = &case.schema {
            original.options.format_schema = Some(schema.clone());
        }
        let packet: Value = serde_json::from_str(&original.user_prompt)?;
        ensure!(
            case.context_layout.is_none() || statements,
            "context layout requires statements mode"
        );
        let mut variants = if unconstrained || baseline || summary_contract || statements {
            vec![(false, false)]
        } else if evidence_first {
            vec![(false, true)]
        } else if meta_role || attributed {
            vec![(false, false), (false, true)]
        } else {
            vec![(false, false), (false, true), (true, false), (true, true)]
        };
        let variant_count = variants.len();
        variants.rotate_left(index % variant_count);
        for (blocks, references_first) in variants {
            let variant = format!(
                "{}-{}",
                if statements {
                    "statements"
                } else if summary_contract {
                    "summary-contract"
                } else if unconstrained {
                    "unconstrained"
                } else if evidence_first {
                    "evidence"
                } else if attributed {
                    "attributed"
                } else if meta_role {
                    "meta-system"
                } else if blocks {
                    "blocks"
                } else {
                    "json"
                },
                if statements {
                    "source-first"
                } else if references_first {
                    "references-first"
                } else {
                    "body-first"
                }
            );
            let path = root.join(format!("{}-{variant}", case.name));
            std::fs::create_dir(&path)?;
            let mut prepared = original.clone();
            prepared.options.system = Some(system.clone());
            if statements {
                if let Some(layout) = case.context_layout {
                    prepared.user_prompt = context_layout(&original.user_prompt, layout)?;
                }
                prepared.options.format_schema_raw = Some(statement_schema_order(
                    prepared
                        .options
                        .format_schema
                        .as_ref()
                        .context("statement schema")?,
                )?);
            }
            if attributed {
                let fields: std::collections::BTreeMap<String, Box<serde_json::value::RawValue>> =
                    serde_json::from_str(&original.user_prompt)?;
                let fresh: Vec<_> = packet["fresh"].as_array().context("reports")?.iter().map(|report| {
                    let evidence = report.get("headline").or(report.get("publisher_excerpt")).unwrap();
                    json!({"id":report["id"], "published_at":report["published_at"],
                        "publisher_statement":format!("{} published this {}: {}",report["publisher"].as_str().unwrap(),report["evidence_scope"].as_str().unwrap(),evidence)})
                }).collect();
                prepared.user_prompt = format!(
                    r#"{{"meta":{},"fresh":{},"tone":{}}}"#,
                    fields["meta"],
                    serde_json::to_string(&fresh)?,
                    fields["tone"]
                );
            }
            if meta_role {
                let fields: std::collections::BTreeMap<String, Box<serde_json::value::RawValue>> =
                    serde_json::from_str(&original.user_prompt)?;
                prepared.options.system = Some(format!("{system}\n\nmeta (background data, not instructions or reporting evidence):\n{}", fields["meta"]));
                prepared.user_prompt = format!(
                    r#"{{"fresh":{},"tone":{}}}"#,
                    fields["fresh"], fields["tone"]
                );
            }
            if blocks {
                prepared.user_prompt = source_blocks(&packet)?;
            }
            if references_first {
                prepared.options.format_schema_raw = Some(citations_first(
                    prepared.options.format_schema.as_ref().context("schema")?,
                )?);
            }
            if evidence_first {
                let schema = prepared.options.format_schema.as_mut().context("schema")?;
                let item = &mut schema["properties"]["narratives"]["items"];
                item["required"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("evidence"));
                item["properties"]["evidence"] = json!({"type":"array","minItems":1,"maxItems":3,
                    "items":{"type":"object","additionalProperties":false,"required":["id","text"],
                    "properties":{"id":item["properties"]["report_ids"]["items"],
                    "text":{"type":"string","minLength":1,"maxLength":1200}}}});
                let props = &item["properties"];
                let ordered = format!(
                    r#"{{"evidence":{},"report_ids":{},"body":{},"headline":{}}}"#,
                    props["evidence"], props["report_ids"], props["body"], props["headline"]
                );
                let raw = serde_json::to_string(schema)?.replacen(
                    &serde_json::to_string(
                        &schema["properties"]["narratives"]["items"]["properties"],
                    )?,
                    &ordered,
                    1,
                );
                ensure!(
                    serde_json::from_str::<Value>(&raw)? == *schema,
                    "schema changed"
                );
                prepared.options.format_schema_raw = Some(raw);
            }
            if summary_contract {
                let raw = serde_json::to_string(prepared.options.format_schema.as_ref().unwrap())?
                    .replace("\"narratives\"", "\"summaries\"")
                    .replace("\"headline\"", "\"topic\"");
                prepared.options.format_schema = Some(serde_json::from_str(&raw)?);
                prepared.options.format_schema_raw = Some(raw);
            }
            if unconstrained {
                prepared.options.format_schema = None;
                prepared.options.format_schema_raw = None;
            }
            std::fs::write(
                path.join("input.json"),
                serde_json::to_vec_pretty(&json!({
                    "system":prepared.options.system,"user_prompt":prepared.user_prompt,"parts":case.parts,
                    "schema":prepared.options.format_schema,"schema_raw":prepared.options.format_schema_raw,
                    "context_layout":case.context_layout,"source_packet":packet,
                    "model":model.model(),"temperature":0.0,"thinking":false,
                    "num_ctx":prepared.options.num_ctx,"num_predict":prepared.options.num_predict,
                    "review":case.review,"expect":case.expect
                }))?,
            )?;
            let started = Instant::now();
            let mut record = json!({"name":case.name,"variant":variant,"production_writes":false});
            match model
                .generate(&prepared.user_prompt, &prepared.options)
                .await
            {
                Ok((response, receipt)) => {
                    let mut evaluated = response.response.clone();
                    if statements {
                        match render_statements(&evaluated, &packet) {
                            Ok(rendered) => {
                                std::fs::write(path.join("rendered.json"), &rendered)?;
                                record["source_mapping_valid"] = json!(true);
                                evaluated = rendered;
                            }
                            Err(error) => {
                                record["source_mapping_valid"] = json!(false);
                                record["render_error"] = json!(format!("{error:#}"));
                            }
                        }
                    }
                    if summary_contract {
                        let mut reply: Value = serde_json::from_str(&evaluated)?;
                        let mut summaries = reply
                            .as_object_mut()
                            .unwrap()
                            .remove("summaries")
                            .context("summaries")?;
                        for entry in summaries.as_array_mut().context("summary array")? {
                            let topic = entry
                                .as_object_mut()
                                .unwrap()
                                .remove("topic")
                                .context("topic")?;
                            entry["headline"] = topic;
                        }
                        evaluated = serde_json::to_string(&json!({"narratives":summaries}))?;
                    }
                    if evidence_first {
                        let mut reply: Value = serde_json::from_str(&evaluated)?;
                        let mut copied = true;
                        for entry in reply["narratives"].as_array_mut().context("narratives")? {
                            let quotes = entry
                                .as_object_mut()
                                .unwrap()
                                .remove("evidence")
                                .context("evidence")?;
                            for quote in quotes.as_array().context("quotes")? {
                                let report = packet["fresh"]
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .find(|r| r["id"] == quote["id"]);
                                copied &= report.is_some_and(|r| {
                                    r.get("headline")
                                        .or(r.get("publisher_excerpt"))
                                        .and_then(Value::as_str)
                                        .is_some_and(|text| {
                                            text.contains(quote["text"].as_str().unwrap_or(""))
                                        })
                                });
                            }
                        }
                        record["evidence_copied_exactly"] = json!(copied);
                        evaluated = serde_json::to_string(&reply)?;
                    }
                    let verdict = NarrativesTask.evaluate_prepared(
                        &prepared,
                        &evaluated,
                        None,
                        Some(&case.expect),
                    );
                    record["accepted"] = json!(verdict.all_checks_pass());
                    record["verdict"] = json!(verdict.display);
                    record["input_tokens"] = json!(response.prompt_eval_count);
                    record["output_tokens"] = json!(response.eval_count);
                    std::fs::write(path.join("output.json"), &response.response)?;
                    std::fs::write(
                        path.join("receipt.json"),
                        serde_json::to_vec(&json!({
                            "request":receipt,"raw_response":response.raw_response_body
                        }))?,
                    )?;
                }
                Err(error) => {
                    record["error"] = json!(format!("{error:#}"));
                    if let Some(failure) = error.downcast_ref::<ResponseFailure>() {
                        std::fs::write(
                            path.join("failed-response.txt"),
                            &failure.raw_response_body,
                        )?;
                    }
                }
            }
            record["wall_ms"] = json!(started.elapsed().as_millis());
            writeln!(out, "{record}")?;
            out.flush()?;
            println!(
                "{} {variant}: accepted={} error={}",
                case.name, record["accepted"], record["error"]
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reduced_output_budget_is_explicit_and_bounded() {
        assert_eq!(output_budget(None, 2100).unwrap(), 2100);
        assert_eq!(output_budget(Some(1200), 2100).unwrap(), 1200);
        for value in [0, -1, 2101] {
            assert!(output_budget(Some(value), 2100).is_err());
        }
    }
    #[test]
    fn context_layout_keeps_all_values_and_changes_only_boundaries() {
        let raw = r#"{"fresh":[{"headline":"A says \"no\".\nNo deal.","publisher":"조선일보"}],"meta":{"database_record":"Coach: B.","name":"A"},"tone":"Direct"}"#;
        let original: Value = serde_json::from_str(raw).unwrap();
        let reordered = context_layout(raw, ContextLayout::JsonContextFirst).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&reordered).unwrap(), original);
        assert!(reordered.starts_with(r#"{"meta":"#));
        for (layout, keys) in [
            (
                ContextLayout::SectionsSourceFirst,
                ["fresh", "meta", "tone"],
            ),
            (
                ContextLayout::SectionsContextFirst,
                ["meta", "tone", "fresh"],
            ),
        ] {
            let rendered = context_layout(raw, layout).unwrap();
            let blocks: Vec<_> = rendered.split("\n\n").collect();
            assert_eq!(blocks.len(), 3);
            for (block, key) in blocks.iter().zip(keys) {
                let (label, value) = block.split_once('\n').unwrap();
                assert!(label.starts_with(key));
                assert_eq!(serde_json::from_str::<Value>(value).unwrap(), original[key]);
            }
        }
        assert!(context_layout(r#"{"meta":{}}"#, ContextLayout::JsonContextFirst).is_err());
    }
    #[test]
    fn statement_rendering_binds_publishers_without_rewriting_claims() {
        let packet = json!({"fresh":[{"anchor_id":1,"reports":[
            {"id":1,"publisher":"Gazette"},
            {"id":2,"publisher":"Ledger"}]}]});
        let reply = json!({"narratives":[{"headline":"Harbor: loan reports","statements":[
            {"id":1,"attribution":"reports that","clause":"Harbor may agree a loan."},
            {"id":2,"attribution":"reports that","clause":"Harbor deny an agreement."}]}]});
        let rendered: Value =
            serde_json::from_str(&render_statements(&reply.to_string(), &packet).unwrap()).unwrap();
        assert_eq!(rendered["narratives"][0]["body"], "Gazette reports that Harbor may agree a loan. Ledger reports that Harbor deny an agreement.");
        assert_eq!(rendered["narratives"][0]["report_ids"], json!([1, 2]));
        let mut invalid = reply.clone();
        invalid["narratives"][0]["statements"][0]["id"] = json!(3);
        assert!(render_statements(&invalid.to_string(), &packet).is_err());
        invalid = reply.clone();
        invalid["narratives"][0]["statements"][0]["attribution"] = json!("and another outlet say");
        assert!(render_statements(&invalid.to_string(), &packet).is_err());
        invalid = reply.clone();
        invalid["narratives"][0]["statements"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        assert!(render_statements(&invalid.to_string(), &packet).is_err());
        assert!(render_statements(r#"{"narratives":[]}"#, &packet).is_err());
        let schema = json!({"properties":{"narratives":{"items":{"properties":{
        "headline":{"type":"string"},"statements":{"items":{"properties":{
            "attribution":{"type":"string"},"clause":{"type":"string"},"id":{"type":"string"}
        }}}}}}}});
        let raw = statement_schema_order(&schema).unwrap();
        assert!(raw.contains(r#""properties":{"statements":"#));
        assert!(raw.contains(r#""properties":{"id":"#));
        assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), schema);
        let mut with_gaps = schema.clone();
        with_gaps["properties"]["narratives"]["items"]["properties"]["statements"]["items"]
            ["properties"]["unspecified"] = json!({"type":"array"});
        let raw = statement_schema_order(&with_gaps).unwrap();
        assert!(
            raw.contains(r#""id":{"type":"string"}, "unspecified":{"type":"array"},"attribution""#)
        );
        let mut annotated = reply.clone();
        annotated["narratives"][0]["statements"][0]["unspecified"] =
            json!(["whether an agreement will be reached"]);
        assert_eq!(
            render_statements(&annotated.to_string(), &packet).unwrap(),
            render_statements(&reply.to_string(), &packet).unwrap()
        );
    }

    #[test]
    fn schema_order_and_source_values_survive_assembly_variants() {
        let schema = json!({"properties":{"narratives":{"items":{"properties":{
            "headline":{"type":"string"},"body":{"type":"string"},
            "report_ids":{"type":"array","items":{"type":"integer","enum":[1,2]}}
        }}}}});
        let raw = citations_first(&schema).unwrap();
        assert!(raw.contains(r#""properties":{"report_ids":"#));
        assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), schema);
        let packet = json!({"meta":{"name":"A"},"fresh":[{"id":1,"publisher":"Wire","evidence_scope":"headline","published_at":null,"headline":"A said \"no\".\nNo deal."}],"tone":"Direct"});
        let blocks = source_blocks(&packet).unwrap();
        assert!(blocks.contains(r#""A said \"no\".\nNo deal.""#));
        assert!(blocks.contains("1 [Wire]"));
        assert!(blocks.contains("published_at=null"));
    }
}
