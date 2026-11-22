//! Postgres connection pool (sqlx).

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

// One connection for LISTEN, one for the locked outbox row, one for its reaction.
pub const MIN_WORKER_CONNECTIONS: u32 = 3;

pub async fn build_worker_pool(database_url: &str, max_conns: u32) -> Result<PgPool> {
    anyhow::ensure!(
        max_conns >= MIN_WORKER_CONNECTIONS,
        "COGNITION_DB_MAX_CONNS must be at least {MIN_WORKER_CONNECTIONS} for the worker (got {max_conns})"
    );
    build_pool(database_url, max_conns).await
}

pub async fn build_pool(database_url: &str, max_conns: u32) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(max_conns)
        .acquire_timeout(Duration::from_secs(10))
        .connect(database_url)
        .await
        .context("connect to postgres")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn undersized_worker_pools_fail_before_connecting() {
        for size in 0..MIN_WORKER_CONNECTIONS {
            let error = build_worker_pool("", size).await.unwrap_err();
            assert!(error.to_string().contains("COGNITION_DB_MAX_CONNS"));
        }
    }
}
