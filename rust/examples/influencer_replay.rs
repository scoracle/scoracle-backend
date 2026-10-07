//! Read-only current-corpus Influencer replay, with exact inputs and model receipts.
//! INFLUENCER_REPLAY_DATABASE_URL and SCORACLE_MEMORY_STUDY_BIN are required.
//! cargo run --example influencer_replay -- CASES.json OUTPUT.jsonl [OLLAMA_URL MODEL]
//! Recorded evidence needs no database: --recorded INPUT.jsonl OUTPUT.jsonl OLLAMA_URL MODEL
use anyhow::{ensure, Context, Result};
use scoracle_cognition::harness::model::{GenerateOptions, GenerateResult, Inference};
use scoracle_cognition::harness::providers::ollama::OllamaClient;
use scoracle_cognition::plugins::{harvester::delivery, influencer};
use scoracle_cognition::tools::meta::{lookup_entity_name, EntityMeta};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Deserialize)]
struct Case {
    entity_type: String,
    entity_id: i32,
    sport: String,
    article_id: i64,
}

struct RecordedModel {
    inner: OllamaClient,
    calls: Mutex<Vec<Value>>,
}
#[async_trait::async_trait]
impl Inference for RecordedModel {
    async fn generate(
        &self,
        prompt: &str,
        options: &GenerateOptions,
    ) -> Result<(GenerateResult, Value)> {
        let result = self.inner.generate(prompt, options).await;
        let mut call = json!({"request": self.inner.request_body(prompt, options)});
        match &result {
            Ok((reply, _)) => call["raw_response_body"] = json!(reply.raw_response_body),
            Err(error) => call["error"] = json!(format!("{error:#}")),
        }
        self.calls.lock().unwrap().push(call);
        result
    }
    fn model(&self) -> &str {
        self.inner.model()
    }
    fn request_body(&self, prompt: &str, options: &GenerateOptions) -> Value {
        self.inner.request_body(prompt, options)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--recorded") {
        ensure!(
            args.len() == 6,
            "usage: --recorded INPUT.jsonl OUTPUT.jsonl OLLAMA_URL MODEL"
        );
        return replay_recorded(&args[2], &args[3], &args[4], &args[5]).await;
    }
    ensure!(
        args.len() == 3 || args.len() == 5,
        "usage: influencer_replay CASES.json OUTPUT.jsonl [OLLAMA_URL MODEL]"
    );
    let cases: Vec<Case> = serde_json::from_str(&std::fs::read_to_string(&args[1])?)?;
    let connect: sqlx::postgres::PgConnectOptions = std::env::var("INFLUENCER_REPLAY_DATABASE_URL")
        .context("INFLUENCER_REPLAY_DATABASE_URL is required")?
        .parse()?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(connect.options([
            ("default_transaction_read_only", "on"),
            ("statement_timeout", "15000"),
        ]))
        .await?;
    let readonly: String = sqlx::query_scalar("SHOW transaction_read_only")
        .fetch_one(&pool)
        .await?;
    ensure!(
        readonly == "on",
        "replay requires read-only database transactions"
    );
    let model = if args.len() == 5 {
        Some(RecordedModel {
            inner: OllamaClient::with_think(
                &args[3],
                &args[4],
                Duration::from_secs(120),
                Some(false),
            )?,
            calls: Mutex::new(Vec::new()),
        })
    } else {
        None
    };
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&args[2])?;
    for case in cases {
        let now = influencer::now();
        let mut record = json!({"article_id":case.article_id,"captured_at":now,
            "prompt_version":influencer::prompt::VIBE_PROMPT_VERSION,"database_read_only":true});
        let result: Result<()> = async {
            let subject = EntityMeta {
                name: lookup_entity_name(&pool, &case.entity_type, case.entity_id, &case.sport)
                    .await?,
                entity_type: case.entity_type.clone(),
                entity_id: case.entity_id,
                sport: case.sport.clone(),
            };
            let sources = delivery::load_for_character(
                &pool,
                influencer::manifest::MANIFEST.id.as_str(),
                &case.entity_type,
                case.entity_id,
                &case.sport,
            )
            .await?;
            let source = sources
                .iter()
                .find(|s| s.article_id == case.article_id)
                .context("requested source is not an eligible pending delivery")?;
            let (assignment, disposition) =
                influencer::prompt::prepare_assignment(&pool, subject, source, now).await?;
            record["disposition"] = disposition;
            if let Some(assignment) = assignment {
                record["world"] =
                    serde_json::from_str(&influencer::prompt::assembled_prompt(&assignment))?;
                record["input_components"] =
                    serde_json::from_str(&assignment.input_components_json)?;
                record["input_hash"] = json!(assignment.input_hash);
                if let Some(model) = &model {
                    let (product, receipt) = influencer::create(model, &assignment, 4096).await?;
                    record["receipt"] = receipt;
                    record["headline"] = json!(product.as_ref().and_then(|p| p.hook.clone()));
                    record["score"] = json!(product.as_ref().and_then(|p| p.sentiment));
                    record["body"] = json!(product.and_then(|p| p.vibe_prompt.clone()));
                }
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            record["error"] = json!(format!("{error:#}"));
        }
        if let Some(model) = &model {
            record["calls"] = json!(std::mem::take(&mut *model.calls.lock().unwrap()));
        }
        writeln!(output, "{record}")?;
        output.flush()?;
        println!(
            "article={} history={} error={}",
            case.article_id,
            record["world"]["memories"].as_array().map_or(0, Vec::len),
            record["error"].as_str().unwrap_or("none")
        );
    }
    Ok(())
}

/// Hold retained evidence constant while exercising the current production creation path.
async fn replay_recorded(input: &str, output: &str, url: &str, model: &str) -> Result<()> {
    let model = RecordedModel {
        inner: OllamaClient::with_think(url, model, Duration::from_secs(120), Some(false))?,
        calls: Mutex::new(Vec::new()),
    };
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output)?;
    for line in std::fs::read_to_string(input)?
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        let retained: Value = serde_json::from_str(line)?;
        let parts: influencer::prompt::Parts =
            serde_json::from_value(retained["input_components"].clone())?;
        let assignment = influencer::prompt::Assignment::from_parts(parts)?;
        let mut record = json!({"article_id":assignment.parts.sources[0].article_id,
            "name":retained["name"],"review":retained["review"],
            "recorded_evidence":true,"captured_at":influencer::now(),
            "prompt_version":influencer::prompt::VIBE_PROMPT_VERSION,
            "world":serde_json::from_str::<Value>(&influencer::prompt::assembled_prompt(&assignment))?});
        match influencer::create(&model, &assignment, 4096).await {
            Ok((product, receipt)) => {
                record["receipt"] = receipt;
                record["headline"] = json!(product.as_ref().and_then(|p| p.hook.clone()));
                record["score"] = json!(product.as_ref().and_then(|p| p.sentiment));
                record["body"] = json!(product.and_then(|p| p.vibe_prompt.clone()));
            }
            Err(error) => record["error"] = json!(format!("{error:#}")),
        }
        record["calls"] = json!(std::mem::take(&mut *model.calls.lock().unwrap()));
        writeln!(output, "{record}")?;
        output.flush()?;
        println!(
            "article={} error={}",
            assignment.parts.sources[0].article_id,
            record["error"].as_str().unwrap_or("none")
        );
    }
    Ok(())
}
