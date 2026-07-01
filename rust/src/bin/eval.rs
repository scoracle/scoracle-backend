//! A/B model eval harness — the router's eval discipline made executable (Plan §2.2).
//!
//! Runs a labeled set through the role's INCUMBENT (`router.for_role`) AND its optional
//! CANDIDATE (`router.candidate_for`), scoring each against the labels, and — for the L7
//! news-model A/B — capturing each model's **prose** (the vibe felt-read) and **throughput**
//! (Ollama's per-call eval tokens / generation time). This is what turns "add a model" from
//! an assertion into an experiment: a model is adopted ONLY on a measured win, and adoption is
//! a HUMAN editing `COGNITION_ROUTE_<ROLE>` after reading this report — the router NEVER
//! auto-promotes. "A new model is a config change + an eval win — never an act of faith."
//!
//! Two measurement axes, both label-free where they need to be:
//!   - QUALITY: the side-by-side prose (SCORE + VIBE) per entity — the right axis for a
//!     news/emotion model (a blind read of which felt-read is better). No labels required.
//!   - THROUGHPUT: per-call tok/s from `GenerateResult` (eval_count / total_duration). The
//!     batch runs all-incumbent then all-candidate, so the candidate's FIRST call carries the
//!     single model-swap cost (cold load) and its warm calls show steady tok/s — exactly the
//!     "sequential residence + batch-by-model" property L7 is testing on the one 1070.
//!   - MAE (optional): when a case carries `=human_label`, mean absolute error vs the label.
//!
//! Scope: L2 evaluates the vibe task (`Role::EmotionalNews`) — the stage with a scoring
//! function + the prose we care about. The labeled set is `entity_type:id:sport[=human_score]`;
//! the `=human_score` is OPTIONAL (omit it for a label-free quality+throughput A/B).
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
//!   eval player:237:NBA team:14:NBA            # label-free quality+throughput A/B
//!   eval player:237:NBA=72 team:14:NBA=55      # + MAE vs human labels
//!   COGNITION_ROUTE_EMOTIONAL_NEWS_CANDIDATE=mistral:7b eval player:237:NBA   # with a challenger

use anyhow::{anyhow, Context, Result};
use scoracle_cognition::config::{Config, RouteConfig};
use scoracle_cognition::db;
use scoracle_cognition::harness::Harness;
use scoracle_cognition::ollama::GenerateOptions;
use scoracle_cognition::route::{Inference, Role, Router};
use scoracle_cognition::vibe::{
    build_sentiment_prompt, load_latest_narratives, load_transfer_heat, lookup_entity_name,
    parse_sentiment_and_prompt, VIBE_NUM_PREDICT, VIBE_SYSTEM_PROMPT,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

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

/// EvalCase pairs an entity with its OPTIONAL human label — the ground truth a model is scored
/// against. For vibe (EmotionalNews) the label is a human sentiment read (1-100); `None` runs a
/// label-free quality+throughput A/B (the L7 news-model comparison).
#[derive(Clone, Debug)]
struct EvalCase {
    entity: EntitySpec,
    label: Option<f64>,
}

/// CaseResult is one backend's answer for one entity: the parsed score (None = unparseable),
/// the felt-read prose, and Ollama's perf metrics for the call (tokens + generation time).
#[derive(Clone, Debug)]
struct CaseResult {
    key: String,
    score: Option<i32>,
    vibe: String,
    total_duration: Duration,
    eval_count: i32,
}

impl CaseResult {
    fn tok_per_s(&self) -> f64 {
        let s = self.total_duration.as_secs_f64();
        if s > 0.0 && self.eval_count > 0 {
            self.eval_count as f64 / s
        } else {
            0.0
        }
    }
}

/// ModelScore is how one backend did over the labeled set: mean absolute error vs the human
/// labels (when labels were given), and how many cases produced a parseable score.
#[derive(Clone, Debug)]
struct ModelScore {
    model: String,
    /// Mean absolute error vs the human labels; `None` when no labeled case scored.
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

    // No cases → just show what the config resolves to (a zero-DB, zero-Ollama smoke that
    // proves `RouteConfig::from_env` parsed) and how to run a real A/B.
    if cases.is_empty() {
        print_route_table(&cfg.route);
        println!(
            "\nusage: eval <entity_type:id:sport[=human_label]> ...\n  \
             e.g. eval player:237:NBA team:14:NBA   (label-free quality+throughput A/B)\n  \
             e.g. eval player:237:NBA=72            (+ MAE vs a human label)\n  \
             set COGNITION_ROUTE_{}_CANDIDATE=<model> to enable the A/B challenger",
            EVAL_ROLE.env_suffix()
        );
        return Ok(());
    }

    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    // Offline A/B harness — single-flight, so the GPU governor is moot; pin 1.
    let router = Router::from_config(&cfg.route, cfg.ollama_timeout, 1)?;
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

    println!(
        "\nincumbent: {} (drain all of this model first)",
        incumbent.model()
    );
    let (incumbent_score, inc_results) = score_backend(&harness, &incumbent, &cases).await?;

    let (candidate_score, cand_results) = match candidate.as_ref() {
        Some(c) => {
            println!("\ncandidate: {} (ONE swap to here, then drain)", c.model());
            let (s, r) = score_backend(&harness, c, &cases).await?;
            (Some(s), Some(r))
        }
        None => {
            println!(
                "\ncandidate: none (set COGNITION_ROUTE_{}_CANDIDATE=<model> to A/B)",
                EVAL_ROLE.env_suffix()
            );
            (None, None)
        }
    };

    print_report(&EvalReport {
        role: EVAL_ROLE,
        incumbent: incumbent_score,
        candidate: candidate_score,
        n: cases.len(),
    });

    // Throughput — the L7 axis. The candidate's first-call time carries the single model-swap
    // cost (cold load); warm tok/s is the steady batch rate once resident.
    println!("\n=== throughput (Ollama eval tokens / generation time) ===");
    print_throughput(incumbent.model(), &inc_results);
    if let Some(c) = candidate.as_ref() {
        if let Some(cr) = cand_results.as_ref() {
            print_throughput(c.model(), cr);
        }
    }

    // Quality — the side-by-side prose for a blind read of which felt-read is better.
    if let (Some(c), Some(cr)) = (candidate.as_ref(), cand_results.as_ref()) {
        print_side_by_side(incumbent.model(), &inc_results, c.model(), cr);
    }
    Ok(())
}

/// score_backend runs every case through one backend and returns its MAE (over labeled cases)
/// plus the per-case results (score, prose, perf). It reuses the SAME public vibe loaders +
/// prompt the production handler runs (only the backend differs), at temperature 0. Cases with
/// no corpus (no narratives AND no heat) are skipped — they carry no model judgment to compare.
async fn score_backend(
    hx: &Harness,
    backend: &Arc<dyn Inference>,
    cases: &[EvalCase],
) -> Result<(ModelScore, Vec<CaseResult>)> {
    let opts = GenerateOptions {
        system: Some(VIBE_SYSTEM_PROMPT.to_string()),
        temperature: Some(EVAL_TEMPERATURE),
        num_predict: VIBE_NUM_PREDICT,
        json_mode: false,
    };

    let mut abs_err_sum = 0.0f64;
    let mut mae_n = 0usize;
    let mut results: Vec<CaseResult> = Vec::with_capacity(cases.len());
    for case in cases {
        let prompt = match build_vibe_prompt(hx, &case.entity).await? {
            Some(p) => p,
            None => {
                println!("  – {} : no corpus (skipped)", case.entity.key());
                continue;
            }
        };
        let gen = match backend.generate(&prompt, &opts).await {
            Ok(g) => g,
            Err(e) => {
                // Under GPU contention a call can time out waiting in Ollama's queue; skip it
                // rather than abort the whole batch (a partial A/B is still useful).
                println!(
                    "  ! {} : generate failed ({e:#}) — skipped",
                    case.entity.key()
                );
                continue;
            }
        };
        let (score, vibe) = match parse_sentiment_and_prompt(&gen.response) {
            Ok((s, v)) => (Some(s), v),
            Err(e) => {
                println!("  ! {} : unparseable ({e:#})", case.entity.key());
                (None, String::new())
            }
        };
        let result = CaseResult {
            key: case.entity.key(),
            score,
            vibe,
            total_duration: gen.total_duration,
            eval_count: gen.eval_count,
        };
        // MAE only when both a label and a parsed score exist.
        if let (Some(s), Some(label)) = (score, case.label) {
            abs_err_sum += (s as f64 - label).abs();
            mae_n += 1;
        }
        let label_note = match (score, case.label) {
            (Some(s), Some(label)) => format!(" human={label} |Δ|={:.0}", (s as f64 - label).abs()),
            _ => String::new(),
        };
        println!(
            "  · {} : score={} {:.1} tok/s ({} tok / {:.1}s){label_note}",
            result.key,
            score.map(|s| s.to_string()).unwrap_or_else(|| "??".into()),
            result.tok_per_s(),
            result.eval_count,
            result.total_duration.as_secs_f64(),
        );
        if !result.vibe.is_empty() {
            println!("      vibe: {}", result.vibe);
        }
        results.push(result);
    }

    let scored = results.iter().filter(|r| r.score.is_some()).count();
    Ok((
        ModelScore {
            model: backend.model().to_string(),
            mae: (mae_n > 0).then(|| abs_err_sum / mae_n as f64),
            scored,
        },
        results,
    ))
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

/// print_throughput summarizes one model's batch: the first-call time (which for the candidate
/// carries the single cold model-swap cost) and the WARM tok/s (calls 2..n, once resident) —
/// the steady rate that matters for "drain the resident model's whole queue before swapping."
fn print_throughput(model: &str, results: &[CaseResult]) {
    let timed: Vec<&CaseResult> = results.iter().filter(|r| r.eval_count > 0).collect();
    if timed.is_empty() {
        println!("  {model:<16} no timed calls");
        return;
    }
    let first = timed[0];
    let warm = &timed[1..];
    let (warm_tok, warm_sec) = warm.iter().fold((0i64, 0.0f64), |(t, s), r| {
        (t + r.eval_count as i64, s + r.total_duration.as_secs_f64())
    });
    let warm_tokps = if warm_sec > 0.0 {
        warm_tok as f64 / warm_sec
    } else {
        0.0
    };
    let warm_mean_s = if !warm.is_empty() {
        warm_sec / warm.len() as f64
    } else {
        0.0
    };
    println!(
        "  {model:<16} first-call {:.1}s ({} tok) | warm {:.2} tok/s, mean {:.1}s/call over {} calls",
        first.total_duration.as_secs_f64(),
        first.eval_count,
        warm_tokps,
        warm_mean_s,
        warm.len(),
    );
}

/// print_side_by_side renders incumbent-vs-candidate prose per entity — the blind quality read
/// for "which felt-read is the better news/emotion answer." Matched by entity key (both
/// backends skip the same no-corpus cases, so a missing side is rare and shown as such).
fn print_side_by_side(inc_model: &str, inc: &[CaseResult], cand_model: &str, cand: &[CaseResult]) {
    let cmap: HashMap<&str, &CaseResult> = cand.iter().map(|r| (r.key.as_str(), r)).collect();
    println!("\n=== SIDE-BY-SIDE prose (blind A/B fodder) ===");
    for a in inc {
        println!("\n[{}]", a.key);
        println!(
            "  A {inc_model:<14} score={} | {}",
            a.score
                .map(|s| s.to_string())
                .unwrap_or_else(|| "??".into()),
            a.vibe,
        );
        match cmap.get(a.key.as_str()) {
            Some(b) => println!(
                "  B {cand_model:<14} score={} | {}",
                b.score
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "??".into()),
                b.vibe,
            ),
            None => println!("  B {cand_model:<14} (no result)"),
        }
    }
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
                _ => println!(
                    "→ no MAE winner (label-free run or too few labeled cases). Judge on the \
                     side-by-side prose + throughput below."
                ),
            }
        }
    }
}

fn fmt_score(s: &ModelScore, n: usize) -> String {
    match s.mae {
        Some(mae) => format!("MAE={mae:.2} (scored {}/{})", s.scored, n),
        None => format!("MAE=n/a  (scored {}/{n})", s.scored),
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
            Some(c) => println!(
                "  {:<14} → {incumbent}  [candidate: {}]",
                role.as_str(),
                c.model
            ),
            None => println!("  {:<14} → {incumbent}", role.as_str()),
        }
    }
}

/// parse_cases reads `entity_type:id:sport[=human_label]` tokens from the CLI args.
fn parse_cases(args: impl Iterator<Item = String>) -> Result<Vec<EvalCase>> {
    args.map(|a| parse_case(&a)).collect()
}

fn parse_case(arg: &str) -> Result<EvalCase> {
    match arg.split_once('=') {
        Some((entity_part, label_part)) => {
            let label: f64 = label_part
                .trim()
                .parse()
                .with_context(|| format!("bad label in {arg:?}"))?;
            Ok(EvalCase {
                entity: parse_entity(entity_part)?,
                label: Some(label),
            })
        }
        None => Ok(EvalCase {
            entity: parse_entity(arg)?,
            label: None,
        }),
    }
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
