//! L11 transfers parity harness — the proof that the Rust transfers stage's MACHINERY matches Go.
//!
//! For each (team, player) candidate pair it runs the SAME deterministic prefix the production
//! `TransferHandler` uses — `build_pair_request` (compute_transfer_heat → corpus → relationship →
//! `build_transfer_prompt` → the t4 options → the exact wire body) — and writes a source='rust' row
//! to `transfer_rumors_shadow` with the parity axes: `built_prompt`, `ollama_request`,
//! `model_version`, `prompt_version`. The Go side (`go/internal/ml/transfer_parity_test.go`) writes
//! source='go' rows the same way; the proof is a self-join diff in that table.
//!
//! NO MODEL CALL by default (the L2 finding): the parity axes are all deterministic — the verdict
//! (is_rumor/stage/summary) is NOT a temp-0 parity axis, so the gate needs no GPU and is fully
//! deterministic. The diff expects `built_prompt` byte-equal and `ollama_request` equal EXCEPT the
//! `system` field — the ONE intended divergence (Go's frozen t3 vs Rust's t4: the roundup/listicle
//! clause + never-invent-a-fee, the L9 false-heat root fix single-homed in Rust). Set
//! `TRANSFER_PARITY_VET=1` to ALSO run the model at temp 0 and record the verdict columns — for
//! eyeballing that t4 actually clears roundups / never invents a fee (inspection, not the gate).
//!
//! Pair set: `load_candidates` (the production loader — identical SQL to Go) then SORT BY player_id
//! then take `TRANSFER_PARITY_MAX_PAIRS` (default 4) — a bin-only deterministic cap so the Rust and
//! Go runs cover the IDENTICAL pairs regardless of the loader's tie order. The production loader is
//! untouched (still byte-identical to Go — the parity contract).
//!
//! SAFETY: read-only on the live pipeline. It NEVER writes transfer_rumors, NEVER claims/enqueues
//! pipeline_work, NEVER runs the Go stage. It reads the corpus + writes the standalone shadow table.
//! Run it while Go's drainTransfers keeps owning the live transfers stage.
//!
//! Usage (env from .env.local: DATABASE_PRIVATE_URL + OLLAMA_*):
//!   transfer_parity                       # auto-select TRANSFER_PARITY_TEAMS (default 3) teams
//!   transfer_parity team:14:NBA team:9:FOOTBALL   # explicit teams (team:id:sport)

use anyhow::{anyhow, Context, Result};
use scoracle_cognition::config::Config;
use scoracle_cognition::harness::Harness;
use scoracle_cognition::ollama::OllamaClient;
use scoracle_cognition::route::Router;
use scoracle_cognition::transfer::{
    analyze_pair, build_pair_request, direction_for, load_candidates, load_tier_map, PairBuild,
    TransferCandidate, TransferPairOutput, TRANSFER_DEFAULT_MIN_ARTICLES, TRANSFER_PROMPT_VERSION,
};
use scoracle_cognition::{db, vibe};
use sqlx::PgPool;

/// The explicit, deterministic temperature the parity diff is taken at (pins temp-0 in the body).
const PARITY_TEMPERATURE: f64 = 0.0;

#[derive(Clone, Debug)]
struct TeamSpec {
    team_id: i32,
    sport: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    let vet = std::env::var("TRANSFER_PARITY_VET").is_ok_and(|v| !v.is_empty() && v != "0");
    if vet {
        // Only the --vet path calls the model; ping so a dead Ollama fails fast.
        let ollama =
            OllamaClient::new(&cfg.ollama_base_url, &cfg.ollama_model, cfg.ollama_timeout)?;
        ollama
            .ping()
            .await
            .context("ollama must be reachable for TRANSFER_PARITY_VET=1")?;
    }

    // The same capability context the production worker builds (offline → governor 1, no embedder).
    let harness = Harness {
        pool,
        router: Router::from_config(&cfg.route, cfg.ollama_timeout, 1)?,
        embedder: None,
        resolve: cfg.resolve.clone(),
    };

    let max_pairs: usize = std::env::var("TRANSFER_PARITY_MAX_PAIRS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);

    let specs = match parse_specs(std::env::args().skip(1))? {
        v if !v.is_empty() => v,
        _ => {
            let limit: i64 = std::env::var("TRANSFER_PARITY_TEAMS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3);
            auto_select(&harness.pool, limit).await?
        }
    };

    println!(
        "transfers parity harness — source=rust, prompt_version={TRANSFER_PROMPT_VERSION}, temp={PARITY_TEMPERATURE}, vet={vet}, {} team(s) → transfer_rumors_shadow",
        specs.len()
    );
    if specs.is_empty() {
        return Err(anyhow!(
            "no teams to process — pass team:id:sport specs, or ensure recent vetted co-mention corpus exists"
        ));
    }

    let mut pairs = 0usize;
    for s in &specs {
        let team_name =
            match vibe::lookup_entity_name(&harness.pool, "team", s.team_id, &s.sport).await {
                Ok(n) => n,
                Err(e) => {
                    println!("  ✗ team/{} ({}) → name lookup: {e:#}", s.team_id, s.sport);
                    continue;
                }
            };
        let tiers = match load_tier_map(&harness.pool).await {
            Ok(t) => t,
            Err(e) => {
                println!("  ✗ team/{} → load tiers: {e:#}", s.team_id);
                continue;
            }
        };
        let mut candidates = match load_candidates(
            &harness.pool,
            s.team_id,
            &s.sport,
            TRANSFER_DEFAULT_MIN_ARTICLES,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                println!("  ✗ team/{} → load candidates: {e:#}", s.team_id);
                continue;
            }
        };
        // Deterministic, Go-matchable cap: sort by player_id, take the first N.
        candidates.sort_by_key(|c| c.player_id);
        candidates.truncate(max_pairs);
        println!(
            "  team/{} {} ({}) — {} candidate pair(s): [{}]",
            s.team_id,
            team_name,
            s.sport,
            candidates.len(),
            candidates
                .iter()
                .map(|c| c.player_id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        for c in &candidates {
            match run_pair(&harness, s, &team_name, c, &tiers, vet).await {
                Ok(true) => pairs += 1,
                Ok(false) => println!(
                    "    · player/{} ({}) → skipped (no pair corpus)",
                    c.player_id, c.player_name
                ),
                Err(e) => println!("    ✗ player/{} → {e:#}", c.player_id),
            }
        }
    }

    println!("done — {pairs} pair(s) written to transfer_rumors_shadow (source=rust)");
    Ok(())
}

/// run_pair dumps one pair's deterministic parity axes (and, with vet, the temp-0 verdict). Returns
/// Ok(false) when the pair has no corpus (Skipped — no row written), Ok(true) when a row is written.
async fn run_pair(
    hx: &Harness,
    s: &TeamSpec,
    team_name: &str,
    c: &TransferCandidate,
    tiers: &std::collections::HashMap<String, f64>,
    vet: bool,
) -> Result<bool> {
    let ready = match build_pair_request(
        hx,
        s.team_id,
        team_name,
        c,
        &s.sport,
        tiers,
        PARITY_TEMPERATURE,
    )
    .await?
    {
        PairBuild::Skipped { .. } => return Ok(false),
        PairBuild::Ready(r) => *r,
    };

    // Optional model vetting (temp 0) — verdict columns for INSPECTION only (never the gate).
    let vetted: Option<TransferPairOutput> = if vet {
        Some(
            analyze_pair(
                hx,
                s.team_id,
                team_name,
                c,
                &s.sport,
                tiers,
                PARITY_TEMPERATURE,
            )
            .await?,
        )
    } else {
        None
    };

    persist_shadow(&hx.pool, s, c.player_id, &ready, vetted.as_ref()).await?;
    let label = vetted
        .as_ref()
        .map(|v| format!("{:?}", v.outcome))
        .unwrap_or_else(|| "deterministic".to_string());
    println!(
        "    ✓ player/{} ({}) → heat {} | {} | {} bytes prompt",
        c.player_id,
        c.player_name,
        ready.heat,
        label,
        ready.built_prompt.len()
    );
    Ok(true)
}

/// persist_shadow writes one source='rust' row. `built_prompt` + `ollama_request` (+ model/prompt
/// version) are the deterministic parity axes from `build_pair_request`; the verdict columns are
/// filled only when `vetted` is present (TRANSFER_PARITY_VET=1) — inspection, not the gate.
async fn persist_shadow(
    pool: &PgPool,
    s: &TeamSpec,
    player_id: i32,
    ready: &scoracle_cognition::transfer::PairReady,
    vetted: Option<&TransferPairOutput>,
) -> Result<()> {
    // Deterministic direction (matches Go's directionFor(relationship)) — recorded for inspection.
    let direction = direction_for(&ready.relationship).to_string();
    let request_json = ready.request_body.to_string();

    // Verdict columns — NULL unless vetted.
    let row = vetted.and_then(|v| v.row.clone());
    let is_rumor = row.as_ref().and_then(|r| r.is_rumor);
    let stage = row.as_ref().and_then(|r| r.stage.clone());
    let summary = row.as_ref().and_then(|r| r.summary.clone());
    let attribution = row
        .as_ref()
        .and_then(|r| r.attribution.clone())
        .or_else(|| (!ready.attribution.is_empty()).then(|| ready.attribution.clone()));
    let confidence: Option<f32> = row.as_ref().and_then(|r| r.confidence).map(|c| c as f32);
    let trigger_payload = row
        .as_ref()
        .map(|r| r.trigger_payload.clone())
        .unwrap_or_else(|| "{}".to_string());

    sqlx::query(
        r#"
        INSERT INTO transfer_rumors_shadow (
            source, team_id, player_id, sport, trigger_type, trigger_payload,
            heat, heat_components, is_rumor, direction, stage, gemma_summary,
            source_attribution, confidence, input_news_ids, model_version, prompt_version,
            temperature, built_prompt, ollama_request
        ) VALUES ('rust',$1,$2,$3,'periodic',$4::jsonb,$5,$6::jsonb,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18::jsonb)
        "#,
    )
    .bind(s.team_id)
    .bind(player_id)
    .bind(&s.sport)
    .bind(&trigger_payload)
    .bind(ready.heat)
    .bind(&ready.components)
    .bind(is_rumor)
    .bind(&direction)
    .bind(stage)
    .bind(summary)
    .bind(attribution)
    .bind(confidence)
    .bind(ready.news_ids.as_slice())
    .bind(&ready.model_configured)
    .bind(TRANSFER_PROMPT_VERSION)
    .bind(PARITY_TEMPERATURE as f32)
    .bind(&ready.built_prompt)
    .bind(request_json)
    .execute(pool)
    .await
    .context("persist transfer shadow row")?;
    Ok(())
}

/// parse_specs reads `team:id:sport` tokens (transfers is team-keyed).
fn parse_specs(args: impl Iterator<Item = String>) -> Result<Vec<TeamSpec>> {
    let mut out = Vec::new();
    for a in args {
        let parts: Vec<&str> = a.split(':').collect();
        if parts.len() != 3 {
            return Err(anyhow!("bad spec {a:?}; want team:id:sport"));
        }
        if parts[0].to_lowercase() != "team" {
            return Err(anyhow!(
                "bad spec {a:?}; transfers is team-keyed (want team:id:sport)"
            ));
        }
        let team_id: i32 = parts[1]
            .parse()
            .with_context(|| format!("bad id in {a:?}"))?;
        out.push(TeamSpec {
            team_id,
            sport: parts[2].to_uppercase(),
        });
    }
    Ok(out)
}

/// auto_select picks teams with the most recent vetted team-links — those most likely to have
/// co-mention candidate pairs with corpus (so the run lands on real, scoreable rows).
async fn auto_select(pool: &PgPool, limit: i64) -> Result<Vec<TeamSpec>> {
    let rows: Vec<(i32, String)> = sqlx::query_as(
        r#"
        SELECT te.entity_id, te.sport
        FROM news_article_entities te
        WHERE te.entity_type = 'team'
          AND te.vetted IS TRUE
          AND te.created_at > NOW() - INTERVAL '14 days'
        GROUP BY te.entity_id, te.sport
        ORDER BY count(DISTINCT te.article_id) DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("auto-select parity teams")?;
    Ok(rows
        .into_iter()
        .map(|(team_id, sport)| TeamSpec { team_id, sport })
        .collect())
}
