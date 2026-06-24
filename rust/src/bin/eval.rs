//! A/B model eval harness — the router's eval discipline made executable (Plan §2.2).
//!
//! Runs a human-labeled set through the role's INCUMBENT (`router.for_role`) AND its optional
//! CANDIDATE (`router.candidate_for`), scores each against the labels, and prints the delta.
//! This is what turns "add Mistral" from an assertion into an experiment: a model is adopted
//! ONLY on a measured win, and adoption is a HUMAN editing `COGNITION_ROUTE_<ROLE>` after
//! reading this report — the router NEVER auto-promotes. "A new model is a config change + an
//! eval win — never an act of faith."
//!
//! Scope: L2 evaluates the vibe task (`Role::EmotionalNews`) — the only stage with a scoring
//! function + labels today. The labeled set is `entity_type:id:sport=human_score` (the human's
//! 1-100 sentiment read). When later stages land, this grows a per-role prompt+score builder;
//! the `for_role` / `candidate_for` plumbing is already role-general.
//!
//! Scoring runs at temperature 0 (deterministic, so the comparison is reproducible and free
//! of single-sample sampling noise) over the SAME public vibe loaders + prompt the production
//! handler uses — so the eval measures the real prompt, only the backend differs.
//!
//! SAFETY: like `bin/parity`, this is read-only on the live pipeline. It NEVER claims/enqueues
//! `pipeline_work`, NEVER writes a product table, and NEVER runs the service binary. It reads
//! the corpus tables and POSTs to Ollama; nothing else.
//!
//! Usage (env from .env.local: DATABASE_PRIVATE_URL + OLLAMA_*):
//!   eval                                       # print the resolved route table + usage
//!   eval player:237:NBA=72 team:14:NBA=55      # A/B the EmotionalNews incumbent vs candidate
//!   COGNITION_ROUTE_EMOTIONAL_NEWS_CANDIDATE=<model> eval player:237:NBA=72   # with a challenger

use anyhow::{anyhow, Context, Result};
use scoracle_cognition::config::{Config, RouteConfig};
use scoracle_cognition::harness::Harness;
use scoracle_cognition::ollama::GenerateOptions;
use scoracle_cognition::route::{Inference, Role, Router};
use scoracle_cognition::vibe::{
    build_sentiment_prompt, load_latest_narratives, load_transfer_heat, lookup_entity_name,
    parse_sentiment_and_prompt, VIBE_NUM_PREDICT, VIBE_SYSTEM_PROMPT,
};
use scoracle_cognition::db;
use std::sync::Arc;

/// Deterministic eval temperature — the comparison is reproducible and free of sampling noise
/// (production vibe runs at 0.7; the A/B compares each model's most-likely answer).
const EVAL_TEMPERATURE: f64 = 0.0;

/// The role this harness evaluates. L2: vibe is the only scorable stage.
const EVAL_ROLE: Role = Role::EmotionalNews;

#[derive(Clone, Debug)]
struct EntitySpec {
    entity_type: String,
    entity_id: i32,
    sport: String,
}

impl EntitySpec {
    fn key(&self) -> String {
        format!("{}:{}:{}", self.entity_type, self.entity_id, self.sport)
    }
}

/// EvalCase pairs an entity with its human label — the ground truth a model is scored against.
/// For vibe (EmotionalNews) the label is a human sentiment read (1-100).
#[derive(Clone, Debug)]
struct EvalCase {
    entity: EntitySpec,
    label: f64,
}

/// ModelScore is how one backend did over the labeled set: mean absolute error vs the human
/// labels, and how many cases produced a parseable score (no-corpus / unparseable are skipped).
#[derive(Clone, Debug)]
struct ModelScore {
    model: String,
    /// Mean absolute error vs the human labels; `None` when nothing scored.
    mae: Option<f64>,
    scored: usize,
}

/// EvalReport is the side-by-side the human reads: incumbent vs candidate over `n` cases. The
/// router never acts on this — a measured win is adopted by editing `COGNITION_ROUTE_<role>`.
#[derive(Clone, Debug)]
struct EvalReport {
    role: Role,
    incumbent: ModelScore,
    candidate: Option<ModelScore>,
    n: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    let cases = parse_cases(std::env::args().skip(1))?;

    // No labeled set → just show what the config resolves to (a zero-DB, zero-Ollama smoke
    // that proves `RouteConfig::from_env` parsed) and how to run a real A/B.
    if cases.is_empty() {
        print_route_table(&cfg.route);
        println!(
            "\nusage: eval <entity_type:id:sport=human_label> ...\n  \
             e.g. eval player:237:NBA=72 team:14:NBA=55\n  \
             set COGNITION_ROUTE_{}_CANDIDATE=<model> to enable the A/B challenger",
            EVAL_ROLE.env_suffix()
        );
        return Ok(());
    }

    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    let router = Router::from_config(&cfg.route, cfg.ollama_timeout)?;
    let harness = Harness {
        pool,
        router,
        embedder: None,
        resolve: cfg.resolve.clone(),
    };

    let incumbent = harness.router.for_role(EVAL_ROLE);
    let candidate = harness.router.candidate_for(EVAL_ROLE);

    println!(
        "eval — role={} n={} temp={} (deterministic)",
        EVAL_ROLE.as_str(),
        cases.len(),
        EVAL_TEMPERATURE
    );

    println!("\nincumbent: {}", incumbent.model());
    let incumbent_score = score_backend(&harness, &incumbent, &cases).await?;

    let candidate_score = match candidate.as_ref() {
        Some(c) => {
            println!("\ncandidate: {}", c.model());
            Some(score_backend(&harness, c, &cases).await?)
        }
        None => {
            println!(
                "\ncandidate: none (set COGNITION_ROUTE_{}_CANDIDATE=<model> to A/B)",
                EVAL_ROLE.env_suffix()
            );
            None
        }
    };

    print_report(&EvalReport {
        role: EVAL_ROLE,
        incumbent: incumbent_score,
        candidate: candidate_score,
        n: cases.len(),
    });
    Ok(())
}

/// score_backend runs every case through one backend and returns its MAE vs the human labels.
/// It reuses the SAME public vibe loaders + prompt the production handler runs (only the
/// backend differs), at temperature 0. Cases with no corpus (no narratives AND no heat) and
/// unparseable replies are skipped from the score, not counted as zero.
async fn score_backend(
    hx: &Harness,
    backend: &Arc<dyn Inference>,
    cases: &[EvalCase],
) -> Result<ModelScore> {
    let opts = GenerateOptions {
        system: Some(VIBE_SYSTEM_PROMPT.to_string()),
        temperature: Some(EVAL_TEMPERATURE),
        num_predict: VIBE_NUM_PREDICT,
        json_mode: false,
    };

    let mut abs_err_sum = 0.0f64;
    let mut scored = 0usize;
    for case in cases {
        let prompt = match build_vibe_prompt(hx, &case.entity).await? {
            Some(p) => p,
            None => {
                println!("  – {} : no corpus (skipped)", case.entity.key());
                continue;
            }
        };
        let gen = backend
            .generate(&prompt, &opts)
            .await
            .with_context(|| format!("generate for {}", case.entity.key()))?;
        match parse_sentiment_and_prompt(&gen.response) {
            Ok((model_score, _vibe)) => {
                let err = (model_score as f64 - case.label).abs();
                abs_err_sum += err;
                scored += 1;
                println!(
                    "  · {} : model={model_score} human={} |Δ|={err:.0}",
                    case.entity.key(),
                    case.label
                );
            }
            Err(e) => println!("  ! {} : unparseable ({e:#})", case.entity.key()),
        }
    }

    Ok(ModelScore {
        model: backend.model().to_string(),
        mae: (scored > 0).then(|| abs_err_sum / scored as f64),
        scored,
    })
}

/// build_vibe_prompt loads the entity's narratives + transfer heat and assembles the exact
/// production vibe prompt. `None` when the entity has no corpus (vibe would write a no-corpus
/// marker, not call the model) — such a case carries no model judgment to score.
async fn build_vibe_prompt(hx: &Harness, s: &EntitySpec) -> Result<Option<String>> {
    let name = lookup_entity_name(&hx.pool, &s.entity_type, s.entity_id, &s.sport).await?;
    // Reads use the upper-cased sport (the spec is already upper); the prompt uses the same
    // value the production path passes through, mirroring generate_vibe.
    let sport = s.sport.to_uppercase();
    let (narratives, _ids) =
        load_latest_narratives(&hx.pool, &s.entity_type, s.entity_id, &sport).await?;
    let heat = load_transfer_heat(&hx.pool, &s.entity_type, s.entity_id, &sport).await?;
    if narratives.is_empty() && heat.is_empty() {
        return Ok(None);
    }
    Ok(Some(build_sentiment_prompt(
        &s.entity_type,
        &name,
        &s.sport,
        &narratives,
        &heat,
    )))
}

/// print_report renders the incumbent-vs-candidate verdict and — crucially — states that the
/// router does NOT act on it: a measured win is adopted by a human editing the config.
fn print_report(r: &EvalReport) {
    println!("\n=== eval report — role={} n={} ===", r.role.as_str(), r.n);
    println!(
        "incumbent  {:<24} {}",
        r.incumbent.model,
        fmt_score(&r.incumbent, r.n)
    );
    match &r.candidate {
        None => {
            println!("candidate  (none configured — incumbent-only run)");
        }
        Some(c) => {
            println!("candidate  {:<24} {}", c.model, fmt_score(c, r.n));
            match (r.incumbent.mae, c.mae) {
                (Some(inc), Some(cand)) => {
                    let delta = inc - cand; // > 0 ⇒ candidate has lower error ⇒ better
                    if delta > 0.0 {
                        println!(
                            "→ candidate is BETTER by {delta:.2} MAE. To ADOPT (a human decision; \
                             the router never auto-promotes): set COGNITION_ROUTE_{}={}",
                            r.role.env_suffix(),
                            c.model
                        );
                    } else if delta < 0.0 {
                        println!(
                            "→ incumbent is better by {:.2} MAE. Keep the current config.",
                            -delta
                        );
                    } else {
                        println!("→ tie. Keep the incumbent (no measured win).");
                    }
                }
                _ => println!("→ not enough scored cases on both sides to call a winner."),
            }
        }
    }
}

fn fmt_score(s: &ModelScore, n: usize) -> String {
    match s.mae {
        Some(mae) => format!("MAE={mae:.2} (scored {}/{})", s.scored, n),
        None => format!("MAE=n/a  (scored 0/{n})"),
    }
}

/// print_route_table shows what each role resolves to (incumbent + any candidate) — the
/// no-args smoke output, proving the `COGNITION_ROUTE_*` config parsed.
fn print_route_table(cfg: &RouteConfig) {
    println!("configured route table (role → incumbent [+ candidate]):");
    for role in Role::all() {
        let incumbent = cfg
            .roles
            .get(&role)
            .map(|s| s.model.as_str())
            .unwrap_or("<unset>");
        match cfg.candidates.get(&role) {
            Some(c) => println!("  {:<14} → {incumbent}  [candidate: {}]", role.as_str(), c.model),
            None => println!("  {:<14} → {incumbent}", role.as_str()),
        }
    }
}

/// parse_cases reads `entity_type:id:sport=human_label` tokens from the CLI args.
fn parse_cases(args: impl Iterator<Item = String>) -> Result<Vec<EvalCase>> {
    args.map(|a| parse_case(&a)).collect()
}

fn parse_case(arg: &str) -> Result<EvalCase> {
    let (entity_part, label_part) = arg
        .split_once('=')
        .ok_or_else(|| anyhow!("bad eval case {arg:?}; want entity_type:id:sport=human_label"))?;
    let label: f64 = label_part
        .trim()
        .parse()
        .with_context(|| format!("bad label in {arg:?}"))?;
    Ok(EvalCase {
        entity: parse_entity(entity_part)?,
        label,
    })
}

fn parse_entity(s: &str) -> Result<EntitySpec> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return Err(anyhow!("bad entity {s:?}; want entity_type:id:sport"));
    }
    let entity_type = parts[0].to_lowercase();
    if entity_type != "player" && entity_type != "team" {
        return Err(anyhow!("bad entity_type in {s:?}; want player|team"));
    }
    let entity_id: i32 = parts[1]
        .parse()
        .with_context(|| format!("bad id in {s:?}"))?;
    Ok(EntitySpec {
        entity_type,
        entity_id,
        sport: parts[2].to_uppercase(),
    })
}
