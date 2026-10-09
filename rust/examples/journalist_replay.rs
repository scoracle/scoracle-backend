//! Read-only Journalist preparation/articulation replay; no publication.
//! Preparation is offline. Articulation also requires JOURNALIST_REPLAY_DATABASE_URL
//! for the pure SQL source-activity query (no tables or product writes).
//! cargo run --example journalist_replay -- INPUT.jsonl OUTPUT.jsonl [OLLAMA_URL [auto|true|false|compare [schema|unconstrained [MODEL]]]]
use anyhow::{Context, Result};
use scoracle_cognition::harness::model::{
    GenerateOptions, GenerateResult, Inference, ResponseFailure,
};
use scoracle_cognition::harness::providers::ollama::OllamaClient;
use scoracle_cognition::harness::Studio;
use scoracle_cognition::plugins::journalist::{self, prompt};
use scoracle_cognition::tools::meta::EntityMeta;
use serde::Deserialize;
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// Observe the production provider without changing its request, output or parser.
// Keep final content and reasoning measurements; do not put reasoning into prose.
struct RecordedModel {
    inner: OllamaClient,
    unconstrained: bool,
    calls: Mutex<Vec<serde_json::Value>>,
}
#[async_trait::async_trait]
impl Inference for RecordedModel {
    async fn generate(
        &self,
        prompt: &str,
        options: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)> {
        let options = self.options(options);
        let started = Instant::now();
        let result = Inference::generate(&self.inner, prompt, &options).await;
        let mut record = json!({
            "request": self.inner.request_body(prompt, &options),
            "elapsed_ms": started.elapsed().as_millis(),
        });
        match &result {
            Ok((response, _)) => record_response(&mut record, &response.raw_response_body),
            Err(error) => {
                record["error"] = json!(format!("{error:#}"));
                if let Some(failure) = error.downcast_ref::<ResponseFailure>() {
                    record_response(&mut record, &failure.raw_response_body);
                }
            }
        }
        self.calls.lock().unwrap().push(record);
        result
    }
    fn model(&self) -> &str {
        self.inner.model()
    }
    fn request_body(&self, prompt: &str, options: &GenerateOptions) -> serde_json::Value {
        self.inner.request_body(prompt, &self.options(options))
    }
}
// Preserve incomplete replies and provider metrics, keeping reasoning out of prose.
fn record_response(record: &mut serde_json::Value, body: &str) {
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(mut raw) => {
            if let Some(message) = raw
                .get_mut("message")
                .and_then(serde_json::Value::as_object_mut)
            {
                let thinking = message.remove("thinking");
                record["thinking_chars"] = json!(thinking
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .map_or(0, |t| t.chars().count()));
            }
            record["response"] = raw;
        }
        Err(_) => record["raw_response_body"] = json!(body),
    }
}

impl RecordedModel {
    fn options(&self, options: &GenerateOptions) -> GenerateOptions {
        let mut options = options.clone();
        if self.unconstrained {
            options.format_schema = None;
            options.format_schema_raw = None;
            options.json_mode = false;
        }
        options
    }
}

#[derive(Deserialize)]
struct Case {
    name: String,
    subject: EntityMeta,
    reports: Vec<prompt::CorpusItem>,
    now: i64,
    #[serde(default)]
    prior_reported: Vec<String>,
    #[serde(default, alias = "memories")]
    published_reports: Vec<prompt::CorpusItem>,
}
#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    anyhow::ensure!(
        (3..=7).contains(&args.len()),
        "usage: journalist_replay INPUT.jsonl OUTPUT.jsonl [OLLAMA_URL [auto|true|false|compare [schema|unconstrained [MODEL]]]]"
    );
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&args[2])?;
    let modes: Vec<Option<bool>> = match args.get(4).map(String::as_str).unwrap_or("auto") {
        "auto" => vec![None],
        "true" => vec![Some(true)],
        "false" => vec![Some(false)],
        "compare" => vec![Some(false), Some(true)],
        mode => anyhow::bail!("unknown thinking mode: {mode}"),
    };
    let unconstrained = match args.get(5).map(String::as_str).unwrap_or("schema") {
        "schema" => false,
        "unconstrained" => true,
        mode => anyhow::bail!("unknown decoding mode: {mode}"),
    };
    let model = args
        .get(6)
        .map(String::as_str)
        .unwrap_or("alibayram/smollm3");
    let models = args
        .get(3)
        .map(|url| {
            modes
                .iter()
                .map(|&think| {
                    Ok(RecordedModel {
                        inner: OllamaClient::with_think(
                            url,
                            model,
                            Duration::from_secs(120),
                            think,
                        )?,
                        unconstrained,
                        calls: Mutex::new(Vec::new()),
                    })
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?;
    let activity_pool = if models.is_some() {
        Some(sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(
            &std::env::var("JOURNALIST_REPLAY_DATABASE_URL")
                .context("articulation replay needs JOURNALIST_REPLAY_DATABASE_URL for SQL-owned activity")?
        ).await?)
    } else {
        None
    };
    for (case_index, line) in BufReader::new(std::fs::File::open(&args[1])?)
        .lines()
        .enumerate()
    {
        let case: Case = serde_json::from_str(&line?)?;
        let published_reports = case
            .published_reports
            .into_iter()
            .chain(
                case.prior_reported
                    .into_iter()
                    .map(|context| prompt::CorpusItem {
                        classifier_world: None,
                        id: 0,
                        title: String::new(),
                        context,
                        source: String::new(),
                        published_at_epoch: None,
                    }),
            )
            .collect::<Vec<_>>();
        let assignment = prompt::prepare(case.subject, case.reports, &published_reports, case.now)
            .context(case.name.clone())?;
        let base_record = json!({"name":case.name, "input_hash":assignment.input_hash,
            "dispositions":assignment.dispositions.iter().map(|d|json!({"article_id":d.article_id,"reason":d.reason})).collect::<Vec<_>>(),
            "deferred":assignment.deferred_ids, "prompt":prompt::prompt(&assignment),
            "system":prompt::TASK});
        // Alternate which mode runs first to reduce a fixed cache/order advantage.
        let mut order = (0..modes.len()).collect::<Vec<_>>();
        if case_index % 2 == 1 {
            order.reverse();
        }
        for index in order {
            let mut record = base_record.clone();
            record["think"] = json!(modes[index]);
            record["decoding"] = json!(if unconstrained {
                "unconstrained"
            } else {
                "schema"
            });
            if let Some(models) = &models {
                let model = &models[index];
                match journalist::create(
                    activity_pool.as_ref().unwrap(),
                    &Studio::new(model),
                    &assignment,
                    case.now,
                    4096,
                )
                .await
                {
                    Ok(result) => {
                        record["model"] = json!(result.provenance.model_version);
                        record["called"] = json!(result.was_called());
                        record["headline"] = json!(result.headline);
                        record["card_score"] = json!(result.card_score);
                        record["reports"] = json!(result
                            .narratives
                            .iter()
                            .map(|n| json!({"title":n.title,"body":n.body,"ids":n.input_news_ids}))
                            .collect::<Vec<_>>());
                        record["metrics"] = result.context_budget(json!({}));
                    }
                    Err(e) => record["error"] = json!(format!("{e:#}")),
                }
                record["calls"] = json!(std::mem::take(&mut *model.calls.lock().unwrap()));
            }
            writeln!(output, "{record}")?;
            output.flush()?;
            eprintln!("{} think={:?}", case.name, modes[index]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_provider_reply_keeps_partial_content_metrics_and_completion_reason() {
        let mut record = json!({});
        record_response(
            &mut record,
            r#"{"done_reason":"length","eval_count":900,"message":{"content":"{\"report_1\":","thinking":"fixture reasoning"}}"#,
        );
        assert_eq!(record["response"]["done_reason"], "length");
        assert_eq!(record["response"]["eval_count"], 900);
        assert_eq!(record["response"]["message"]["content"], "{\"report_1\":");
        assert!(record["response"]["message"].get("thinking").is_none());
        assert_eq!(record["thinking_chars"], 17);
        record_response(&mut record, "HTTP failure without JSON");
        assert_eq!(record["raw_response_body"], "HTTP failure without JSON");
    }
}
