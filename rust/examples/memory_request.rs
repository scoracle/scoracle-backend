//! Execute a plugin-style memory request without generation or publication.
//! SCORACLE_MEMORY_DATABASE_URL=... SCORACLE_MEMORY_STUDY_BIN=... cargo run
//! --example memory_request -- REQUEST.json
use anyhow::{Context, Result};
use scoracle_cognition::plugins::{memories, meta::EntityMeta};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "study", rename_all = "snake_case")]
enum Request {
    Reporting {
        subject: EntityMeta,
        from: i64,
        before: i64,
        limit: usize,
        #[serde(default)]
        pair: Option<EntityMeta>,
        #[serde(default)]
        predicates: Vec<String>,
        #[serde(default)]
        exclude: Vec<i64>,
    },
    TeamStatistic {
        subject: EntityMeta,
        metric: String,
        league_id: i32,
        season: i32,
        from: i64,
        split: i64,
        before: i64,
    },
}
#[tokio::main]
async fn main() -> Result<()> {
    let path = std::env::args()
        .nth(1)
        .context("request JSON path required")?;
    let request: Request = serde_json::from_slice(&std::fs::read(path)?)?;
    let url = std::env::var("SCORACLE_MEMORY_DATABASE_URL")
        .context("SCORACLE_MEMORY_DATABASE_URL required")?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET default_transaction_read_only=on")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await?;
    let started = std::time::Instant::now();
    let result = match request {
        Request::Reporting {
            subject,
            from,
            before,
            limit,
            pair,
            predicates,
            exclude,
        } => serde_json::to_value(
            memories::reporting_scope(
                &pool,
                &subject,
                from,
                before,
                &exclude,
                limit,
                pair.as_ref(),
                &predicates,
            )
            .await?,
        )?,
        Request::TeamStatistic {
            subject,
            metric,
            league_id,
            season,
            from,
            split,
            before,
        } => serde_json::to_value(
            memories::statistic::team_matches(
                &pool, &subject, &metric, league_id, season, from, split, before,
            )
            .await?,
        )?,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"elapsed_ms":started.elapsed().as_millis(),"study":result})
        )?
    );
    Ok(())
}
