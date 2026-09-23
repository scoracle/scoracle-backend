//! Diagnostic: freeze the exact Scout prompt for one entity through the real
//! preparation path (build_rating_request), and optionally create the card
//! through the real grounded parser (scout::create). No writes are made.
//!
//! Usage:
//!   cargo run --example scout_freeze -- -sport NBA -entity-type player -entity-id 56677822
//!   ... add -generate to also produce the card via the routed StatsLogic model.
use anyhow::{anyhow, Result};
use scoracle_cognition::application::models::Models;
use scoracle_cognition::plugins::scout::adapter::{build_rating_request, RatingReq};
use scoracle_cognition::plugins::scout::cognition::{RatingBuild, RATING_TEMPERATURE};
use scoracle_cognition::runtime::config::Config;
use scoracle_cognition::runtime::db;
use scoracle_cognition::runtime::route::Router;
use scoracle_cognition::studio::Studio;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    let mut entity_type = "player".to_string();
    let mut entity_id = 0i32;
    let mut sport = String::new();
    let mut season = None;
    let mut generate = false;
    let mut sport_label: Option<String> = None;
    let mut args = std::env::args().skip(1).peekable();
    while let Some(a) = args.next() {
        match a.as_str() {
            "-entity-type" => {
                entity_type = args
                    .next()
                    .ok_or_else(|| anyhow!("-entity-type requires a value"))?
            }
            "-entity-id" => {
                entity_id = args
                    .next()
                    .ok_or_else(|| anyhow!("-entity-id requires a value"))?
                    .parse()?
            }
            "-sport" => {
                sport = args
                    .next()
                    .ok_or_else(|| anyhow!("-sport requires a value"))?
                    .to_uppercase()
            }
            "-season" => {
                season = Some(
                    args.next()
                        .ok_or_else(|| anyhow!("-season requires a value"))?
                        .parse()?,
                )
            }
            "-generate" => generate = true,
            "-sport-label" => {
                sport_label = Some(
                    args.next()
                        .ok_or_else(|| anyhow!("-sport-label requires a value"))?,
                )
            }
            other => return Err(anyhow!("unknown argument {other:?}")),
        }
    }
    if entity_id <= 0 || sport.is_empty() {
        return Err(anyhow!("-entity-id and -sport are required"));
    }

    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    let models = Models {
        router: Router::from_config(&cfg.route, Duration::from_secs(600), 1)?,
        handler_budget: Duration::ZERO,
        voice_num_ctx: cfg.voice_num_ctx,
    };
    let name = scoracle_cognition::evidence::corpus::lookup_entity_name(
        &pool,
        &entity_type,
        entity_id,
        &sport,
    )
    .await?;
    let req = RatingReq {
        entity_type,
        entity_id,
        entity_name: name.clone(),
        sport,
        season,
        trigger_type: "manual".to_string(),
    };

    let mut assignment =
        match build_rating_request(&pool, models.voice_num_ctx, &req, RATING_TEMPERATURE, true)
            .await?
        {
            RatingBuild::NoStats { season } => {
                println!("no rating stats for season {season}");
                return Ok(());
            }
            RatingBuild::Ready(assignment) => *assignment,
        };
    if let Some(label) = sport_label {
        // Diagnostic ablation only: swap the raw sport id in the BUILT prompt
        // (system prompt untouched) before generation.
        let raw_sport = req.sport.clone();
        assignment.built_prompt = assignment.built_prompt.replace(&raw_sport, &label);
        println!("=== ablation: {raw_sport} -> {label:?} in built prompt ===");
    }

    println!(
        "=== model: {}",
        models
            .router
            .for_route(scoracle_cognition::plugins::scout::manifest::ROUTE)
            .model()
    );
    println!("=== input_hash: {}", assignment.input_hash);
    println!(
        "=== notability: {} {:?}",
        assignment.notability, assignment.notability_components
    );
    println!("=== exclusions: {:#?}", assignment.exclusions);
    println!(
        "=== trajectory: {} {:?}",
        assignment.rating_trajectory.key, assignment.rating_trajectory.components
    );
    println!(
        "=== system prompt ===\n{}\n=== user prompt ===",
        &*scoracle_cognition::plugins::scout::cognition::RATING_SYSTEM_PROMPT
    );
    println!("{}", assignment.built_prompt);

    if generate {
        let backend = models
            .router
            .for_route(scoracle_cognition::plugins::scout::manifest::ROUTE);
        let studio = Studio::new(backend.as_ref());
        let out =
            scoracle_cognition::plugins::scout::cognition::create(&studio, assignment).await?;
        println!("=== card ===");
        println!(
            "abstained: {}; skipped_no_stats: {}",
            out.product.abstained, out.product.skipped_no_stats
        );
        match (&out.product.headline, &out.product.body) {
            (Some(h), Some(b)) => println!("HEADLINE: {h}\n{b}"),
            (h, Some(b)) => println!("(headline {h:?})\n{b}"),
            _ => println!("(no body — abstained or marker)"),
        }
        println!("=== provenance: {:?} ===", out.provenance);
    }
    Ok(())
}
