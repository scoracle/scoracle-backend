//! Read-only corpus replay. No database credentials, queue, or publication handles.
//! cargo run --example context_harvest -- INPUT.jsonl OUTPUT.jsonl [ENDPOINT]
//! With no endpoint, emit both prepared cascade requests without inference.
use anyhow::{Context, Result};
use scoracle_cognition::application::plugins::build_harvester;
use scoracle_cognition::plugins::harvester::{cognition, Article};
use serde_json::json;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(
        (3..=4).contains(&args.len()),
        "usage: context_harvest INPUT.jsonl OUTPUT.jsonl [ENDPOINT]"
    );
    let input = BufReader::new(File::open(&args[1])?);
    // Never overwrite a prior run's evidence.
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    let harvester = args
        .get(3)
        .map(|url| build_harvester(url.clone()))
        .transpose()?;
    let started = Instant::now();
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for (i, line) in input.lines().enumerate() {
        let article: Article =
            serde_json::from_str(&line?).with_context(|| format!("input line {}", i + 1))?;
        let start = Instant::now();
        let result = async {
            match &harvester {
                Some(plugin) => plugin.harvest(&article).await,
                None => {
                    let (prepared_text, relevance) = cognition::prepare_relevance(&article)?;
                    let character_routing =
                        cognition::prepare_character_routing(&article, &prepared_text);
                    Ok(
                        json!({"article_id":article.article_id,"prepared_text":prepared_text,
                        "requests":{"relevance":relevance,"character_routing":character_routing},
                        "disposition":"prepared"}),
                    )
                }
            }
        }
        .await;
        let mut record = result.unwrap_or_else(|e| json!({"article_id":article.article_id,"disposition":"error","error":format!("{e:#}")}));
        record["elapsed_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
        *counts
            .entry(record["disposition"].as_str().unwrap().to_string())
            .or_default() += 1;
        writeln!(output, "{}", serde_json::to_string(&record)?)?;
        output.flush()?;
        if (i + 1) % 10 == 0 {
            eprintln!("{} articles: {:?}", i + 1, counts);
        }
    }
    eprintln!(
        "completed in {:.2}s: {:?}",
        started.elapsed().as_secs_f64(),
        counts
    );
    Ok(())
}
