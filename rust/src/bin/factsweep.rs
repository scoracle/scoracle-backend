//! Operator entry point for the Investigator's non-queue person-metadata sweep.

use anyhow::{anyhow, Result};
use scoracle_cognition::application::models::Models;
use scoracle_cognition::plugins::investigator::adapter::{
    run_factsweep, FactsweepRequest, FactsweepRunContext,
};
use scoracle_cognition::runtime::{config::Config, db, route::Router};
use std::time::Duration;

#[derive(Clone, Debug)]
struct Args {
    request: FactsweepRequest,
    context: FactsweepRunContext,
}

fn parse_args(mut it: impl Iterator<Item = String>) -> Result<Args> {
    let mut sport = String::new();
    let mut limit = 60;
    let mut person = None;
    let mut context = FactsweepRunContext::Commit;
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "-sport" => sport = it.next().ok_or_else(|| anyhow!("-sport needs a value"))?,
            "-limit" => {
                limit = it
                    .next()
                    .ok_or_else(|| anyhow!("-limit needs a value"))?
                    .parse()?
            }
            "-person" => {
                person = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("-person needs a value"))?
                        .parse()?,
                )
            }
            "-dry-run" => context = FactsweepRunContext::DryRun,
            other => return Err(anyhow!("unknown flag {other:?}")),
        }
    }
    if sport.is_empty() {
        return Err(anyhow!("-sport is required"));
    }
    Ok(Args {
        request: FactsweepRequest {
            sport,
            limit,
            person,
        },
        context,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    let models = Models {
        router: Router::from_config(&cfg.route, cfg.ollama_timeout, cfg.ollama_max_concurrent)?,
        // An operator sweep is not governed by a queue lease deadline.
        handler_budget: Duration::ZERO,
        voice_num_ctx: cfg.voice_num_ctx,
    };
    let capabilities =
        models.capabilities(&scoracle_cognition::plugins::investigator::manifest::MANIFEST)?;
    run_factsweep(&pool, &capabilities, &args.request, args.context).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_non_queue_context_explicitly() {
        let args = parse_args(
            ["-sport", "football", "-person", "57", "-dry-run"]
                .into_iter()
                .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(args.request.sport, "football");
        assert_eq!(args.request.person, Some(57));
        assert_eq!(args.context, FactsweepRunContext::DryRun);
    }
}
