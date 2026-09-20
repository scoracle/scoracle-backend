//! Run one complete production-built Scout request without persistence.
//!
//! The tool reads the same database rows and route configuration as the live
//! application adapter, calls the configured provider once, and emits an audit artifact to
//! stdout. It never checks debounce state, writes a product, or touches queue work.
//!
//! DATABASE_PRIVATE_URL=... cargo run --example scout_request_inspect -- \
//!   FOOTBALL player 4592198 [SEASON]

use anyhow::{ensure, Context, Result};
use scoracle_cognition::{
    application::models::Models,
    application::scout::{build_rating_request, RatingReq},
    evidence::corpus::lookup_entity_name,
    runtime::{
        config::Config,
        db,
        route::{Role, Router},
    },
    studio::scout::{RatingBuild, RatingRequestParser, RATING_TEMPERATURE},
    studio::Parser,
};
use serde_json::json;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        (3..=4).contains(&args.len()),
        "usage: scout_request_inspect SPORT ENTITY_TYPE ID [SEASON]"
    );
    let sport = args[0].to_uppercase();
    let entity_type = args[1].clone();
    ensure!(
        matches!(entity_type.as_str(), "player" | "team"),
        "Scout supports player or team"
    );
    let entity_id = args[2].parse().context("invalid entity ID")?;
    let season = args.get(3).map(|s| s.parse()).transpose()?;

    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, 2).await?;
    let router = Router::from_config(&cfg.route, cfg.ollama_timeout, 1)?;
    let entity_name = lookup_entity_name(&pool, &entity_type, entity_id, &sport).await?;
    let models = Models {
        router,
        handler_budget: Duration::ZERO,
        voice_num_ctx: cfg.voice_num_ctx,
    };
    let req = RatingReq {
        entity_type,
        entity_id,
        entity_name,
        sport,
        season,
        trigger_type: "memory_inspection".into(),
    };
    let ready = match build_rating_request(&pool, &models, &req, RATING_TEMPERATURE, true).await? {
        RatingBuild::NoStats { season } => {
            anyhow::bail!("Scout builder found no usable statistics for season {season}")
        }
        RatingBuild::Ready(ready) => *ready,
    };

    let backend = models.router.for_role(Role::StatsLogic);
    let (generated, sent_request) = backend.generate(&ready.built_prompt, &ready.opts).await?;
    let parsed_provider_response =
        serde_json::from_str::<serde_json::Value>(&generated.raw_response_body)
            .unwrap_or_else(|_| json!({"raw": generated.raw_response_body}));
    let parser = RatingRequestParser::new(
        &ready.built_prompt,
        &ready.comparison_directions,
        &ready.measurement_bands,
    );
    let (parser_pass, parser_error) = match parser.parse(&generated.response) {
        Ok(Some(_)) => (true, None),
        Ok(None) => (false, Some("parser returned no product".to_string())),
        Err(error) => (false, Some(error.to_string())),
    };
    let input_components: serde_json::Value = serde_json::from_str(&ready.input_components)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "entity": {
                "type": req.entity_type,
                "id": req.entity_id,
                "name": req.entity_name,
                "sport": req.sport,
                "season": ready.season,
            },
            "input": {
                "fingerprint": ready.input_hash,
                "components": input_components,
                "system": ready.opts.system,
                "user": ready.built_prompt,
            },
            "provider_request": {
                "model": backend.model(),
                "temperature": ready.opts.temperature,
                "num_ctx": ready.opts.num_ctx,
                "num_predict": ready.opts.num_predict,
                "body": sent_request,
            },
            "provider_response": {
                "model": generated.model,
                "prompt_tokens": generated.prompt_eval_count,
                "output_tokens": generated.eval_count,
                "completion_reason": generated.completion_reason,
                "total_duration_ms": generated.total_duration.as_millis(),
                "unmodified_output": generated.response,
                "thinking": generated.thinking,
                "raw_body": parsed_provider_response,
                "parser_pass": parser_pass,
                "parser_error": parser_error,
            },
            "notes": [
                "Read-only diagnostic: no debounce check, product persistence, queue mutation or parser rewrite.",
                "Parser acceptance does not establish factual or editorial quality."
            ]
        }))?
    );
    Ok(())
}
