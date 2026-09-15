//! Read-only diagnostic for the Scout's current attributed-report slice.

use anyhow::{ensure, Context, Result};
use scoracle_cognition::{
    evidence::personnel::load_scout_reports,
    runtime::{config::Config, db},
};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 3,
        "usage: scout_reports_inspect SPORT ENTITY_TYPE ID"
    );
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, 2).await?;
    let claims = load_scout_reports(
        &pool,
        &args[1],
        args[2].parse().context("invalid entity ID")?,
        &args[0].to_uppercase(),
    )
    .await?;
    let output = claims
        .into_iter()
        .map(|marked| {
            serde_json::json!({
                "article_id": marked.claim.article_id,
                "source": marked.claim.source,
                "fact": marked.claim.fact,
                "published_at": marked.claim.published_at,
                "story_type": marked.claim.story_type,
                "contested": marked.marked,
            })
        })
        .collect::<Vec<_>>();
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
