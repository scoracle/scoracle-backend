//! Offline native qualification boundary. No database, inference or publication.
//! Prepare: classifier_qualify SOURCE.jsonl REQUESTS.jsonl
//! Decode:  classifier_qualify SOURCE.jsonl REPLIES.jsonl RECEIPTS.jsonl
//! Replies require article_id, target, model, revision, request_sha256 and raw_response.
use anyhow::{ensure, Context, Result};
use scoracle_cognition::plugins::classifier::{self, Source};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;

#[derive(Deserialize)]
struct Reply {
    article_id: i64,
    target: Value,
    model: String,
    revision: String,
    request_sha256: String,
    raw_response: String,
    #[serde(flatten)]
    provenance: BTreeMap<String, Value>,
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 3 || args.len() == 4,
        "usage: classifier_qualify SOURCE.jsonl [REPLIES.jsonl] OUTPUT.jsonl"
    );
    let mut sources = BTreeMap::new();
    for line in std::fs::read_to_string(&args[1])?
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        let source: Source = serde_json::from_str(line)?;
        ensure!(
            !source.query_entities.is_empty(),
            "source has no candidate targets"
        );
        ensure!(
            sources.insert(source.article_id, source).is_none(),
            "duplicate canonical source"
        );
    }
    ensure!(!sources.is_empty(), "nonempty retained sources required");
    let replies = if args.len() == 4 {
        Some(
            std::fs::read_to_string(&args[2])?
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(serde_json::from_str::<Reply>)
                .collect::<std::result::Result<Vec<_>, _>>()?,
        )
    } else {
        None
    };
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(args.last().unwrap())?;
    let mut failures = 0;
    if let Some(replies) = replies {
        ensure!(
            !replies.is_empty(),
            "nonempty qualification replies required"
        );
        for reply in replies {
            let mut receipt = json!({"contract": "classifier-qualification-receipt-v1",
                "article_id": reply.article_id, "target": reply.target,
                "model": reply.model, "revision": reply.revision,
                "request_sha256": reply.request_sha256, "raw_response": reply.raw_response,
                "model_provenance": reply.provenance,
                "production_eligible": false, "semantic_review": null,
                "unknown": ["emotion scores", "relevance head", "topic head", "discourse head", "temporal head", "target ordinals", "item-wide routing"]});
            let result = (|| {
                let source = sources
                    .get(&reply.article_id)
                    .context("reply source missing")?;
                receipt["source"] = serde_json::to_value(source)?;
                let request = classifier::request(source, &reply.target)?;
                receipt["request"] = request.clone();
                ensure!(
                    !reply.model.trim().is_empty() && !reply.revision.trim().is_empty(),
                    "model and checkpoint identity required"
                );
                if let Some(status) = reply.provenance.get("inference_status") {
                    ensure!(
                        status == "complete",
                        "qualification inference did not complete"
                    );
                }
                ensure!(
                    hex::encode(Sha256::digest(request.to_string().as_bytes()))
                        == reply.request_sha256,
                    "source, target or qualification request drift"
                );
                let (record, _) = classifier::qualify(source, &reply.target, &reply.raw_response)?;
                let world = classifier::emotional_world(source, &record)?;
                receipt["qualification"] = serde_json::to_value(record)?;
                receipt["selection"] = world;
                Ok::<_, anyhow::Error>(())
            })();
            match result {
                Ok(()) => receipt["status"] = json!("source_bound_provisional"),
                Err(error) => {
                    receipt["status"] = json!("error");
                    receipt["error"] = json!(format!("{error:#}"));
                    failures += 1;
                }
            }
            writeln!(output, "{receipt}")?;
            output.flush()?;
        }
    } else {
        for source in sources.values() {
            for target in &source.query_entities {
                let request = classifier::request(source, target)?;
                writeln!(
                    output,
                    "{}",
                    json!({"contract": "classifier-qualification-request-v1",
                    "article_id": source.article_id, "target": target,
                    "request_sha256": hex::encode(Sha256::digest(request.to_string().as_bytes())), "request": request,
                    "token_budget_status": "unverified_do_not_submit", "production_eligible": false})
                )?;
            }
        }
    }
    ensure!(
        failures == 0,
        "{failures} qualification failures retained in output"
    );
    Ok(())
}
