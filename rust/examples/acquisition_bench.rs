//! Live acquisition replay on an isolated database. Never runs models or production queues.
//! BENCH_DATABASE_URL=postgres://.../classifier_bench_* cargo run --example acquisition_bench -- 4 1200
use anyhow::{ensure, Context, Result};
use futures::{stream::FuturesUnordered, StreamExt};
use scoracle_cognition::{
    harness::{
        plugin::{PluginOutcome, StudioPlugin},
        queue::work,
        tools::WebBroker,
    },
    plugins::classifier::{adapter::AcquireHandler, manifest},
};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let concurrency: usize = args.next().as_deref().unwrap_or("4").parse()?;
    let seconds: u64 = args.next().as_deref().unwrap_or("1200").parse()?;
    ensure!(
        (1..=16).contains(&concurrency) && seconds > 0 && args.next().is_none(),
        "usage: acquisition_bench [concurrency 1..16] [seconds]"
    );
    let pool = PgPoolOptions::new()
        .max_connections(25)
        .connect(&std::env::var("BENCH_DATABASE_URL").context("BENCH_DATABASE_URL required")?)
        .await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    ensure!(
        database.starts_with("classifier_bench_"),
        "only an isolated classifier_bench_* database is allowed"
    );
    let initial: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE stage='classifier_acquire'")
            .fetch_one(&pool)
            .await?;
    ensure!(initial > 0, "seed an acquisition cohort first");
    let handler = Arc::new(AcquireHandler::new(
        pool.clone(),
        Arc::new(WebBroker::new(0)?),
    ));
    let started = Instant::now();
    let mut pending = FuturesUnordered::new();
    let mut infrastructure_errors = 0;
    loop {
        let remaining = Duration::from_secs(seconds).saturating_sub(started.elapsed());
        if remaining.is_zero() {
            break;
        }
        let slots = concurrency.saturating_sub(pending.len());
        if slots > 0 {
            for item in work::claim_with_policy(
                &pool,
                manifest::ACQUIRE_TASK,
                manifest::ACQUIRE_MANIFEST.claim_policy,
                slots as i64,
            )
            .await?
            {
                let handler = handler.clone();
                pending.push(async move {
                    let attempt = Instant::now();
                    let result =
                        tokio::time::timeout(Duration::from_secs(1200), handler.execute(&item))
                            .await;
                    (item, result, attempt.elapsed())
                });
            }
        }
        if pending.is_empty() {
            let outstanding: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pipeline_work WHERE stage='classifier_acquire' AND attempts<$1",
            )
            .bind(work::MAX_ATTEMPTS)
            .fetch_one(&pool)
            .await?;
            if outstanding == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1).min(remaining)).await;
            continue;
        }
        let Ok(Some((item, result, elapsed))) =
            tokio::time::timeout(remaining, pending.next()).await
        else {
            break;
        };
        match result {
            Ok(Ok(PluginOutcome::Deferred {
                delay,
                note,
                made_progress,
            })) => {
                ensure!(made_progress, "deferral without durable progress");
                ensure!(
                    work::defer(&pool, &item, delay, &note).await?,
                    "deferral lost its claim"
                );
                println!(
                    "{}",
                    json!({"article_id":item.entity_id,"sport":item.sport,"outcome":"deferred","seconds":elapsed.as_secs_f64(),"retry_in_seconds":delay.as_secs()})
                );
            }
            Ok(Ok(PluginOutcome::Committed)) => {
                let row: (String,String) = sqlx::query_as("SELECT status,reason FROM classifier_acquisitions WHERE article_id=$1 AND sport=$2 ORDER BY id DESC LIMIT 1")
                    .bind(item.entity_id).bind(&item.sport).fetch_one(&pool).await?;
                println!(
                    "{}",
                    json!({"article_id":item.entity_id,"sport":item.sport,"outcome":row.0,"reason":row.1,"seconds":elapsed.as_secs_f64()})
                );
            }
            other => {
                infrastructure_errors += 1;
                let detail = format!("{other:?}");
                work::fail(
                    &pool,
                    &item,
                    &detail,
                    Duration::from_secs(30),
                    work::MAX_ATTEMPTS,
                )
                .await?;
                println!(
                    "{}",
                    json!({"article_id":item.entity_id,"sport":item.sport,"outcome":"infrastructure_error","detail":detail})
                );
            }
        }
    }
    // No future may continue fetching after the measured interval.
    drop(pending);
    let outcomes: Vec<(String,String,i64)> = sqlx::query_as("SELECT status,reason,count(*) FROM classifier_acquisitions GROUP BY status,reason ORDER BY status,reason").fetch_all(&pool).await?;
    let acquired: i64 = sqlx::query_scalar("SELECT count(DISTINCT (article_id,sport)) FROM classifier_acquisitions WHERE status='acquired'").fetch_one(&pool).await?;
    let skipped: i64 = sqlx::query_scalar("SELECT count(DISTINCT (article_id,sport)) FROM classifier_acquisitions WHERE status='skipped'").fetch_one(&pool).await?;
    let outstanding: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE stage='classifier_acquire'")
            .fetch_one(&pool)
            .await?;
    let elapsed = started.elapsed().as_secs_f64();
    ensure!(
        acquired + skipped + outstanding == initial,
        "cohort accounting mismatch"
    );
    println!(
        "{}",
        json!({"summary":true,"database":database,"concurrency":concurrency,"initial":initial,"acquired":acquired,"skipped":skipped,"outstanding":outstanding,"infrastructure_errors":infrastructure_errors,"seconds":elapsed,"outcomes":outcomes,
        "projected_acquired_one_hour":acquired as f64/elapsed*3600.0,"projected_acquired_two_hours":acquired as f64/elapsed*7200.0})
    );
    ensure!(
        infrastructure_errors == 0,
        "benchmark had infrastructure failures"
    );
    Ok(())
}
