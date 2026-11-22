//! Read-only native decision calibration and throughput replay. No database writes or character dispatch.
//! CLASSIFIER_URL=http://localhost:8081 cargo run --release --example classifier_replay -- cases.jsonl 4
use anyhow::{ensure, Context, Result};
use futures::{stream, StreamExt};
use scoracle_cognition::harness::skills::classifier::decision::{self, Decision};
use serde::Deserialize;
use serde_json::json;
use std::{collections::BTreeMap, time::Instant};

#[derive(Deserialize)]
struct Case {
    id: String,
    sport: String,
    headline: String,
    publisher: String,
    meta: serde_json::Value,
    #[serde(default)]
    expected: BTreeMap<String, bool>,
    #[serde(default)]
    replay_spectrum: Option<BTreeMap<String, f64>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .context("usage: classifier_replay CASES.jsonl [concurrency|--requests-only]")?;
    let mode = args.next();
    let export = mode.as_deref() == Some("--requests-only");
    let routes_only = mode.as_deref() == Some("--routes-only");
    let concurrency: usize = if export || routes_only {
        1
    } else {
        mode.as_deref().unwrap_or("1").parse()?
    };
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
    if routes_only {
        for case in cases {
            let scores = case
                .replay_spectrum
                .context("--routes-only requires replay_spectrum")?;
            println!(
                "{}",
                json!({"id":case.id,"gate":decision::gate_for_target(&scores,&case.headline,&case.meta)?})
            );
        }
        return Ok(());
    }
    if export {
        for case in cases {
            println!(
                "{}",
                json!({"id":case.id,"headline":case.headline,"publisher":case.publisher,"meta":case.meta,"sport":case.sport,"expected":case.expected,
                "request_version":decision::REQUEST_VERSION,"criteria_version":decision::CRITERIA_VERSION,
                "request":scoracle_cognition::harness::skills::classifier::request::request(&case.headline,&case.publisher,&case.sport,&case.meta)?})
            );
        }
        return Ok(());
    }
    let model = Decision::from_env()?;
    let started = Instant::now();
    let mut pending = stream::iter(cases.into_iter().map(|case| {
        let model = model.clone();
        async move {
            let started = Instant::now();
            let spectrum = model
                .spectrum(&case.headline, &case.publisher, &case.sport, &case.meta)
                .await;
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
                let gate = decision::gate_for_target(&spectrum, &case.headline, &case.meta)?;
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
                    json!({"id":case.id,"sport":case.sport,"seconds":elapsed,
                    "spectrum":spectrum,"admitted":gate["admitted"],"gate":gate,"expected":case.expected,
                    "model":measurement.model,"request_hash":measurement.request_hash,"receipt":measurement.receipt})
                );
            }
            Err(error) => {
                failed += 1;
                println!(
                    "{}",
                    json!({"id":case.id,"seconds":elapsed,"error":format!("{error:#}"),
                    "raw_response":error.downcast_ref::<scoracle_cognition::harness::model::ResponseFailure>().map(|failure| &failure.raw_response_body)})
                );
            }
        }
    }
    latencies.sort_by(f64::total_cmp);
    let elapsed = started.elapsed().as_secs_f64();
    eprintln!(
        "{}",
        json!({"criteria_version":decision::CRITERIA_VERSION,"request_version":decision::REQUEST_VERSION,"concurrency":concurrency,
        "completed":completed,"failed":failed,"seconds":elapsed,"requests_per_second":completed as f64 / elapsed,
        "p95_seconds":latencies.get((latencies.len() * 95).div_ceil(100).saturating_sub(1)),
        "labels":labels,"mistakes_at_50_percent":mistakes,
        "brier":if labels > 0 { Some(squared_error / labels as f64) } else { None }})
    );
    ensure!(failed == 0, "{failed} replay requests failed");
    Ok(())
}
