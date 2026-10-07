//! Complete source windows for Classifier measurement and review.
//! cargo run --example classifier_windows -- INPUT.jsonl OUTPUT.jsonl [MAX_WINDOWS]
use anyhow::{Context, Result};
use scoracle_cognition::tools::source::windows;
use serde_json::Value;
use std::io::{BufRead, Write};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 3 || args.len() == 4,
        "usage: classifier_windows INPUT.jsonl OUTPUT.jsonl [MAX_WINDOWS]"
    );
    let max_windows = args
        .get(3)
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(256);
    let input = std::io::BufReader::new(std::fs::File::open(&args[1])?);
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    for line in input.lines() {
        let mut item: Value = serde_json::from_str(&line?)?;
        let body = item["body"].as_str().context("retained body required")?;
        item["windows"] = serde_json::to_value(windows(body, max_windows)?)?;
        writeln!(output, "{item}")?;
    }
    Ok(())
}
