//! Shared fixtures for `TEST_DATABASE_URL` integration checks.
//!
//! The publication-fencing suites across the plugin packages each used to carry their own
//! `pool()` / `clean()` / `claim_one()` / `counts()` pair. Those four bodies were 90%+
//! identical and differ only in the tables they touch, so they live here once.
//!
//! Ordinary runs never reach this module: every caller is `#[ignore]`d and the pool is
//! only built when a test asks for it.
//!
//! ponytail: `clean` takes a table list rather than a fixed set, so a suite that touches a
//! new table adds a line instead of a helper. Callers must pass tables that actually have a
//! `sport` column — every table below does.

#![cfg(test)]

use crate::harness::queue::work::{self, Item, TaskKey};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

/// A pool against the isolated test database. Panics with the caller's hint when
/// `TEST_DATABASE_URL` is unset, so each suite still names the migrations it needs.
pub async fn pool(requirement: &str) -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| panic!("set TEST_DATABASE_URL: {requirement}"));
    PgPoolOptions::new()
        .max_connections(3)
        .connect(&url)
        .await
        .expect("connect TEST_DATABASE_URL")
}

/// Delete every row a suite owns, then ensure its test sport exists.
///
/// Tables are deleted in the order given; foreign keys mean the caller lists children
/// first. `display_name` labels the synthetic sport row.
pub async fn clean(pool: &PgPool, sport: &str, tables: &[&str], display_name: &str) {
    for table in tables {
        sqlx::query(&format!("DELETE FROM {table} WHERE sport = $1"))
            .bind(sport)
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("clean {table}: {e}"));
    }
    sqlx::query(
        "INSERT INTO sports (id, display_name, current_season) VALUES ($1,$2,2026) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(sport)
    .bind(display_name)
    .execute(pool)
    .await
    .expect("ensure test sport");
}

/// An unclaimed item, optionally pinned to an input revision.
pub fn item(stage: TaskKey, sport: &str, entity_id: i64, revision: Option<&str>) -> Item {
    Item {
        stage,
        entity_type: "team".to_string(),
        entity_id,
        sport: sport.to_string(),
        input_version: revision.map(str::to_string),
        attempts: 0,
        claim_token: None,
    }
}

/// Claim exactly one row for `stage`, failing the test if the queue did not hand one back.
pub async fn claim_one(pool: &PgPool, stage: TaskKey, context: &str) -> Item {
    let mut claimed = work::claim(pool, stage, 1)
        .await
        .unwrap_or_else(|e| panic!("claim {context}: {e}"));
    assert_eq!(
        claimed.len(),
        1,
        "claim {context}: expected exactly one row"
    );
    claimed.remove(0)
}

/// Recycle work rows that a crashed predecessor left claimed, so a suite starts clean.
pub async fn requeue_stale(pool: &PgPool) {
    work::requeue_stale(pool, Duration::from_secs(30 * 60))
        .await
        .expect("requeue stale work");
}

/// Row count for one of `clean`'s tables.
pub async fn count(pool: &PgPool, table: &str, sport: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table} WHERE sport = $1"))
        .bind(sport)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("count {table}: {e}"))
}
