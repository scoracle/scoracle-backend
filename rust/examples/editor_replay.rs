//! Read-only Harvester -> Editor corpus replay. No queue or database publication.
//! EDITOR_REPLAY_DATABASE_URL, HARVESTER_MODEL_ENDPOINT and EDITOR_MODEL_ENDPOINT required.
use anyhow::{ensure, Result};
use scoracle_cognition::plugins::{editor, harvester, system_one};
use scoracle_cognition::tools::meta::{lookup_entity_name, EntityMeta};
use serde::Deserialize;
use serde_json::json;
use std::io::Write;

#[derive(Deserialize)]
struct Case {
    article_id: i64,
    entity_type: String,
    entity_id: i32,
    sport: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 3,
        "usage: editor_replay CASES.json OUTPUT.jsonl"
    );
    let cases: Vec<Case> = serde_json::from_str(&std::fs::read_to_string(&args[1])?)?;
    let connect: sqlx::postgres::PgConnectOptions =
        std::env::var("EDITOR_REPLAY_DATABASE_URL")?.parse()?;
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
    ensure!(readonly == "on", "replay database must be read-only");
    let headlines = system_one::bind(harvester::prompt::MODEL_ENDPOINT_ENV)?;
    let articles = system_one::bind(editor::prompt::MODEL_ENDPOINT_ENV)?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    for case in cases {
        let subject = EntityMeta {
            name: lookup_entity_name(&pool, &case.entity_type, case.entity_id, &case.sport).await?,
            entity_type: case.entity_type,
            entity_id: case.entity_id,
            sport: case.sport,
        };
        let (title, source, url, date, body): (String, String, String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT title, COALESCE(source,''), url, published_at::text, full_text FROM news_articles WHERE id=$1"
        ).bind(case.article_id).fetch_one(&pool).await?;
        let article = harvester::Article {
            article_id: case.article_id,
            title,
            source,
            url,
            published_at: date,
            body: body.unwrap_or_default(),
            hypothesis: subject,
            feed_rank: None,
        };
        let result = async {
            let gate = harvester::context::classify_headline(headlines.as_ref(), &article).await?;
            harvester::context::classify_after_headline(articles.as_ref(), &article, &gate).await
        }
        .await;
        let record = match result {
            Ok(read) => {
                json!({"article_id": case.article_id, "database_read_only": true, "receipt": read})
            }
            Err(error) => {
                json!({"article_id": case.article_id, "database_read_only": true, "error": format!("{error:#}")})
            }
        };
        writeln!(output, "{record}")?;
    }
    Ok(())
}
