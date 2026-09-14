//! Inspect the same memory package used by a writer, without inference or writes.
//! DATABASE_PRIVATE_URL=... cargo run --example memory_inspect -- FOOTBALL player 4592198 scout
//! Optional trailing season and pair-team ID select a historical report or exact pair.

use anyhow::{ensure, Context, Result};
use scoracle_cognition::composition::memories::{self, MemoryRequest};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        (4..=6).contains(&args.len()),
        "usage: memory_inspect SPORT ENTITY_TYPE ID MISSION [SEASON [PAIR_TEAM_ID]]"
    );
    let mission = serde_json::from_value(json!(args[3])).context("unknown mission")?;
    let id = args[2].parse().context("invalid entity ID")?;
    let mut request = MemoryRequest::new(mission, &args[1], id, &args[0]);
    request.season = args.get(4).map(|s| s.parse()).transpose()?;
    request.pair_team_id = args.get(5).map(|s| s.parse()).transpose()?;
    let url = std::env::var("DATABASE_PRIVATE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .context("DATABASE_PRIVATE_URL or DATABASE_URL required")?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    let package = memories::load(&pool, request).await?;
    let rendered = package.render()?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"fingerprint":package.fingerprint()?,"rendered_bytes":rendered.len(),"rendered":rendered,"package":package})
        )?
    );
    Ok(())
}
