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
        /// Articles the caller already resolved. Empty is the subject-wide study.
        #[serde(default)]
        include: Vec<i64>,
        #[serde(default)]
        exclude: Vec<i64>,
        /// Group the study over canonical article blocks instead of one topic
        /// per canonical article. A plugin supplies its own grouping function;
        /// this is the shape of it for a manual request.
        #[serde(default)]
        group_by_canonical_block: bool,
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
            include,
            exclude,
            group_by_canonical_block,
        } => {
            let by_block = |o: &memories::Observation| {
                group_by_canonical_block.then(|| format!("block/{}", o.canonical_id / 100))
            };
            let topic: Option<memories::Topic<'_>> = group_by_canonical_block.then_some(&by_block);
            serde_json::to_value(
                memories::reporting_scope(
                    &pool, &subject, from, before, &exclude, limit, &include, topic,
                )
                .await?,
            )?
        }
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
