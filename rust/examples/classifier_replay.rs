//! Read-only Kev calibration and throughput replay. No database writes or character dispatch.
//! CLASSIFIER_URL=http://localhost:8081 cargo run --release --example classifier_replay -- cases.jsonl 4
use anyhow::{ensure, Context, Result};
use futures::{stream, StreamExt};
use scoracle_cognition::plugins::classifier::decision::{self, Decision};
use serde::Deserialize;
use serde_json::json;
use std::{collections::BTreeMap, time::Instant};

#[derive(Deserialize)]
struct Case {
    id: String,
    target: String,
    body: String,
    #[serde(default)]
    expected: BTreeMap<String, bool>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .context("usage: classifier_replay CASES.jsonl [concurrency]")?;
    let concurrency: usize = args.next().as_deref().unwrap_or("1").parse()?;
    ensure!(
        concurrency > 0 && args.next().is_none(),
        "positive concurrency required"
    );
    let cases: Vec<Case> = std::fs::read_to_string(path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    ensure!(!cases.is_empty(), "empty calibration set");
    let model = Decision::from_env()?;
    let started = Instant::now();
    let mut pending = stream::iter(cases.into_iter().map(|case| {
        let model = model.clone();
        async move {
            let started = Instant::now();
            let spectrum = model.spectrum(&case.body, &case.target).await;
            (case, spectrum, started.elapsed().as_secs_f64())
        }
    }))
    .buffer_unordered(concurrency);
    let (mut completed, mut failed, mut labels, mut mistakes) = (0, 0, 0, 0);
    let mut squared_error = 0.0;
    let mut latencies = Vec::new();
    while let Some((case, result, elapsed)) = pending.next().await {
        match result {
            Ok(measurement) => {
                let spectrum = measurement.spectrum;
                let gate = decision::gate(&spectrum)?;
                for (key, expected) in &case.expected {
                    let probability = spectrum
                        .get(key)
                        .with_context(|| format!("unknown label {key}"))?
                        / 100.0;
                    labels += 1;
                    mistakes += usize::from((probability >= 0.5) != *expected);
                    squared_error += (probability - f64::from(u8::from(*expected))).powi(2);
                }
                completed += 1;
                latencies.push(elapsed);
                println!(
                    "{}",
                    json!({"id":case.id,"target":case.target,"seconds":elapsed,
                    "spectrum":spectrum,"gate":gate,"expected":case.expected,
                    "model":measurement.model,"request_hash":measurement.request_hash})
                );
            }
            Err(error) => {
                failed += 1;
                println!(
                    "{}",
                    json!({"id":case.id,"seconds":elapsed,"error":format!("{error:#}")})
                );
            }
        }
    }
    latencies.sort_by(f64::total_cmp);
    let elapsed = started.elapsed().as_secs_f64();
    eprintln!(
        "{}",
        json!({"criteria_version":decision::CRITERIA_VERSION,"concurrency":concurrency,
        "completed":completed,"failed":failed,"seconds":elapsed,"requests_per_second":completed as f64 / elapsed,
        "p95_seconds":latencies.get((latencies.len() * 95).div_ceil(100).saturating_sub(1)),
        "labels":labels,"mistakes_at_50":mistakes,
        "brier":if labels > 0 { Some(squared_error / labels as f64) } else { None }})
    );
    ensure!(failed == 0, "{failed} replay requests failed");
    Ok(())
}
