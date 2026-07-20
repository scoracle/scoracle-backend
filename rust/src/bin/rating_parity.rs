//! L12 rating parity harness — the proof that the Rust rating stage's MACHINERY matches Go.
//!
//! For each entity (player|team) it runs the SAME deterministic prefix the production path uses —
//! `build_rating_request` (load_rating_profile → input_components/hash → notability →
//! `build_stat_prompt` → the s7 options → the exact wire body) — and writes a source='rust' row to
//! `stat_summaries_shadow` with the parity axes: `built_prompt`, `ollama_request`, `model_version`,
//! `prompt_version`, AND `input_hash` (the 5th axis transfers lacked — rating debounces on it). The
//! Go side (`go/internal/ml/rating_parity_test.go`) writes source='go' rows the same way; the proof
//! is a self-join diff in that table.
//!
//! NO MODEL CALL by default (the L2 finding): the parity axes are all deterministic — the prose
//! (body/divined_peak) is NOT a temp-0 parity axis, so the gate needs no GPU. Set
//! `RATING_PARITY_VET=1` to ALSO run the
//! model at temp 0 and record body/divined_peak/notability — for eyeballing that the model verbalizes
//! the labeled TIER pctBand supplies and writes ~3 short paragraphs (inspection, not the gate).
//!
//! SAFETY: read-only on the live pipeline. It NEVER writes stat_summaries, NEVER claims/enqueues
//! pipeline_work, NEVER runs the Go batch. It reads the rating profile + writes the standalone shadow.
//!
//! Usage (env from .env.local: DATABASE_PRIVATE_URL + OLLAMA_*):
//!   rating_parity player:15:NBA team:18:FOOTBALL   # explicit entities (entity_type:id:sport)

use anyhow::{anyhow, Context, Result};
use scoracle_cognition::config::Config;
use scoracle_cognition::harness::Harness;
use scoracle_cognition::ollama::OllamaClient;
use scoracle_cognition::rating::{
    build_rating_request, generate_rating, RatingBuild, RatingOutput, RatingReady, RatingReq,
    RATING_PROMPT_VERSION,
};
use scoracle_cognition::route::Router;
use scoracle_cognition::{corpus, db};
use sqlx::PgPool;

/// The explicit, deterministic temperature the parity diff is taken at (pins temp-0 in the body).
const PARITY_TEMPERATURE: f64 = 0.0;

#[derive(Clone, Debug)]
struct EntitySpec {
    entity_type: String, // "player" | "team"
    entity_id: i32,
    sport: String, // UPPER
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    let vet = std::env::var("RATING_PARITY_VET").is_ok_and(|v| !v.is_empty() && v != "0");
    if vet {
        // Only the --vet path calls the model; ping so a dead Ollama fails fast.
        let ollama =
            OllamaClient::new(&cfg.ollama_base_url, &cfg.ollama_model, cfg.ollama_timeout)?;
        ollama
            .ping()
            .await
            .context("ollama must be reachable for RATING_PARITY_VET=1")?;
    }

    // The same capability context the production worker builds (offline → governor 1, no embedder).
    let harness = Harness {
        pool,
        router: Router::from_config(&cfg.route, cfg.ollama_timeout, 1)?,
        embedder: None,
        resolve: cfg.resolve.clone(),
        scrub: cfg.scrub.clone(),
        bucket_classifier: None,
    };

    let specs = parse_specs(std::env::args().skip(1))?;
    if specs.is_empty() {
        return Err(anyhow!(
            "no entities to process — pass entity_type:id:sport specs (player|team), e.g. player:15:NBA team:18:FOOTBALL"
        ));
    }

    println!(
        "rating parity harness — source=rust, prompt_version={RATING_PROMPT_VERSION}, temp={PARITY_TEMPERATURE}, vet={vet}, {} entit(ies) → stat_summaries_shadow",
        specs.len()
    );

    let mut written = 0usize;
    for s in &specs {
        match run_entity(&harness, s, vet).await {
            Ok(true) => written += 1,
            Ok(false) => println!(
                "  · {}/{} ({}) → skipped (no usable rating profile)",
                s.entity_type, s.entity_id, s.sport
            ),
            Err(e) => println!(
                "  ✗ {}/{} ({}) → {e:#}",
                s.entity_type, s.entity_id, s.sport
            ),
        }
    }

    println!("done — {written} entit(ies) written to stat_summaries_shadow (source=rust)");
    Ok(())
}

/// run_entity dumps one entity's deterministic parity axes (and, with vet, the temp-0 prose). Returns
/// Ok(false) when there is no usable rating profile (NoStats — no row written), Ok(true) on a write.
async fn run_entity(hx: &Harness, s: &EntitySpec, vet: bool) -> Result<bool> {
    let name = corpus::lookup_entity_name(&hx.pool, &s.entity_type, s.entity_id, &s.sport).await?;
    let req = RatingReq {
        entity_type: s.entity_type.clone(),
        entity_id: s.entity_id,
        entity_name: name.clone(),
        sport: s.sport.clone(),
        season: None, // latest — both sides resolve the same season via the same ORDER BY
        trigger_type: "periodic".to_string(),
    };

    // with_memory=false: parity pins the memory-free prompt shape (the s12/n8 discipline)
    // so the diff axes stay byte-stable against the Go-era rows.
    let ready = match build_rating_request(hx, &req, PARITY_TEMPERATURE, false).await? {
        RatingBuild::NoStats { .. } => return Ok(false),
        RatingBuild::Ready(r) => *r,
    };

    // Optional model run (temp 0) — body/divined_peak for INSPECTION only (never the gate).
    let vetted: Option<RatingOutput> = if vet {
        Some(generate_rating(hx, &req, PARITY_TEMPERATURE, false, false).await?)
    } else {
        None
    };

    persist_shadow(&hx.pool, s, &ready, vetted.as_ref()).await?;
    let tail = vetted
        .as_ref()
        .and_then(|v| v.divined_peak.clone())
        .map(|p| format!("PEAK={p}"))
        .unwrap_or_else(|| "deterministic".to_string());
    println!(
        "  ✓ {}/{} {} ({}) → season {} | notability {} | {} | {} bytes prompt | hash {}",
        s.entity_type,
        s.entity_id,
        name,
        s.sport,
        ready.season,
        ready.notability,
        tail,
        ready.built_prompt.len(),
        ready.input_hash,
    );
    Ok(true)
}

/// persist_shadow writes one source='rust' row. The deterministic axes (built_prompt, ollama_request,
/// model/prompt version, input_hash, notability, input_components) come from `build_rating_request`;
/// body/divined_peak are filled only when `vetted` is present (RATING_PARITY_VET=1) — inspection.
async fn persist_shadow(
    pool: &PgPool,
    s: &EntitySpec,
    ready: &RatingReady,
    vetted: Option<&RatingOutput>,
) -> Result<()> {
    let request_json = ready.request_body.to_string();
    let notability = ready.notability as i16;
    let ncomp_json = ready.notability_components.to_string();
    let body = vetted.and_then(|v| v.body.clone());
    let divined_peak = vetted.and_then(|v| v.divined_peak.clone());

    sqlx::query(
        r#"
        INSERT INTO stat_summaries_shadow (
            source, entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            body, divined_peak, notability, notability_components, input_components, input_hash,
            model_version, prompt_version, temperature, built_prompt, ollama_request
        ) VALUES ('rust',$1,$2,$3,$4,'periodic','{}'::jsonb,$5,$6,$7,$8::jsonb,$9::jsonb,$10,$11,$12,$13,$14,$15::jsonb)
        "#,
    )
    .bind(&s.entity_type)
    .bind(s.entity_id)
    .bind(&s.sport)
    .bind(ready.season)
    .bind(body)
    .bind(divined_peak)
    .bind(notability)
    .bind(&ncomp_json)
    .bind(&ready.input_components)
    .bind(&ready.input_hash)
    .bind(&ready.model_configured)
    .bind(RATING_PROMPT_VERSION)
    .bind(PARITY_TEMPERATURE as f32)
    .bind(&ready.built_prompt)
    .bind(request_json)
    .execute(pool)
    .await
    .context("persist rating shadow row")?;
    Ok(())
}

/// parse_specs reads `entity_type:id:sport` tokens (player|team; rating is entity-keyed like vibe/sigil).
fn parse_specs(args: impl Iterator<Item = String>) -> Result<Vec<EntitySpec>> {
    let mut out = Vec::new();
    for a in args {
        let parts: Vec<&str> = a.split(':').collect();
        if parts.len() != 3 {
            return Err(anyhow!("bad spec {a:?}; want entity_type:id:sport"));
        }
        let entity_type = parts[0].to_lowercase();
        if entity_type != "player" && entity_type != "team" {
            return Err(anyhow!("bad spec {a:?}; entity_type must be player|team"));
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
