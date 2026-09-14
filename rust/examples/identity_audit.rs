//! Read current player metadata and transfer application history without writing.
//! DATABASE_PRIVATE_URL=... cargo run --example identity_audit -- FOOTBALL 4592198
//! Use official-web verification alongside this report before proposing corrections.

use anyhow::{ensure, Context, Result};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(args.len() == 2, "usage: identity_audit SPORT PLAYER_ID");
    let sport = args[0].to_uppercase();
    ensure!(
        matches!(sport.as_str(), "FOOTBALL" | "NBA" | "NFL"),
        "unsupported sport"
    );
    let player: i32 = args[1].parse().context("player ID must be an integer")?;
    let url = std::env::var("DATABASE_PRIVATE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .context("DATABASE_PRIVATE_URL or DATABASE_URL required")?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout = '20s'")
        .execute(&mut *tx)
        .await?;
    let report: serde_json::Value = sqlx::query_scalar(include_str!("identity_audit.sql"))
        .bind(sport)
        .bind(player)
        .fetch_one(&mut *tx)
        .await?;
    tx.rollback().await?;
    ensure!(
        !report["identity"].is_null(),
        "player identity does not exist"
    );
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
