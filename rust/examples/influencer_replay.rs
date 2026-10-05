//! Export the production cognition package and check retained replies for form only.
use anyhow::Result;
use scoracle_cognition::plugins::influencer::{self, memories::HistoryItem, prompt};
use scoracle_cognition::plugins::{harvester::delivery::SourceContext, meta::EntityMeta};
use scoracle_cognition::studio::Parser;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{BufRead, Write};

#[derive(Deserialize)]
struct Case {
    key: String,
    subject: EntityMeta,
    source: SourceContext,
    #[serde(default)]
    history: Vec<HistoryItem>,
}
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let validate = args.get(1).is_some_and(|a| a == "--validate");
    let path = args
        .get(if validate { 2 } else { 1 })
        .ok_or_else(|| anyhow::anyhow!("provide a JSONL fixture or --validate response file"))?;
    let lines = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut stdout = std::io::stdout().lock();
    let mut checked = 0;
    let mut failures = 0;
    for line in lines.lines() {
        let line = line?;
        if validate {
            let row: Value = serde_json::from_str(&line)?;
            checked += 1;
            let parsed = influencer::VibeParser
                .parse(row["response"]["message"]["content"].as_str().unwrap_or(""));
            match parsed {
                Ok(_) if row["response"]["done"] == true => {
                    writeln!(stdout, "{}: form passed", row["key"])?;
                }
                Ok(_) => {
                    failures += 1;
                    writeln!(
                        stdout,
                        "{}: FAILED (done={})",
                        row["key"], row["response"]["done"]
                    )?;
                }
                Err(error) => {
                    failures += 1;
                    writeln!(stdout, "{}: FAILED ({error})", row["key"])?;
                }
            }
        } else {
            let case: Case = serde_json::from_str(&line)?;
            let assignment = prompt::Assignment {
                subject: case.subject,
                source: case.source,
                history: case.history,
                input_components_json: line.clone(),
                input_hash: hex::encode(Sha256::digest(line.as_bytes())),
            };
            let opts = prompt::generation_options(
                prompt::VIBE_TEMPERATURE,
                4096,
                prompt::VIBE_NUM_PREDICT,
            );
            let backend = scoracle_cognition::runtime::providers::ollama::OllamaClient::with_think(
                "http://127.0.0.1:11434",
                "alibayram/smollm3:latest",
                std::time::Duration::from_secs(120),
                Some(false),
            )?;
            let request = backend.request_body(&prompt::assembled_prompt(&assignment), &opts);
            writeln!(stdout, "{}", json!({"key":case.key,"request":request}))?;
        }
    }
    if validate {
        anyhow::ensure!(checked > 0, "no retained replies to validate");
        writeln!(
            stdout,
            "{} passed; {failures} failed; {checked} checked",
            checked - failures
        )?;
        anyhow::ensure!(
            failures == 0,
            "{failures} retained replies failed validation"
        );
    }
    Ok(())
}
