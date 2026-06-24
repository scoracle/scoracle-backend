//! Phase 1 vibe parity harness — the proof that the Rust vibe stage matches the Go one.
//!
//! For each requested entity it runs the SAME `generate_vibe` core the production
//! `VibeHandler` uses, but at an EXPLICIT `temperature: 0` (deterministic — verified),
//! and writes the result to `vibe_scores_shadow` with `source='rust'`, alongside the
//! exact prompt and the exact Ollama request body. The Go side (an env-gated test in
//! package `ml`, see `go/internal/ml/vibe_parity_test.go`) writes `source='go'` rows for
//! the same entities the same way. The proof is then a single self-join in that table:
//! same SCORE, same VIBE, jsonb-equal request body.
//!
//! SAFETY: this harness is read-only on the live pipeline. It NEVER writes vibe_scores,
//! NEVER claims/enqueues pipeline_work, and NEVER runs the Go stage. It only reads the
//! corpus tables (news_summaries, transfer_rumors, players, teams) and writes the
//! standalone shadow table. Run it while Go's drainVibe keeps owning the live vibe stage.
//!
//! Usage (env from .env.local: DATABASE_PRIVATE_URL + OLLAMA_*):
//!   parity                              # auto-select PARITY_LIMIT (default 3) entities
//!   parity player:237:NBA team:14:NBA   # explicit entities (entity_type:id:sport)

use anyhow::{anyhow, Context, Result};
use scoracle_cognition::config::Config;
use scoracle_cognition::harness::Harness;
use scoracle_cognition::ollama::OllamaClient;
use scoracle_cognition::route::Router;
use scoracle_cognition::vibe::{generate_vibe, VibeOutput, VIBE_PROMPT_VERSION};
use scoracle_cognition::{db, vibe};
use sqlx::PgPool;
use std::sync::Arc;

/// The explicit, deterministic temperature the parity diff is taken at.
const PARITY_TEMPERATURE: f64 = 0.0;

#[derive(Clone, Debug)]
struct EntitySpec {
    entity_type: String,
    entity_id: i32,
    sport: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    let ollama = OllamaClient::new(&cfg.ollama_base_url, &cfg.ollama_model, cfg.ollama_timeout)?;
    ollama
        .ping()
        .await
        .context("ollama must be reachable for the parity run")?;

    // The same capability context the production worker builds — the L1 minimal router over
    // the pinged client — so the parity run exercises the exact route + extract path. The
    // harness writes nothing on its own; persistence here targets only the shadow table.
    let harness = Harness {
        pool,
        router: Router::single(Arc::new(ollama)),
        embedder: None,
    };

    let specs = match parse_specs(std::env::args().skip(1))? {
        v if !v.is_empty() => v,
        _ => {
            let limit: i64 = std::env::var("PARITY_LIMIT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3);
            auto_select(&harness.pool, limit).await?
        }
    };

    println!(
        "vibe parity harness — source=rust, temp={PARITY_TEMPERATURE}, {} entit{} → vibe_scores_shadow",
        specs.len(),
        if specs.len() == 1 { "y" } else { "ies" }
    );

    let mut ok = 0usize;
    for s in &specs {
        match run_one(&harness, s).await {
            Ok(out) => {
                ok += 1;
                match out.sentiment {
                    Some(score) => println!(
                        "  ✓ {}/{} ({}) → SCORE {} | VIBE {:?}",
                        s.entity_type,
                        s.entity_id,
                        s.sport,
                        score,
                        out.vibe_prompt.as_deref().unwrap_or("")
                    ),
                    None => println!(
                        "  ✓ {}/{} ({}) → no-corpus NULL marker",
                        s.entity_type, s.entity_id, s.sport
                    ),
                }
            }
            Err(e) => println!(
                "  ✗ {}/{} ({}) → {e:#}",
                s.entity_type, s.entity_id, s.sport
            ),
        }
    }
    println!(
        "done — {ok}/{} entities written to vibe_scores_shadow",
        specs.len()
    );
    Ok(())
}

async fn run_one(hx: &Harness, s: &EntitySpec) -> Result<VibeOutput> {
    let name = vibe::lookup_entity_name(&hx.pool, &s.entity_type, s.entity_id, &s.sport).await?;
    let out = generate_vibe(
        hx,
        &s.entity_type,
        s.entity_id,
        &name,
        &s.sport,
        PARITY_TEMPERATURE,
    )
    .await?;

    // The exact request body that was POSTed for this entity is captured by `extract` and
    // carried on the output (`None` for the no-corpus marker, which makes no model call) — it
    // is the very body the call sent (the client's single source of truth), so it can't drift
    // from what generate sent. The temperature inside it is the explicit `Some(0.0)` that
    // pins determinism (generate_vibe was called at PARITY_TEMPERATURE).
    persist_shadow(&hx.pool, s, &out, out.request_body.as_ref()).await?;
    Ok(out)
}

/// persist_shadow writes one source='rust' row to vibe_scores_shadow. Mirrors the
/// vibe_scores persist (trigger 'periodic', trigger_payload JSON null, empty felt-read →
/// NULL) plus the harness-only columns (temperature, built_prompt, ollama_request).
async fn persist_shadow(
    pool: &PgPool,
    s: &EntitySpec,
    out: &VibeOutput,
    request: Option<&serde_json::Value>,
) -> Result<()> {
    let sentiment: Option<i16> = out.sentiment.map(|n| n as i16);
    let request_json: Option<String> = request.map(|v| v.to_string());
    sqlx::query(
        r#"
        INSERT INTO vibe_scores_shadow (
            source, entity_type, entity_id, sport,
            trigger_type, trigger_payload,
            sentiment, prompt, input_news_ids,
            model_version, prompt_version,
            temperature, built_prompt, ollama_request
        ) VALUES ('rust',$1,$2,$3,'periodic','null'::jsonb,$4,$5,$6,$7,$8,$9,$10,$11::jsonb)
        "#,
    )
    .bind(&s.entity_type)
    .bind(s.entity_id)
    .bind(s.sport.to_uppercase())
    .bind(sentiment)
    .bind(out.vibe_prompt.as_deref())
    .bind(out.input_news_ids.as_slice())
    .bind(out.model.as_str())
    .bind(out.prompt_version)
    .bind(PARITY_TEMPERATURE as f32)
    .bind(out.built_prompt.as_deref())
    .bind(request_json)
    .execute(pool)
    .await
    .context("persist shadow row")?;
    let _ = VIBE_PROMPT_VERSION; // (re-exported for the Go test's prompt_version assertion)
    Ok(())
}

/// parse_specs reads `entity_type:id:sport` tokens from the CLI args.
fn parse_specs(args: impl Iterator<Item = String>) -> Result<Vec<EntitySpec>> {
    let mut out = Vec::new();
    for a in args {
        let parts: Vec<&str> = a.split(':').collect();
        if parts.len() != 3 {
            return Err(anyhow!("bad entity spec {a:?}; want entity_type:id:sport"));
        }
        let entity_type = parts[0].to_lowercase();
        if entity_type != "player" && entity_type != "team" {
            return Err(anyhow!("bad entity_type in {a:?}; want player|team"));
        }
        let entity_id: i32 = parts[1]
            .parse()
            .with_context(|| format!("bad id in {a:?}"))?;
        out.push(EntitySpec {
            entity_type,
            entity_id,
            sport: parts[2].to_uppercase(),
        });
    }
    Ok(out)
}

/// auto_select picks entities whose LATEST news_summaries generation has at least one
/// body-NOT-NULL narrative — guaranteeing a real (non-marker) scored model call, the
/// interesting case for a SCORE/VIBE diff.
async fn auto_select(pool: &PgPool, limit: i64) -> Result<Vec<EntitySpec>> {
    let rows: Vec<(String, i32, String)> = sqlx::query_as(
        r#"
        SELECT entity_type, entity_id, sport
        FROM news_summaries ns
        WHERE body IS NOT NULL
          AND generated_at = (
              SELECT max(generated_at) FROM news_summaries x
              WHERE x.entity_type = ns.entity_type
                AND x.entity_id = ns.entity_id
                AND x.sport = ns.sport
          )
        GROUP BY entity_type, entity_id, sport
        ORDER BY max(generated_at) DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("auto-select parity entities")?;

    Ok(rows
        .into_iter()
        .map(|(entity_type, entity_id, sport)| EntitySpec {
            entity_type,
            entity_id,
            sport,
        })
        .collect())
}
