//! crown_probe — eyeball the crown (Oracle) BEFORE deploying it. For a sample of real
//! entities it runs the EXACT production path — load_pillars → deterministic convergence + omen →
//! the one OracleLogic call → {reading, score} (blind to memories since or9) — and
//! prints the reading, score, omen, and convergence. Lets us read the actual voice + verdict and
//! tune the system prompt before it ships.
//!
//! SAFETY: read-only. Never claims pipeline_work, never writes any table.
//!
//! Usage (env from .env.local):
//!   cargo run --release --example crown_probe                    # 6 recent scored entities
//!   CROWN_PROBE_N=10 CROWN_PROBE_SPORT=FOOTBALL \
//!   cargo run --release --example crown_probe

use anyhow::{Context, Result};
use scoracle_cognition::config::Config;
use scoracle_cognition::corpus::lookup_entity_name;
use scoracle_cognition::db;
use scoracle_cognition::harness::Harness;

use scoracle_cognition::junctions::oracle::{
    build_crown_prompt, build_pillar_divergence, compute_omen, load_pillars, oracle_format_schema,
    pillar_convergence, CrownParser, ORACLE_NUM_PREDICT, ORACLE_PROMPT_VERSION,
    ORACLE_SYSTEM_PROMPT, ORACLE_TEMPERATURE,
};
use scoracle_cognition::ollama::GenerateOptions;
use scoracle_cognition::ollama::OllamaClient;
use scoracle_cognition::route::{Role, Router};
use sqlx::Row;

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;

    let ollama = OllamaClient::new(&cfg.ollama_base_url, &cfg.ollama_model, cfg.ollama_timeout)?;
    ollama
        .ping()
        .await
        .context("ollama must be reachable for the crown probe")?;

    let harness = Harness {
        pool,
        router: Router::from_config(&cfg.route, cfg.ollama_timeout, 1)?,
        embedder: None,
        // Unbounded: a probe is not a queue item and has no worker timeout to land inside.
        handler_budget: std::time::Duration::ZERO,
        voice_num_ctx: cfg.voice_num_ctx,
    };

    let n: i64 = std::env::var("CROWN_PROBE_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6);
    let sport_filter = std::env::var("CROWN_PROBE_SPORT").ok();

    // A sample of entities that currently carry a scored crown row — they have real pillars, so
    // the crown has cards to read. Latest-per-entity, freshest first.
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT ON (entity_type, entity_id, sport, season)
               entity_type, entity_id, sport
        FROM sigil_synthesis
        WHERE score IS NOT NULL
          AND ($1::text IS NULL OR sport = $1)
        ORDER BY entity_type, entity_id, sport, season, generated_at DESC
        "#,
    )
    .bind(sport_filter.as_deref())
    .fetch_all(&harness.pool)
    .await
    .context("load probe entities")?;

    // Take the first N distinct entities (the query already deduped per entity-season).
    let sample: Vec<(String, i32, String)> = rows
        .iter()
        .take(n as usize)
        .map(|r| {
            (
                r.get::<String, _>("entity_type"),
                r.get::<i32, _>("entity_id"),
                r.get::<String, _>("sport"),
            )
        })
        .collect();

    println!(
        "=== CROWN PROBE — {} entities, prompt {} ===\n",
        sample.len(),
        ORACLE_PROMPT_VERSION
    );

    for (entity_type, entity_id, sport_raw) in sample {
        let sport = sport_raw.to_uppercase();
        let name = lookup_entity_name(&harness.pool, &entity_type, entity_id, &sport_raw)
            .await
            .unwrap_or_else(|_| format!("{entity_type}#{entity_id}"));

        let (_season, narratives, rating, vibe, momentum, transfers) =
            match load_pillars(&harness, &entity_type, entity_id, &sport).await {
                Ok(p) => p,
                Err(e) => {
                    println!("-- {name} ({sport} {entity_type}): load_pillars failed: {e}\n");
                    continue;
                }
            };
        if narratives.is_empty()
            && rating.is_none()
            && vibe.is_none()
            && momentum.empty()
            && transfers.is_empty()
        {
            println!("-- {name} ({sport} {entity_type}): no pillars (marker) — skipped\n");
            continue;
        }

        let comparisons =
            build_pillar_divergence(&narratives, rating.as_ref(), vibe.as_ref(), &momentum);
        let convergence = pillar_convergence(&comparisons);
        let (omen, omen_reason) = compute_omen(convergence, &momentum);
        // or9: the crown is blind to memories — no prior-read or relational-memory load.
        let prompt = build_crown_prompt(
            &entity_type,
            &name,
            &sport_raw,
            &narratives,
            rating.as_ref(),
            vibe.as_ref(),
            &momentum,
            &transfers,
            omen,
            &omen_reason,
            None,
        );
        // Probe knob: CROWN_PROBE_NUM_CTX lets us test the context-window hypothesis. 0 keeps the
        // current (broken) behavior — Ollama's 2048 default truncates the system prompt.
        let num_ctx: i32 = std::env::var("CROWN_PROBE_NUM_CTX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let opts = GenerateOptions {
            system: Some(ORACLE_SYSTEM_PROMPT.to_string()),
            temperature: Some(ORACLE_TEMPERATURE),
            num_predict: ORACLE_NUM_PREDICT,
            num_ctx,
            json_mode: false,
            format_schema: Some(oracle_format_schema()),
            format_schema_raw: None,
        };

        // Rough token estimate (≈ chars/4) for system + user + the reserved response budget —
        // the sum that must fit the context window (default 4096 on this box).
        let sys_chars = ORACLE_SYSTEM_PROMPT.len();
        let est_input = (sys_chars + prompt.len()) / 4;
        let est_ctx_needed = est_input + ORACLE_NUM_PREDICT as usize;

        // Size-only mode: scan prompt sizes across many entities without paying for GPU calls.
        if std::env::var("CROWN_PROBE_SIZE_ONLY").is_ok() {
            println!(
                "{:>6} ctx  {:>5} in  | {name} ({sport} {entity_type})",
                est_ctx_needed, est_input
            );
            continue;
        }

        print!(
            "── {name} ({sport} {entity_type})  omen={omen}  convergence={}\n   prompt: {} user chars + {} sys chars ≈ {} input tokens, ~{} needed w/ response (window {})\n",
            convergence.map(|c| c.to_string()).unwrap_or_else(|| "–".into()),
            prompt.len(), sys_chars, est_input, est_ctx_needed,
            if num_ctx > 0 { num_ctx.to_string() } else { "default~4096".into() },
        );
        match harness
            .extract(Role::OracleLogic, &prompt, &opts, &CrownParser)
            .await
        {
            Ok(ex) => match ex.value {
                Some(reply) => {
                    println!("   SCORE: {}", reply.score);
                    println!("   READING: {}\n", reply.reading);
                }
                None => println!("   (parser returned no value)\n"),
            },
            Err(e) => println!("   crown call failed: {e}\n"),
        }
    }

    Ok(())
}
