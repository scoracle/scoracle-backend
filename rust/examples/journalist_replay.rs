//! Read-only Journalist preparation/articulation replay; no database or publication.
//! cargo run --example journalist_replay -- INPUT.jsonl OUTPUT.jsonl [OLLAMA_URL [auto|true|false|compare [schema|unconstrained [MODEL]]]]
use anyhow::{Context, Result};
use scoracle_cognition::plugins::{journalist::cognition as journalist, meta::EntityMeta};
use scoracle_cognition::runtime::providers::ollama::OllamaClient;
use scoracle_cognition::studio::model::{GenerateOptions, GenerateResult, Inference};
use scoracle_cognition::studio::Studio;
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
            Ok((response, _)) => {
                let mut raw: serde_json::Value = serde_json::from_str(&response.raw_response_body)?;
                raw["message"].as_object_mut().unwrap().remove("thinking");
                record["response"] = raw;
                record["thinking_chars"] = json!(response.thinking.chars().count());
            }
            Err(error) => record["error"] = json!(format!("{error:#}")),
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
    reports: Vec<journalist::CorpusItem>,
    now: i64,
    #[serde(default)]
    prior_reported: Vec<String>,
    #[serde(default)]
    memories: Vec<journalist::CorpusItem>,
    #[serde(default)]
    memory_study: Option<scoracle_cognition::plugins::memories::Study>,
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
    for (case_index, line) in BufReader::new(std::fs::File::open(&args[1])?)
        .lines()
        .enumerate()
    {
        let case: Case = serde_json::from_str(&line?)?;
        let memory = journalist::Continuity {
            published_reports: case
                .memories
                .into_iter()
                .chain(
                    case.prior_reported
                        .into_iter()
                        .map(|context| journalist::CorpusItem {
                            id: 0,
                            title: String::new(),
                            context,
                            source: String::new(),
                            published_at_epoch: None,
                        }),
                )
                .collect(),
            previous_score: None,
            study: case.memory_study,
        };
        let assignment = journalist::prepare(case.subject, case.reports, &memory, case.now)
            .context(case.name.clone())?;
        let base_record = json!({"name":case.name, "input_hash":assignment.input_hash,
            "dispositions":assignment.dispositions.iter().map(|d|json!({"article_id":d.article_id,"reason":d.reason})).collect::<Vec<_>>(),
            "deferred":assignment.deferred_ids, "prompt":journalist::prompt(&assignment),
            "system":journalist::system_prompt(&assignment)});
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
                match journalist::create(&Studio::new(model), &assignment, case.now, 4096).await {
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
