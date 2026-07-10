//! A/B model eval harness — the router's eval discipline made executable (Plan §2.2), now
//! generalized to a per-lens TASK REGISTRY (Multi-Lens Cognition Panel, Phase 3).
//!
//! It runs a task's INCUMBENT (`router.for_role`) AND its optional CANDIDATE (`router.candidate_for`)
//! over a set of cases, scoring each via the task's `evaluate`. This is what turns "add a model"
//! from an assertion into an experiment: a model is adopted ONLY on a measured win, and adoption is
//! a HUMAN editing `COGNITION_ROUTE_<ROLE>` after reading this report — the router NEVER
//! auto-promotes. "A new model is a config change + an eval win — never an act of faith."
//!
//! Three modes:
//!   - LIVE (`eval [--task T] <entity:id:sport[=label]> ...`): builds each task's real production
//!     prompt from the LIVE corpus, at temperature 0 (deterministic). MAE vs a `=label` where the
//!     task has a numeric axis; throughput + side-by-side prose always. NOT reproducible once the
//!     corpus moves on.
//!   - FIXTURES (`eval --task T --fixtures [filter]`): runs FROZEN fixtures from `fixtures/<T>/`
//!     through the model and checks each fixture's `Expect` properties → a per-property ✓/✗ table.
//!     Reproducible — the regression gate. DB-free (Router-only).
//!   - CAPTURE (`eval --capture --task T <entity:id:sport>`): emits a fixture skeleton (frozen
//!     system + built prompt, empty `expect`) to STDOUT for a human to annotate. The bootstrap tool
//!     + stand-in for the Phase 2 ledger.
//!
//! Two live measurement axes:
//!   - QUALITY: the side-by-side prose per entity — a blind read of which answer is better. No
//!     labels required.
//!   - THROUGHPUT: per-call tok/s from `GenerateResult` (eval_count / total_duration). The batch
//!     runs all-incumbent then all-candidate, so the candidate's FIRST call carries the single
//!     model-swap cost (cold load) and its warm calls show steady tok/s.
//!   - MAE (optional): when a case carries `=human_label` and the task scores numerically.
//!
//! Scoring runs at temperature 0 (deterministic, reproducible, free of single-sample sampling
//! noise) over the SAME public loaders + prompt the production handler uses — so the eval measures
//! the real prompt, only the backend differs.
//!
//! SAFETY: like `bin/parity`, this is read-only on the live pipeline. It NEVER claims/enqueues
//! `pipeline_work`, NEVER writes a product table, and NEVER runs the service binary. Fixture mode
//! builds no DB pool at all; capture writes only stdout. It reads the corpus tables and POSTs to
//! Ollama; nothing else.
//!
//! Usage (env from .env.local: DATABASE_PRIVATE_URL + OLLAMA_*):
//!   eval                                          # print the resolved route table + usage
//!   eval player:237:NBA team:14:NBA               # vibe (default): label-free quality+throughput A/B
//!   eval player:237:NBA=72                        # + MAE vs a human label
//!   eval --task sigil player:237:NBA              # a different lens (live)
//!   eval --task transfer team:14:player:237:NBA   # transfer live pair A/B
//!   eval --task sigil --fixtures                  # frozen-fixture gate (reproducible)
//!   eval --capture --task sigil player:237:NBA    # emit a fixture skeleton to stdout
//!   COGNITION_ROUTE_STATS_LOGIC_CANDIDATE=mistral:7b eval --task sigil player:237:NBA  # A/B a challenger

use anyhow::{anyhow, Context, Result};
use scoracle_cognition::config::{Config, RouteConfig};
use scoracle_cognition::db;
use scoracle_cognition::eval_tasks::{
    all_task_names, fixture_drift, resolve_task, CaseVerdict, EntitySpec, Expect, Fixture, LensTask,
};
use scoracle_cognition::harness::Harness;
use scoracle_cognition::route::{Inference, Router};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Deterministic eval temperature — the live comparison is reproducible and free of sampling noise
/// (production stages sample warmer; the A/B compares each model's most-likely answer). Fixtures
/// carry their own frozen temperature.
const EVAL_TEMPERATURE: f64 = 0.0;

/// EvalCase pairs an entity with its OPTIONAL human label — the ground truth for the MAE axis. The
/// label is only meaningful for a task with a numeric score (vibe); `None` runs a label-free A/B.
#[derive(Clone, Debug)]
struct EvalCase {
    entity: EntitySpec,
    label: Option<f64>,
}

/// CaseResult is one backend's scored answer for one case: the task's verdict plus Ollama's perf
/// metrics for the call.
#[derive(Clone, Debug)]
struct CaseResult {
    key: String,
    verdict: CaseVerdict,
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

/// ModelScore is how one backend did over the set: mean absolute error vs the human labels (when a
/// numeric axis + labels were given), and how many cases produced a parseable answer.
#[derive(Clone, Debug)]
struct ModelScore {
    model: String,
    mae: Option<f64>,
    scored: usize,
}

/// EvalReport is the side-by-side the human reads: incumbent vs candidate over `n` cases.
#[derive(Clone, Debug)]
struct EvalReport {
    task: String,
    role: String,
    role_env: String,
    incumbent: ModelScore,
    candidate: Option<ModelScore>,
    n: usize,
}

enum Mode {
    Live,
    Fixtures { filter: Option<String> },
    Capture,
}

struct Args {
    task_name: String,
    mode: Mode,
    cases: Vec<EvalCase>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    let args = parse_args(std::env::args().skip(1))?;

    let task = resolve_task(&args.task_name).ok_or_else(|| {
        anyhow!(
            "unknown task {:?}; known tasks: {}",
            args.task_name,
            all_task_names().join(", ")
        )
    })?;

    match args.mode {
        Mode::Live => run_live(&cfg, task.as_ref(), &args.cases).await,
        Mode::Fixtures { filter } => run_fixtures(&cfg, task.as_ref(), filter.as_deref()).await,
        Mode::Capture => run_capture(&cfg, task.as_ref(), &args.cases).await,
    }
}

/// parse_args peels the flags (`--task`, `--fixtures`, `--capture`); the remaining positionals are
/// entity tokens. Default task is `vibe`, so the historical `eval player:237:NBA=72` is unchanged.
fn parse_args(argv: impl Iterator<Item = String>) -> Result<Args> {
    let mut task_name = "vibe".to_string();
    let mut mode = Mode::Live;
    let mut positionals: Vec<String> = Vec::new();

    let mut it = argv.peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--task" => {
                task_name = it
                    .next()
                    .ok_or_else(|| anyhow!("--task needs a value (e.g. --task sigil)"))?;
            }
            "--fixtures" => {
                // Optional filename-substring filter: the next arg iff it is not another flag.
                let filter = match it.peek() {
                    Some(n) if !n.starts_with("--") => it.next(),
                    _ => None,
                };
                mode = Mode::Fixtures { filter };
            }
            "--capture" => mode = Mode::Capture,
            other => positionals.push(other.to_string()),
        }
    }

    let cases = positionals
        .iter()
        .map(|s| parse_case(s))
        .collect::<Result<Vec<_>>>()?;
    Ok(Args {
        task_name,
        mode,
        cases,
    })
}

// ---------------------------------------------------------------------------
// LIVE mode
// ---------------------------------------------------------------------------

async fn run_live(cfg: &Config, task: &dyn LensTask, cases: &[EvalCase]) -> Result<()> {
    // No cases → just show what the config resolves to (zero-DB, zero-Ollama) + how to run.
    if cases.is_empty() {
        print_route_table(&cfg.route);
        println!(
            "\nusage: eval [--task <name>] <entity_type:id:sport[=human_label]> ...\n  \
             tasks: {}\n  \
             eval player:237:NBA team:14:NBA            (vibe, label-free quality+throughput A/B)\n  \
             eval player:237:NBA=72                     (+ MAE vs a human label)\n  \
             eval --task sigil --fixtures               (frozen-fixture regression gate)\n  \
             eval --capture --task sigil player:237:NBA (emit a fixture skeleton to stdout)\n  \
             eval --task transfer team:14:player:237:NBA (transfer live pair A/B)\n  \
             set COGNITION_ROUTE_{}_CANDIDATE=<model> to enable the A/B challenger",
            all_task_names().join(", "),
            task.role().env_suffix()
        );
        return Ok(());
    }

    let harness = build_harness(cfg).await?;
    let incumbent = harness.router.for_role(task.role());
    let candidate = harness.router.candidate_for(task.role());

    println!(
        "eval — task={} role={} n={} temp={} (deterministic)",
        task.name(),
        task.role().as_str(),
        cases.len(),
        EVAL_TEMPERATURE
    );

    println!(
        "\nincumbent: {} (drain all of this model first)",
        incumbent.model()
    );
    let (incumbent_score, inc_results) = score_backend(&harness, task, &incumbent, cases).await?;

    let (candidate_score, cand_results) = match candidate.as_ref() {
        Some(c) => {
            println!("\ncandidate: {} (ONE swap to here, then drain)", c.model());
            let (s, r) = score_backend(&harness, task, c, cases).await?;
            (Some(s), Some(r))
        }
        None => {
            println!(
                "\ncandidate: none (set COGNITION_ROUTE_{}_CANDIDATE=<model> to A/B)",
                task.role().env_suffix()
            );
            (None, None)
        }
    };

    print_report(&EvalReport {
        task: task.name().to_string(),
        role: task.role().as_str().to_string(),
        role_env: task.role().env_suffix().to_string(),
        incumbent: incumbent_score,
        candidate: candidate_score,
        n: cases.len(),
    });

    println!("\n=== throughput (Ollama eval tokens / generation time) ===");
    print_throughput(incumbent.model(), &inc_results);
    if let (Some(c), Some(cr)) = (candidate.as_ref(), cand_results.as_ref()) {
        print_throughput(c.model(), cr);
        print_side_by_side(incumbent.model(), &inc_results, c.model(), cr);
    }
    Ok(())
}

/// score_backend runs every case through one backend via the task's build_prompt/gen_options/
/// evaluate, and returns its MAE (over labeled cases with a numeric axis) plus the per-case results.
/// Cases with no corpus are skipped — they carry no model judgment to compare.
async fn score_backend(
    hx: &Harness,
    task: &dyn LensTask,
    backend: &Arc<dyn Inference>,
    cases: &[EvalCase],
) -> Result<(ModelScore, Vec<CaseResult>)> {
    let mut abs_err_sum = 0.0f64;
    let mut mae_n = 0usize;
    let mut results: Vec<CaseResult> = Vec::with_capacity(cases.len());
    for case in cases {
        let opts = task.gen_options_for(EVAL_TEMPERATURE, &case.entity);
        let prompt = match task.build_prompt(hx, &case.entity).await? {
            Some(p) => p,
            None => {
                println!("  – {} : no corpus (skipped)", case.entity.key());
                continue;
            }
        };
        let gen = match backend.generate(&prompt, &opts).await {
            Ok((g, _request_body)) => g,
            Err(e) => {
                // Under GPU contention a call can time out in Ollama's queue; skip it rather than
                // abort the whole batch (a partial A/B is still useful).
                println!(
                    "  ! {} : generate failed ({e:#}) — skipped",
                    case.entity.key()
                );
                continue;
            }
        };
        let verdict = task.evaluate(&gen.response, case.label, None);
        if let Some(err) = verdict.abs_err {
            abs_err_sum += err;
            mae_n += 1;
        }
        let label_note = match (verdict.abs_err, case.label) {
            (Some(err), Some(label)) => format!(" human={label} |Δ|={err:.0}"),
            _ => String::new(),
        };
        let result = CaseResult {
            key: case.entity.key(),
            verdict,
            total_duration: gen.total_duration,
            eval_count: gen.eval_count,
        };
        println!(
            "  · {} : {} {:.1} tok/s ({} tok / {:.1}s){label_note}",
            result.key,
            result.verdict.display,
            result.tok_per_s(),
            result.eval_count,
            result.total_duration.as_secs_f64(),
        );
        results.push(result);
    }

    let scored = results.iter().filter(|r| r.verdict.parsed).count();
    Ok((
        ModelScore {
            model: backend.model().to_string(),
            mae: (mae_n > 0).then(|| abs_err_sum / mae_n as f64),
            scored,
        },
        results,
    ))
}

// ---------------------------------------------------------------------------
// FIXTURE mode — the reproducible regression gate (DB-free, Router-only)
// ---------------------------------------------------------------------------

async fn run_fixtures(cfg: &Config, task: &dyn LensTask, filter: Option<&str>) -> Result<()> {
    // Router-only: no DB pool, no Harness — fixtures carry their own frozen prompts.
    let router = Router::from_config(&cfg.route, cfg.ollama_timeout, 1)?;
    let incumbent = router.for_role(task.role());
    let candidate = router.candidate_for(task.role());

    let dir = fixtures_dir(task.name());
    let fixtures = load_fixtures(&dir, filter)
        .with_context(|| format!("loading fixtures from {}", dir.display()))?;
    if fixtures.is_empty() {
        println!(
            "no fixtures in {}/ (filter={:?}). Author some, or `eval --capture --task {} <entity>`.",
            dir.display(),
            filter,
            task.name()
        );
        return Ok(());
    }

    println!(
        "fixtures — task={} n={} dir={}/  incumbent={}",
        task.name(),
        fixtures.len(),
        dir.display(),
        incumbent.model()
    );

    let mut inc_pass = 0usize;
    let mut inc_total = 0usize;
    for fx in &fixtures {
        if let Some(warn) = fixture_drift(fx, task) {
            println!("  ⚠ WARN {warn}");
        }
        println!("\n[{}]  (temp={})", fx.name, fx.temperature);
        let (p, t) = run_one_fixture("A", &incumbent, task, fx).await;
        inc_pass += p;
        inc_total += t;
        if let Some(c) = candidate.as_ref() {
            run_one_fixture("B", c, task, fx).await;
        }
    }

    println!(
        "\n=== fixture summary — {} : {inc_pass}/{inc_total} property checks passed ===",
        incumbent.model()
    );
    println!(
        "(a red check on a 'target' fixture is the documented honesty gap, not a harness failure)"
    );
    Ok(())
}

/// run_one_fixture runs one frozen fixture through one backend and prints the per-property table.
/// It uses the fixture's FROZEN system prompt (the point of a frozen fixture) with the task's
/// num_predict/json_mode. Returns (checks_passed, checks_total) for the summary tally.
async fn run_one_fixture(
    label: &str,
    backend: &Arc<dyn Inference>,
    task: &dyn LensTask,
    fx: &Fixture,
) -> (usize, usize) {
    let mut opts = task.gen_options(fx.temperature);
    opts.system = Some(fx.system.clone());
    let gen = match backend.generate(&fx.user_prompt, &opts).await {
        Ok((g, _)) => g,
        Err(e) => {
            println!("  {label} {:<16} generate failed ({e:#})", backend.model());
            return (0, 0);
        }
    };
    let verdict = task.evaluate(&gen.response, None, Some(&fx.expect));
    println!("  {label} {:<16} {}", backend.model(), verdict.display);
    for c in &verdict.checks {
        let mark = if c.pass { "✓" } else { "✗" };
        if c.detail.is_empty() {
            println!("      [{mark}] {}", c.name);
        } else {
            println!("      [{mark}] {} — {}", c.name, c.detail);
        }
    }
    (verdict.checks_passed(), verdict.checks.len())
}

// ---------------------------------------------------------------------------
// CAPTURE mode — emit a fixture skeleton to stdout
// ---------------------------------------------------------------------------

async fn run_capture(cfg: &Config, task: &dyn LensTask, cases: &[EvalCase]) -> Result<()> {
    let case = cases.first().ok_or_else(|| {
        anyhow!(
            "--capture needs one case: eval --capture --task {} <entity_type:id:sport> \
             (transfer: team:<team_id>:player:<player_id>:sport)",
            task.name()
        )
    })?;
    let harness = build_harness(cfg).await?;
    let user_prompt = task
        .build_prompt(&harness, &case.entity)
        .await?
        .ok_or_else(|| anyhow!("no corpus for {} — nothing to capture", case.entity.key()))?;
    // The frozen system is the task's system const (what the model actually sees).
    let system = task
        .gen_options_for(EVAL_TEMPERATURE, &case.entity)
        .system
        .unwrap_or_default();
    let fx = Fixture {
        name: format!("{}-CHANGE-ME", case.entity.key().replace(':', "-")),
        task: task.name().to_string(),
        prompt_version: task.prompt_version().to_string(),
        system,
        user_prompt,
        temperature: EVAL_TEMPERATURE,
        expect: Expect::default(),
    };
    // stdout only — the human redirects into fixtures/<task>/<name>.json and fills in `expect`.
    println!("{}", serde_json::to_string_pretty(&fx)?);
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// build_harness constructs the read-only harness (pool + router; no embedder/bucket_classifier).
/// Single-flight, so the GPU governor is moot; pin 1.
async fn build_harness(cfg: &Config) -> Result<Harness> {
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    let router = Router::from_config(&cfg.route, cfg.ollama_timeout, 1)?;
    Ok(Harness {
        pool,
        router,
        embedder: None,
        resolve: cfg.resolve.clone(),
        scrub: cfg.scrub.clone(),
        bucket_classifier: None,
    })
}

/// fixtures_dir is `fixtures/<task>` relative to CWD (the `rust/` crate root when run via cargo).
fn fixtures_dir(task_name: &str) -> PathBuf {
    PathBuf::from("fixtures").join(task_name)
}

/// load_fixtures reads every `*.json` in the task's fixture dir (sorted), optionally filtered by a
/// filename substring. No `glob` dep — `std::fs::read_dir`.
fn load_fixtures(dir: &Path, filter: Option<&str>) -> Result<Vec<Fixture>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("read_dir {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();

    let mut out = Vec::new();
    for p in paths {
        let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if let Some(f) = filter {
            if !fname.contains(f) {
                continue;
            }
        }
        let text = std::fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
        let fx: Fixture =
            serde_json::from_str(&text).with_context(|| format!("parse {}", p.display()))?;
        out.push(fx);
    }
    Ok(out)
}

/// print_throughput summarizes one model's batch: the first-call time (which for the candidate
/// carries the single cold model-swap cost) and the WARM tok/s (calls 2..n, once resident).
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

/// print_side_by_side renders incumbent-vs-candidate prose per entity — the blind quality read.
fn print_side_by_side(inc_model: &str, inc: &[CaseResult], cand_model: &str, cand: &[CaseResult]) {
    let cmap: HashMap<&str, &CaseResult> = cand.iter().map(|r| (r.key.as_str(), r)).collect();
    println!("\n=== SIDE-BY-SIDE (blind A/B fodder) ===");
    for a in inc {
        println!("\n[{}]", a.key);
        println!("  A {inc_model:<14} {}", a.verdict.display);
        match cmap.get(a.key.as_str()) {
            Some(b) => println!("  B {cand_model:<14} {}", b.verdict.display),
            None => println!("  B {cand_model:<14} (no result)"),
        }
    }
}

/// print_report renders the incumbent-vs-candidate verdict and — crucially — states that the router
/// does NOT act on it: a measured win is adopted by a human editing the config.
fn print_report(r: &EvalReport) {
    println!(
        "\n=== eval report — task={} role={} n={} ===",
        r.task, r.role, r.n
    );
    println!(
        "incumbent  {:<24} {}",
        r.incumbent.model,
        fmt_score(&r.incumbent, r.n)
    );
    match &r.candidate {
        None => println!("candidate  (none configured — incumbent-only run)"),
        Some(c) => {
            println!("candidate  {:<24} {}", c.model, fmt_score(c, r.n));
            match (r.incumbent.mae, c.mae) {
                (Some(inc), Some(cand)) => {
                    let delta = inc - cand; // > 0 ⇒ candidate lower error ⇒ better
                    if delta > 0.0 {
                        println!(
                            "→ candidate is BETTER by {delta:.2} MAE. To ADOPT (a human decision; \
                             the router never auto-promotes): set COGNITION_ROUTE_{}={}",
                            r.role_env, c.model
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
                    "→ no MAE winner (label-free run, non-numeric task, or too few labels). Judge \
                     on the side-by-side prose + throughput below."
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

/// print_route_table shows what each role resolves to (incumbent + any candidate) — the no-args
/// smoke output, proving the `COGNITION_ROUTE_*` config parsed.
fn print_route_table(cfg: &RouteConfig) {
    println!("configured route table (role → incumbent [+ candidate]):");
    for role in scoracle_cognition::route::Role::all() {
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

/// parse_case reads `entity_type:id:sport[=human_label]` from a CLI token. Transfer also accepts
/// `team:<team_id>:player:<player_id>:sport`, because the production transfer unit is a pair.
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
    if parts.len() == 5 {
        let entity_type = parts[0].to_lowercase();
        let pair_type = parts[2].to_lowercase();
        if entity_type != "team" || pair_type != "player" {
            return Err(anyhow!(
                "bad pair {s:?}; want team:<team_id>:player:<player_id>:sport"
            ));
        }
        let entity_id: i32 = parts[1]
            .parse()
            .with_context(|| format!("bad team id in {s:?}"))?;
        let player_id: i32 = parts[3]
            .parse()
            .with_context(|| format!("bad player id in {s:?}"))?;
        return Ok(EntitySpec {
            entity_type,
            entity_id,
            sport: parts[4].to_uppercase(),
            pair_player_id: Some(player_id),
        });
    }
    if parts.len() != 3 {
        return Err(anyhow!(
            "bad entity {s:?}; want entity_type:id:sport or team:<team_id>:player:<player_id>:sport"
        ));
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
        pair_player_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_entity_accepts_legacy_entity_shape() {
        let e = parse_entity("player:237:nba").unwrap();
        assert_eq!(e.entity_type, "player");
        assert_eq!(e.entity_id, 237);
        assert_eq!(e.sport, "NBA");
        assert_eq!(e.pair_player_id, None);
        assert_eq!(e.key(), "player:237:NBA");
    }

    #[test]
    fn parse_entity_accepts_transfer_pair_shape() {
        let e = parse_entity("team:14:player:237:nba").unwrap();
        assert_eq!(e.entity_type, "team");
        assert_eq!(e.entity_id, 14);
        assert_eq!(e.sport, "NBA");
        assert_eq!(e.pair_player_id, Some(237));
        assert_eq!(e.key(), "team:14:player:237:NBA");
    }

    #[test]
    fn parse_entity_rejects_non_team_pair_shape() {
        let err = parse_entity("player:14:player:237:nba").unwrap_err();
        assert!(err.to_string().contains("bad pair"));
    }
}
