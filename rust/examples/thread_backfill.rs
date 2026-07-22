//! thread_backfill — Characters Phase C: replay recent news_summaries history through the
//! thread attach routine (threads.rs / mig 181), so storylines carry their progression from
//! day one instead of starting cold.
//!
//! What it does, per entity with un-threaded rows in the window (default 45d):
//!   1. Loads its scored rows (body + impact + title, thread_id IS NULL) OLDEST-FIRST and
//!      embeds title+body (BGE-small, CPU, batched) — the same text the live attach embeds.
//!   2. Replays each GENERATION (rows sharing generated_at) chronologically through
//!      `attach_generation` with `at_epoch = Some(generation time)`, first fading threads
//!      already quiet >= 21d at that moment (`seal_stale_open_threads` — the sweep the
//!      nightly cron would have run).
//!   3. Writes thread_id + the re-classified trajectory back to each row — HEALING the
//!      trajectory resets the title-match seam caused (07-22 n10 wave), with a
//!      `"backfill": true` marker in trajectory_components.
//!
//! RUN ORDER MATTERS (chronological invariant): run this AFTER mig 181 and BEFORE the
//! thread-attaching binary deploys, so every row it replays predates every live thread.
//! It only touches thread_id IS NULL rows, so re-running is safe and a quick second pass
//! right before the deploy catches rows the old binary wrote during the first pass.
//!
//! Usage (env from .env.local): cargo run --release --example thread_backfill [-- -days 45 -entities 0]
//!   -days N      lookback window (default 45)
//!   -entities N  stop after N entities (0 = all; for smoke runs)

use anyhow::{Context, Result};
use scoracle_cognition::config::EmbedConfig;
use scoracle_cognition::embed::Embedder;
use scoracle_cognition::threads::{attach_generation, seal_stale_open_threads, AttachItem};
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::time::Instant;

const EMBED_BATCH: usize = 32;

struct SummaryRow {
    id: i64,
    title: String,
    body: String,
    impact: i32,
    source_names: Vec<String>,
    epoch: i64,
}

fn parse_args() -> (i32, usize) {
    let args: Vec<String> = std::env::args().collect();
    let mut days = 45i32;
    let mut entities = 0usize;
    let mut i = 1;
    while i + 1 < args.len() {
        match args[i].as_str() {
            "-days" => days = args[i + 1].parse().expect("-days takes an integer"),
            "-entities" => entities = args[i + 1].parse().expect("-entities takes an integer"),
            _ => {}
        }
        i += 2;
    }
    (days, entities)
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let (days, max_entities) = parse_args();
    let db_url = std::env::var("DATABASE_PRIVATE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .context("DATABASE_PRIVATE_URL / DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db_url)
        .await
        .context("connect postgres")?;

    println!("loading embedder (CPU)...");
    let embedder = Embedder::from_config(&EmbedConfig::from_env()?)?;

    let entities = sqlx::query(
        r#"
        SELECT entity_type, entity_id, sport, count(*) AS rows
        FROM news_summaries
        WHERE generated_at > now() - make_interval(days => $1)
          AND body IS NOT NULL AND impact IS NOT NULL AND narrative_title IS NOT NULL
          AND thread_id IS NULL
        GROUP BY 1, 2, 3
        ORDER BY 3, 1, 2
        "#,
    )
    .bind(days)
    .fetch_all(&pool)
    .await
    .context("list entities")?;
    let total_entities = entities.len();
    println!("{total_entities} entities with un-threaded rows in the last {days}d");

    let start = Instant::now();
    let (mut done, mut rows_attached, mut threads_opened, mut threads_faded) = (0usize, 0u64, 0u64, 0u64);
    for ent in entities {
        if max_entities > 0 && done >= max_entities {
            break;
        }
        let entity_type: String = ent.get("entity_type");
        let entity_id: i32 = ent.get("entity_id");
        let sport: String = ent.get("sport");

        let rows: Vec<SummaryRow> = sqlx::query(
            r#"
            SELECT id, narrative_title, body, impact::int AS impact, source_names,
                   extract(epoch FROM generated_at)::bigint AS epoch
            FROM news_summaries
            WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
              AND generated_at > now() - make_interval(days => $4)
              AND body IS NOT NULL AND impact IS NOT NULL AND narrative_title IS NOT NULL
              AND thread_id IS NULL
            ORDER BY generated_at ASC, id ASC
            "#,
        )
        .bind(&entity_type)
        .bind(entity_id)
        .bind(&sport)
        .bind(days)
        .fetch_all(&pool)
        .await
        .context("load entity rows")?
        .into_iter()
        .map(|r| SummaryRow {
            id: r.get("id"),
            title: r.get("narrative_title"),
            body: r.get("body"),
            impact: r.get("impact"),
            source_names: r.get("source_names"),
            epoch: r.get("epoch"),
        })
        .collect();
        if rows.is_empty() {
            done += 1;
            continue;
        }

        // One embed pass per entity, chunked — same text shape as the live attach.
        let texts: Vec<String> = rows
            .iter()
            .map(|r| format!("{}\n{}", r.title, r.body))
            .collect();
        let mut vectors = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(EMBED_BATCH) {
            vectors.extend(embedder.embed_batch(chunk)?);
        }

        let mut tx = pool.begin().await.context("begin entity tx")?;
        let mut i = 0usize;
        while i < rows.len() {
            // A generation = the run of rows sharing generated_at (one persist transaction).
            let epoch = rows[i].epoch;
            let mut j = i;
            while j < rows.len() && rows[j].epoch == epoch {
                j += 1;
            }
            threads_faded +=
                seal_stale_open_threads(&mut tx, &sport, &entity_type, entity_id, epoch).await?;
            let items: Vec<AttachItem> = (i..j)
                .map(|k| AttachItem {
                    title: &rows[k].title,
                    impact: rows[k].impact,
                    source_names: &rows[k].source_names,
                    vector: vectors[k].clone(),
                })
                .collect();
            let outcomes =
                attach_generation(&mut tx, &sport, &entity_type, entity_id, &items, Some(epoch))
                    .await?;
            for (k, o) in (i..j).zip(&outcomes) {
                let reason = match o.delta_reason {
                    "up" => "impact_up",
                    "down" => "impact_down",
                    "stable" => "impact_stable",
                    other => other,
                };
                let components = serde_json::json!({
                    "previous_impact": o.previous_impact,
                    "current_impact": rows[k].impact,
                    "impact_delta": o.impact_delta,
                    "reason": reason,
                    "thread_id": o.thread_id,
                    "thread_action": if o.opened { "opened" } else { "attached" },
                    "thread_cosine": o.cosine,
                    "backfill": true,
                });
                sqlx::query(
                    r#"UPDATE news_summaries
                       SET thread_id = $1, trajectory = $2, trajectory_components = $3::jsonb
                       WHERE id = $4"#,
                )
                .bind(o.thread_id)
                .bind(o.trajectory)
                .bind(components.to_string())
                .bind(rows[k].id)
                .execute(&mut *tx)
                .await
                .context("stamp row thread")?;
                rows_attached += 1;
                if o.opened {
                    threads_opened += 1;
                }
            }
            i = j;
        }
        tx.commit().await.context("commit entity tx")?;

        done += 1;
        if done % 100 == 0 {
            println!(
                "{done}/{total_entities} entities | {rows_attached} rows attached | {threads_opened} threads opened | {threads_faded} faded | {:.0}s",
                start.elapsed().as_secs_f64()
            );
        }
    }

    println!(
        "DONE: {done} entities | {rows_attached} rows attached | {threads_opened} threads opened | {threads_faded} faded in replay | {:.0}s",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
