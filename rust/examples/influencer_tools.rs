//! Read-only pilot: DATABASE_URL=... cargo run --example influencer_tools -- SPORT team ID MODEL
use anyhow::{Context, Result};
use scoracle_cognition::{
    plugins::{influencer::cognition::research, meta::EntityMeta},
    runtime::providers::ollama::OllamaClient,
};
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        args.len() == 4,
        "usage: influencer_tools SPORT team|player ID MODEL"
    );
    anyhow::ensure!(
        matches!(args[1].as_str(), "team" | "player"),
        "expected team or player"
    );
    let url = std::env::var("DATABASE_PRIVATE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .context("set DATABASE_PRIVATE_URL or DATABASE_URL")?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET default_transaction_read_only=on")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("SET statement_timeout='15s'")
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await?;
    let sport = args[0].to_uppercase();
    let entity_id = args[2].parse()?;
    let name = scoracle_cognition::evidence::corpus::lookup_entity_name(
        &pool, &args[1], entity_id, &sport,
    )
    .await?;
    let subject = EntityMeta {
        name,
        entity_type: args[1].clone(),
        entity_id,
        sport,
    };
    let backend = OllamaClient::with_think(
        std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into()),
        &args[3],
        std::time::Duration::from_secs(120),
        Some(false),
    )?;
    let capture = research::read(&pool, &backend, &subject, 4096).await?;
    println!("{}", serde_json::to_string_pretty(&capture)?);
    anyhow::ensure!(
        capture["accepted"] == true,
        "pilot did not produce a structurally accepted answer; see capture"
    );
    Ok(())
}
