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
//!     system + built prompt, empty `expect`) to STDOUT for a human to annotate.
//!   - LEDGER CAPTURE (`eval --capture-ledger <ledger_id> --task T`): emits a fixture skeleton from
//!     the exact request/prompt persisted in `public.cognition_ledger`, so future fixtures can come
//!     from production diagnostics instead of hand-capture.
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
//! SAFETY: this is read-only on the live pipeline. It NEVER claims/enqueues
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
//!   eval --capture-ledger 123 --task sigil         # fixture skeleton from cognition_ledger row 123
//!   COGNITION_ROUTE_STATS_LOGIC_CANDIDATE=mistral:7b eval --task sigil player:237:NBA  # A/B a challenger
//!   COGNITION_ROUTE_STATS_LOGIC_CANDIDATE=qwen3:8b eval --task rating --fixtures
//!   COGNITION_ROUTE_STATS_LOGIC_CANDIDATE=qwen3:8b eval --task momentum --fixtures

use anyhow::{anyhow, Context, Result};
use scoracle_cognition::config::{Config, RouteConfig};
use scoracle_cognition::db;
use scoracle_cognition::eval_tasks::{
    all_task_names, fixture_drift, resolve_task, CaseVerdict, EntitySpec, Expect, Fixture, LensTask,
};
use scoracle_cognition::harness::Harness;
use scoracle_cognition::judge::VoiceSpec;
use scoracle_cognition::route::{Inference, Role, Router};
use serde_json::Value;
use sqlx::Row;
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
    CaptureLedger { ledger_id: i64 },
}

struct Args {
    task_name: String,
    mode: Mode,
    cases: Vec<EvalCase>,
    /// --judge: score each reply with the independent critic model (COGNITION_JUDGE_MODEL,
    /// default gemma3:4b) on specificity/grounding/non-genericness — the Phase 4 quality axis.
    judge: bool,
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

    // The judge is OFF-path by construction: a plain client on a model that serves no
    // production role, built only when --judge is passed.
    let judge_backend: Option<Arc<dyn Inference>> = if args.judge {
        let judge_model = std::env::var("COGNITION_JUDGE_MODEL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "gemma3:4b".to_string());
        println!(
            "judge — model={judge_model} ({})",
            scoracle_cognition::judge::JUDGE_PROMPT_VERSION
        );
        Some(Arc::new(scoracle_cognition::ollama::OllamaClient::new(
            &cfg.ollama_base_url,
            &judge_model,
            cfg.ollama_timeout,
        )?))
    } else {
        None
    };

    match args.mode {
        Mode::Live => run_live(&cfg, task.as_ref(), &args.cases).await,
        Mode::Fixtures { filter } => {
            run_fixtures(&cfg, task.as_ref(), filter.as_deref(), judge_backend).await
        }
        Mode::Capture => run_capture(&cfg, task.as_ref(), &args.cases).await,
        Mode::CaptureLedger { ledger_id } => {
            run_capture_ledger(&cfg, task.as_ref(), ledger_id).await
        }
    }
}

/// parse_args peels the flags (`--task`, `--fixtures`, `--capture`); the remaining positionals are
/// entity tokens. Default task is `vibe`, so the historical `eval player:237:NBA=72` is unchanged.
fn parse_args(argv: impl Iterator<Item = String>) -> Result<Args> {
    let mut task_name = "vibe".to_string();
    let mut mode = Mode::Live;
    let mut judge = false;
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
            "--judge" => judge = true,
            "--capture" => mode = Mode::Capture,
            "--capture-ledger" => {
                let raw = it
                    .next()
                    .ok_or_else(|| anyhow!("--capture-ledger needs a cognition_ledger id"))?;
                let ledger_id = raw
                    .parse::<i64>()
                    .with_context(|| format!("bad --capture-ledger id {raw:?}"))?;
                mode = Mode::CaptureLedger { ledger_id };
            }
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
        judge,
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
             eval --capture-ledger 123 --task sigil      (emit a fixture skeleton from cognition_ledger)\n  \
             eval --task transfer team:14:player:237:NBA (transfer live pair A/B)\n  \
             eval --task rating --fixtures               (PEAK/stat reasoning fixture gate)\n  \
             eval --task momentum --fixtures             (trajectory reasoning fixture gate)\n  \
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
        "eval — task={} rail={} role={} n={} temp={} (deterministic)",
        task.name(),
        task.parameters().rail.as_str(),
        task.role().as_str(),
        cases.len(),
        EVAL_TEMPERATURE
    );
    println!(
        "lens — operator={} | mandate={} | guard={}",
        task.parameters().operator,
        task.parameters().mandate,
        task.parameters().credibility_guard
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

/// **THE GATE IS ONLY VALID WITH THE COGNITION DAEMON STOPPED. STOP IT FIRST:**
///
/// ```text
/// systemctl --user stop scoracle-cognition
/// cargo build --bin eval && ./target/debug/eval --task editor --fixtures
/// systemctl --user start scoracle-cognition
/// ```
///
/// This is not hygiene, it is the difference between a gauge and a rumour. **Measured on archbox,
/// 2026-08-06 (D-T19), ten runs of the editor set, same binary, same fixtures, same `gemma3:4b`,
/// every fixture pinned at `temperature: 0.0`:**
///
/// | daemon | scores | model output across the 5 runs | wall |
/// |---|---|---|---|
/// | **stopped** | **47/53 ×5** | ONE hash — all 53 checks identical every run | **96s** |
/// | running | 47,47,47,47,48 | FIVE hashes — all five runs differed | ~290s |
///
/// Nothing inside this eval is concurrent — the fixture loop is sequential and the Router is
/// built with one permit — but the SERVER is. `scoracle-cognition` drains the editor stage
/// against the same Ollama at `OLLAMA_NUM_PARALLEL=4` (5–12 live reads/minute, counted in
/// `editor_reads`, not in the journal), so the gate's requests get batched alongside live
/// traffic. Batched inference changes the floating-point reduction order, and a changed reduction
/// order moves the argmax on near-ties — which is why GREEDY DECODE IS NOT DETERMINISTIC ON A
/// BUSY GPU. Under load the `fan-protest-register-outrage` fixture emitted 2 names on one run and
/// 5 on the next, off a byte-identical prompt.
///
/// **A `seed` would not have fixed this and was not added.** At `temperature: 0.0` the sampler is
/// greedy and never consults the RNG; the divergence is upstream of sampling, in the kernels. The
/// only lever that pins it is an idle server.
///
/// **Read the summary line with suspicion — it hides its own movement.** Under load the tally sat
/// at 47/53 four times running while two checks on ONE fixture flipped in OPPOSITE directions
/// (`name_found[Moyes]` and `name_absent[Gwladys]` are the same coin: a longer `names[]` catches
/// the manager and the stand together). A stable total is not a stable gate. When comparing runs,
/// diff the per-check table, never the score.
async fn run_fixtures(
    cfg: &Config,
    task: &dyn LensTask,
    filter: Option<&str>,
    judge: Option<Arc<dyn Inference>>,
) -> Result<()> {
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
        "fixtures — task={} rail={} n={} dir={}/  incumbent={}",
        task.name(),
        task.parameters().rail.as_str(),
        fixtures.len(),
        dir.display(),
        incumbent.model()
    );
    println!(
        "lens — operator={} | mandate={} | guard={}",
        task.parameters().operator,
        task.parameters().mandate,
        task.parameters().credibility_guard
    );
    // D-T19: the one condition that decides whether this run is a measurement or a rumour.
    // Stated on every run because a doc comment cannot be read by someone who never opens the file.
    println!(
        "VALID ONLY WITH THE DAEMON STOPPED — `systemctl --user stop scoracle-cognition` first, or\n\
         these numbers are a busy GPU's, not the model's (greedy decode is not deterministic under\n\
         batching). Comparing two runs? Diff the per-check table, never the score."
    );

    let mut inc_pass = 0usize;
    let mut inc_total = 0usize;
    let mut cand_pass = 0usize;
    let mut cand_total = 0usize;
    let mut inc_judge = JudgeAgg::default();
    let mut cand_judge = JudgeAgg::default();
    for fx in &fixtures {
        if let Some(warn) = fixture_drift(fx, task) {
            println!("  ⚠ WARN {warn}");
        }
        println!("\n[{}]  (temp={})", fx.name, fx.temperature);
        let (p, t) =
            run_one_fixture("A", &incumbent, task, judge.as_ref(), &mut inc_judge, fx).await;
        inc_pass += p;
        inc_total += t;
        if let Some(c) = candidate.as_ref() {
            let (p, t) = run_one_fixture("B", c, task, judge.as_ref(), &mut cand_judge, fx).await;
            cand_pass += p;
            cand_total += t;
        }
    }

    println!(
        "\n=== fixture summary — {} : {inc_pass}/{inc_total} property checks passed ===",
        incumbent.model()
    );
    if let Some(line) = inc_judge.summary() {
        println!("=== judge — {} : {line} ===", incumbent.model());
    }
    if let Some(c) = candidate.as_ref() {
        println!(
            "=== fixture summary — {} : {cand_pass}/{cand_total} property checks passed ===",
            c.model()
        );
        if let Some(line) = cand_judge.summary() {
            println!("=== judge — {} : {line} ===", c.model());
        }
    }
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
    // --judge: the independent critic + the per-model aggregate it feeds.
    judge: Option<&Arc<dyn Inference>>,
    judge_agg: &mut JudgeAgg,
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
    if !verdict.parsed {
        println!("      raw: {}", fixture_raw_excerpt(&gen.response));
        let expected = expected_property_count(&fx.expect);
        if expected > 0 {
            return (0, expected);
        }
    }
    for c in &verdict.checks {
        let mark = if c.pass { "✓" } else { "✗" };
        if c.detail.is_empty() {
            println!("      [{mark}] {}", c.name);
        } else {
            println!("      [{mark}] {} — {}", c.name, c.detail);
        }
    }
    if let Some(j) = judge {
        // Voice axis (judge-v2): cast stages — every LensTask on its own character role — are
        // judged in character via the registry's identity; utility tasks (graph, still on the
        // shared Role::EmotionalNews) keep the three-axis rubric.
        let params = task.parameters();
        let voice_spec = VoiceSpec {
            character: params.operator,
            mandate: params.mandate,
        };
        let voice = (task.role() != Role::EmotionalNews).then_some(&voice_spec);
        match scoracle_cognition::judge::judge_reply(
            j.as_ref(),
            task.name(),
            &fx.user_prompt,
            &gen.response,
            voice,
        )
        .await
        {
            Ok(Some(v)) => {
                let worst = if v.worst_claim.is_empty() {
                    String::new()
                } else {
                    format!(" — worst: {}", v.worst_claim)
                };
                let voice_score = v
                    .voice_fidelity
                    .map(|n| format!(" voice={n}"))
                    .unwrap_or_default();
                println!(
                    "      [judge] specificity={} grounding={} non-generic={}{voice_score}{worst}",
                    v.specificity, v.grounding, v.non_generic
                );
                judge_agg.add(&v);
            }
            Ok(None) => println!("      [judge] unparseable verdict (unjudged)"),
            Err(e) => println!("      [judge] failed ({e:#})"),
        }
    }
    (verdict.checks_passed(), verdict.checks.len())
}

/// JudgeAgg accumulates per-model judge scores across a fixture run.
#[derive(Default)]
struct JudgeAgg {
    n: usize,
    specificity: i64,
    grounding: i64,
    non_generic: i64,
    // voice_fidelity is judged only on cast (character) replies, so it carries its own n.
    voice_n: usize,
    voice_fidelity: i64,
}

impl JudgeAgg {
    fn add(&mut self, v: &scoracle_cognition::judge::JudgeVerdict) {
        self.n += 1;
        self.specificity += i64::from(v.specificity);
        self.grounding += i64::from(v.grounding);
        self.non_generic += i64::from(v.non_generic);
        if let Some(voice) = v.voice_fidelity {
            self.voice_n += 1;
            self.voice_fidelity += i64::from(voice);
        }
    }
    fn summary(&self) -> Option<String> {
        (self.n > 0).then(|| {
            let f = |s: i64| s as f64 / self.n as f64;
            let voice = if self.voice_n > 0 {
                format!(
                    " · voice {:.1}",
                    self.voice_fidelity as f64 / self.voice_n as f64
                )
            } else {
                String::new()
            };
            format!(
                "specificity {:.1} · grounding {:.1} · non-generic {:.1}{voice} (n={})",
                f(self.specificity),
                f(self.grounding),
                f(self.non_generic),
                self.n
            )
        })
    }
}

/// expected_property_count mirrors the fixture schema: if a reply is unparseable, every authored
/// expectation should count as failed rather than disappearing from the denominator.
///
/// **It must stay in step with every `evaluate` in `eval_tasks.rs`, one arm per pushed check.**
/// It did not, and that was an instrument defect in its own right (D-T19, 2026-08-06): the
/// function knew the voice axes and NONE of the Editor's, so an unparseable editor fixture
/// contributed `0/0` instead of `0/N` and simply vanished from the tally. The gate would then
/// report a smaller denominator with no warning — `47/53` and `47/46` print the same shape of
/// success, and the second one is a fixture that died. A denominator that moves with the model's
/// output is not a denominator.
///
/// Three fields are deliberately NOT counted, because they are prompt/resolver INPUTS a fixture
/// declares rather than assertions it makes: `reader_vetted` (the hypothesis list handed to the
/// parser), `resolver_surfaces` (the surface table `group_hits` runs against) and
/// `graph_candidate_types` (the numbered candidate list). That is the whole of the gap between
/// the editor set's 60 authored expect-keys and the 53 checks it scores.
fn expected_property_count(x: &Expect) -> usize {
    let mut n = 0usize;
    n += x.score_min.is_some() as usize;
    n += x.score_max.is_some() as usize;
    n += x.convergence_min.is_some() as usize;
    n += x.convergence_max.is_some() as usize;
    n += x.disagreement_nonempty.is_some() as usize;
    n += x.why_now_nonempty.is_some() as usize;
    n += x.disagreement_includes.as_ref().map_or(0, Vec::len);
    n += x.disagreement_excludes.as_ref().map_or(0, Vec::len);
    n += x.blurb_includes.as_ref().map_or(0, Vec::len);
    n += x.blurb_excludes.as_ref().map_or(0, Vec::len);
    n += x.narratives_min.is_some() as usize;
    n += x.narratives_max.is_some() as usize;
    n += x.title_includes.as_ref().map_or(0, Vec::len);
    n += x.title_excludes.as_ref().map_or(0, Vec::len);
    n += x.body_includes.as_ref().map_or(0, Vec::len);
    n += x.body_excludes.as_ref().map_or(0, Vec::len);
    n += x.all_cite_articles.is_some() as usize;
    n += x.max_article_num.is_some() as usize;
    n += x.transfer_is_rumor.is_some() as usize;
    n += x.transfer_direction.is_some() as usize;
    n += x.transfer_stage.is_some() as usize;
    n += x.subject_includes.as_ref().map_or(0, Vec::len);
    n += x.subject_excludes.as_ref().map_or(0, Vec::len);
    n += x.summary_includes.as_ref().map_or(0, Vec::len);
    n += x.summary_excludes.as_ref().map_or(0, Vec::len);
    n += x.confidence_min.is_some() as usize;
    n += x.confidence_max.is_some() as usize;
    n += x.peak_includes.as_ref().map_or(0, Vec::len);
    n += x.peak_excludes.as_ref().map_or(0, Vec::len);
    n += x.prose_includes.as_ref().map_or(0, Vec::len);
    n += x.prose_excludes.as_ref().map_or(0, Vec::len);
    n += x.prose_min_words.is_some() as usize;
    n += x.prose_max_words.is_some() as usize;
    n += x.reading_includes.as_ref().map_or(0, Vec::len);
    n += x.reading_excludes.as_ref().map_or(0, Vec::len);
    n += x.reading_min_sentences.is_some() as usize;
    n += x.reading_max_sentences.is_some() as usize;
    n += x.reading_max_peers.is_some() as usize;
    // `hook_nonempty` mirrors its evaluator exactly: only `Some(true)` pushes a check.
    n += (x.hook_nonempty == Some(true)) as usize;
    // One check for the whole synonym set, not one per word.
    n += x.body_includes_any.is_some() as usize;
    // The graph axes.
    n += x.relations_include.as_ref().map_or(0, Vec::len);
    n += x.relations_exclude.as_ref().map_or(0, Vec::len);
    n += x.relations_max.is_some() as usize;
    n += x.persons_include.as_ref().map_or(0, Vec::len);
    n += x.persons_exclude.as_ref().map_or(0, Vec::len);
    // The Editor / ArticleReader axes (ep1 + ar6).
    n += x.article_relevant.is_some() as usize;
    n += x.key_facts_include.as_ref().map_or(0, Vec::len);
    n += x.key_facts_exclude.as_ref().map_or(0, Vec::len);
    n += x.names_include.as_ref().map_or(0, Vec::len);
    n += x.names_exclude.as_ref().map_or(0, Vec::len);
    n += x.register_is.is_some() as usize;
    n += x.story_type_is.is_some() as usize;
    n += x.name_kind_is.as_ref().map_or(0, |m| m.len());
    n += x.name_descriptor_nonempty.as_ref().map_or(0, Vec::len);
    n += x.result_line_includes.as_ref().map_or(0, Vec::len);
    n += x.result_line_parses.is_some() as usize;
    n += x.resolver_links_include.as_ref().map_or(0, Vec::len);
    n += x.resolver_links_exclude.as_ref().map_or(0, Vec::len);
    n += x.resolver_unresolved_include.as_ref().map_or(0, Vec::len);
    n += x.resolver_refused_include.as_ref().map_or(0, Vec::len);
    n
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
// LEDGER CAPTURE mode — emit a fixture skeleton from cognition_ledger
// ---------------------------------------------------------------------------

async fn run_capture_ledger(cfg: &Config, task: &dyn LensTask, ledger_id: i64) -> Result<()> {
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    let row = sqlx::query(
        r#"
        SELECT
            id, stage, lens, entity_type, entity_id, sport,
            pair_entity_type, pair_entity_id,
            prompt_version, request_body, built_prompt
        FROM public.cognition_ledger
        WHERE id = $1
        "#,
    )
    .bind(ledger_id)
    .fetch_optional(&pool)
    .await
    .with_context(|| format!("read cognition_ledger id={ledger_id}"))?
    .ok_or_else(|| anyhow!("cognition_ledger id={ledger_id} not found"))?;

    let stage: String = row.get("stage");
    let lens: String = row.get("lens");
    if !ledger_row_matches_task(task.name(), &stage, &lens) {
        return Err(anyhow!(
            "cognition_ledger id={} is stage={} lens={}, not task={}",
            ledger_id,
            stage,
            lens,
            task.name()
        ));
    }

    let request_body: Value = row.get::<Option<Value>, _>("request_body").ok_or_else(|| {
        anyhow!(
            "cognition_ledger id={ledger_id} has no request_body; no model-call fixture to capture"
        )
    })?;
    let user_prompt = request_body
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| row.get::<Option<String>, _>("built_prompt"))
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow!("cognition_ledger id={ledger_id} has no prompt to freeze"))?;
    let system = request_body
        .get("system")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let temperature = request_body
        .get("options")
        .and_then(|v| v.get("temperature"))
        .and_then(Value::as_f64)
        .unwrap_or(EVAL_TEMPERATURE);

    let entity_type: String = row.get("entity_type");
    let entity_id: i32 = row.get("entity_id");
    let sport: String = row.get("sport");
    let pair_entity_type: Option<String> = row.get("pair_entity_type");
    let pair_entity_id: Option<i32> = row.get("pair_entity_id");
    let prompt_version: String = row.get("prompt_version");

    let fx = Fixture {
        name: fixture_name_from_ledger(
            ledger_id,
            &stage,
            &entity_type,
            entity_id,
            pair_entity_type.as_deref(),
            pair_entity_id,
            &sport,
        ),
        task: task.name().to_string(),
        prompt_version,
        system,
        user_prompt,
        temperature,
        expect: Expect::default(),
    };
    println!("{}", serde_json::to_string_pretty(&fx)?);
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn ledger_row_matches_task(task_name: &str, stage: &str, lens: &str) -> bool {
    task_name == stage
        || task_name == lens
        || (task_name == "transfer" && (stage == "transfers" || lens == "transfer"))
}

fn fixture_name_from_ledger(
    ledger_id: i64,
    stage: &str,
    entity_type: &str,
    entity_id: i32,
    pair_entity_type: Option<&str>,
    pair_entity_id: Option<i32>,
    sport: &str,
) -> String {
    let raw = match (pair_entity_type, pair_entity_id) {
        (Some(pair_type), Some(pair_id)) => format!(
            "ledger-{ledger_id}-{stage}-{entity_type}-{entity_id}-{pair_type}-{pair_id}-{sport}"
        ),
        _ => format!("ledger-{ledger_id}-{stage}-{entity_type}-{entity_id}-{sport}"),
    };
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn fixture_raw_excerpt(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return "(empty response)".to_string();
    }
    const MAX_CHARS: usize = 240;
    let mut out = String::new();
    for (idx, ch) in s.chars().enumerate() {
        if idx >= MAX_CHARS {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

/// build_harness constructs the read-only harness (pool + router; no embedder).
/// Single-flight, so the GPU governor is moot; pin 1.
async fn build_harness(cfg: &Config) -> Result<Harness> {
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    let router = Router::from_config(&cfg.route, cfg.ollama_timeout, 1)?;
    Ok(Harness {
        pool,
        router,
        embedder: None,
        // Unbounded: an inspection run drives its entity to completion. Nothing here is racing a
        // worker timeout, and a truncated eval would be a worse artifact than a slow one.
        handler_budget: Duration::ZERO,
        rail: scoracle_cognition::config::Rail::Legacy,
        voice_num_ctx: scoracle_cognition::route::VOICE_NUM_CTX,
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
    // `article` is live-eval'able too: graph and editor are article-keyed, not entity-keyed. Both
    // tasks' build_prompt tells you to pass `article:<id>:<SPORT>` — and this check rejected it,
    // so their live modes were unreachable from the day they were written.
    if entity_type != "player" && entity_type != "team" && entity_type != "article" {
        return Err(anyhow!(
            "bad entity_type in {s:?}; want player|team|article"
        ));
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

    #[test]
    fn parse_args_accepts_capture_ledger() {
        let args = parse_args(
            ["--capture-ledger", "42", "--task", "sigil"]
                .into_iter()
                .map(String::from),
        )
        .unwrap();
        assert_eq!(args.task_name, "sigil");
        assert!(args.cases.is_empty());
        match args.mode {
            Mode::CaptureLedger { ledger_id } => assert_eq!(ledger_id, 42),
            _ => panic!("expected CaptureLedger mode"),
        }
    }

    #[test]
    fn ledger_match_accepts_transfer_stage_plural() {
        assert!(ledger_row_matches_task("transfer", "transfers", "transfer"));
        assert!(!ledger_row_matches_task("sigil", "transfers", "transfer"));
    }

    #[test]
    fn fixture_name_from_ledger_sanitizes_pair_name() {
        assert_eq!(
            fixture_name_from_ledger(7, "transfers", "team", 14, Some("player"), Some(237), "NBA"),
            "ledger-7-transfers-team-14-player-237-nba"
        );
    }

    #[test]
    fn fixture_raw_excerpt_handles_empty_and_truncates() {
        assert_eq!(fixture_raw_excerpt("  \n"), "(empty response)");
        let long = "x".repeat(300);
        let out = fixture_raw_excerpt(&long);
        assert!(out.ends_with("..."));
        assert!(out.len() < long.len());
    }

    #[test]
    fn unparseable_fixture_counts_authored_expectations() {
        let x = Expect {
            prose_includes: Some(vec!["PEAK".into(), "Vibe".into()]),
            prose_max_words: Some(80),
            ..Default::default()
        };
        assert_eq!(expected_property_count(&x), 3);
    }

    /// The editor gate's denominator must be DERIVABLE FROM THE FIXTURE FILES, and it must not
    /// depend on what the model happened to say (D-T19). This walks the real fixture dir and
    /// asserts the authored total is the 53 the gate reports — so a fixture that fails to parse
    /// now scores `0/N` and the denominator holds at 53 instead of silently shrinking.
    ///
    /// If you add an editor fixture or an expect-key, this number moves ON PURPOSE and you
    /// update it here. That is the point: the denominator changes when the FILES change, never
    /// when a reply does.
    #[test]
    fn editor_fixture_denominator_is_derivable_from_the_files() {
        let dir = fixtures_dir("editor");
        let fixtures = load_fixtures(&dir, None).expect("load editor fixtures");
        assert_eq!(fixtures.len(), 12, "editor fixture count");
        let total: usize = fixtures
            .iter()
            .map(|f| expected_property_count(&f.expect))
            .sum();
        assert_eq!(total, 53, "authored editor property checks");
    }
}
