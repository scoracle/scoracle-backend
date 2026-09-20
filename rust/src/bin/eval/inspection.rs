//! Read prepared evidence without inference or publication.
use anyhow::{bail, ensure, Context, Result};
use scoracle_cognition::{
    evidence::{
        memories::{self, MemoryRequest},
        personnel::load_scout_reports,
    },
    runtime::{config::Config, db},
};
use serde_json::{json, Value};

pub async fn run(cfg: &Config, args: &[String]) -> Result<()> {
    let Some((kind, args)) = args.split_first() else {
        bail!("--inspect needs memory, reports or identity");
    };
    let valid = match kind.as_str() {
        "memory" => (4..=6).contains(&args.len()),
        "reports" => args.len() == 3,
        "identity" => args.len() == 2,
        _ => false,
    };
    ensure!(valid, "usage: eval --inspect memory SPORT TYPE ID MISSION [SEASON [PAIR_TEAM_ID]] | reports SPORT TYPE ID | identity SPORT PLAYER_ID");
    let sport = args[0].to_uppercase();
    ensure!(
        matches!(sport.as_str(), "FOOTBALL" | "NBA" | "NFL"),
        "unsupported sport"
    );
    let pool = db::build_pool(&cfg.database_url, 1).await?;
    let output: Value = match kind.as_str() {
        "memory" => {
            let mission = serde_json::from_value(json!(args[3])).context("unknown mission")?;
            let mut request = MemoryRequest::new(mission, &args[1], args[2].parse()?, &sport);
            request.season = args.get(4).map(|s| s.parse()).transpose()?;
            request.pair_team_id = args.get(5).map(|s| s.parse()).transpose()?;
            let package = memories::load(&pool, request).await?;
            let audit = package.render()?;
            let model = package.render_for_model()?;
            json!({"fingerprint": package.fingerprint()?, "audit_rendered_bytes": audit.len(), "audit_rendered": audit, "model_rendered_bytes": model.len(), "model_rendered": model, "package": package})
        }
        "reports" => {
            let claims = load_scout_reports(&pool, &args[1], args[2].parse()?, &sport).await?;
            json!(claims
                .into_iter()
                .map(|marked| json!({
                    "article_id": marked.claim.article_id,
                    "source": marked.claim.source,
                    "fact": marked.claim.fact,
                    "published_at": marked.claim.published_at,
                    "story_type": marked.claim.story_type,
                    "contested": marked.marked,
                }))
                .collect::<Vec<_>>())
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
