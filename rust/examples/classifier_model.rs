//! Exercise the Classifier's configured model plugin without database or production writes.
//! classifier_model SOURCES.jsonl RECEIPTS.jsonl
//! Configure the model through COGNITION_ROUTE_CLASSIFIER*; tuning lives in classifier/prompt.rs.
use anyhow::{ensure, Result};
use scoracle_cognition::{
    harness::{config::RouteConfig, route::Router},
    plugins::classifier::{self, Source},
};
use std::{collections::HashMap, io::Write, time::Duration};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    ensure!(
        args.len() == 3,
        "usage: classifier_model SOURCES.jsonl RECEIPTS.jsonl"
    );
    let base = std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into());
    let mut config = RouteConfig {
        roles: HashMap::new(),
        candidates: HashMap::new(),
        backend_concurrency: HashMap::new(),
    };
    classifier::prompt::configure(&mut config, &base);
    let router = Router::from_config(&config, Duration::from_secs(600), 1)?;
    let model = router.for_route(classifier::prompt::MODEL);
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    let mut failed = 0;
    let mut count = 0;
    for line in std::fs::read_to_string(&args[1])?
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        let source: Source = serde_json::from_str(line)?;
        ensure!(
            !source.query_entities.is_empty(),
            "source requires candidate identity"
        );
        for target in &source.query_entities {
            let receipt = classifier::adapter::measure(model.as_ref(), &source, target).await?;
            failed += usize::from(receipt["status"] == "error");
            count += 1;
            writeln!(output, "{receipt}")?;
            output.flush()?;
        }
    }
    ensure!(count > 0, "nonempty sources required");
    eprintln!("{count} receipts retained; {failed} failed; calibration unassessed");
    ensure!(
        failed == 0,
        "Classifier model returned failed receipts; inspect retained output"
    );
    Ok(())
}
