//! Re-parse saved character replies with current production factories and guards; no model or DB.
//! Usage: cargo run --example qualify_saved -- /private/path/to/saved-run
use anyhow::{Context, Result};
use scoracle_cognition::evaluation::tasks::{resolve_task, Fixture};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf};
fn main() -> Result<()> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .context("saved qualification directory required")?,
    );
    let mut entries = std::fs::read_dir(&root)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.path());
    let mut counts: BTreeMap<String, [usize; 3]> = BTreeMap::new();
    for entry in entries {
        let path = entry.path();
        if !path.join("fixture.json").is_file() {
            continue;
        }
        let fixture: Fixture = serde_json::from_slice(&std::fs::read(path.join("fixture.json"))?)?;
        let task = resolve_task(&fixture.task).context("unknown saved task")?;
        let prepared = match &fixture.parts {
            Some(parts) => task.assemble(parts)?,
            None => task.prepare_fixture(&fixture)?,
        };
        let old: Value = serde_json::from_str(&fixture.user_prompt)?;
        let current: Value = serde_json::from_str(&prepared.user_prompt)?;
        anyhow::ensure!(
            old["fresh"] == current["fresh"] && old["meta"] == current["meta"],
            "saved factual evidence changed: {}",
            fixture.name
        );
        let count = counts.entry(fixture.task.clone()).or_default();
        if !prepared.should_call {
            count[2] += 1;
            println!(
                "{}",
                json!({"task":fixture.task,"name":fixture.name,"status":"no_call"})
            );
            continue;
        }
        let raw = std::fs::read_to_string(path.join("output.json"))?;
        let verdict = task.evaluate_prepared(&prepared, &raw, None, Some(&fixture.expect));
        let accepted = verdict.parsed && verdict.all_checks_pass();
        count[if accepted { 0 } else { 1 }] += 1;
        println!(
            "{}",
            json!({"task":fixture.task,"name":fixture.name,"accepted":accepted,"checks":verdict.checks.iter().map(|check| json!({"name":check.name,"pass":check.pass,"detail":check.detail})).collect::<Vec<_>>()})
        );
    }
    anyhow::ensure!(!counts.is_empty(), "no saved fixtures found");
    println!(
        "{}",
        json!({"summary":counts,"columns":["mechanically_accepted","rejected","no_call"],"model_calls":0,"human_factual_approval":false,"note":"Saved replies were generated under their archived contracts; this is a current-parser regression replay, not model promotion."})
    );
    Ok(())
}
