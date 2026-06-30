//! L3 sigil parity harness — the proof that the Rust sigil stage matches the Go one.
//!
//! For each requested entity it runs the SAME `generate_sigil` core the production
//! `SigilHandler` uses, but at an EXPLICIT `temperature: 0` (deterministic on the byte axes),
//! and writes the result to `sigil_synthesis_shadow` with `source='rust'`, alongside the exact
//! prompt, the exact Ollama request body, the season, and the canonical input-components JSON +
//! its `input_hash`. The Go side (an env-gated test in package `ml`, see
//! `go/internal/ml/sigil_parity_test.go`) writes `source='go'` rows for the same entities the
//! same way. The proof is a single self-join in that table: identical built_prompt bytes,
//! jsonb-equal ollama_request, identical model_version, and identical input_hash (the four
//! DETERMINISTIC axes). SCORE/BLURB are compared only within ONE model-load window — gemma4:e4b
//! at temp 0 is not reliably deterministic (L2 FINDING), so they are not a regression signal.
//!
//! SAFETY: this harness is read-only on the live pipeline. It NEVER writes sigil_synthesis,
//! NEVER claims/enqueues pipeline_work, and NEVER runs the Go stage. It only reads the pillar
//! tables (news_summaries, stat_summaries, vibe_scores, event_*_stats, fixtures, sports) and
//! writes the standalone shadow table. Run it while Go's drainSigil keeps owning the live stage.
//!
//! Usage (env: DATABASE_PRIVATE_URL + OLLAMA_*):
//!   sigil_parity                              # auto-select SIGIL_PARITY_LIMIT (default 3) entities
//!   sigil_parity player:237:NBA team:14:NBA   # explicit entities (entity_type:id:sport)

use anyhow::{anyhow, Context, Result};
use scoracle_cognition::config::Config;
use scoracle_cognition::db;
use scoracle_cognition::harness::Harness;
use scoracle_cognition::ollama::OllamaClient;
use scoracle_cognition::route::Router;
use scoracle_cognition::sigil::{generate_sigil, SigilOutput};
use sqlx::PgPool;

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

    // The same capability context the production worker builds — the config-driven router
    // (all-Gemma by default) — so the parity run exercises the exact route + extract path the
    // service does. The harness writes nothing on its own; persistence targets only the shadow.
    let harness = Harness {
        pool,
        router: Router::from_config(&cfg.route, cfg.ollama_timeout, 1)?, // offline; single-flight
        embedder: None,
        resolve: cfg.resolve.clone(),
    };

    let specs = match parse_specs(std::env::args().skip(1))? {
        v if !v.is_empty() => v,
        _ => {
            let limit: i64 = std::env::var("SIGIL_PARITY_LIMIT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3);
            auto_select(&harness.pool, limit).await?
        }
    };

    println!(
        "sigil parity harness — source=rust, temp={PARITY_TEMPERATURE}, {} entit{} → sigil_synthesis_shadow",
        specs.len(),
        if specs.len() == 1 { "y" } else { "ies" }
    );

    let mut ok = 0usize;
    for s in &specs {
        match run_one(&harness, s).await {
            Ok(out) => {
                ok += 1;
                match out.score {
                    Some(score) => println!(
                        "  ✓ {}/{} ({}) season {} → SCORE {} | hash {} | BLURB {:?}",
                        s.entity_type,
                        s.entity_id,
                        s.sport,
                        out.season,
                        score,
                        out.input_hash.as_deref().unwrap_or(""),
                        out.blurb.as_deref().unwrap_or("")
                    ),
                    None => println!(
                        "  ✓ {}/{} ({}) season {} → no-pillar NULL marker",
                        s.entity_type, s.entity_id, s.sport, out.season
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
        "done — {ok}/{} entities written to sigil_synthesis_shadow",
        specs.len()
    );
    Ok(())
}

async fn run_one(hx: &Harness, s: &EntitySpec) -> Result<SigilOutput> {
    let name = scoracle_cognition::vibe::lookup_entity_name(
        &hx.pool,
        &s.entity_type,
        s.entity_id,
        &s.sport,
    )
    .await?;
    let out = generate_sigil(
        hx,
        &s.entity_type,
        s.entity_id,
        &name,
        &s.sport,
        PARITY_TEMPERATURE,
    )
    .await?;

    persist_shadow(&hx.pool, s, &out).await?;
    Ok(out)
}

/// persist_shadow writes one source='rust' row to sigil_synthesis_shadow. Mirrors the
/// sigil_synthesis persist (trigger 'periodic', trigger_payload '{}', empty blurb → "" for a
/// scored row / NULL for a marker) plus the harness-only columns (temperature, built_prompt,
/// ollama_request). input_components is the canonical JSON (its SHA-256 is input_hash).
async fn persist_shadow(pool: &PgPool, s: &EntitySpec, out: &SigilOutput) -> Result<()> {
    let score: Option<i16> = out.score.map(|n| n as i16);
    let request_json: Option<String> = out.request_body.as_ref().map(|v| v.to_string());
    sqlx::query(
        r#"
        INSERT INTO sigil_synthesis_shadow (
            source, entity_type, entity_id, sport, season,
            trigger_type, trigger_payload,
            score, blurb, input_components, input_hash,
            model_version, prompt_version,
            temperature, built_prompt, ollama_request
        ) VALUES ('rust',$1,$2,$3,$4,'periodic','{}'::jsonb,$5,$6,$7::jsonb,$8,$9,$10,$11,$12,$13::jsonb)
        "#,
    )
    .bind(&s.entity_type)
    .bind(s.entity_id)
    .bind(s.sport.to_uppercase())
    .bind(out.season)
    .bind(score)
    .bind(out.blurb.as_deref())
    .bind(out.input_components_json.as_str())
    .bind(out.input_hash.as_deref())
    .bind(out.model.as_str())
    .bind(out.prompt_version)
    .bind(PARITY_TEMPERATURE as f32)
    .bind(out.built_prompt.as_deref())
    .bind(request_json)
    .execute(pool)
    .await
    .context("persist shadow row")?;
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
/// body-NOT-NULL narrative — guaranteeing a real narrative pillar (a non-marker scored
/// synthesis), the interesting case for a SCORE/BLURB diff.
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
