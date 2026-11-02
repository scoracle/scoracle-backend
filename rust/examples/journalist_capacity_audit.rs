//! Read-only production preparation and exact token admission; never generates or publishes.
//! SUBJECTS.jsonl SYSTEM.txt NEW_OUTPUT_DIR MODEL_URL MODEL
use anyhow::{ensure, Result};
use scoracle_cognition::{
    harness::{model::Inference, providers::llama::LlamaClient},
    plugins::journalist::{fresh, prompt},
    tools::meta::EntityMeta,
};
use serde_json::json;
use std::{io::Write, path::PathBuf, time::Duration};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 6,
        "SUBJECTS.jsonl SYSTEM.txt NEW_OUTPUT_DIR MODEL_URL MODEL"
    );
    let subjects: Vec<EntityMeta> = std::fs::read_to_string(&args[1])?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    ensure!(
        !subjects.is_empty() && subjects.len() <= 10,
        "requires 1–10 subjects"
    );
    let system = std::fs::read_to_string(&args[2])?;
    let root = PathBuf::from(&args[3]);
    std::fs::create_dir(&root)?;
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(root.join("results.jsonl"))?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET default_transaction_read_only=on")
                    .execute(&mut *connection)
                    .await?;
                sqlx::query("SET statement_timeout='15s'")
                    .execute(&mut *connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&std::env::var("JOURNALIST_REPLAY_DATABASE_URL")?)
        .await?;
    let model = LlamaClient::new(&args[4], &args[5], Duration::from_secs(60), Some(false))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    for subject in subjects {
        let mut record = json!({"subject":subject,"captured_at_epoch":now,"inference_calls":0,"production_writes":false});
        match fresh::load_narratives_material(&pool, subject, now).await {
            Err(error) => record["preparation_error"] = json!(format!("{error:#}")),
            Ok(material) => {
                record["sources"] = json!(material.sources.len());
                record["selected"] = json!(material.assignment.selected.len());
                record["parts"] = serde_json::to_value(prompt::Parts {
                    subject: material.assignment.subject.clone(),
                    reports: material.assignment.selected.clone(),
                    database_record: material.assignment.database_record.clone(),
                })?;
                record["dispositions"] = json!(material
                    .assignment
                    .dispositions
                    .iter()
                    .map(|d| json!({"article_id":d.article_id,"reason":d.reason}))
                    .collect::<Vec<_>>());
                let user = prompt::prompt(&material.assignment);
                let mut options = prompt::generation_options(&material.assignment, 10000);
                options.system = Some(system.clone());
                record["input"] = model.request_body(&user, &options);
                record["reserved_output"] = json!(options.num_predict);
                match model.prepare(&user, &options).await {
                    Ok(prepared) => record["prepared"] = serde_json::to_value(prepared)?,
                    Err(error) => record["admission_error"] = json!(format!("{error:#}")),
                }
            }
        }
        writeln!(output, "{record}")?;
        output.flush()?;
        println!(
            "{}: selected={} error={}",
            record["subject"]["name"],
            record["selected"],
            record
                .get("preparation_error")
                .or(record.get("admission_error"))
                .unwrap_or(&serde_json::Value::Null)
        );
    }
    Ok(())
}
