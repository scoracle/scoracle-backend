//! Read-only production-loader capture: character_capture [--identity-only] NEW_DIR TYPE:ID:SPORT [...]
use anyhow::{ensure, Context, Result};
use scoracle_cognition::{
    evaluation::tasks::{resolve_task, EntitySpec, Expect, Fixture},
    harness::{config::Config, models::Models, route::Router},
};
use serde_json::json;
use std::{io::Write, path::Path};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let identity_only = args.first().is_some_and(|arg| arg == "--identity-only");
    let args = &args[usize::from(identity_only)..];
    ensure!(
        args.len() >= 2,
        "character_capture NEW_DIR TYPE:ID:SPORT [...]"
    );
    let root = Path::new(&args[0]);
    std::fs::create_dir(root).context("capture directory must be new")?;
    let cfg = Config::from_env()?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET default_transaction_read_only=on")
                    .execute(&mut *connection)
                    .await?;
                sqlx::query("SET statement_timeout='30s'")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&cfg.database_url)
        .await?;
    let models = Models {
        router: Router::from_config(&cfg.route, cfg.model_timeout, 1)?,
        voice_num_ctx: cfg.voice_num_ctx,
    };
    let mut ledger = std::fs::File::create(root.join("capture.jsonl"))?;
    for entity in &args[1..] {
        let fields: Vec<_> = entity.split(':').collect();
        ensure!(fields.len() == 3, "expected TYPE:ID:SPORT");
        let e = EntitySpec {
            entity_type: fields[0].into(),
            entity_id: fields[1].parse()?,
            sport: fields[2].into(),
            pair_player_id: None,
        };
        if identity_only {
            let mut tx = pool.begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
                .execute(&mut *tx)
                .await?;
            let legacy = scoracle_cognition::tools::meta::load_identity_record_on(
                &mut tx,
                &e.entity_type,
                e.entity_id,
                &e.sport,
            )
            .await?;
            let attributes = scoracle_cognition::tools::meta::load_character_identity_on(
                &mut tx,
                &e.entity_type,
                e.entity_id,
                &e.sport,
            )
            .await?;
            tx.commit().await?;
            writeln!(
                ledger,
                "{}",
                json!({"entity":entity,"legacy":legacy,"attributes":attributes,
                "database_read_only":true,"model_calls":0})
            )?;
            ledger.flush()?;
            continue;
        }
        for name in [
            "narratives",
            "vibe",
            "transfer",
            "rating",
            "momentum",
            "oracle",
        ] {
            if e.entity_type == "person" && matches!(name, "rating" | "momentum" | "oracle") {
                continue;
            }
            let task = resolve_task(name).unwrap();
            let started = std::time::Instant::now();
            let mut record =
                json!({"entity":entity,"task":name,"database_read_only":true,"model_calls":0});
            match task.build_request(&pool, &models, &e).await {
                Ok(Some(request)) => {
                    let filename = format!("{name}-{}.json", entity.replace(':', "-"));
                    let fixture = Fixture {
                        name: format!("real-{entity}-{name}"), task: name.into(), sport: e.sport.clone(),
                        prompt_version: task.prompt_version().into(), system: String::new(),
                        user_prompt: request.user_prompt, parts: request.parts, temperature: request.options.temperature.unwrap_or(0.0),
                        expect: Expect::default(), review: vec!["Does this fulfill the character's purpose? Is its central reading supported? Record material errors separately from awkward wording and processing cost.".into()],
                    };
                    std::fs::write(
                        root.join(&filename),
                        serde_json::to_string_pretty(&fixture)? + "\n",
                    )?;
                    record["fixture"] = json!(filename);
                    record["should_call"] = json!(request.should_call);
                }
                Ok(None) => record["status"] = json!("no_material"),
                Err(error) => record["error"] = json!(format!("{error:#}")),
            }
            record["capture_ms"] = json!(started.elapsed().as_millis());
            writeln!(ledger, "{record}")?;
            ledger.flush()?;
            eprintln!(
                "{entity} {name}: {}",
                record
                    .get("error")
                    .or(record.get("status"))
                    .unwrap_or(&json!("captured"))
            );
        }
    }
    Ok(())
}
