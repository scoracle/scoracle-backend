//! Reuse production's exact publisher windows for every comparison model.
//! cargo run --example classifier_windows -- INPUT.jsonl OUTPUT.jsonl
use anyhow::{Context, Result};
use scoracle_cognition::plugins::harvester::cognition::prepare_text;
use serde_json::Value;
use std::io::{BufRead, Write};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 3,
        "usage: classifier_windows INPUT.jsonl OUTPUT.jsonl"
    );
    let input = std::io::BufReader::new(std::fs::File::open(&args[1])?);
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    for line in input.lines() {
        let mut item: Value = serde_json::from_str(&line?)?;
        let body = item["body"].as_str().context("retained body required")?;
        item["windows"] = serde_json::to_value(prepare_text(body)?.model_inputs)?;
        writeln!(output, "{item}")?;
    }
    Ok(())
}
