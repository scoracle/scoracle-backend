//! Per-lens eval task registry (Multi-Lens Cognition Panel).
//!
//! `bin/eval` used to be hardwired to the vibe task (`Role::EmotionalNews`) and the live corpus.
//! A `LensTask` is the seam that generalizes it: each task knows its `Role`, its `GenerateOptions`
//! (system + num_predict + json_mode), how to build the exact PRODUCTION prompt for an entity, and
//! how to `evaluate` a raw reply into a `CaseVerdict`. It COMPOSES the capability library — the
//! stage loaders + prompt builders + parsers already in the lib — rather than reinventing them, so
//! the eval measures the real prompt with only the backend swapped.
//!
//! The active rail taxonomy is:
//!   - emotional/news rail: `narratives`, `transfer`, `vibe` (`Role::EmotionalNews`).
//!   - stats/analytical rail: `rating`/PEAK (`Role::StatsLogic`) plus `momentum` on its own
//!     `Role::MomentumLogic` (identity split 2026-07-11 — un-configured it resolves to the same
//!     default model, so eval candidates configure `COGNITION_ROUTE_MOMENTUM_LOGIC_CANDIDATE`).
//!   - synthesis rail: `sigil` on `Role::SynthesisLogic` (same identity split), so a stats-rail
//!     route change can never silently flip the un-bake-off'd synthesis stage.
//!
//! `momentum` is deliberately eval-first: production Momentum is deterministic DB/read-model
//! trajectory math today, not a queue stage and not a served model call. The task exists so candidate
//! analytical models can be measured on trajectory reasoning before a versioned Momentum generation
//! or route split is introduced.
//!
//! Two scoring axes, unified in `CaseVerdict`:
//!   - MAE (vibe live): `abs_err = |score - human_label|`.
//!   - property rubric (fixtures): named boolean `PropertyCheck`s from a fixture's `Expect`.
//!
//! The rubric lives in the fixture's `Expect`, not the task, so a task stays entity-agnostic
//! (task = the lens; a fixture SET like "disagreement" is a collection of `Expect`s over it).
//!
//! SAFETY: like `bin/eval` itself, tasks are read-only on the pipeline — they read corpus tables to
//! build a prompt and POST to the model; they NEVER claim `pipeline_work` or write a product table.

use crate::corpus::{load_transfer_heat, lookup_entity_name};
use crate::harness::{Harness, Parser};
use crate::momentum::{
    build_momentum_prompt, parse_momentum_reply, MOMENTUM_NUM_PREDICT, MOMENTUM_PROMPT_VERSION,
    MOMENTUM_SYSTEM_PROMPT,
};
use crate::narratives::{
    build_narratives_prompt, load_vetted_corpus, NarrativesParser, NarrativesReq,
    NARRATIVES_NUM_CTX, NARRATIVES_NUM_PREDICT, NARRATIVES_PROMPT_VERSION,
    NARRATIVES_SYSTEM_PROMPT,
};
use crate::ollama::GenerateOptions;
use crate::oracle::{
    build_oracle_prompt, compute_omen, count_sentences, load_latest_sigil, oracle_format_schema,
    parse_oracle_reply, ORACLE_NUM_PREDICT, ORACLE_PROMPT_VERSION, ORACLE_SYSTEM_PROMPT,
};
use crate::rating::{
    build_rating_request, RatingBuild, RatingParser, RatingReq, RATING_NUM_PREDICT,
    RATING_PROMPT_VERSION, RATING_SYSTEM_PROMPT,
};
use crate::route::Role;
use crate::sigil::{
    build_synthesis_prompt, load_pillars, parse_synthesis_response, SIGIL_NUM_PREDICT,
    SIGIL_PROMPT_VERSION, SIGIL_SYSTEM_PROMPT,
};
use crate::transfer::{
    build_pair_request, load_candidates, load_tier_map, transfer_system_prompt, PairBuild,
    TransferParser, TRANSFER_DEFAULT_MIN_ARTICLES, TRANSFER_NUM_PREDICT, TRANSFER_PROMPT_VERSION,
};
use crate::vibe::{
    build_sentiment_prompt, load_latest_narratives, parse_sentiment_and_prompt, VIBE_NUM_PREDICT,
    VIBE_PROMPT_VERSION, VIBE_SYSTEM_PROMPT,
};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// EntitySpec is one entity a case scores. Lives here (not in the bin) so `build_prompt` and the
/// tests can construct it; the bin's CLI parser builds it from `entity_type:id:sport` tokens.
#[derive(Clone, Debug)]
pub struct EntitySpec {
    pub entity_type: String,
    pub entity_id: i32,
    pub sport: String,
    /// Transfer evals are scored on a production team-player pair, not a standalone entity.
    /// `None` keeps the original `entity_type:id:sport` shape for every other task.
    pub pair_player_id: Option<i32>,
}

impl EntitySpec {
    pub fn key(&self) -> String {
        match self.pair_player_id {
            Some(player_id) => format!(
                "{}:{}:player:{}:{}",
                self.entity_type, self.entity_id, player_id, self.sport
            ),
            None => format!("{}:{}:{}", self.entity_type, self.entity_id, self.sport),
        }
    }
}

/// The broad model-family lane a lens belongs to. This is product taxonomy, not a new route: roles
/// remain the serving primitive until evals prove a split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rail {
    EmotionalNews,
    StatsAnalytical,
    Synthesis,
}

impl Rail {
    pub fn as_str(self) -> &'static str {
        match self {
            Rail::EmotionalNews => "emotional/news",
            Rail::StatsAnalytical => "stats/analytical",
            Rail::Synthesis => "synthesis",
        }
    }
}

/// Product-level operating parameters for a lens. These are the "who is thinking?" and "what must
/// they optimize for?" notes that should shape prompts, fixtures, and adoption decisions without
/// hard-coding a model id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LensParameters {
    pub rail: Rail,
    pub operator: &'static str,
    pub mandate: &'static str,
    pub credibility_guard: &'static str,
}

/// lens_parameters is the code home for the current six-lens taxonomy.
pub fn lens_parameters(name: &str) -> Option<LensParameters> {
    match name {
        "narratives" => Some(LensParameters {
            rail: Rail::EmotionalNews,
            operator: "beat writer",
            mandate: "Compile the stories swirling around the entity into grounded storylines.",
            credibility_guard: "Group what sources actually say; do not inflate vague hype or off-entity noise.",
        }),
        "transfer" => Some(LensParameters {
            rail: Rail::EmotionalNews,
            operator: "transfer expert",
            mandate: "Get movement predictions out quickly while preserving long-term credibility.",
            credibility_guard: "Fail closed on name-drops, stale links, weak sourcing, and misleading heat.",
        }),
        "vibe" => Some(LensParameters {
            rail: Rail::EmotionalNews,
            operator: "content creator",
            mandate: "Read the entity's current vibe so a creator can piggyback on the conversation.",
            credibility_guard: "Separate interactable mood from durable truth; do not invent a narrative hook.",
        }),
        "rating" => Some(LensParameters {
            rail: Rail::StatsAnalytical,
            operator: "opposing team scout",
            mandate: "Prepare for the entity by naming the greatest strength to stop and the greatest weakness to exploit.",
            credibility_guard: "Use supplied tiers and datapoints only; never turn average marks into strengths.",
        }),
        "momentum" => Some(LensParameters {
            rail: Rail::StatsAnalytical,
            operator: "nimble trader",
            mandate: "Read PEAK/rating trajectory as price action and Vibe/news as investor sentiment, then decide whether momentum is rising, falling, or a hold.",
            credibility_guard: "Stay detached and results-only; do not chase sentiment hype or cling to stale PEAK strength.",
        }),
        "sigil" => Some(LensParameters {
            rail: Rail::Synthesis,
            operator: "reasoned expert network panelist",
            mandate: "Summarize all pillars into the final Scoracle read.",
            credibility_guard: "Preserve real disagreement between pillars instead of flattening it.",
        }),
        "oracle" => Some(LensParameters {
            rail: Rail::Synthesis,
            operator: "the Oracle",
            mandate: "Read the assembled cards aloud and deliver the entity's reading in the house voice.",
            credibility_guard: "The mysticism lives in the telling, never the facts — every claim traces to a card; nothing invented.",
        }),
        _ => None,
    }
}

/// One named boolean assertion over a parsed reply (fixture property axis).
#[derive(Clone, Debug)]
pub struct PropertyCheck {
    pub name: String,
    pub pass: bool,
    /// Human-readable evidence for the ✓/✗ (e.g. `conv=70 ≤ 55`).
    pub detail: String,
}

/// CaseVerdict is one backend's scored answer for one case, task-agnostic: it carries BOTH the MAE
/// axis (`abs_err`, vibe live) and the property axis (`checks`, fixtures). `display` is the
/// one-line echo for the side-by-side. Perf metrics are held by the caller (identical per task).
#[derive(Clone, Debug)]
pub struct CaseVerdict {
    /// The reply parsed to the task's validated `T` (drives "scored N/n").
    pub parsed: bool,
    /// Mean-absolute-error axis: `Some` only when a numeric label AND a parsed score both exist.
    pub abs_err: Option<f64>,
    /// Property axis: empty for a pure-MAE (live, no expect) case.
    pub checks: Vec<PropertyCheck>,
    /// One-line score/prose echo.
    pub display: String,
}

impl CaseVerdict {
    pub fn all_checks_pass(&self) -> bool {
        self.checks.iter().all(|c| c.pass)
    }

    pub fn checks_passed(&self) -> usize {
        self.checks.iter().filter(|c| c.pass).count()
    }
}

/// Expect is the union of expected properties a fixture can assert. Each task reads only the subset
/// it understands and ignores the rest, so the fixture schema stays uniform and the loader
/// task-agnostic. `#[serde(default)]` lets a hand-authored fixture omit every field it does not use.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Expect {
    // vibe fixture score band (per-case boolean stand-in for the aggregate MAE axis).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_max: Option<i32>,
    // sigil panel-disagreement rubric.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub convergence_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub convergence_max: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disagreement_nonempty: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why_now_nonempty: Option<bool>,
    /// Catches example-parroting / asserts the real conflict is named. Scored against the parsed
    /// `disagreement`, which `parse_synthesis_response` already normalizes (N/A → None, quotes
    /// stripped), so the eval reflects exactly what gets persisted + served.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disagreement_includes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disagreement_excludes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blurb_includes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blurb_excludes: Option<Vec<String>>,
    // narrative grouping + grounding rubric.
    /// Count discipline: the model must return at least / at most this many storylines. A quiet or
    /// hype-only cycle should stay LOW (the system prompt: "A quiet cycle can return one narrative or
    /// none"; "Ignore vague hype").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narratives_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narratives_max: Option<i32>,
    /// Specificity: at least one returned title contains each string (the real storyline is named).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_includes: Option<Vec<String>>,
    /// Specificity / no-invention: no returned title contains any of these (catches generic
    /// "Transfer news" titles and wrong-storyline framings).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_excludes: Option<Vec<String>>,
    /// Grounding: at least one returned body contains each string (names the who/what/where).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_includes: Option<Vec<String>>,
    /// No-invention: no returned body contains any of these (e.g. claiming THIS entity is moving when
    /// the corpus only has other teams scheming around them — the system prompt's hardest rule).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_excludes: Option<Vec<String>>,
    /// Grounding: every returned storyline must (`true`) cite ≥1 article number — an uncited storyline
    /// is ungrounded and dropped downstream.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_cite_articles: Option<bool>,
    /// Grounding: no cited article number may fall outside `1..=max` — an out-of-range number is an
    /// invented reference. The fixture sets this to its numbered-corpus length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_article_num: Option<i32>,
    // transfer false-positive / true-positive rubric.
    /// Transfer adjudication: assert whether the model commits to a served rumor (`true`) or clears
    /// the pair (`false`). `None` in a parsed verdict is the UNKNOWN/fail-closed path and fails
    /// either explicit boolean expectation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer_is_rumor: Option<bool>,
    /// Direction relative to the named team (`incoming`, `outgoing`, `unclear`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer_direction: Option<String>,
    /// Stage ladder expectation (`speculation`, `concrete_interest`, `advanced_talks`, `here_we_go`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer_stage: Option<String>,
    /// Subject discipline: the parser should identify the exact person the sources are really about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_includes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_excludes: Option<Vec<String>>,
    /// Summary specificity / no-invention checks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_includes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_excludes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_max: Option<f64>,
    // rating / stats-lens specificity + prose richness rubric.
    /// PEAK identity specificity: the first-line PEAK label should name the actual standout skill,
    /// not a generic role or an average datapoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_includes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_excludes: Option<Vec<String>>,
    /// Scouting-report body checks. Kept separate from narrative `body_*` so stats fixtures can
    /// describe prose richness without changing storyline semantics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prose_includes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prose_excludes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prose_min_words: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prose_max_words: Option<i32>,
    // oracle / persona-reading rubric.
    /// Reading substring checks, matched CASE-INSENSITIVELY (a voice lens varies casing freely;
    /// the jargon-exclusion checks must catch "Convergence" as well as "convergence").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_includes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_excludes: Option<Vec<String>>,
    /// The conventions' 2-4 sentence read budget, encoded as fixture validation
    /// (`oracle::count_sentences`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_min_sentences: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_max_sentences: Option<i32>,
    // momentum / trajectory reasoning rubric.
    /// Signed Momentum score band (-5..5, the model's conviction in the DECIDED direction).
    /// Direction itself left the reply contract in momentum-s4 — it is computed in code
    /// (`momentum::momentum_direction_from_score`) and supplied to the prompt as a fact, so
    /// fixtures assert the score band and prose instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub momentum_score_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub momentum_score_max: Option<i32>,
}

/// A frozen eval case: the exact `system` + `user_prompt` (captured or hand-authored), the run
/// `temperature`, the `prompt_version` it was frozen under (drift-checked vs the live task), and
/// the expected properties. This is the reproducible regression unit — the same fixture yields the
/// same output every run (temperature 0).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fixture {
    pub name: String,
    pub task: String,
    pub prompt_version: String,
    pub system: String,
    pub user_prompt: String,
    pub temperature: f64,
    #[serde(default)]
    pub expect: Expect,
}

/// A lens eval task: the routing + prompt + scoring seam `bin/eval` runs against. Object-safe
/// (`build_prompt` boxed by `async_trait`), so tasks dispatch through `Box<dyn LensTask>`.
#[async_trait]
pub trait LensTask: Send + Sync {
    /// Registry key (`"vibe"`, `"sigil"`) — also the `fixtures/<name>/` dir.
    fn name(&self) -> &'static str;
    /// Product operating parameters for the lens. Not used for routing; useful for eval reports and
    /// prompt/fixture review.
    fn parameters(&self) -> LensParameters {
        lens_parameters(self.name()).unwrap_or_else(|| {
            unreachable!(
                "registered LensTask without lens_parameters: {}",
                self.name()
            )
        })
    }
    /// The role whose incumbent/candidate this task A/Bs.
    fn role(&self) -> Role;
    /// The stage's prompt-contract version — single-sourced from the stage const, drift-checked
    /// against a fixture's frozen `prompt_version`.
    fn prompt_version(&self) -> &'static str;
    /// system + num_predict + json_mode from the stage consts; the caller chooses `temperature`
    /// (live = 0.0 deterministic; fixture = the fixture's frozen value).
    fn gen_options(&self, temperature: f64) -> GenerateOptions;
    /// Optional per-case override for tasks whose system prompt depends on the live case.
    fn gen_options_for(&self, temperature: f64, _e: &EntitySpec) -> GenerateOptions {
        self.gen_options(temperature)
    }
    /// Build the EXACT production user-prompt for an entity. `Ok(None)` = no-corpus skip (the stage
    /// would write a marker without a model call — nothing to score).
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>>;
    /// Parse + score one raw reply. Pure/sync/offline. `label` drives the MAE axis (vibe live);
    /// `expect` drives the property axis (fixtures). Both optional and independent.
    fn evaluate(&self, raw: &str, label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict;
}

/// resolve_task maps a task name to its `LensTask`. Adding a task = a new unit struct + one arm.
pub fn resolve_task(name: &str) -> Option<Box<dyn LensTask>> {
    match name {
        "vibe" => Some(Box::new(VibeTask)),
        "sigil" => Some(Box::new(SigilTask)),
        "oracle" => Some(Box::new(OracleTask)),
        "narratives" => Some(Box::new(NarrativeTask)),
        "transfer" => Some(Box::new(TransferTask)),
        "rating" => Some(Box::new(RatingTask)),
        "momentum" => Some(Box::new(MomentumTask)),
        _ => None,
    }
}

/// all_task_names lists the registered tasks (for usage output + unknown-task errors).
pub fn all_task_names() -> &'static [&'static str] {
    &[
        "vibe",
        "sigil",
        "oracle",
        "narratives",
        "transfer",
        "rating",
        "momentum",
    ]
}

/// fixture_drift returns a warning when a fixture was frozen under a different prompt contract than
/// the live task — the frozen `system`/`user_prompt` are then stale and the fixture should be
/// re-captured + re-annotated. Warn, never fail (a bump is a signal, not an error).
pub fn fixture_drift(fx: &Fixture, task: &dyn LensTask) -> Option<String> {
    if fx.prompt_version != task.prompt_version() {
        Some(format!(
            "fixture-rot: {} was frozen at prompt_version={} but task {} is now {} — re-capture + re-annotate",
            fx.name,
            fx.prompt_version,
            task.name(),
            task.prompt_version()
        ))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// VibeTask — behavior-preserving port of the original hardcoded eval path.
// ---------------------------------------------------------------------------

pub struct VibeTask;

#[async_trait]
impl LensTask for VibeTask {
    fn name(&self) -> &'static str {
        "vibe"
    }
    fn role(&self) -> Role {
        Role::EmotionalNews
    }
    fn prompt_version(&self) -> &'static str {
        VIBE_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> GenerateOptions {
        GenerateOptions {
            system: Some(VIBE_SYSTEM_PROMPT.to_string()),
            temperature: Some(temperature),
            num_predict: VIBE_NUM_PREDICT,
            num_ctx: 0,
            json_mode: false,
            format_schema: None,
        }
    }
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>> {
        let name = lookup_entity_name(&hx.pool, &e.entity_type, e.entity_id, &e.sport).await?;
        // Reads use the upper-cased sport; the prompt uses the request-case value, mirroring
        // generate_vibe (and the original build_vibe_prompt).
        let sport = e.sport.to_uppercase();
        let (narratives, _ids) =
            load_latest_narratives(&hx.pool, &e.entity_type, e.entity_id, &sport).await?;
        let heat = load_transfer_heat(&hx.pool, &e.entity_type, e.entity_id, &sport).await?;
        if narratives.is_empty() && heat.is_empty() {
            return Ok(None);
        }
        Ok(Some(build_sentiment_prompt(
            &e.entity_type,
            &name,
            &e.sport,
            &narratives,
            &heat,
        )))
    }
    fn evaluate(&self, raw: &str, label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        match parse_sentiment_and_prompt(raw) {
            Ok((s, v)) => {
                let mut checks = Vec::new();
                if let Some(x) = expect {
                    if let Some(min) = x.score_min {
                        checks.push(PropertyCheck {
                            name: "score_ge".into(),
                            pass: s >= min,
                            detail: format!("score={s} ≥ {min}"),
                        });
                    }
                    if let Some(max) = x.score_max {
                        checks.push(PropertyCheck {
                            name: "score_le".into(),
                            pass: s <= max,
                            detail: format!("score={s} ≤ {max}"),
                        });
                    }
                }
                CaseVerdict {
                    parsed: true,
                    abs_err: label.map(|l| (s as f64 - l).abs()),
                    checks,
                    display: format!("score={s} | {v}"),
                }
            }
            Err(_) => CaseVerdict {
                parsed: false,
                abs_err: None,
                checks: Vec::new(),
                display: "unparseable".into(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// SigilTask — panel synthesis + the disagreement rubric.
// ---------------------------------------------------------------------------

pub struct SigilTask;

/// disp_opt renders an optional convergence for the detail/echo lines.
fn disp_opt(o: Option<i32>) -> String {
    o.map(|c| c.to_string()).unwrap_or_else(|| "–".into())
}

#[async_trait]
impl LensTask for SigilTask {
    fn name(&self) -> &'static str {
        "sigil"
    }
    fn role(&self) -> Role {
        Role::SynthesisLogic
    }
    fn prompt_version(&self) -> &'static str {
        SIGIL_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> GenerateOptions {
        GenerateOptions {
            system: Some(SIGIL_SYSTEM_PROMPT.to_string()),
            temperature: Some(temperature),
            num_predict: SIGIL_NUM_PREDICT,
            num_ctx: 0,
            json_mode: false,
            format_schema: None,
        }
    }
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>> {
        let name = lookup_entity_name(&hx.pool, &e.entity_type, e.entity_id, &e.sport).await?;
        let sport = e.sport.to_uppercase();
        let (_season, narratives, rating, vibe, momentum, transfers) =
            load_pillars(hx, &e.entity_type, e.entity_id, &sport).await?;
        // No-pillar path: the stage would persist a marker without a model call (sigil.rs) — no
        // synthesis to score.
        if narratives.is_empty()
            && rating.is_none()
            && vibe.is_none()
            && momentum.empty()
            && transfers.is_empty()
        {
            return Ok(None);
        }
        // prev_sigil = None: deterministic + reproducible, exactly as the parity path (sigil.rs).
        Ok(Some(build_synthesis_prompt(
            &e.entity_type,
            &name,
            &e.sport,
            &narratives,
            rating.as_ref(),
            vibe.as_ref(),
            &momentum,
            &transfers,
            None,
        )))
    }
    fn evaluate(&self, raw: &str, _label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        let p = parse_synthesis_response(raw);
        // Mirrors SigilParser's fail-closed gate: no parseable SCORE ⇒ score 0 ⇒ not a valid reply.
        let parsed = p.score != 0;
        // `parse_synthesis_response` already normalizes DISAGREEMENT (N/A → None, quotes stripped),
        // so this reflects exactly what is persisted + served.
        let disagreement = p.disagreement.as_deref();
        let mut checks = Vec::new();

        if let Some(x) = expect {
            if let Some(max) = x.convergence_max {
                checks.push(PropertyCheck {
                    name: "convergence_le".into(),
                    pass: p.convergence.is_some_and(|c| c <= max),
                    detail: format!("conv={} ≤ {max}", disp_opt(p.convergence)),
                });
            }
            if let Some(min) = x.convergence_min {
                checks.push(PropertyCheck {
                    name: "convergence_ge".into(),
                    pass: p.convergence.is_some_and(|c| c >= min),
                    detail: format!("conv={} ≥ {min}", disp_opt(p.convergence)),
                });
            }
            if let Some(want) = x.disagreement_nonempty {
                checks.push(PropertyCheck {
                    name: if want {
                        "disagreement_present".into()
                    } else {
                        "disagreement_absent".into()
                    },
                    pass: disagreement.is_some() == want,
                    detail: format!("disagreement={}", disagreement.unwrap_or("(none)")),
                });
            }
            for s in x.disagreement_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("disagreement_includes:{s}"),
                    pass: disagreement.is_some_and(|d| d.contains(s.as_str())),
                    detail: format!("disagreement={}", disagreement.unwrap_or("(none)")),
                });
            }
            for s in x.disagreement_excludes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("disagreement_excludes:{s}"),
                    pass: disagreement.is_none_or(|d| !d.contains(s.as_str())),
                    detail: format!("disagreement={}", disagreement.unwrap_or("(none)")),
                });
            }
            if let Some(want) = x.why_now_nonempty {
                checks.push(PropertyCheck {
                    name: if want {
                        "why_now_present".into()
                    } else {
                        "why_now_absent".into()
                    },
                    pass: p.why_now.is_some() == want,
                    detail: format!("why_now={}", p.why_now.as_deref().unwrap_or("(none)")),
                });
            }
            for s in x.blurb_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("blurb_includes:{s}"),
                    pass: p.blurb.contains(s.as_str()),
                    detail: String::new(),
                });
            }
            for s in x.blurb_excludes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("blurb_excludes:{s}"),
                    pass: !p.blurb.contains(s.as_str()),
                    detail: String::new(),
                });
            }
        }

        CaseVerdict {
            parsed,
            abs_err: None,
            checks,
            display: format!(
                "score={} conv={} | {}",
                p.score,
                disp_opt(p.convergence),
                p.blurb
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// OracleTask — the persona reading over the assembled cards (downstream of Sigil).
// ---------------------------------------------------------------------------

pub struct OracleTask;

#[async_trait]
impl LensTask for OracleTask {
    fn name(&self) -> &'static str {
        "oracle"
    }
    fn role(&self) -> Role {
        Role::OracleLogic
    }
    fn prompt_version(&self) -> &'static str {
        ORACLE_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> GenerateOptions {
        GenerateOptions {
            system: Some(ORACLE_SYSTEM_PROMPT.to_string()),
            temperature: Some(temperature),
            num_predict: ORACLE_NUM_PREDICT,
            num_ctx: 0,
            json_mode: false,
            // Grammar-constrained single-field reply, matching the live stage.
            format_schema: Some(oracle_format_schema()),
        }
    }
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>> {
        let name = lookup_entity_name(&hx.pool, &e.entity_type, e.entity_id, &e.sport).await?;
        let sport = e.sport.to_uppercase();
        let (season, narratives, rating, vibe, momentum, transfers) =
            load_pillars(hx, &e.entity_type, e.entity_id, &sport).await?;
        // No scored Sigil ⇒ the stage would persist a marker without a model call — nothing to score.
        let Some(sigil) =
            load_latest_sigil(&hx.pool, &e.entity_type, e.entity_id, &sport, season).await?
        else {
            return Ok(None);
        };
        let (omen, omen_reason) = compute_omen(&sigil, rating.as_ref(), &momentum);
        Ok(Some(build_oracle_prompt(
            &e.entity_type,
            &name,
            &e.sport,
            &sigil,
            &narratives,
            rating.as_ref(),
            vibe.as_ref(),
            &momentum,
            &transfers,
            omen,
            &omen_reason,
        )))
    }
    fn evaluate(&self, raw: &str, _label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        let Some(reading) = parse_oracle_reply(raw) else {
            return CaseVerdict {
                parsed: false,
                abs_err: None,
                checks: Vec::new(),
                display: "unparseable".into(),
            };
        };
        let lower = reading.to_lowercase();
        let sentences = count_sentences(&reading);
        let mut checks = Vec::new();

        if let Some(x) = expect {
            if let Some(min) = x.reading_min_sentences {
                checks.push(PropertyCheck {
                    name: "reading_min_sentences".into(),
                    pass: sentences as i32 >= min,
                    detail: format!("sentences={sentences} ≥ {min}"),
                });
            }
            if let Some(max) = x.reading_max_sentences {
                checks.push(PropertyCheck {
                    name: "reading_max_sentences".into(),
                    pass: sentences as i32 <= max,
                    detail: format!("sentences={sentences} ≤ {max}"),
                });
            }
            for s in x.reading_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("reading_includes:{s}"),
                    pass: lower.contains(&s.to_lowercase()),
                    detail: String::new(),
                });
            }
            for s in x.reading_excludes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("reading_excludes:{s}"),
                    pass: !lower.contains(&s.to_lowercase()),
                    detail: String::new(),
                });
            }
        }

        CaseVerdict {
            parsed: true,
            abs_err: None,
            checks,
            display: reading,
        }
    }
}

// ---------------------------------------------------------------------------
// NarrativeTask — storyline grouping + grounding (the narrative lens's non-vibe half).
// ---------------------------------------------------------------------------

pub struct NarrativeTask;

#[async_trait]
impl LensTask for NarrativeTask {
    fn name(&self) -> &'static str {
        "narratives"
    }
    fn role(&self) -> Role {
        Role::NarrativeLogic
    }
    fn prompt_version(&self) -> &'static str {
        NARRATIVES_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> GenerateOptions {
        GenerateOptions {
            system: Some(NARRATIVES_SYSTEM_PROMPT.to_string()),
            temperature: Some(temperature),
            num_predict: NARRATIVES_NUM_PREDICT,
            num_ctx: NARRATIVES_NUM_CTX,
            json_mode: false,
            // Grammar-constrained, matching the live stage (Phase 5).
            format_schema: Some(crate::narratives::narratives_format_schema()),
        }
    }
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>> {
        let name = lookup_entity_name(&hx.pool, &e.entity_type, e.entity_id, &e.sport).await?;
        // Reads use the upper-cased sport; the prompt renders the request-case value (build_narratives_request).
        let sport = e.sport.to_uppercase();
        let corpus = load_vetted_corpus(&hx.pool, &e.entity_type, e.entity_id, &sport).await?;
        // No corpus ⇒ the stage writes the NULL-narrative marker without a model call — nothing to score.
        if corpus.is_empty() {
            return Ok(None);
        }
        let heat = load_transfer_heat(&hx.pool, &e.entity_type, e.entity_id, &sport).await?;
        // Direct builder, mirroring VibeTask/SigilTask: the embedder-only near-duplicate dedup is a
        // live value-add outside the deterministic prompt contract, so the eval scores the same
        // grounded prompt on every run.
        let req = NarrativesReq {
            entity_type: e.entity_type.clone(),
            entity_id: e.entity_id,
            entity_name: name,
            sport: e.sport.clone(),
            trigger_type: "periodic".to_string(),
        };
        Ok(Some(build_narratives_prompt(&req, &corpus, &heat)))
    }
    fn evaluate(&self, raw: &str, _label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        // Compose the stage's tolerant salvager so the eval scores exactly the storylines the pipeline
        // would keep: Err ⇒ a malformed/truncated reply (unparseable); Ok(Some(empty)) ⇒ a valid
        // quiet cycle with zero storylines (parsed, but count 0).
        let doc = match NarrativesParser.parse(raw) {
            Ok(Some(p)) => p,
            _ => {
                return CaseVerdict {
                    parsed: false,
                    abs_err: None,
                    checks: Vec::new(),
                    display: "unparseable".into(),
                }
            }
        };
        let items: Vec<(&str, &str, &[i32])> = doc.returned().collect();
        let n = items.len() as i32;
        let titles = items
            .iter()
            .map(|(t, _, _)| *t)
            .collect::<Vec<_>>()
            .join(" ⏐ ");
        let mut checks = Vec::new();

        if let Some(x) = expect {
            if let Some(min) = x.narratives_min {
                checks.push(PropertyCheck {
                    name: "narratives_ge".into(),
                    pass: n >= min,
                    detail: format!("count={n} ≥ {min}"),
                });
            }
            if let Some(max) = x.narratives_max {
                checks.push(PropertyCheck {
                    name: "narratives_le".into(),
                    pass: n <= max,
                    detail: format!("count={n} ≤ {max}"),
                });
            }
            for s in x.title_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("title_includes:{s}"),
                    pass: items.iter().any(|(t, _, _)| t.contains(s.as_str())),
                    detail: format!("titles={titles}"),
                });
            }
            for s in x.title_excludes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("title_excludes:{s}"),
                    pass: !items.iter().any(|(t, _, _)| t.contains(s.as_str())),
                    detail: format!("titles={titles}"),
                });
            }
            for s in x.body_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("body_includes:{s}"),
                    pass: items.iter().any(|(_, b, _)| b.contains(s.as_str())),
                    detail: String::new(),
                });
            }
            for s in x.body_excludes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("body_excludes:{s}"),
                    pass: !items.iter().any(|(_, b, _)| b.contains(s.as_str())),
                    detail: String::new(),
                });
            }
            if let Some(want) = x.all_cite_articles {
                // "Every storyline cites ≥1 article." An empty set can never satisfy `true` (there is
                // nothing grounded to show).
                let all_cite = !items.is_empty() && items.iter().all(|(_, _, a)| !a.is_empty());
                let uncited = items.iter().filter(|(_, _, a)| a.is_empty()).count();
                checks.push(PropertyCheck {
                    name: "all_cite_articles".into(),
                    pass: all_cite == want,
                    detail: format!("{uncited}/{n} storylines cite no article"),
                });
            }
            if let Some(max) = x.max_article_num {
                let overs: Vec<i32> = items
                    .iter()
                    .flat_map(|(_, _, a)| a.iter().copied())
                    .filter(|&num| num < 1 || num > max)
                    .collect();
                checks.push(PropertyCheck {
                    name: "articles_in_range".into(),
                    pass: overs.is_empty(),
                    detail: if overs.is_empty() {
                        format!("all cited in 1..={max}")
                    } else {
                        format!("invented refs {overs:?} (corpus 1..={max})")
                    },
                });
            }
        }

        CaseVerdict {
            parsed: true,
            abs_err: None,
            checks,
            display: format!("{n} storylines | {titles}"),
        }
    }
}

// ---------------------------------------------------------------------------
// TransferTask — transfer/trade FP/TP adjudication (fixture-first).
// ---------------------------------------------------------------------------

pub struct TransferTask;

fn normalized_token(s: &str) -> String {
    s.trim().replace(' ', "_").to_lowercase()
}

#[async_trait]
impl LensTask for TransferTask {
    fn name(&self) -> &'static str {
        "transfer"
    }
    fn role(&self) -> Role {
        Role::EmotionalNews
    }
    fn prompt_version(&self) -> &'static str {
        TRANSFER_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> GenerateOptions {
        GenerateOptions {
            // Transfer's system prompt is sport-sensitive. Fixture mode overwrites this with the
            // frozen per-case system; live/capture pair mode uses `gen_options_for`.
            system: Some(transfer_system_prompt("FOOTBALL")),
            temperature: Some(temperature),
            num_predict: TRANSFER_NUM_PREDICT,
            num_ctx: 0,
            json_mode: true,
            format_schema: None,
        }
    }
    fn gen_options_for(&self, temperature: f64, e: &EntitySpec) -> GenerateOptions {
        let sport = e.sport.to_uppercase();
        GenerateOptions {
            system: Some(transfer_system_prompt(&sport)),
            temperature: Some(temperature),
            num_predict: TRANSFER_NUM_PREDICT,
            num_ctx: 0,
            json_mode: true,
            format_schema: None,
        }
    }
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>> {
        if e.entity_type != "team" {
            anyhow::bail!(
                "transfer live/capture evals are team-player pairs; got {}",
                e.key()
            );
        }
        let player_id = e.pair_player_id.ok_or_else(|| {
            anyhow::anyhow!(
                "transfer live/capture evals need a pair: use team:<team_id>:player:<player_id>:sport"
            )
        })?;
        let sport = e.sport.to_uppercase();
        let team_name = lookup_entity_name(&hx.pool, "team", e.entity_id, &sport).await?;
        let tiers = load_tier_map(&hx.pool).await?;
        let candidate = load_candidates(&hx.pool, e.entity_id, &sport, TRANSFER_DEFAULT_MIN_ARTICLES)
            .await?
            .into_iter()
            .find(|c| c.player_id == player_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "player/{player_id} is not a current production transfer candidate for team/{} ({sport})",
                    e.entity_id
                )
            })?;

        let relationship =
            crate::transfer::team_relationship(&hx.pool, e.entity_id, player_id, &sport).await?;
        match build_pair_request(
            hx,
            e.entity_id,
            &team_name,
            &candidate,
            &sport,
            &tiers,
            relationship,
            0.0,
        )
        .await?
        {
            PairBuild::Skipped { .. } => Ok(None),
            PairBuild::Ready(r) => Ok(Some(r.built_prompt)),
        }
    }
    fn evaluate(&self, raw: &str, _label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        let v = match TransferParser.parse(raw) {
            Ok(Some(v)) => v,
            _ => {
                return CaseVerdict {
                    parsed: false,
                    abs_err: None,
                    checks: Vec::new(),
                    display: "unparseable".into(),
                }
            }
        };

        let mut checks = Vec::new();
        if let Some(x) = expect {
            if let Some(want) = x.transfer_is_rumor {
                checks.push(PropertyCheck {
                    name: if want {
                        "transfer_is_rumor".into()
                    } else {
                        "transfer_not_rumor".into()
                    },
                    pass: v.is_rumor == Some(want),
                    detail: format!("is_rumor={}", disp_bool(v.is_rumor)),
                });
            }
            if let Some(want) = x.transfer_direction.as_deref() {
                checks.push(PropertyCheck {
                    name: format!("transfer_direction:{want}"),
                    pass: normalized_token(&v.direction) == normalized_token(want),
                    detail: format!("direction={}", empty_dash(&v.direction)),
                });
            }
            if let Some(want) = x.transfer_stage.as_deref() {
                checks.push(PropertyCheck {
                    name: format!("transfer_stage:{want}"),
                    pass: normalized_token(&v.stage) == normalized_token(want),
                    detail: format!("stage={}", empty_dash(&v.stage)),
                });
            }
            for s in x.subject_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("subject_includes:{s}"),
                    pass: v.subject.contains(s.as_str()),
                    detail: format!("subject={}", empty_dash(&v.subject)),
                });
            }
            for s in x.subject_excludes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("subject_excludes:{s}"),
                    pass: !v.subject.contains(s.as_str()),
                    detail: format!("subject={}", empty_dash(&v.subject)),
                });
            }
            for s in x.summary_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("summary_includes:{s}"),
                    pass: v.summary.contains(s.as_str()),
                    detail: String::new(),
                });
            }
            for s in x.summary_excludes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("summary_excludes:{s}"),
                    pass: !v.summary.contains(s.as_str()),
                    detail: String::new(),
                });
            }
            if let Some(min) = x.confidence_min {
                checks.push(PropertyCheck {
                    name: "confidence_ge".into(),
                    pass: v.confidence >= min,
                    detail: format!("confidence={:.2} ≥ {min:.2}", v.confidence),
                });
            }
            if let Some(max) = x.confidence_max {
                checks.push(PropertyCheck {
                    name: "confidence_le".into(),
                    pass: v.confidence <= max,
                    detail: format!("confidence={:.2} ≤ {max:.2}", v.confidence),
                });
            }
        }

        CaseVerdict {
            parsed: true,
            abs_err: None,
            checks,
            display: format!(
                "is_rumor={} subject={} stage={} conf={:.2} | {}",
                disp_bool(v.is_rumor),
                empty_dash(&v.subject),
                empty_dash(&v.stage),
                v.confidence,
                v.summary
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// RatingTask — stats/analytical rail, PEAK identity specificity + prose richness.
// ---------------------------------------------------------------------------

pub struct RatingTask;

#[async_trait]
impl LensTask for RatingTask {
    fn name(&self) -> &'static str {
        "rating"
    }
    fn role(&self) -> Role {
        Role::StatsLogic
    }
    fn prompt_version(&self) -> &'static str {
        RATING_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> GenerateOptions {
        GenerateOptions {
            system: Some(RATING_SYSTEM_PROMPT.to_string()),
            temperature: Some(temperature),
            num_predict: RATING_NUM_PREDICT,
            num_ctx: 0,
            json_mode: false,
            format_schema: None,
        }
    }
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>> {
        let sport = e.sport.to_uppercase();
        let name = lookup_entity_name(&hx.pool, &e.entity_type, e.entity_id, &sport).await?;
        let req = RatingReq {
            entity_type: e.entity_type.clone(),
            entity_id: e.entity_id,
            entity_name: name,
            sport,
            season: None,
            trigger_type: "periodic".to_string(),
        };
        match build_rating_request(hx, &req, 0.0).await? {
            RatingBuild::NoStats { .. } => Ok(None),
            RatingBuild::Ready(r) => Ok(Some(r.built_prompt)),
        }
    }
    fn evaluate(&self, raw: &str, _label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        let reply = match RatingParser.parse(raw) {
            Ok(Some(r)) if !r.body.trim().is_empty() => r,
            _ => {
                return CaseVerdict {
                    parsed: false,
                    abs_err: None,
                    checks: Vec::new(),
                    display: "unparseable".into(),
                }
            }
        };
        let mut checks = Vec::new();
        let word_count = reply.body.split_whitespace().count() as i32;

        if let Some(x) = expect {
            for s in x.peak_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("peak_includes:{s}"),
                    pass: contains_ci(&reply.divined_peak, s),
                    detail: format!("peak={}", empty_dash(&reply.divined_peak)),
                });
            }
            for s in x.peak_excludes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("peak_excludes:{s}"),
                    pass: !contains_ci(&reply.divined_peak, s),
                    detail: format!("peak={}", empty_dash(&reply.divined_peak)),
                });
            }
            for s in x.prose_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("prose_includes:{s}"),
                    pass: contains_ci(&reply.body, s),
                    detail: String::new(),
                });
            }
            for s in x.prose_excludes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("prose_excludes:{s}"),
                    pass: !contains_ci(&reply.body, s),
                    detail: String::new(),
                });
            }
            if let Some(min) = x.prose_min_words {
                checks.push(PropertyCheck {
                    name: "prose_words_ge".into(),
                    pass: word_count >= min,
                    detail: format!("words={word_count} ≥ {min}"),
                });
            }
            if let Some(max) = x.prose_max_words {
                checks.push(PropertyCheck {
                    name: "prose_words_le".into(),
                    pass: word_count <= max,
                    detail: format!("words={word_count} ≤ {max}"),
                });
            }
        }

        CaseVerdict {
            parsed: true,
            abs_err: None,
            checks,
            display: format!("peak={} | {}", empty_dash(&reply.divined_peak), reply.body),
        }
    }
}

// ---------------------------------------------------------------------------
// MomentumTask — fixture-first stats/analytical trajectory reasoning.
// ---------------------------------------------------------------------------

pub struct MomentumTask;

// Momentum's prompt contract lives in `crate::momentum` (the production stage) — the eval task
// imports it rather than carrying a copy. It USED to carry its own fork ("momentum-eval-v3",
// a duplicate system prompt, its own parser): a relic from momentum's fixture-first era that
// silently diverged from production — the eval was measuring a prompt and a parser production
// no longer ran. Unified 2026-07-12 (lens quality plan Phase 1).

#[async_trait]
impl LensTask for MomentumTask {
    fn name(&self) -> &'static str {
        "momentum"
    }
    fn role(&self) -> Role {
        Role::MomentumLogic
    }
    fn prompt_version(&self) -> &'static str {
        MOMENTUM_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> GenerateOptions {
        GenerateOptions {
            system: Some(MOMENTUM_SYSTEM_PROMPT.to_string()),
            temperature: Some(temperature),
            num_predict: MOMENTUM_NUM_PREDICT,
            num_ctx: 0,
            json_mode: false,
            format_schema: None,
        }
    }
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>> {
        let name = lookup_entity_name(&hx.pool, &e.entity_type, e.entity_id, &e.sport).await?;
        let sport = e.sport.to_uppercase();
        let (_season, _narratives, rating, vibe, momentum, _transfers) =
            load_pillars(hx, &e.entity_type, e.entity_id, &sport).await?;
        if rating.is_none() && vibe.is_none() && momentum.empty() {
            return Ok(None);
        }
        Ok(Some(build_momentum_prompt(
            &e.entity_type,
            &name,
            &e.sport,
            rating.as_ref(),
            vibe.as_ref(),
            &momentum,
        )))
    }
    fn evaluate(&self, raw: &str, _label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        let reply = match parse_momentum_reply(raw) {
            Some(r) => r,
            None => {
                return CaseVerdict {
                    parsed: false,
                    abs_err: None,
                    checks: Vec::new(),
                    display: "unparseable".into(),
                }
            }
        };
        let mut checks = Vec::new();
        let word_count = reply.blurb.split_whitespace().count() as i32;

        if let Some(x) = expect {
            if let Some(min) = x.momentum_score_min {
                checks.push(PropertyCheck {
                    name: "momentum_score_ge".into(),
                    pass: reply.score >= min,
                    detail: format!("score={} ≥ {min}", reply.score),
                });
            }
            if let Some(max) = x.momentum_score_max {
                checks.push(PropertyCheck {
                    name: "momentum_score_le".into(),
                    pass: reply.score <= max,
                    detail: format!("score={} ≤ {max}", reply.score),
                });
            }
            for s in x.prose_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("prose_includes:{s}"),
                    pass: contains_ci(&reply.blurb, s),
                    detail: String::new(),
                });
            }
            for s in x.prose_excludes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("prose_excludes:{s}"),
                    pass: !contains_ci(&reply.blurb, s),
                    detail: String::new(),
                });
            }
            if let Some(min) = x.prose_min_words {
                checks.push(PropertyCheck {
                    name: "prose_words_ge".into(),
                    pass: word_count >= min,
                    detail: format!("words={word_count} ≥ {min}"),
                });
            }
            if let Some(max) = x.prose_max_words {
                checks.push(PropertyCheck {
                    name: "prose_words_le".into(),
                    pass: word_count <= max,
                    detail: format!("words={word_count} ≤ {max}"),
                });
            }
        }

        CaseVerdict {
            parsed: true,
            abs_err: None,
            checks,
            display: format!("score={} | {}", reply.score, reply.blurb),
        }
    }
}

fn disp_bool(b: Option<bool>) -> &'static str {
    match b {
        Some(true) => "true",
        Some(false) => "false",
        None => "unknown",
    }
}

fn empty_dash(s: &str) -> &str {
    if s.trim().is_empty() {
        "–"
    } else {
        s
    }
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_resolves_known_tasks_and_rejects_unknown() {
        assert!(resolve_task("vibe").is_some());
        assert!(resolve_task("sigil").is_some());
        assert!(resolve_task("narratives").is_some());
        assert!(resolve_task("transfer").is_some());
        assert!(resolve_task("rating").is_some());
        assert!(resolve_task("momentum").is_some());
        assert!(resolve_task("nope").is_none());
        assert_eq!(resolve_task("vibe").unwrap().name(), "vibe");
        assert_eq!(resolve_task("sigil").unwrap().name(), "sigil");
        assert_eq!(resolve_task("narratives").unwrap().name(), "narratives");
        assert_eq!(resolve_task("transfer").unwrap().name(), "transfer");
        assert_eq!(resolve_task("rating").unwrap().name(), "rating");
        assert_eq!(resolve_task("momentum").unwrap().name(), "momentum");
    }

    #[test]
    fn all_task_names_are_unique_and_resolvable() {
        let names = all_task_names();
        let mut seen = std::collections::HashSet::new();
        for n in names {
            assert!(seen.insert(*n), "duplicate task name {n}");
            assert!(resolve_task(n).is_some(), "{n} not resolvable");
            assert!(lens_parameters(n).is_some(), "{n} has no lens parameters");
        }
    }

    #[test]
    fn lens_parameters_capture_current_operating_personas() {
        let rating = lens_parameters("rating").unwrap();
        assert_eq!(rating.rail, Rail::StatsAnalytical);
        assert_eq!(rating.operator, "opposing team scout");
        assert!(rating.mandate.contains("greatest strength"));

        let transfer = lens_parameters("transfer").unwrap();
        assert_eq!(transfer.rail, Rail::EmotionalNews);
        assert_eq!(transfer.operator, "transfer expert");

        let sigil = lens_parameters("sigil").unwrap();
        assert_eq!(sigil.rail, Rail::Synthesis);
        assert_eq!(sigil.operator, "reasoned expert network panelist");
    }

    #[test]
    fn entity_key_renders_transfer_pair_when_present() {
        let e = EntitySpec {
            entity_type: "team".into(),
            entity_id: 14,
            sport: "NBA".into(),
            pair_player_id: Some(237),
        };
        assert_eq!(e.key(), "team:14:player:237:NBA");
    }

    // --- sigil disagreement rubric ------------------------------------------------

    const CONFLICTED: &str = "SCORE: 68\nCONVERGENCE: 40\nDISAGREEMENT: strong PEAK vs sliding momentum and negative narrative\nWHY_NOW: trade-demand reports\nBLURB: Elite wing under pressure.";
    const CONVERGENT: &str =
        "SCORE: 87\nCONVERGENCE: 95\nBLURB: A rising guard drawing All-Star buzz.";

    #[test]
    fn sigil_rubric_passes_on_conflicted_reply() {
        let x = Expect {
            convergence_max: Some(55),
            disagreement_nonempty: Some(true),
            disagreement_includes: Some(vec!["PEAK".into()]),
            ..Default::default()
        };
        let v = SigilTask.evaluate(CONFLICTED, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn sigil_rubric_fails_convergent_reply_against_conflict_expect() {
        let x = Expect {
            convergence_max: Some(55),
            disagreement_nonempty: Some(true),
            ..Default::default()
        };
        let v = SigilTask.evaluate(CONVERGENT, None, Some(&x));
        // 95 is not <= 55, and there is no disagreement line.
        assert!(!v.all_checks_pass());
        assert_eq!(v.checks_passed(), 0);
    }

    #[test]
    fn sigil_aligned_expect_inverts_between_the_two_replies() {
        let x = Expect {
            convergence_min: Some(70),
            disagreement_nonempty: Some(false),
            ..Default::default()
        };
        assert!(SigilTask
            .evaluate(CONVERGENT, None, Some(&x))
            .all_checks_pass());
        assert!(!SigilTask
            .evaluate(CONFLICTED, None, Some(&x))
            .all_checks_pass());
    }

    #[test]
    fn disagreement_excludes_catches_parroted_example() {
        // The model parrots the system-prompt example for a case with no such conflict.
        let parroted = "SCORE: 65\nCONVERGENCE: 80\nDISAGREEMENT: \"strong PEAK vs sliding momentum and negative narrative\"\nBLURB: Role player amid trade talk.";
        let x = Expect {
            disagreement_excludes: Some(vec!["sliding momentum".into()]),
            ..Default::default()
        };
        let v = SigilTask.evaluate(parroted, None, Some(&x));
        assert!(
            !v.all_checks_pass(),
            "excludes should catch the parroted string"
        );
    }

    #[test]
    fn placeholder_disagreement_scores_as_absent() {
        // `DISAGREEMENT: N/A` (and quoted / none / dash) must score as ABSENT. Normalization lives
        // in `parse_synthesis_response` (single source of truth); this guards it end-to-end via the
        // eval's evaluate path, so the fixtures reflect what actually gets persisted + served.
        for raw in [
            "SCORE: 87\nCONVERGENCE: 95\nDISAGREEMENT: N/A\nBLURB: aligned.",
            "SCORE: 87\nCONVERGENCE: 95\nDISAGREEMENT: \"none\"\nBLURB: aligned.",
            "SCORE: 87\nCONVERGENCE: 95\nDISAGREEMENT: -\nBLURB: aligned.",
        ] {
            let x = Expect {
                disagreement_nonempty: Some(false),
                ..Default::default()
            };
            let v = SigilTask.evaluate(raw, None, Some(&x));
            assert!(
                v.all_checks_pass(),
                "placeholder should be absent for {raw:?}: {:?}",
                v.checks
            );
        }
        // An excludes check must not match a placeholder either.
        let x = Expect {
            disagreement_excludes: Some(vec!["sliding".into()]),
            ..Default::default()
        };
        let v = SigilTask.evaluate("SCORE: 50\nDISAGREEMENT: N/A\nBLURB: x.", None, Some(&x));
        assert!(v.all_checks_pass());
    }

    #[test]
    fn sigil_unparseable_reply_is_not_parsed() {
        let v = SigilTask.evaluate("the sigil feels like a 64 today", None, None);
        assert!(!v.parsed); // no SCORE line ⇒ score 0
    }

    // --- vibe MAE axis ------------------------------------------------------------

    #[test]
    fn vibe_evaluate_computes_abs_err() {
        let v = VibeTask.evaluate("SCORE: 30\nVIBE: grim outlook", Some(80.0), None);
        assert!(v.parsed);
        assert_eq!(v.abs_err, Some(50.0));
    }

    #[test]
    fn vibe_unparseable_has_no_abs_err() {
        let v = VibeTask.evaluate("no score here at all", Some(80.0), None);
        assert!(!v.parsed);
        assert_eq!(v.abs_err, None);
    }

    #[test]
    fn vibe_score_band_checks() {
        let x = Expect {
            score_max: Some(40),
            ..Default::default()
        };
        assert!(VibeTask
            .evaluate("SCORE: 30\nVIBE: grim", None, Some(&x))
            .all_checks_pass());
        assert!(!VibeTask
            .evaluate("SCORE: 70\nVIBE: bright", None, Some(&x))
            .all_checks_pass());
    }

    // --- narrative grouping + grounding rubric ------------------------------------

    // Two clean, grounded storylines over a 3-article corpus.
    const GROUNDED: &str = r#"{"narratives":[
        {"title":"Marcus Vale trade demand","body":"Beat writers report Vale privately asked about his future amid coaching friction.","articles":[1,2]},
        {"title":"Vale's efficient scoring stretch","body":"He is posting top-percentile efficiency over the last five games.","articles":[3]}
    ]}"#;

    #[test]
    fn narratives_grounded_reply_passes_grounding_rubric() {
        let x = Expect {
            narratives_min: Some(1),
            narratives_max: Some(6),
            title_includes: Some(vec!["Vale".into()]),
            title_excludes: Some(vec!["Transfer news".into()]),
            all_cite_articles: Some(true),
            max_article_num: Some(3),
            ..Default::default()
        };
        let v = NarrativeTask.evaluate(GROUNDED, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn narratives_invented_article_reference_fails_range_check() {
        // Cites article 9 when the corpus only has 3 — an invented reference.
        let reply = r#"{"narratives":[{"title":"Vale rumor","body":"x","articles":[1,9]}]}"#;
        let x = Expect {
            max_article_num: Some(3),
            ..Default::default()
        };
        let v = NarrativeTask.evaluate(reply, None, Some(&x));
        assert!(v.parsed);
        assert!(!v.all_checks_pass(), "9 is out of the 1..=3 corpus range");
    }

    #[test]
    fn narratives_uncited_storyline_fails_all_cite() {
        let reply = r#"{"narratives":[{"title":"Vale buzz","body":"vague hype with no article","articles":[]}]}"#;
        let x = Expect {
            all_cite_articles: Some(true),
            ..Default::default()
        };
        let v = NarrativeTask.evaluate(reply, None, Some(&x));
        assert!(!v.all_checks_pass(), "an uncited storyline is ungrounded");
    }

    #[test]
    fn narratives_generic_title_and_invented_move_are_caught() {
        // A generic title AND a fabricated "moving to" storyline the corpus never supports.
        let reply = r#"{"narratives":[{"title":"Transfer news","body":"Vale is moving to the Kings next week.","articles":[1]}]}"#;
        let x = Expect {
            title_excludes: Some(vec!["Transfer news".into()]),
            body_excludes: Some(vec!["moving to".into()]),
            ..Default::default()
        };
        let v = NarrativeTask.evaluate(reply, None, Some(&x));
        assert_eq!(
            v.checks_passed(),
            0,
            "both excludes should fire: {:?}",
            v.checks
        );
    }

    #[test]
    fn narratives_quiet_cycle_is_parsed_with_zero_count() {
        // An empty array is a legitimate quiet cycle — parsed, count 0 (NOT unparseable).
        let v = NarrativeTask.evaluate(
            r#"{"narratives":[]}"#,
            None,
            Some(&Expect {
                narratives_max: Some(1),
                narratives_min: Some(1),
                ..Default::default()
            }),
        );
        assert!(v.parsed);
        // max(1) passes (0 ≤ 1); min(1) fails (0 < 1).
        assert_eq!(v.checks_passed(), 1);
    }

    #[test]
    fn narratives_malformed_reply_is_unparseable() {
        let v = NarrativeTask.evaluate("the news feels grouped today", None, None);
        assert!(!v.parsed);
    }

    // --- transfer FP/TP adjudication rubric --------------------------------------

    const TRUE_TRANSFER: &str = r#"{"is_rumor":true,"subject":"Lina Foss","direction":"incoming","stage":"advanced_talks","summary":"Everton are in advanced talks to sign Lina Foss from Brann, according to TV2.","confidence":0.83}"#;

    #[test]
    fn transfer_true_positive_passes_adjudication_rubric() {
        let x = Expect {
            transfer_is_rumor: Some(true),
            transfer_direction: Some("incoming".into()),
            transfer_stage: Some("advanced_talks".into()),
            subject_includes: Some(vec!["Lina Foss".into()]),
            summary_includes: Some(vec!["Everton".into(), "Brann".into()]),
            confidence_min: Some(0.7),
            ..Default::default()
        };
        let v = TransferTask.evaluate(TRUE_TRANSFER, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn transfer_live_options_use_sport_specific_noun() {
        let football = EntitySpec {
            entity_type: "team".into(),
            entity_id: 9,
            sport: "football".into(),
            pair_player_id: Some(70),
        };
        let nba = EntitySpec {
            entity_type: "team".into(),
            entity_id: 14,
            sport: "nba".into(),
            pair_player_id: Some(237),
        };
        let football_system = TransferTask.gen_options_for(0.0, &football).system.unwrap();
        let nba_system = TransferTask.gen_options_for(0.0, &nba).system.unwrap();
        assert!(football_system.contains("current transfer"));
        assert!(nba_system.contains("current trade"));
    }

    #[test]
    fn transfer_false_positive_reply_clears_not_rumor_expect() {
        let raw = r#"{"is_rumor":false,"subject":"Mika Salo","direction":"unclear","stage":"speculation","summary":"","confidence":0.12}"#;
        let x = Expect {
            transfer_is_rumor: Some(false),
            subject_includes: Some(vec!["Mika Salo".into()]),
            subject_excludes: Some(vec!["Lina Foss".into()]),
            confidence_max: Some(0.3),
            ..Default::default()
        };
        let v = TransferTask.evaluate(raw, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn transfer_invented_fee_is_caught_by_summary_excludes() {
        let raw = r#"{"is_rumor":true,"subject":"Lina Foss","direction":"incoming","stage":"concrete_interest","summary":"Everton want Lina Foss in a £12m move.","confidence":0.74}"#;
        let x = Expect {
            transfer_is_rumor: Some(true),
            summary_excludes: Some(vec!["£12m".into()]),
            ..Default::default()
        };
        let v = TransferTask.evaluate(raw, None, Some(&x));
        assert!(!v.all_checks_pass());
    }

    #[test]
    fn transfer_unknown_commit_fails_boolean_expect() {
        let raw = r#"{"subject":"Lina Foss","direction":"incoming","stage":"speculation","summary":"","confidence":0.2}"#;
        let x = Expect {
            transfer_is_rumor: Some(true),
            ..Default::default()
        };
        let v = TransferTask.evaluate(raw, None, Some(&x));
        assert!(v.parsed);
        assert!(!v.all_checks_pass());
    }

    #[test]
    fn transfer_malformed_reply_is_unparseable() {
        let v = TransferTask.evaluate("looks like a rumor", None, None);
        assert!(!v.parsed);
    }

    // --- rating / stats-lens rubric ---------------------------------------------

    const RATING_REPLY: &str = "PEAK: Rim protection\nAn elite rim protector who grades at the 94th percentile in blocks and anchors the paint without fouling. The profile is thinner as a creator, but the defensive identity is clear and valuable.";

    #[test]
    fn rating_rubric_scores_peak_specificity_and_prose_richness() {
        let x = Expect {
            peak_includes: Some(vec!["Rim protection".into()]),
            peak_excludes: Some(vec!["No standout".into()]),
            prose_includes: Some(vec!["94th percentile".into(), "defensive identity".into()]),
            prose_excludes: Some(vec!["triple-double".into()]),
            prose_min_words: Some(20),
            prose_max_words: Some(60),
            ..Default::default()
        };
        let v = RatingTask.evaluate(RATING_REPLY, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn rating_rubric_catches_generic_peak_and_thin_prose() {
        let x = Expect {
            peak_includes: Some(vec!["Rim protection".into()]),
            peak_excludes: Some(vec!["No standout".into()]),
            prose_min_words: Some(20),
            ..Default::default()
        };
        let v = RatingTask.evaluate("PEAK: No standout skill\nAverage profile.", None, Some(&x));
        assert!(v.parsed);
        assert_eq!(v.checks_passed(), 0, "checks: {:?}", v.checks);
    }

    // --- momentum fixture-first trajectory rubric ---------------------------------

    #[test]
    fn momentum_parser_extracts_score_and_read() {
        // s4 contract: SCORE + READ; a stray MOMENTUM line (a model echoing the decided
        // direction) is tolerated and ignored.
        let raw = "MOMENTUM: rising\nSCORE: 3\nREAD: PEAK is rising while Vibe is steady, so the current direction is modestly positive.";
        let parsed = parse_momentum_reply(raw).unwrap();
        assert_eq!(parsed.score, 3);
        assert!(parsed.blurb.contains("PEAK is rising"));
    }

    #[test]
    fn momentum_rubric_scores_signed_band_and_prose() {
        let x = Expect {
            momentum_score_max: Some(-2),
            prose_includes: Some(vec!["Vibe".into()]),
            prose_excludes: Some(vec!["surging".into()]),
            ..Default::default()
        };
        let raw = "SCORE: -3\nREAD: Vibe is pulling the profile down despite a steadier PEAK read.";
        let v = MomentumTask.evaluate(raw, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn momentum_unparseable_reply_is_not_parsed() {
        let v = MomentumTask.evaluate("the trend is probably fine", None, None);
        assert!(!v.parsed);
    }

    // --- fixture serde + drift ----------------------------------------------------

    #[test]
    fn fixture_round_trips_and_defaults_expect() {
        let json = r#"{
            "name": "aligned-convergent",
            "task": "sigil",
            "prompt_version": "s11",
            "system": "SYS",
            "user_prompt": "Entity: X",
            "temperature": 0.0,
            "expect": { "convergence_min": 70, "disagreement_nonempty": false }
        }"#;
        let fx: Fixture = serde_json::from_str(json).unwrap();
        assert_eq!(fx.name, "aligned-convergent");
        assert_eq!(fx.expect.convergence_min, Some(70));
        assert_eq!(fx.expect.disagreement_nonempty, Some(false));
        assert_eq!(fx.expect.score_min, None); // defaulted
                                               // A fixture may omit expect entirely.
        let bare = r#"{"name":"n","task":"sigil","prompt_version":"s11","system":"s","user_prompt":"u","temperature":0.0}"#;
        let fx2: Fixture = serde_json::from_str(bare).unwrap();
        assert_eq!(fx2.expect.convergence_min, None);
    }

    #[test]
    fn fixture_drift_flags_prompt_version_mismatch() {
        let mut fx = Fixture {
            name: "f".into(),
            task: "sigil".into(),
            prompt_version: SIGIL_PROMPT_VERSION.into(),
            system: "s".into(),
            user_prompt: "u".into(),
            temperature: 0.0,
            expect: Expect::default(),
        };
        assert!(fixture_drift(&fx, &SigilTask).is_none());
        fx.prompt_version = "s1".into();
        assert!(fixture_drift(&fx, &SigilTask).is_some());
    }
}
