//! Evaluation adapters reuse application preparation and Studio parsing.
//! Each task supplies its route, current model options and optional fixture assertions.
//! Live cases read corpus data without claiming work or publishing products.
//! Mechanical checks and optional numeric labels complement human review of the generated work.

use crate::application::models::Models;
use crate::evidence::corpus::lookup_entity_name;
use crate::plugins::analyst::parser::parse_momentum_reply;
use crate::plugins::analyst::prompt::load_momentum_context;
use crate::plugins::graph::adapter::load_graph_article_context;
use crate::plugins::graph::cognition::{
    build_graph_prompt, graph_opts, GraphCandidate, GraphParser, GRAPH_PROMPT_VERSION,
};
use crate::plugins::influencer::{prompt::VIBE_NUM_PREDICT, VibeParser};
use crate::plugins::insider::prompt::preview as preview_insider;
use crate::plugins::insider::prompt::{
    options as insider_options, PROMPT_VERSION as INSIDER_PROMPT_VERSION,
};
use crate::plugins::insider::Reply;
use crate::plugins::investigator::cognition::prompt::{
    prose_opts, ProseReadParser, INVESTIGATOR_PROSE_CONTRACT_VERSION,
};
use crate::plugins::journalist::prompt::CorpusItem;
use crate::plugins::oracle::prompt::load_pillars;
use crate::plugins::oracle::prompt::{
    assemble as assemble_oracle_context, generation_options as oracle_generation_options,
    Subject as OracleSubject, ORACLE_PROMPT_VERSION,
};
use crate::plugins::scout::prompt::{build_rating_request, RatingBuild, RatingReq};
use crate::plugins::support::guards::count_sentences;
use crate::runtime::route::RouteKey;
use crate::studio::model::GenerateOptions;
use crate::studio::Parser;
use crate::util::truncate;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// EntitySpec is one entity a case scores. Lives here (not in the bin) so `build_request` and the
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

/// Product-level operating parameters for a lens. These are the "who is thinking?" and "what must
/// they optimize for?" notes that should shape prompts, fixtures, and adoption decisions without
/// hard-coding a model id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LensParameters {
    pub operator: &'static str,
    pub mandate: &'static str,
    pub credibility_guard: &'static str,
}

/// lens_parameters is the code home for the lens taxonomy — the six public characters plus the
/// two evaluated internal seats (investigator, graph). `operator` carries the character identity
/// (the cast locked in wiki/Characters.md, 2026-07-21); the junction's system prompt is that
/// character's voice, so a voice change is a prompt change, never a rename here.
///
/// There is no `rail` here any more. It was product taxonomy from the two-rail era — a guess that
/// lenses would eventually route by model family — and its own doc admitted roles were the serving
/// primitive "until evals prove a split". The split never came: routing is per route
/// (`COGNITION_ROUTE_<ROLE>`), and the real topology is two HOSTS, not two rails. It had also gone
/// wrong on its own terms, filing the Editor, the Investigator and graph under "emotional/news"
/// because a two-rail world had nowhere else to put a seat that reads text.
pub fn lens_parameters(name: &str) -> Option<LensParameters> {
    match name {
        "narratives" => Some(LensParameters {
            operator: "The Journalist",
            mandate: "Compile the stories swirling around the entity into grounded storylines.",
            credibility_guard: "Group what sources actually say; do not inflate vague hype or off-entity noise.",
        }),
        "transfer" => Some(LensParameters {
            operator: "The Insider",
            mandate: "Get movement predictions out quickly while preserving long-term credibility.",
            credibility_guard: "Fail closed on name-drops, stale links, weak sourcing, and misleading heat.",
        }),
        "vibe" => Some(LensParameters {
            operator: "The Influencer",
            mandate: "Articulate the plugin's supplied publisher reporting with its attribution and qualifications.",
            credibility_guard: "Express feelings only when supplied by the source. History is context; sentiment remains unknown.",
        }),
        "rating" => Some(LensParameters {
            operator: "The Scout",
            mandate: "Tell the story of the entity's playing characteristics, expected contributions and supported profile direction.",
            credibility_guard: "Interpret supplied measurements together; use metadata as context and preserve the distinction between profile, form and relative standing.",
        }),
        "momentum" => Some(LensParameters {
            operator: "The Analyst",
            mandate: "Read the directional force of form (the rating trajectory) and feeling (the news mood), then narrate the decided direction with conviction.",
            credibility_guard: "Stay detached and results-only; do not chase sentiment hype or cling to stale profile strength.",
        }),
        "oracle" => Some(LensParameters {
            operator: "the Oracle",
            mandate: "Read the available evidence, deliver the entity's reading in the house voice, then render the score earned by its circumstances.",
            credibility_guard: "The mysticism lives in the telling, never the facts — ground every claim in the supplied evidence; invent nothing and expose no internal field or product names.",
        }),
        "investigator" => Some(LensParameters {
            operator: "The Investigator",
            mandate: "Read one Wikipedia page summary and quote verbatim what it says about a name the news wrote differently — the connecting name form, the occupation phrase, the teams — so code can verify every quote by containment and decide.",
            credibility_guard: "Copy, never conclude: a field that is not a contiguous run of page text is discarded by the gate; only this page, never model knowledge of the person.",
        }),
        "graph" => Some(LensParameters {
            operator: "narrative archivist",
            mandate: "Extract the typed relations and person discoveries one vetted article actually states into the graph.",
            credibility_guard: "Closed candidate list only; attach each relation to the true counterparty; an empty extraction beats an invented one.",
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
    /// Whether the articulation should pass instead of publish a card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abstain: Option<bool>,
    // Score bands for products that supply a score.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_max: Option<i32>,
    // (The v13/v17 `hook_nonempty`/`hook_max_words`/`hook_excludes` axes retired 08-19: the
    // hook contract is a GLOBAL invariant — one `hook_contract` check per reply via
    // `guards::hook_violation`, the same rule `VibeParser` enforces in production.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blurb_includes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blurb_excludes: Option<Vec<String>>,
    /// Grounding: at least one returned body contains each string (names the who/what/where).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_includes: Option<Vec<String>>,
    /// No-invention: no returned body contains any of these (e.g. claiming THIS entity is moving when
    /// the corpus only has other teams scheming around them — the system prompt's hardest rule).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_excludes: Option<Vec<String>>,
    /// Edition budget (n18): total sentences across ALL returned bodies must not exceed this.
    /// Counted crudely (terminal .!? runs) — a ceiling against padding, not a style meter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_sentences_max: Option<i32>,
    // transfer false-positive / true-positive rubric.
    /// Transfer adjudication: assert whether the model commits to a served rumor (`true`) or clears
    /// the pair (`false`). `None` in a parsed verdict is the UNKNOWN/fail-closed path and fails
    /// either explicit boolean expectation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer_is_rumor: Option<bool>,
    /// Direction relative to the named team (`incoming`, `outgoing`, `unclear`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer_direction: Option<String>,
    /// TaskKey ladder expectation (`speculation`, `concrete_interest`, `advanced_talks`, `here_we_go`).
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
    /// Identity specificity, asserted on the brief's prose: the brief should name the actual
    /// standout skill, not a generic role or an average datapoint. (Named `peak_includes`/
    /// `peak_excludes` until the PEAK-era vocabulary sweep; no frozen fixture carried the old
    /// keys.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_includes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_excludes: Option<Vec<String>>,
    /// Scouting-report body checks. Kept separate from narrative `body_*` so stats fixtures can
    /// describe prose richness without changing storyline semantics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prose_includes: Option<Vec<String>>,
    /// ANY-of groups over the reply prose: each entry is ONE check — a pipe-delimited synonym
    /// group ("form|tape|performances") that passes when at least one alternative appears
    /// (contains_ci each). Multiple entries = multiple independent checks, so a fixture can
    /// require BOTH signals named ("form|tape…", "mood|emotion…"). Added for momentum s15,
    /// where "name the signal" stopped meaning a product name ("PEAK") and started meaning the
    /// sport's own words — which legitimately vary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prose_includes_any: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prose_excludes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prose_min_words: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prose_max_words: Option<i32>,
    // graph typed-extraction rubric (number-level: N = the fixture prompt's candidate numbering).
    /// The fixture prompt's candidate list as entity TYPES by number ("player"/"team") — evaluate
    /// reconstructs the GraphParser's candidate list from this (ids = the 1-based number), so the
    /// REAL production parser runs and the checks assert on its resolved output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_candidate_types: Option<Vec<String>>,
    /// Attachment discipline: each "subject:predicate:object" triple must exist in the parsed
    /// relations (numbers; object "-" = unary/no counterparty; predicate "*" = any predicate).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relations_include: Option<Vec<String>>,
    /// The object-attachment pin: no parsed relation may match any of these triples (same syntax) —
    /// e.g. the g2-measured Rogers→Arsenal slip where Chelsea was the counterparty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relations_exclude: Option<Vec<String>>,
    /// Over-extraction guard: at most this many relations (0 pins the clean-empty case).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relations_max: Option<i32>,
    /// Person discovery: each "Name:kind" (kind optional) must appear in the parsed persons.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persons_include: Option<Vec<String>>,
    /// No player leakage / no invention: no parsed person name may contain any of these.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persons_exclude: Option<Vec<String>>,
    // The Investigator's prose contract (`ip1`) axes — verbatim-quote fields, checked as
    // fragments of what the model copied (containment against the page is the GATE's job;
    // the fixture asserts the model quoted the right things at all).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_kind_is: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_includes: Option<Vec<String>>,
    /// `true` asserts the model connected NOTHING — the negative-page discipline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_empty: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occupation_includes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prose_teams_include: Option<Vec<String>>,
    // oracle / persona-reading rubric.
    /// Reading substring checks, matched CASE-INSENSITIVELY (a voice lens varies casing freely;
    /// the jargon-exclusion checks must catch "Convergence" as well as "convergence").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_includes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_excludes: Option<Vec<String>>,
    /// The conventions' 2-4 sentence read budget, encoded as fixture validation
    /// (`guards::count_sentences`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_min_sentences: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_max_sentences: Option<i32>,
    // momentum / trajectory reasoning rubric: PROSE ONLY.
    // momentum_score_min/max were removed in s11 — the Analyst no longer emits a score, so
    // there was nothing left for them to assert. Both numbers (direction and the ±5
    // conviction) are computed by the junction and unit-tested there, not gated here.
}

/// Selected evidence plus assertions and review criteria. `system` exists only for historical replay.
///
/// `parts` is the plugin's own input, kept so the harness can re-assemble the
/// package with the plugin's CURRENT assembler. `user_prompt` is the captured
/// render, retained for review and for the tasks that have no parts yet; a test
/// asserts the two agree, so a change to a plugin's assembler fails here rather
/// than silently invalidating a stored string. A plugin that assembles from
/// parts records them; a plugin whose prompt is still a string records only
/// `user_prompt`, and a task declares which it is through
/// [`LensTask::stores_parts`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fixture {
    pub name: String,
    pub task: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sport: String,
    pub prompt_version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub system: String,
    pub user_prompt: String,
    /// The plugin's input, in the shape its assembler consumes. Present for a
    /// task whose `stores_parts()` is true; absent for one still holding a
    /// captured string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<serde_json::Value>,
    pub temperature: f64,
    #[serde(default)]
    pub expect: Expect,
    /// Human assessment of factual meaning and character expression; never sent to the model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review: Vec<String>,
}

/// A lens eval task: the routing + prompt + scoring seam `bin/eval` runs against. Object-safe
/// (`build_request` boxed by `async_trait`), so tasks dispatch through `Box<dyn LensTask>`.
#[async_trait]
pub trait LensTask: Send + Sync {
    /// Registry key (`"vibe"`, `"oracle"`) — also the `fixtures/quality/<name>/` dir.
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
    fn role(&self) -> RouteKey;
    /// The stage's prompt-contract version — single-sourced from the stage const, drift-checked
    /// against a fixture's frozen `prompt_version`.
    fn prompt_version(&self) -> &'static str;
    /// system + num_predict + json_mode from the stage consts; the caller chooses `temperature`
    /// (live = 0.0; fixture = the authored value).
    ///
    /// Fallible because a plugin whose decode contract is a function of the
    /// prepared world — the Journalist's keyed schema and its history-bearing
    /// manual — cannot supply one option set. Those plugins return their real
    /// options from [`LensTask::assemble`] and refuse here, rather than handing
    /// back a plausible-looking contract that does not match the package.
    fn gen_options(&self, temperature: f64) -> Result<GenerateOptions>;
    /// Optional per-case override for tasks whose system prompt depends on the live case.
    fn gen_options_for(&self, temperature: f64, e: &EntitySpec) -> Result<GenerateOptions> {
        self.gen_options_for_sport(temperature, &e.sport)
    }
    fn gen_options_for_sport(&self, temperature: f64, _sport: &str) -> Result<GenerateOptions> {
        self.gen_options(temperature)
    }
    /// Build the EXACT production user-prompt for an entity. `Ok(None)` = no-corpus skip (the stage
    /// would write a marker without a model call — nothing to score).
    async fn build_request(
        &self,
        pool: &sqlx::PgPool,
        models: &Models,
        e: &EntitySpec,
    ) -> Result<Option<Prepared>>;
    /// Parse + score one raw reply. Pure/sync/offline. `label` drives the MAE axis (vibe live);
    /// `expect` drives the property axis (fixtures). Both optional and independent.
    fn evaluate(&self, raw: &str, label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict;
    /// Rebuild this plugin's request from stored fixture `parts`.
    ///
    /// The package AND the decode options, because for a plugin whose contract
    /// depends on the world they are one artifact: replaying a stored prompt
    /// against a fixed schema tests a request production never sends. The default
    /// refuses rather than falling back to the stored string, so a plugin without
    /// an assembler cannot quietly pass on a stale capture.
    fn assemble(&self, _parts: &serde_json::Value) -> Result<Prepared> {
        anyhow::bail!(
            "{} has no parts assembler; its fixtures store a captured prompt",
            self.name()
        )
    }
    /// Whether this task's fixtures store `parts` rather than only a prompt string.
    ///
    /// True means every one of this task's fixtures must carry parts that
    /// `assemble` rebuilds byte-identically to the stored `user_prompt`. It is a
    /// declaration, so a plugin that migrates states it rather than leaving the
    /// harness to guess.
    fn stores_parts(&self) -> bool {
        false
    }

    fn prepare_fixture(&self, fixture: &Fixture) -> Result<Prepared> {
        anyhow::ensure!(
            !self.stores_parts() || fixture.parts.is_some(),
            "{} fixture stores no parts",
            self.name()
        );
        let mut request = if let Some(parts) = &fixture.parts {
            let request = self.assemble(parts)?;
            anyhow::ensure!(
                request.user_prompt == fixture.user_prompt,
                "{} fixture package drift; recapture from parts",
                self.name()
            );
            request
        } else {
            Prepared::captured(
                fixture.user_prompt.clone(),
                self.gen_options_for_sport(fixture.temperature, &fixture.sport)?,
            )
        };
        request.options.temperature = Some(fixture.temperature);
        Ok(request)
    }

    fn evaluate_prepared(
        &self,
        request: &Prepared,
        raw: &str,
        label: Option<f64>,
        expect: Option<&Expect>,
    ) -> CaseVerdict {
        let _ = request;
        self.evaluate(raw, label, expect)
    }
}

/// One plugin request rebuilt from a fixture's parts.
///
/// Produced by the plugin's own preparation, so the prompt, the manual and the
/// response schema are the same three artifacts production sends. Keeping them
/// together is the point: a harness that reassembles the prompt but keeps a
/// fixed schema has tested a request that does not exist.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub user_prompt: String,
    pub options: GenerateOptions,
    pub parts: Option<serde_json::Value>,
    pub should_call: bool,
}

impl Prepared {
    pub fn captured(user_prompt: String, options: GenerateOptions) -> Self {
        Self {
            user_prompt,
            options,
            parts: None,
            should_call: true,
        }
    }
}

fn rejected(reason: &str) -> CaseVerdict {
    CaseVerdict {
        parsed: false,
        abs_err: None,
        checks: vec![],
        display: reason.into(),
    }
}

/// resolve_task maps a task name to its `LensTask`. Adding a task = a new unit struct + one arm.
pub fn resolve_task(name: &str) -> Option<Box<dyn LensTask>> {
    match name {
        "narratives" => Some(Box::new(NarrativesTask)),
        "vibe" => Some(Box::new(VibeTask)),
        "oracle" => Some(Box::new(OracleTask)),
        "transfer" => Some(Box::new(TransferTask)),
        "rating" => Some(Box::new(RatingTask)),
        "momentum" => Some(Box::new(MomentumTask)),
        "graph" => Some(Box::new(GraphTask)),
        "investigator" => Some(Box::new(InvestigatorTask)),
        _ => None,
    }
}

/// all_task_names lists the registered tasks (for usage output + unknown-task errors).
pub fn all_task_names() -> &'static [&'static str] {
    &[
        "narratives",
        "vibe",
        "oracle",
        "transfer",
        "rating",
        "momentum",
        "graph",
        "investigator",
    ]
}

// ---------------------------------------------------------------------------
// NarrativesTask — the Journalist's own parts, assembler and keyed parser.
//
// The window that deployed this plugin had no shared-harness coverage at all:
// `fixtures/quality/narratives/` was empty and "narratives" was absent from
// `all_task_names()`, so the only gate was a bespoke replay example. Fixtures
// here store `journalist::prompt::Parts` and are rebuilt through the plugin's current
// assembler, so a change to the package is a test failure rather than a stored
// string that quietly stops matching production.
// ---------------------------------------------------------------------------
pub struct NarrativesTask;

#[async_trait]
impl LensTask for NarrativesTask {
    fn name(&self) -> &'static str {
        "narratives"
    }
    fn role(&self) -> RouteKey {
        crate::plugins::journalist::manifest::ROUTE
    }
    fn prompt_version(&self) -> &'static str {
        crate::plugins::journalist::prompt::NARRATIVES_PROMPT_VERSION
    }
    fn gen_options(&self, _temperature: f64) -> Result<GenerateOptions> {
        // The system prompt and the response schema both depend on what the world
        // holds — the schema is keyed by report count, the manual by whether any
        // report carries history — so there is no one option set to return here.
        // `assemble` produces the real request; handing back a fixed one would
        // let a fixture pass against a contract production never sends.
        anyhow::bail!(
            "narratives options depend on the prepared world; assemble a fixture's parts instead"
        )
    }
    async fn build_request(
        &self,
        pool: &sqlx::PgPool,
        _models: &Models,
        e: &EntitySpec,
    ) -> Result<Option<Prepared>> {
        let subject = crate::plugins::meta::EntityMeta {
            name: lookup_entity_name(pool, &e.entity_type, e.entity_id, &e.sport).await?,
            entity_type: e.entity_type.clone(),
            entity_id: e.entity_id,
            sport: e.sport.to_uppercase(),
        };
        let sources = crate::plugins::harvester::delivery::load_for_character(
            pool,
            crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
            &subject.entity_type,
            subject.entity_id,
            &subject.sport,
        )
        .await?;
        if sources.is_empty() {
            return Ok(None);
        }
        let corpus = sources.iter().map(CorpusItem::from).collect::<Vec<_>>();
        let now = crate::plugins::influencer::now();
        let continuity =
            crate::plugins::journalist::memories::load_for_assignment(pool, &subject, &corpus, now)
                .await?;
        let assignment =
            crate::plugins::journalist::prompt::prepare(subject, corpus, &continuity, now)?;
        // No selected report is a no-call, not a package with nothing in it.
        if assignment.selected.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.assemble(&serde_json::to_value(
            crate::plugins::journalist::prompt::Parts {
                subject: assignment.subject,
                reports: assignment.selected,
                memory: assignment.memories,
            },
        )?)?))
    }
    fn evaluate(&self, _raw: &str, _label: Option<f64>, _expect: Option<&Expect>) -> CaseVerdict {
        rejected("Journalist evaluation requires the prepared request")
    }
    fn evaluate_prepared(
        &self,
        request: &Prepared,
        raw: &str,
        _label: Option<f64>,
        expect: Option<&Expect>,
    ) -> CaseVerdict {
        let Some(count) = request
            .parts
            .as_ref()
            .and_then(|p| p["reports"].as_array())
            .map(Vec::len)
        else {
            return rejected("Journalist evaluation requires report parts");
        };
        match crate::plugins::support::form::parse_journalist(raw, count) {
            Ok(reply) => {
                let bodies = reply
                    .narratives
                    .iter()
                    .map(|report| report.text.as_str())
                    .collect::<Vec<_>>();
                let joined = bodies.join("\n\n");
                let mut checks = Vec::new();
                if let Some(expect) = expect {
                    for needle in expect.body_includes.iter().flatten() {
                        let pass = contains_ci(&joined, needle);
                        checks.push(PropertyCheck {
                            name: format!("body includes {needle:?}"),
                            pass,
                            detail: String::new(),
                        });
                    }
                    for needle in expect.body_excludes.iter().flatten() {
                        let pass = !contains_ci(&joined, needle);
                        checks.push(PropertyCheck {
                            name: format!("body excludes {needle:?}"),
                            pass,
                            detail: String::new(),
                        });
                    }
                    if let Some(max) = expect.total_sentences_max {
                        let sentences: i32 = bodies
                            .iter()
                            .map(|body| {
                                crate::plugins::support::guards::count_sentences(body) as i32
                            })
                            .sum();
                        checks.push(PropertyCheck {
                            name: format!("total sentences ≤ {max}"),
                            pass: sentences <= max,
                            detail: format!("{sentences} sentences"),
                        });
                    }
                }
                CaseVerdict {
                    parsed: true,
                    abs_err: None,
                    checks,
                    display: joined,
                }
            }
            Err(_) => CaseVerdict {
                parsed: false,
                abs_err: None,
                checks: vec![],
                display: "unparseable".into(),
            },
        }
    }
    fn assemble(&self, stored_parts: &serde_json::Value) -> Result<Prepared> {
        let parts: crate::plugins::journalist::prompt::Parts =
            serde_json::from_value(stored_parts.clone())
                .map_err(|e| anyhow::anyhow!("narratives parts: {e}"))?;
        let subject = parts.subject.clone();
        let reports = parts.reports.clone();
        let memories = parts.memory.clone();
        // Rebuild the assignment the parts describe, then take BOTH the package
        // and the options from the plugin's own functions. The system prompt
        // depends on whether any report carries history and the schema is keyed
        // by report count, so neither is knowable from the parts alone.
        let assignment = crate::plugins::journalist::prompt::Assignment {
            subject,
            selected: reports,
            memories,
            memory_receipt: None,
            dispositions: Vec::new(),
            deferred_ids: Vec::new(),
            input_hash: String::new(),
        };
        Ok(Prepared {
            parts: Some(stored_parts.clone()),
            should_call: !assignment.selected.is_empty(),
            user_prompt: crate::plugins::journalist::prompt::prompt(&assignment),
            options: crate::plugins::journalist::prompt::generation_options(&assignment, 0),
        })
    }
    fn stores_parts(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// VibeTask — the same source package and score-free parser as production.
// ---------------------------------------------------------------------------
pub struct VibeTask;
#[async_trait]
impl LensTask for VibeTask {
    fn name(&self) -> &'static str {
        "vibe"
    }
    fn role(&self) -> RouteKey {
        crate::plugins::influencer::manifest::ROUTE
    }
    fn prompt_version(&self) -> &'static str {
        crate::plugins::influencer::prompt::VIBE_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> Result<GenerateOptions> {
        Ok(crate::plugins::influencer::prompt::generation_options(
            temperature,
            0,
            VIBE_NUM_PREDICT,
        ))
    }
    async fn build_request(
        &self,
        pool: &sqlx::PgPool,
        _models: &Models,
        e: &EntitySpec,
    ) -> Result<Option<Prepared>> {
        let subject = crate::plugins::meta::EntityMeta {
            name: lookup_entity_name(pool, &e.entity_type, e.entity_id, &e.sport).await?,
            entity_type: e.entity_type.clone(),
            entity_id: e.entity_id,
            sport: e.sport.to_uppercase(),
        };
        let sources = crate::plugins::harvester::delivery::load_for_character(
            pool,
            crate::plugins::influencer::manifest::MANIFEST.id.as_str(),
            &subject.entity_type,
            subject.entity_id,
            &subject.sport,
        )
        .await?;
        let Some(source) = sources.last() else {
            return Ok(None);
        };
        let (assignment, _) = crate::plugins::influencer::prompt::prepare_assignment(
            pool,
            subject,
            source,
            crate::plugins::influencer::now(),
        )
        .await?;
        assignment
            .map(|a| {
                self.assemble(&serde_json::to_value(
                    crate::plugins::influencer::prompt::Parts {
                        subject: a.subject,
                        source: a.source,
                        history: a.history,
                    },
                )?)
            })
            .transpose()
    }
    fn evaluate(&self, raw: &str, _label: Option<f64>, _expect: Option<&Expect>) -> CaseVerdict {
        match VibeParser.parse(raw) {
            Ok(reply) => CaseVerdict {
                parsed: true,
                abs_err: None,
                checks: vec![],
                display: reply
                    .and_then(|reply| reply.body)
                    .unwrap_or_else(|| "empty reading".into()),
            },
            Err(_) => CaseVerdict {
                parsed: false,
                abs_err: None,
                checks: vec![],
                display: "unparseable".into(),
            },
        }
    }
    fn assemble(&self, stored_parts: &serde_json::Value) -> Result<Prepared> {
        let parts: crate::plugins::influencer::prompt::Parts =
            serde_json::from_value(stored_parts.clone())
                .map_err(|e| anyhow::anyhow!("vibe parts: {e}"))?;
        Ok(Prepared {
            parts: Some(stored_parts.clone()),
            should_call: true,
            user_prompt: parts.assemble(),
            // The Influencer's contract does not vary with the world: one nullable
            // `body` slot, one manual, whatever the source or the history. So the
            // options are the plugin's own, not a per-fixture reconstruction.
            options: crate::plugins::influencer::prompt::generation_options(
                crate::plugins::influencer::prompt::VIBE_TEMPERATURE,
                0,
                VIBE_NUM_PREDICT,
            ),
        })
    }
    fn stores_parts(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// OracleTask — the crown: reads the five finished cards, then emits {reading}.
// (The panel SigilTask was retired in the crown fold, 2026-07-21.)
// ---------------------------------------------------------------------------

pub struct OracleTask;

#[async_trait]
impl LensTask for OracleTask {
    fn name(&self) -> &'static str {
        "oracle"
    }
    fn role(&self) -> RouteKey {
        crate::plugins::oracle::manifest::ROUTE
    }
    fn prompt_version(&self) -> &'static str {
        ORACLE_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> Result<GenerateOptions> {
        Ok(oracle_generation_options(temperature, 0))
    }
    async fn build_request(
        &self,
        pool: &sqlx::PgPool,
        _models: &Models,
        e: &EntitySpec,
    ) -> Result<Option<Prepared>> {
        let name = lookup_entity_name(pool, &e.entity_type, e.entity_id, &e.sport).await?;
        let sport = e.sport.to_uppercase();
        let (_season, cards) = load_pillars(pool, &e.entity_type, e.entity_id, &sport).await?;
        // With no evidence, the stage persists a marker without a model call.
        if cards.readiness() == crate::plugins::oracle::prompt::Readiness::Empty {
            return Ok(None);
        }
        Ok(Some(Prepared::captured(
            assemble_oracle_context(
                &OracleSubject {
                    entity_id: e.entity_id,
                    entity_type: e.entity_type.clone(),
                    entity_name: name,
                    sport: e.sport.clone(),
                },
                &cards,
            ),
            self.gen_options_for(0.0, e)?,
        )))
    }
    fn evaluate(&self, raw: &str, label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        let Some(reading) = serde_json::from_str::<serde_json::Value>(raw)
            .ok()
            .and_then(|v| v.get("reading").and_then(|x| x.as_str()).map(str::to_owned))
            .filter(|s| !s.trim().is_empty())
        else {
            return CaseVerdict {
                parsed: false,
                abs_err: None,
                checks: Vec::new(),
                display: "unparseable".into(),
            };
        };
        let sentences = count_sentences(&reading);
        let mut checks = Vec::new();

        // Contract-level invariants on every reading. Character, product, and system names are
        // backstage vocabulary; the served prose stays with the entity and its circumstances.
        checks.push(product_name_check(&reading));
        // (2) Plain prose: the or8 no-Markdown rule had NO assertion behind it, and the 8B/oMLX
        // baseline (2026-08-10) served `*there*` — italics in crown prose — through a green gate.
        // A rule measured by nothing is advice, not a contract.
        let md = ['*', '#', '`'].iter().find(|c| reading.contains(**c));
        checks.push(PropertyCheck {
            name: "reading_plain_text".into(),
            pass: md.is_none(),
            detail: md.map_or_else(String::new, |c| format!("found {c:?}")),
        });
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
            // contains_ci, not a raw `lower.contains`: these checks need the same typographic
            // fold the prose checks got, or an expect written with an ASCII apostrophe silently
            // never matches the model's U+2019.
            for s in x.reading_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("reading_includes:{s}"),
                    pass: contains_ci(&reading, s),
                    detail: String::new(),
                });
            }
            for s in x.reading_excludes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("reading_excludes:{s}"),
                    pass: !contains_ci(&reading, s),
                    detail: String::new(),
                });
            }
        }

        let _ = label; // The score is deterministic in production, outside the model reply.
        CaseVerdict {
            parsed: true,
            abs_err: None,
            checks,
            display: reading,
        }
    }
}

// ---------------------------------------------------------------------------
// TransferTask — source-backed Insider reading and linked move findings.
// ---------------------------------------------------------------------------

pub struct TransferTask;

#[async_trait]
impl LensTask for TransferTask {
    fn name(&self) -> &'static str {
        "transfer"
    }
    fn role(&self) -> RouteKey {
        crate::plugins::insider::manifest::ROUTE
    }
    fn prompt_version(&self) -> &'static str {
        INSIDER_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> Result<GenerateOptions> {
        let mut options = insider_options(0);
        options.temperature = Some(temperature);
        Ok(options)
    }
    async fn build_request(
        &self,
        pool: &sqlx::PgPool,
        _models: &Models,
        e: &EntitySpec,
    ) -> Result<Option<Prepared>> {
        let (entity_type, entity_id) = if let Some(player_id) = e.pair_player_id {
            ("player", player_id)
        } else {
            (e.entity_type.as_str(), e.entity_id)
        };
        Ok(preview_insider(pool, entity_type, entity_id, &e.sport)
            .await?
            .map(|prompt| Prepared::captured(prompt, insider_options(0))))
    }
    fn evaluate(&self, raw: &str, _label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        let Ok(reply) = serde_json::from_str::<Reply>(raw) else {
            return rejected("unparseable Insider reading and findings");
        };
        if reply.body.trim().is_empty() {
            return rejected("empty Insider reading");
        }
        let mut checks = vec![product_name_check(&reply.body)];
        if let Some(x) = expect {
            if let Some(want) = x.transfer_is_rumor {
                checks.push(PropertyCheck {
                    name: "has_source_linked_move".into(),
                    pass: reply.findings.iter().any(|f| {
                        f.status == crate::plugins::insider::Status::Reported
                            && !reply.findings.iter().any(|later| {
                                later.counterparty == f.counterparty
                                    && later.report_index < f.report_index
                            })
                    }) == want,
                    detail: format!("findings={}", reply.findings.len()),
                });
            }
            if let Some(stage) = x.transfer_stage.as_deref() {
                checks.push(PropertyCheck {
                    name: format!("stage:{stage}"),
                    pass: reply
                        .findings
                        .iter()
                        .any(|f| f.stage.as_deref() == Some(stage)),
                    detail: String::new(),
                });
            }
            for needle in x
                .subject_includes
                .iter()
                .flatten()
                .chain(x.summary_includes.iter().flatten())
            {
                checks.push(PropertyCheck {
                    name: format!("body_includes:{needle}"),
                    pass: contains_ci(&reply.body, needle),
                    detail: String::new(),
                });
            }
            for needle in x
                .subject_excludes
                .iter()
                .flatten()
                .chain(x.summary_excludes.iter().flatten())
            {
                checks.push(PropertyCheck {
                    name: format!("body_excludes:{needle}"),
                    pass: !contains_ci(&reply.body, needle),
                    detail: String::new(),
                });
            }
        }
        CaseVerdict {
            parsed: true,
            abs_err: None,
            checks,
            display: format!("{} finding(s) | {}", reply.findings.len(), reply.body),
        }
    }
}

// ---------------------------------------------------------------------------
// RatingTask — live production evidence plus mechanical/archived fixture checks.
// These checks do not grade factual correctness or editorial quality.
// ---------------------------------------------------------------------------

pub struct RatingTask;

#[async_trait]
impl LensTask for RatingTask {
    fn name(&self) -> &'static str {
        "rating"
    }
    fn role(&self) -> RouteKey {
        crate::plugins::scout::manifest::ROUTE
    }
    fn prompt_version(&self) -> &'static str {
        crate::plugins::scout::prompt::RATING_PROMPT_VERSION
    }
    fn gen_options(&self, _temperature: f64) -> Result<GenerateOptions> {
        // Scout acceptance needs the selected measurements, so callers must carry
        // parts even though this manual and body schema are constant.
        anyhow::bail!("rating acceptance requires prepared parts; use `assemble`")
    }
    async fn build_request(
        &self,
        pool: &sqlx::PgPool,
        models: &Models,
        e: &EntitySpec,
    ) -> Result<Option<Prepared>> {
        let sport = e.sport.to_uppercase();
        let name = lookup_entity_name(pool, &e.entity_type, e.entity_id, &sport).await?;
        let req = RatingReq {
            entity_type: e.entity_type.clone(),
            entity_id: e.entity_id,
            entity_name: name,
            sport,
            season: None,
            trigger_type: "periodic".to_string(),
        };
        // Live evaluation uses the same evidence assembly as production.
        match build_rating_request(pool, models.voice_num_ctx, &req, 0.0, true).await? {
            RatingBuild::NoStats { .. } => Ok(None),
            RatingBuild::Ready(r) => Ok(Some(self.assemble(&serde_json::to_value(&r.parts)?)?)),
        }
    }
    /// Rebuild the production request and retain its measurement context for parsing.
    fn assemble(&self, stored_parts: &serde_json::Value) -> Result<Prepared> {
        let parts: crate::plugins::scout::prompt::Parts =
            serde_json::from_value(stored_parts.clone())
                .map_err(|e| anyhow::anyhow!("rating parts: {e}"))?;
        Ok(Prepared {
            user_prompt: parts.render(),
            options: parts.generation_options(0, crate::plugins::scout::prompt::RATING_TEMPERATURE),
            parts: Some(stored_parts.clone()),
            should_call: parts.has_measured_profile(),
        })
    }

    fn stores_parts(&self) -> bool {
        true
    }
    fn evaluate_prepared(
        &self,
        request: &Prepared,
        raw: &str,
        label: Option<f64>,
        expect: Option<&Expect>,
    ) -> CaseVerdict {
        let validation = request
            .parts
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Scout evaluation requires parts"))
            .and_then(|p| {
                serde_json::from_value::<crate::plugins::scout::prompt::Parts>(p.clone())?
                    .parse(raw)
            });
        let mut verdict = self.evaluate(raw, label, expect);
        if let Err(error) = validation {
            verdict.parsed = false;
            verdict.checks.push(PropertyCheck {
                name: "production_acceptance".into(),
                pass: false,
                detail: error.to_string(),
            });
        }
        verdict
    }
    fn evaluate(&self, raw: &str, _label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        let map = match crate::plugins::support::form::decode_prose_map(raw, &["body".into()]) {
            Ok(map) => map,
            Err(error) => return rejected(&error.to_string()),
        };
        let Some(body) = map.get("body") else {
            return CaseVerdict {
                parsed: true,
                abs_err: None,
                checks: vec![PropertyCheck {
                    name: "abstention_requires_evidence_review".into(),
                    pass: expect.is_some_and(|e| e.abstain == Some(true)),
                    detail: "Valid pass; review whether withholding was warranted.".into(),
                }],
                display: "abstained — no card".into(),
            };
        };
        let reply = crate::plugins::scout::parser::RatingReply {
            body: body.to_string(),
        };
        let mut checks = Vec::new();
        if let Some(want) = expect.and_then(|e| e.abstain) {
            checks.push(PropertyCheck {
                name: "abstain".into(),
                pass: !want,
                detail: "Card supplied".into(),
            });
        }
        let word_count = reply.body.split_whitespace().count() as i32;

        // Contract-level invariant, asserted whether or not this case carries an `expect` (the
        // momentum no_banned_phrases shape): product names are banned from every brief, not from
        // the fixtures that happened to trip one. Case-sensitive — see PRODUCT_NAME_BANS.
        checks.push(product_name_check(&reply.body));
        // The brief's decoration bans (` · ` bullets, `**`) — folded 08-19 from per-fixture
        // `prose_excludes` entries; same list `RatingParser` rejects on in production.
        let banned = crate::plugins::support::guards::first_banned_phrase(
            &reply.body,
            crate::plugins::scout::parser::RATING_BODY_BANS,
        );
        checks.push(PropertyCheck {
            name: "no_banned_phrases".into(),
            pass: banned.is_none(),
            detail: banned.map_or_else(String::new, |p| format!("found {p:?}")),
        });

        if let Some(x) = expect {
            // Identity specificity is asserted on the brief's own prose (the divined label these
            // once matched against retired at s19; the decision card is unchanged).
            for s in x.skill_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("prose_names_skill:{s}"),
                    pass: contains_ci(&reply.body, s),
                    detail: String::new(),
                });
            }
            for s in x.skill_excludes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("prose_avoids_skill:{s}"),
                    pass: !contains_ci(&reply.body, s),
                    detail: String::new(),
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
            // s17 gate growth: a crude whole-body sentence ceiling (the shared n18 counter) —
            // a padding backstop over the Summary's 8-sentence allowance, not a style meter.
            if let Some(max) = x.total_sentences_max {
                let total = sentence_runs(&reply.body);
                checks.push(PropertyCheck {
                    name: "total_sentences_le".into(),
                    pass: total <= max,
                    detail: format!("sentences={total} ≤ {max}"),
                });
            }
        }

        CaseVerdict {
            parsed: true,
            abs_err: None,
            checks,
            display: reply.body.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// MomentumTask — fixture-first stats/analytical trajectory reasoning.
// ---------------------------------------------------------------------------

pub struct MomentumTask;

// Momentum preparation lives in `crate::plugins::analyst::prompt`, reading finished Scout
// and Influencer products plus the dated study. Eval reuses it and the local parser.
// It USED to carry its own fork ("momentum-eval-v3",
// a duplicate system prompt, its own parser): a relic from momentum's fixture-first era that
// silently diverged from production — the eval was measuring a prompt and a parser production
// no longer ran. Unified 2026-07-12 (lens quality plan Phase 1).

#[async_trait]
impl LensTask for MomentumTask {
    fn name(&self) -> &'static str {
        "momentum"
    }
    fn role(&self) -> RouteKey {
        crate::plugins::analyst::manifest::ROUTE
    }
    fn prompt_version(&self) -> &'static str {
        crate::plugins::analyst::prompt::MOMENTUM_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> Result<GenerateOptions> {
        let mut options = crate::plugins::analyst::prompt::generation_options(0);
        options.temperature = Some(temperature);
        Ok(options)
    }
    async fn build_request(
        &self,
        pool: &sqlx::PgPool,
        _models: &Models,
        e: &EntitySpec,
    ) -> Result<Option<Prepared>> {
        let name = lookup_entity_name(pool, &e.entity_type, e.entity_id, &e.sport).await?;
        let sport = e.sport.to_uppercase();
        let context = load_momentum_context(pool, &e.entity_type, e.entity_id, &sport).await?;
        if context.empty() {
            return Ok(None);
        }
        let subject = crate::plugins::meta::EntityMeta {
            name,
            entity_type: e.entity_type.clone(),
            entity_id: e.entity_id,
            sport: e.sport.clone(),
        };
        Ok(Some(Prepared::captured(
            crate::plugins::analyst::prompt::assemble(
                &subject,
                context.rating.as_ref(),
                context.vibe.as_ref(),
                &context.snapshot,
            ),
            self.gen_options_for(0.0, e)?,
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

        // Contract-level invariants, asserted whether or not this case carries an `expect`: the
        // banned phrasings are banned for every READ, not for the fixtures that happened to trip
        // one. See MOMENTUM_BANNED_PHRASES for why this is one check and not one per phrase.
        let banned = MOMENTUM_BANNED_PHRASES
            .iter()
            .find(|p| contains_ci(&reply.blurb, p));
        checks.push(PropertyCheck {
            name: "no_banned_phrases".into(),
            pass: banned.is_none(),
            detail: banned.map_or_else(String::new, |p| format!("found {p:?}")),
        });
        // s15 (Scott, 2026-08-10): the READ speaks the sport's words — "the form", "the emotion
        // around the club" — never the desk's product names. Case-sensitive; see PRODUCT_NAME_BANS.
        // At the s14 baseline this check is EXPECTED red on most fixtures: s14's rule 1 mandated
        // the product names, and this invariant is the measured record of that contract inverting.
        checks.push(product_name_check(&reply.blurb));

        if let Some(x) = expect {
            for s in x.prose_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("prose_includes:{s}"),
                    pass: contains_ci(&reply.blurb, s),
                    detail: String::new(),
                });
            }
            // ANY-of groups (s15): "name the signal" in the sport's words, which legitimately
            // vary. Each entry is one pipe-delimited group and one check.
            for group in x.prose_includes_any.iter().flatten() {
                let hit: Vec<&str> = group
                    .split('|')
                    .filter(|s| !s.is_empty() && contains_ci(&reply.blurb, s))
                    .collect();
                checks.push(PropertyCheck {
                    name: format!("prose_includes_any:[{group}]"),
                    pass: !hit.is_empty(),
                    detail: if hit.is_empty() {
                        "no listed synonym voiced".into()
                    } else {
                        format!("voiced {hit:?}")
                    },
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
            if let Some(max) = x.total_sentences_max {
                let total = sentence_runs(&reply.blurb);
                checks.push(PropertyCheck {
                    name: "total_sentences_le".into(),
                    pass: total <= max,
                    detail: format!("sentences={total} ≤ {max}"),
                });
            }
        }

        CaseVerdict {
            parsed: true,
            abs_err: None,
            checks,
            display: reply.blurb.clone(),
        }
    }
}

/// Case-insensitive substring match, with typographic punctuation AND Latin diacritics folded
/// to ASCII first.
///
/// The folding is not cosmetic — it is what makes a banned-phrase check real. Fixture expects are
/// hand-written with ASCII quotes (`isn't a surge`), while chat-tuned models emit the typographic
/// forms (`isn’t a surge`, U+2019). Without folding, such an exclusion can NEVER fail: it silently
/// passes on output that contains the banned phrase verbatim. That is the toothless-fixture hazard,
/// and it hid a live momentum regression through the whole of s10 — the phrase ban added in s10 was
/// reported as "10 → 0 occurrences" by a grep that could not match the model's own apostrophe.
///
/// The diacritic fold closes the mirror-image hazard on the INCLUDES side: a fixture asserting
/// `Sørensen` false-failed both the 8B and the 14B when the model wrote the honest ASCII form
/// "Sorensen" (D-T55 — the transfer gate's one harness artifact). Names are folded on both sides,
/// so `Sørensen` matches `Sorensen` and vice versa.
///
/// Only quotes and letter diacritics are folded. Dashes are deliberately left alone: an em dash is
/// a real stylistic signal some checks may legitimately want to assert on, and folding it to `-`
/// would make those checks mean something different.
// The matcher and the global ban vocabularies moved to `crate::guards` (2026-08-19, the
// eval→guard migration): production parsers and the gate now read the SAME lists — see
// `guards.rs` for the "one list, one home" ruling and the doc comments that moved with them.
use crate::plugins::support::guards::contains_ci;
pub use crate::plugins::support::guards::{MOMENTUM_BANNED_PHRASES, PRODUCT_NAME_BANS};

// (sentence_runs folded into `guards::count_sentences` 08-19 — one counter for every prose
// lens; the crude version miscounted decimals as sentence stops.)
fn sentence_runs(text: &str) -> i32 {
    crate::plugins::support::guards::count_sentences(text) as i32
}

/// One shared invariant check over a served-prose field: the first product name found, as a
/// `PropertyCheck` every wired seat pushes unconditionally. For rating the check runs on the
/// parsed body only. The list lives in [`crate::plugins::support::guards::PRODUCT_NAME_BANS`];
/// production enforces the same vocabulary.
fn product_name_check(prose: &str) -> PropertyCheck {
    let named = crate::plugins::support::guards::first_product_name(prose);
    PropertyCheck {
        name: "no_product_names".into(),
        pass: named.is_none(),
        detail: named.map_or_else(String::new, |p| format!("found {p:?}")),
    }
}

// (fold_for_match moved to `crate::guards` with the ban vocabularies — imported above.)

// ---------------------------------------------------------------------------
// Graph — the typed-extraction lens (junction rollout step 5). Fixture-gated BEFORE the
// queue stage wires in: the fixtures pin the g2 probe's measured residuals (the
// object-attachment slip, person-discovery misses, over-extraction). Live mode takes
// `article:<id>:<SPORT>` specs and builds the exact production prompt via the shared
// `load_graph_article_context` loader.
// ---------------------------------------------------------------------------

pub struct GraphTask;

#[async_trait]
impl LensTask for GraphTask {
    fn name(&self) -> &'static str {
        "graph"
    }
    fn role(&self) -> RouteKey {
        crate::plugins::graph::manifest::ROUTE
    }
    fn prompt_version(&self) -> &'static str {
        GRAPH_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> Result<GenerateOptions> {
        let mut o = graph_opts();
        o.temperature = Some(temperature);
        Ok(o)
    }
    async fn build_request(
        &self,
        pool: &sqlx::PgPool,
        _models: &Models,
        e: &EntitySpec,
    ) -> Result<Option<Prepared>> {
        if e.entity_type != "article" {
            anyhow::bail!(
                "graph evals are article-keyed: use article:<id>:<SPORT> (got {})",
                e.entity_type
            );
        }
        let sport = e.sport.to_uppercase();
        let Some((article, candidates)) =
            load_graph_article_context(pool, i64::from(e.entity_id), &sport).await?
        else {
            return Ok(None);
        };
        Ok(Some(Prepared::captured(
            build_graph_prompt(
                &article.source,
                &article.published,
                &article.title,
                &article.description,
                &candidates,
            ),
            self.gen_options_for(0.0, e)?,
        )))
    }
    fn evaluate(&self, raw: &str, _label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        // Reconstruct the fixture's candidate list from `graph_candidate_types` (entity
        // ids = the 1-based prompt numbers) so the REAL production parser runs and the
        // triple checks read directly in prompt-number terms.
        let types: Vec<String> = expect
            .and_then(|x| x.graph_candidate_types.clone())
            .unwrap_or_default();
        let candidates: Vec<GraphCandidate> = types
            .iter()
            .enumerate()
            .map(|(i, t)| GraphCandidate {
                entity_type: t.clone(),
                entity_id: (i + 1) as i32,
                descriptor: format!("candidate {}", i + 1),
            })
            .collect();
        let parsed = GraphParser {
            candidates: &candidates,
        }
        .parse(raw)
        .ok()
        .flatten();
        let Some(g) = parsed else {
            return CaseVerdict {
                parsed: false,
                abs_err: None,
                checks: Vec::new(),
                display: "unparseable (fail-closed)".into(),
            };
        };

        // "subject:predicate:object" triple matcher — numbers are the prompt's 1-based
        // candidate numbers (== the reconstructed entity ids); object "-" = unary;
        // predicate "*" = any.
        let triples: Vec<(i32, String, Option<i32>)> = g
            .relations
            .iter()
            .map(|r| (r.subject_id, r.predicate.clone(), r.object_id))
            .collect();
        let matches = |spec: &str, (s, p, o): &(i32, String, Option<i32>)| -> bool {
            let parts: Vec<&str> = spec.split(':').collect();
            if parts.len() != 3 {
                return false;
            }
            let Ok(want_s) = parts[0].parse::<i32>() else {
                return false;
            };
            let pred_ok = parts[1] == "*" || parts[1] == p;
            let obj_ok = if parts[2] == "-" {
                o.is_none()
            } else {
                parts[2].parse::<i32>().ok() == *o
            };
            want_s == *s && pred_ok && obj_ok
        };
        let persons_detail = || {
            format!(
                "persons={:?}",
                g.persons
                    .iter()
                    .map(|p| format!("{}[{}]", p.name, p.kind))
                    .collect::<Vec<_>>()
            )
        };

        let mut checks = Vec::new();
        if let Some(x) = expect {
            if let Some(incl) = &x.relations_include {
                for spec in incl {
                    checks.push(PropertyCheck {
                        name: format!("relation_present[{spec}]"),
                        pass: triples.iter().any(|t| matches(spec, t)),
                        detail: format!("relations={triples:?}"),
                    });
                }
            }
            if let Some(excl) = &x.relations_exclude {
                for spec in excl {
                    checks.push(PropertyCheck {
                        name: format!("relation_absent[{spec}]"),
                        pass: !triples.iter().any(|t| matches(spec, t)),
                        detail: format!("relations={triples:?}"),
                    });
                }
            }
            if let Some(max) = x.relations_max {
                checks.push(PropertyCheck {
                    name: "relations_le".into(),
                    pass: (g.relations.len() as i32) <= max,
                    detail: format!("{} ≤ {max}", g.relations.len()),
                });
            }
            if let Some(incl) = &x.persons_include {
                for spec in incl {
                    let (name, kind) = match spec.split_once(':') {
                        Some((n, k)) => (n, Some(k)),
                        None => (spec.as_str(), None),
                    };
                    checks.push(PropertyCheck {
                        name: format!("person_present[{spec}]"),
                        pass: g.persons.iter().any(|p| {
                            p.name.eq_ignore_ascii_case(name) && kind.is_none_or(|k| p.kind == k)
                        }),
                        detail: persons_detail(),
                    });
                }
            }
            if let Some(excl) = &x.persons_exclude {
                for frag in excl {
                    checks.push(PropertyCheck {
                        name: format!("person_absent[{frag}]"),
                        pass: !g
                            .persons
                            .iter()
                            .any(|p| p.name.to_lowercase().contains(&frag.to_lowercase())),
                        detail: persons_detail(),
                    });
                }
            }
        }
        CaseVerdict {
            parsed: true,
            abs_err: None,
            checks,
            display: format!(
                "{} relation(s), {} person(s)",
                g.relations.len(),
                g.persons.len()
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// investigator — the prose-triage contract (`ip1`), fixture-driven (D-T46)
// ---------------------------------------------------------------------------

pub struct InvestigatorTask;

#[async_trait]
impl LensTask for InvestigatorTask {
    fn name(&self) -> &'static str {
        "investigator"
    }
    fn role(&self) -> RouteKey {
        crate::plugins::investigator::manifest::ROUTE
    }
    fn prompt_version(&self) -> &'static str {
        INVESTIGATOR_PROSE_CONTRACT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> Result<GenerateOptions> {
        let mut o = prose_opts();
        o.temperature = Some(temperature);
        Ok(o)
    }

    /// Fixture-driven on purpose: the production prompt is built from a LIVE Wikipedia
    /// search + summary fetch for a candidate row, which is exactly what a frozen fixture
    /// exists to pin down. Capture new fixtures from `acquisition_runs.query_plan` (the
    /// prose arm records every page it read) rather than re-fetching a moving encyclopedia.
    async fn build_request(
        &self,
        _pool: &sqlx::PgPool,
        _models: &Models,
        _e: &EntitySpec,
    ) -> Result<Option<Prepared>> {
        anyhow::bail!(
            "investigator evals are fixture-driven (eval --task investigator --fixtures); \
             live prompts depend on a Wikipedia fetch — freeze pages into fixtures instead"
        )
    }
    fn evaluate(&self, raw: &str, _label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        let parsed = ProseReadParser.parse(raw).ok().flatten();
        let Some(read) = parsed else {
            return CaseVerdict {
                parsed: false,
                abs_err: None,
                checks: Vec::new(),
                display: "unparseable (fail-closed)".into(),
            };
        };
        let mut checks = Vec::new();
        if let Some(x) = expect {
            if let Some(want) = &x.subject_kind_is {
                checks.push(PropertyCheck {
                    name: format!("subject_kind[{want}]"),
                    pass: read.subject_kind.eq_ignore_ascii_case(want),
                    detail: format!("subject_kind={:?}", read.subject_kind),
                });
            }
            if let Some(incl) = &x.evidence_includes {
                for frag in incl {
                    checks.push(PropertyCheck {
                        name: format!("evidence_has[{frag}]"),
                        pass: read
                            .sought_name_evidence
                            .to_lowercase()
                            .contains(&frag.to_lowercase()),
                        detail: format!("evidence={:?}", read.sought_name_evidence),
                    });
                }
            }
            if x.evidence_empty == Some(true) {
                checks.push(PropertyCheck {
                    name: "evidence_empty".into(),
                    pass: read.sought_name_evidence.trim().is_empty(),
                    detail: format!("evidence={:?}", read.sought_name_evidence),
                });
            }
            if let Some(incl) = &x.occupation_includes {
                for frag in incl {
                    checks.push(PropertyCheck {
                        name: format!("occupation_has[{frag}]"),
                        pass: read
                            .occupation_phrase
                            .to_lowercase()
                            .contains(&frag.to_lowercase()),
                        detail: format!("occupation={:?}", read.occupation_phrase),
                    });
                }
            }
            if let Some(incl) = &x.prose_teams_include {
                let teams = read.team_names.join(" | ");
                for frag in incl {
                    checks.push(PropertyCheck {
                        name: format!("team_named[{frag}]"),
                        pass: teams.to_lowercase().contains(&frag.to_lowercase()),
                        detail: format!("teams=[{teams}]"),
                    });
                }
            }
        }
        CaseVerdict {
            parsed: true,
            abs_err: None,
            checks,
            display: format!(
                "kind={:?} evidence={:?} occupation={:?} teams=[{}]",
                read.subject_kind,
                truncate(&read.sought_name_evidence, 60),
                truncate(&read.occupation_phrase, 60),
                read.team_names.join(", "),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_task_names_are_unique_and_resolvable() {
        let names = all_task_names();
        let mut seen = std::collections::HashSet::new();
        for n in names {
            assert!(seen.insert(*n), "duplicate task name {n}");
            assert!(resolve_task(n).is_some(), "{n} not resolvable");
            assert!(lens_parameters(n).is_some(), "{n} has no lens parameters");
        }
        assert!(resolve_task("nope").is_none());
    }

    #[test]
    fn lens_parameters_capture_the_locked_cast() {
        // The cast is an identity lock (wiki/Characters.md, 2026-07-21) — a rename here is a
        // product decision, not a refactor.
        let rating = lens_parameters("rating").unwrap();
        assert_eq!(rating.operator, "The Scout");
        assert!(rating.mandate.contains("playing characteristics"));

        assert_eq!(
            lens_parameters("narratives").unwrap().operator,
            "The Journalist"
        );
        assert_eq!(lens_parameters("transfer").unwrap().operator, "The Insider");
        assert_eq!(lens_parameters("vibe").unwrap().operator, "The Influencer");
        assert_eq!(lens_parameters("momentum").unwrap().operator, "The Analyst");

        assert_eq!(lens_parameters("oracle").unwrap().operator, "the Oracle");

        // The internal seats carry the cast too — and no longer have to be filed under a rail
        // that never described them.
        assert!(lens_parameters("editor").is_none());
        assert!(resolve_task("editor").is_none());
        assert!(!all_task_names().contains(&"editor"));
        assert_eq!(
            lens_parameters("investigator").unwrap().operator,
            "The Investigator"
        );
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

    // --- crown (Oracle) eval: reading -------------------------------------

    const CROWN_OK: &str = r#"{"reading": "The winger's arc holds under a turning sky; the wind toward Liverpool stirs but nothing has broken. A steady hand on a rising line."}"#;

    #[test]
    fn crown_eval_parses_reading_without_a_model_score() {
        let x = Expect {
            reading_min_sentences: Some(2),
            reading_includes: Some(vec!["Liverpool".into()]),
            ..Default::default()
        };
        let v = OracleTask.evaluate(CROWN_OK, Some(70.0), Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
        assert_eq!(v.abs_err, None);
    }

    #[test]
    fn crown_eval_unparseable_reply_is_not_parsed() {
        let v = OracleTask.evaluate("not json at all", None, None);
        assert!(!v.parsed);
    }

    #[test]
    fn crown_eval_reading_excludes_catches_pundit_register() {
        // The reading must leave the pundit's register at the door.
        let raw = r#"{"reading": "Keep an eye on this one going forward.", "score": 60}"#;
        let x = Expect {
            reading_excludes: Some(vec!["keep an eye".into()]),
            ..Default::default()
        };
        let v = OracleTask.evaluate(raw, None, Some(&x));
        assert!(
            !v.all_checks_pass(),
            "excludes should catch the parroted register"
        );
    }

    #[test]
    fn vibe_evaluation_uses_score_free_production_parser() {
        assert!(VibeTask
            .evaluate(r#"{"body":"Morgan said she felt hopeful."}"#, None, None)
            .all_checks_pass());
        assert!(VibeTask.evaluate(r#"{"body":null}"#, None, None).parsed);
        assert!(
            !VibeTask
                .evaluate(
                    r#"{"score":75,"headline":"Hope","body":"Hope."}"#,
                    None,
                    None
                )
                .parsed
        );
    }

    // --- one-response Insider reading and source-linked findings ------------------

    const TRUE_TRANSFER: &str = r#"{"body":"Lina Foss is the subject of Everton talks, according to TV2, but a deal is not final.","findings":[{"report_index":0,"counterparty":"Everton","status":"reported","stage":"advanced_talks","evidence_quote":"Everton are in advanced talks to sign Lina Foss"}]}"#;

    #[test]
    fn transfer_true_positive_has_one_source_linked_finding() {
        let x = Expect {
            transfer_is_rumor: Some(true),
            transfer_stage: Some("advanced_talks".into()),
            subject_includes: Some(vec!["Lina Foss".into()]),
            summary_includes: Some(vec!["Everton".into()]),
            ..Default::default()
        };
        let v = TransferTask.evaluate(TRUE_TRANSFER, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn transfer_options_declare_one_body_and_findings_reply() {
        let options = TransferTask.gen_options(0.0).unwrap();
        assert_eq!(
            options.format_schema.unwrap()["required"],
            serde_json::json!(["body", "findings"])
        );
    }

    #[test]
    fn transfer_empty_findings_clear_the_move_expectation() {
        let raw = r#"{"body":"Mika Salo is mentioned, but no move is reported.","findings":[]}"#;
        let x = Expect {
            transfer_is_rumor: Some(false),
            subject_includes: Some(vec!["Mika Salo".into()]),
            ..Default::default()
        };
        let v = TransferTask.evaluate(raw, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn transfer_denial_finding_is_not_a_live_rumor() {
        let raw = r#"{"body":"TV2 reports that Everton denied talks for Lina Foss.","findings":[{"report_index":0,"counterparty":"Everton","status":"denied","stage":null,"evidence_quote":"Everton denied talks for Lina Foss"}]}"#;
        let x = Expect {
            transfer_is_rumor: Some(false),
            subject_includes: Some(vec!["Lina Foss".into()]),
            ..Default::default()
        };
        let v = TransferTask.evaluate(raw, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn transfer_new_denial_supersedes_older_report_in_evaluation() {
        let raw = r#"{"body":"TV2 reports that Everton denied talks for Lina Foss after an earlier link.","findings":[{"report_index":1,"counterparty":"Everton","status":"reported","stage":"speculation","evidence_quote":"Everton considered Lina Foss"},{"report_index":0,"counterparty":"Everton","status":"denied","stage":null,"evidence_quote":"Everton denied talks for Lina Foss"}]}"#;
        let x = Expect {
            transfer_is_rumor: Some(false),
            ..Default::default()
        };
        assert!(TransferTask.evaluate(raw, None, Some(&x)).all_checks_pass());
    }

    #[test]
    fn transfer_invented_fee_is_caught_by_body_excludes() {
        let raw = r#"{"body":"Everton want Lina Foss in a £12m move.","findings":[]}"#;
        let x = Expect {
            summary_excludes: Some(vec!["£12m".into()]),
            ..Default::default()
        };
        assert!(!TransferTask.evaluate(raw, None, Some(&x)).all_checks_pass());
    }

    #[test]
    fn transfer_malformed_reply_is_unparseable() {
        assert!(
            !TransferTask
                .evaluate("looks like a rumor", None, None)
                .parsed
        );
    }

    // --- typographic folding in the property matcher -----------------------------

    /// The regression this exists to prevent: a banned-phrase exclusion written with an ASCII
    /// apostrophe must still fail on model output that uses U+2019. Before folding, this check
    /// passed on text containing the banned phrase verbatim — a check that cannot fail is worse
    /// than no check, because the run reports green.
    #[test]
    fn prose_excludes_matches_across_typographic_apostrophes() {
        // Real ministral-3:14b output from the momentum-s11 fixture gate (curly U+2019).
        let reply = r#"{"blurb":"The tape holds firm and the samples are thin. For now, this isn’t a surge—just a brief flash of what might come."}"#;
        let x = Expect {
            prose_excludes: Some(vec!["isn't a surge".into()]),
            ..Default::default()
        };
        let v = MomentumTask.evaluate(reply, None, Some(&x));
        assert!(v.parsed, "reply should parse: {:?}", v.checks);
        assert!(
            !v.all_checks_pass(),
            "ASCII-apostrophe exclusion must catch the U+2019 form; checks: {:?}",
            v.checks
        );
    }

    #[test]
    fn prose_includes_matches_across_typographic_apostrophes() {
        let reply = r#"{"blurb":"Harbor City’s press is tightening cleanly across the last six."}"#;
        let x = Expect {
            prose_includes: Some(vec!["Harbor City's press".into()]),
            ..Default::default()
        };
        let v = MomentumTask.evaluate(reply, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    // (fold_for_match / contains_ci tests moved to `guards::tests` with the functions.)

    // --- rating / stats-lens rubric ---------------------------------------------

    const RATING_REPLY: &str = "An elite rim protector who grades at the 94th percentile in blocks and anchors the paint without fouling. The profile is thinner as a creator, but the defensive identity is clear and valuable.";

    #[test]
    fn rating_rubric_scores_specificity_and_prose_richness() {
        let x = Expect {
            // s19: asserted on the brief's prose (the divined label is retired).
            skill_includes: Some(vec!["rim protector".into()]),
            skill_excludes: Some(vec!["No standout".into()]),
            prose_includes: Some(vec!["94th percentile".into(), "defensive identity".into()]),
            prose_excludes: Some(vec!["triple-double".into()]),
            prose_min_words: Some(20),
            prose_max_words: Some(60),
            ..Default::default()
        };
        let v = RatingTask.evaluate(
            &serde_json::json!({"body":RATING_REPLY}).to_string(),
            None,
            Some(&x),
        );
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn rating_product_name_ban_is_case_sensitive_and_body_scoped() {
        // Lowercase "peak" is honest English and must not trip the product-name ban.
        let clean = "Still at the peak of his powers: an elite rim protector at the 94th percentile in blocks who anchors the paint without fouling, and the defensive identity is clear.";
        let v = RatingTask.evaluate(&serde_json::json!({"body":clean}).to_string(), None, None);
        assert!(
            v.checks.iter().all(|c| c.pass),
            "clean body tripped: {:?}",
            v.checks
        );
        // An echoed product name in the body is exactly what the check exists to catch.
        let echo = "His PEAK skill is rim protection and the staff must scheme away from it, forcing the ball to the perimeter.";
        let v = RatingTask.evaluate(&serde_json::json!({"body":echo}).to_string(), None, None);
        let ban = v
            .checks
            .iter()
            .find(|c| c.name == "no_product_names")
            .expect("invariant check present");
        assert!(!ban.pass, "echoed PEAK not caught: {:?}", v.checks);
    }

    #[test]
    fn rating_rubric_catches_generic_read_and_thin_prose() {
        let x = Expect {
            // s19: prose-anchored — the include names a skill the thin body lacks, the
            // exclude names a phrase the thin body contains.
            skill_includes: Some(vec!["Rim protection".into()]),
            skill_excludes: Some(vec!["Average".into()]),
            prose_min_words: Some(20),
            ..Default::default()
        };
        let v = RatingTask.evaluate(
            r#"{"body":"No standout skill. Average profile."}"#,
            None,
            Some(&x),
        );
        assert!(v.parsed);
        // Every expect-driven check fails; the global invariants (no product names, no
        // decoration) rightly pass on this clean-if-thin body, so they are excluded.
        let expect_passed = v
            .checks
            .iter()
            .filter(|c| c.name != "no_product_names" && c.name != "no_banned_phrases" && c.pass)
            .count();
        assert_eq!(expect_passed, 0, "checks: {:?}", v.checks);
    }

    // --- momentum fixture-first trajectory rubric ---------------------------------

    #[test]
    fn momentum_parser_extracts_the_read() {
        let raw = r#"{"blurb":"Recent form is rising while the mood is steady, so the current direction is modestly positive."}"#;
        let parsed = parse_momentum_reply(raw).unwrap();
        assert!(parsed.blurb.contains("form is rising"));
    }

    #[test]
    fn momentum_rubric_scores_prose() {
        // s11: the signed-band assertions are gone — the score is no longer the model's to
        // get wrong. `momentum_conviction_from_score` is unit-tested in the junction instead.
        // s15: the compliant READ speaks the sport's words — product names now trip the
        // no_product_names invariant, and "name the signal" is an any-of over honest synonyms.
        let x = Expect {
            prose_includes_any: Some(vec!["mood|emotion|feeling".into()]),
            prose_excludes: Some(vec!["surging".into()]),
            ..Default::default()
        };
        let raw = r#"{"blurb":"The mood around the club is pulling the profile down despite steadier recent form."}"#;
        let v = MomentumTask.evaluate(raw, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn momentum_product_names_trip_the_invariant() {
        // The s14-era register itself: exactly what the s15 contract inverts.
        let raw = r#"{"blurb":"Vibe is pulling the profile down despite a steadier PEAK read."}"#;
        let v = MomentumTask.evaluate(raw, None, None);
        let ban = v
            .checks
            .iter()
            .find(|c| c.name == "no_product_names")
            .expect("invariant present");
        assert!(!ban.pass, "checks: {:?}", v.checks);
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
            "name": "crown-read",
            "task": "oracle",
            "prompt_version": "or3",
            "system": "SYS",
            "user_prompt": "Entity: X",
            "temperature": 0.0,
            "expect": { "reading_min_sentences": 2, "score_min": 60 }
        }"#;
        let fx: Fixture = serde_json::from_str(json).unwrap();
        assert_eq!(fx.name, "crown-read");
        assert_eq!(fx.expect.reading_min_sentences, Some(2));
        assert_eq!(fx.expect.score_min, Some(60));
        assert_eq!(fx.expect.score_max, None); // defaulted
                                               // A fixture may omit expect entirely.
        let bare = r#"{"name":"n","task":"oracle","prompt_version":"or3","system":"s","user_prompt":"u","temperature":0.0}"#;
        let fx2: Fixture = serde_json::from_str(bare).unwrap();
        assert_eq!(fx2.expect.reading_min_sentences, None);
    }

    #[test]
    fn current_quality_cases_use_current_contracts_and_explicit_review() {
        for &name in all_task_names() {
            let task = resolve_task(name).unwrap();
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures/quality")
                .join(name);
            let mut count = 0;
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let fx: Fixture =
                    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                assert_eq!(fx.task, name, "{}", path.display());
                assert_eq!(
                    fx.prompt_version,
                    task.prompt_version(),
                    "{} needs recapture",
                    path.display()
                );
                assert!(
                    fx.system.is_empty(),
                    "{} freezes a retired system",
                    path.display()
                );
                assert!(!fx.user_prompt.trim().is_empty());
                assert!(!fx.review.is_empty());
                assert!(fx
                    .review
                    .iter()
                    .all(|criterion| !criterion.trim().is_empty()));
                count += 1;
            }
            assert!(count > 0, "{name} has no quality cases");
        }
    }

    /// F6's gate. A task that declares `stores_parts` must have every fixture
    /// carry parts, and the plugin's current assembler must rebuild the stored
    /// `user_prompt` from them byte for byte.
    ///
    /// This is what makes a changed package a test failure. Before it, the four
    /// vibe fixtures were hand-edited when the package changed, and the
    /// narratives directory was empty — so the harness replayed a string that
    /// production had stopped building, and said nothing.
    #[test]
    fn a_parts_fixture_reassembles_to_its_stored_prompt() {
        let mut checked = 0;
        for &name in all_task_names() {
            let task = resolve_task(name).unwrap();
            if !task.stores_parts() {
                continue;
            }
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures/quality")
                .join(name);
            let mut files: Vec<_> = std::fs::read_dir(&dir)
                .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
                .collect();
            files.sort();
            assert!(
                !files.is_empty(),
                "{name} declares parts and has no fixtures"
            );
            for path in files {
                let fx: Fixture =
                    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                let parts = fx.parts.as_ref().unwrap_or_else(|| {
                    panic!(
                        "{} stores no parts; {} declares a parts assembler, so the stored \
                         prompt cannot be re-derived and a changed package would pass silently",
                        path.display(),
                        name
                    )
                });
                let rebuilt = task
                    .assemble(parts)
                    .unwrap_or_else(|e| panic!("{} did not assemble: {e:#}", path.display()));
                assert_eq!(
                    rebuilt.user_prompt,
                    fx.user_prompt,
                    "{}: the plugin's current assembler no longer produces this package. \
                     The parts are the fixture; recapture the prompt from them.",
                    path.display()
                );
                checked += 1;
            }
        }
        assert!(
            checked > 0,
            "no task declares a parts assembler; the gate covers nothing"
        );
    }

    /// A parts fixture's stored prompt is a check, not the input. If the harness
    /// can silently fall back to it, the gate above proves nothing.
    ///
    /// The property is that a parts fixture has exactly one path, and it runs
    /// the plugin's assembler. A task whose contract varies with the world must
    /// refuse a world-independent option set, because one would be a second
    /// path to the same fixtures. A task whose contract does not vary may offer
    /// one — that set is its real production contract, and the harness already
    /// used it to build the request.
    #[test]
    fn a_parts_fixture_has_exactly_one_path_and_it_runs_the_assembler() {
        let mut parts_tasks = 0;
        for &name in all_task_names() {
            let task = resolve_task(name).unwrap();
            if !task.stores_parts() {
                // Without an assembler the stored string is the only path, which
                // is the state F6 exists to end. These are Windows 4 through 10.
                assert!(
                    task.gen_options(0.0).is_ok(),
                    "{name} has no parts assembler, so its options must come from gen_options"
                );
                continue;
            }
            parts_tasks += 1;
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures/quality")
                .join(name);
            let fixture = std::fs::read_dir(&path)
                .unwrap()
                .filter_map(|e| e.ok().map(|e| e.path()))
                .find(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
                .expect("a parts task has fixtures");
            let fx: Fixture =
                serde_json::from_str(&std::fs::read_to_string(&fixture).unwrap()).unwrap();
            let assembled = task.assemble(fx.parts.as_ref().unwrap()).unwrap();
            assert_eq!(
                assembled.user_prompt,
                fx.user_prompt,
                "{}: the harness would send a different package than the fixture stores",
                fixture.display()
            );
        }
        assert!(parts_tasks > 0, "no task declares a parts assembler");
    }

    /// The Journalist's decode contract is a function of what the world holds:
    /// the schema is keyed by report count and the manual names history only
    /// when a report carries it. A fixture that pins one and not the other is a
    /// contract nothing checks.
    #[test]
    fn the_narratives_contract_follows_the_world_it_was_assembled_from() {
        let base = |reports: usize, history: bool| {
            let reports: Vec<CorpusItem> = (0..reports)
                .map(|index| CorpusItem {
                    id: index as i64 + 1,
                    title: String::new(),
                    context: format!("Cedar reported {index}."),
                    source: "Wire".into(),
                    published_at_epoch: Some(1_790_553_600),
                })
                .collect();
            // Index-aligned with `reports`, exactly as production attaches it.
            let slot = crate::plugins::journalist::memories::Selected {
                items: vec![crate::plugins::memories::HistoryItem {
                    group: None,
                    publisher: "Old Wire".into(),
                    published_at: "2026-09-20T00:00:00Z".into(),
                    reported_headline: "Cedar won earlier".into(),
                }],
                groups: vec![],
            };
            let memory = reports
                .iter()
                .enumerate()
                .map(|(index, _)| (history && index == 0).then(|| slot.clone()))
                .collect::<Vec<_>>();
            serde_json::json!({
                "subject": {"name":"Cedar Comets","entity_type":"team","entity_id":7,"sport":"NBA"},
                "reports": reports,
                "memory": memory,
            })
        };
        let one = NarrativesTask.assemble(&base(1, false)).unwrap();
        let two = NarrativesTask.assemble(&base(2, false)).unwrap();
        let warm = NarrativesTask.assemble(&base(1, true)).unwrap();

        // The schema is keyed by report count, so one option set cannot serve
        // both worlds.
        assert_ne!(
            one.options.format_schema, two.options.format_schema,
            "the response schema must follow the report count"
        );
        // The manual names history only when a report carries some.
        assert_ne!(
            one.options.system.as_deref(),
            warm.options.system.as_deref(),
            "the manual must follow whether history is attached"
        );
        // Memory is absent until an attachment exists, and is keyed to its report.
        assert!(!one.user_prompt.contains(r#""memories""#));
        assert!(warm
            .user_prompt
            .contains(r#""memories":[{"report_key":"report_1""#));
    }

    #[test]
    fn prepared_requests_carry_acceptance_context_and_no_call_dispositions() {
        let mut fx: Fixture = serde_json::from_str(include_str!(
            "../../fixtures/quality/rating/synthetic-strong.json"
        ))
        .unwrap();
        let request = RatingTask.prepare_fixture(&fx).unwrap();
        assert!(request.should_call);
        assert!(
            RatingTask
                .evaluate_prepared(
                    &request,
                    r#"{"body":"The recorded profile describes measured contributions."}"#,
                    None,
                    None
                )
                .parsed
        );
        assert!(
            !RatingTask
                .evaluate_prepared(
                    &request,
                    r#"{"body":"He averages 99999 blocks."}"#,
                    None,
                    None
                )
                .parsed
        );
        assert!(
            !RatingTask
                .evaluate_prepared(&request, "broken", None, None)
                .parsed
        );
        let pass = RatingTask.evaluate_prepared(&request, r#"{"body":null}"#, None, None);
        assert!(pass.parsed && !pass.all_checks_pass());
        fx.user_prompt.push(' ');
        assert!(RatingTask.prepare_fixture(&fx).is_err());
        fx.parts = None;
        assert!(RatingTask.prepare_fixture(&fx).is_err());
        let mut parts = request.parts.unwrap();
        parts["profile"]["values"] = serde_json::json!([]);
        parts["profile"]["composite"] = serde_json::Value::Null;
        assert!(!RatingTask.assemble(&parts).unwrap().should_call);
        parts["profile"]["composite"] = serde_json::json!(52.0);
        assert!(RatingTask.assemble(&parts).unwrap().should_call);

        let mut fx: Fixture = serde_json::from_str(include_str!(
            "../../fixtures/quality/narratives/distinct-developments.json"
        ))
        .unwrap();
        let request = NarrativesTask.prepare_fixture(&fx).unwrap();
        assert!(
            request.parts.as_ref().unwrap()["reports"]
                .as_array()
                .unwrap()
                .len()
                > 1
        );
        assert!(
            !NarrativesTask
                .evaluate_prepared(&request, r#"{"report_1":"A report."}"#, None, None)
                .parsed
        );
        let mut raw = serde_json::Map::new();
        for key in request.options.format_schema.as_ref().unwrap()["required"]
            .as_array()
            .unwrap()
        {
            raw.insert(key.as_str().unwrap().into(), serde_json::Value::Null);
        }
        assert!(
            NarrativesTask
                .evaluate_prepared(
                    &request,
                    &serde_json::Value::Object(raw).to_string(),
                    None,
                    None
                )
                .parsed
        );
        let parts = fx.parts.as_mut().unwrap();
        parts["reports"] = serde_json::json!([]);
        parts["memory"] = serde_json::json!([]);
        assert!(!NarrativesTask.assemble(parts).unwrap().should_call);
    }

    /// Preserve transfer evidence, including an explicit denial.
    #[test]
    fn transfer_fixtures_on_disk_parse_and_current_carry_a_steam_fizzle_axis() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/quality/transfer");
        let mut current_seen = 0;
        for entry in std::fs::read_dir(&dir).expect("read fixtures/transfer") {
            let p = entry.unwrap().path();
            if p.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            let fx: Fixture = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("fixture {} failed to parse: {e}", p.display()));
            assert_eq!(fx.task, "transfer", "{} has wrong task", p.display());
            assert!(
                fx.expect.transfer_stage.is_some()
                    || fx.expect.transfer_is_rumor == Some(false)
                    || fx.expect.confidence_min.is_some()
                    || fx.expect.confidence_max.is_some(),
                "current fixture {} carries no steam/fizzle axis (field-name drop?)",
                p.display()
            );
            current_seen += 1;
        }
        assert!(
            current_seen >= 2,
            "expected archived steam/fizzle fixtures, saw {current_seen}"
        );
    }
}

#[cfg(test)]
mod scout_abstention_tests {
    use super::*;
    #[test]
    fn a_valid_pass_is_visible_without_being_a_vacuous_quality_pass() {
        let result = RatingTask.evaluate(r#"{"body":null}"#, None, None);
        assert!(result.parsed);
        assert!(result.display.contains("abstained"));
        assert!(result.checks.iter().any(|check| !check.pass));
    }
}
