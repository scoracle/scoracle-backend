//! Read prepared evidence without inference or publication.
use anyhow::{bail, ensure, Context, Result};
use scoracle_cognition::{
    harness::{config::Config, db},
    plugins::scout::sources::load_scout_reports,
};
use serde_json::Value;

pub async fn run(cfg: &Config, args: &[String]) -> Result<()> {
    let Some((kind, args)) = args.split_first() else {
        bail!("--inspect needs reports or identity");
    };
    let valid = match kind.as_str() {
        "reports" => args.len() == 3,
        "identity" => args.len() == 2,
        _ => false,
    };
    ensure!(
        valid,
        "usage: eval --inspect reports SPORT TYPE ID | identity SPORT PLAYER_ID"
    );
    let sport = args[0].to_uppercase();
    ensure!(
        matches!(sport.as_str(), "FOOTBALL" | "NBA" | "NFL"),
        "unsupported sport"
    );
    let pool = db::build_pool(&cfg.database_url, 1).await?;
    let output: Value = match kind.as_str() {
        "reports" => {
            let claims = load_scout_reports(&pool, &args[1], args[2].parse()?, &sport).await?;
            serde_json::to_value(claims)?
        }
        "identity" => {
            let player: i32 = args[1].parse().context("player ID must be an integer")?;
            let mut tx = pool.begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
                .execute(&mut *tx)
                .await?;
            sqlx::query("SET LOCAL statement_timeout = '20s'")
                .execute(&mut *tx)
                .await?;
            let report: Value = sqlx::query_scalar(include_str!("identity.sql"))
                .bind(sport)
                .bind(player)
                .fetch_one(&mut *tx)
                .await?;
            tx.rollback().await?;
            ensure!(
                !report["identity"].is_null(),
                "player identity does not exist"
            );
            report
        }
        _ => unreachable!(),
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
