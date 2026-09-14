//! Postgres connection pool (sqlx).

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

/// build_pool opens a bounded connection pool. Keep `max_conns` small — the
/// harness's concurrency ceiling is the GPU, not Postgres, so a handful of
/// connections is plenty (default 5).
pub async fn build_pool(database_url: &str, max_conns: u32) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(max_conns)
        .acquire_timeout(Duration::from_secs(10))
        .connect(database_url)
        .await
        .context("connect to postgres")
}
