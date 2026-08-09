//! Per-lens eval task registry (Multi-Lens Cognition Panel).
//!
//! `bin/eval` used to be hardwired to the vibe task (`Role::EmotionalNews`) and the live corpus.
//! A `LensTask` is the seam that generalizes it: each task knows its `Role`, its `GenerateOptions`
//! (system + num_predict + json_mode), how to build the exact PRODUCTION prompt for an entity, and
//! how to `evaluate` a raw reply into a `CaseVerdict`. It COMPOSES the capability library — the
//! stage loaders + prompt builders + parsers already in the lib — rather than reinventing them, so
//! the eval measures the real prompt with only the backend swapped.
//!
//! Every CHARACTER task owns its role (identity splits: 2026-07-11 momentum, 07-12 narratives +
//! sigil, 07-22 transfers + vibe), so no route change silently flips a sibling's voice:
//! `rating`/PEAK on `Role::StatsLogic` (The Scout), `momentum` on `Role::MomentumLogic`
//! (The Analyst), `narratives` on `Role::NarrativeLogic` (The Journalist), `transfer` on
//! `Role::TransferLogic` (The Insider), `vibe` on `Role::VibeLogic` (The Influencer), and
//! `sigil` on `Role::OracleLogic` (the Oracle). `graph` stays on `Role::EmotionalNews` —
//! the utility role (no character voice). Un-configured, every role resolves to the same
//! default model; eval candidates configure `COGNITION_ROUTE_<ROLE>_CANDIDATE`.
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
use crate::util::truncate;
use crate::junctions::editor::{
    build_editor_prompt_for_eval, derive as editor_derive, editor_opts, EditorRead,
    EditorReadParser, EDITOR_CONTRACT_VERSION,
};
use crate::junctions::graph::{
    build_graph_prompt, graph_opts, load_graph_article_context, GraphCandidate, GraphParser,
    GRAPH_PROMPT_VERSION,
};
use crate::harness::{Harness, Parser};
use crate::junctions::analyst::{
    build_momentum_prompt, parse_momentum_reply, MOMENTUM_NUM_PREDICT, MOMENTUM_PROMPT_VERSION,
    MOMENTUM_SYSTEM_PROMPT,
};
use crate::junctions::journalist::{
    build_narratives_prompt, load_vetted_corpus, NarrativesParser, NarrativesReq,
    NARRATIVES_NUM_CTX, NARRATIVES_NUM_PREDICT, NARRATIVES_PROMPT_VERSION,
    NARRATIVES_SYSTEM_PROMPT,
};
use crate::ollama::GenerateOptions;
use crate::junctions::scout::{
    build_rating_request, RatingBuild, RatingParser, RatingReq, RATING_NUM_PREDICT,
    RATING_PROMPT_VERSION, RATING_SYSTEM_PROMPT,
};
use crate::route::Role;
use crate::junctions::oracle::{
    build_crown_prompt, build_pillar_divergence, compute_omen, count_sentences, load_pillars,
    oracle_format_schema, parse_crown_reply, pillar_convergence, ORACLE_NUM_PREDICT,
    ORACLE_PROMPT_VERSION, ORACLE_SYSTEM_PROMPT,
};
use crate::junctions::insider::{
    build_pair_request, load_candidates, transfer_system_prompt, PairBuild, TransferParser,
    TRANSFER_DEFAULT_MIN_ARTICLES, TRANSFER_NUM_PREDICT, TRANSFER_PROMPT_VERSION,
};
use crate::junctions::influencer::{
    build_sentiment_prompt, load_latest_narratives, parse_vibe_reply, VIBE_NUM_PREDICT,
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

/// lens_parameters is the code home for the current six-lens taxonomy. `operator` carries the
/// character identity (the cast locked in wiki/Characters.md, 2026-07-21); the junction's system
/// prompt is that character's voice, so a voice change is a prompt change, never a rename here.
pub fn lens_parameters(name: &str) -> Option<LensParameters> {
    match name {
        "narratives" => Some(LensParameters {
            rail: Rail::EmotionalNews,
            operator: "The Journalist",
            mandate: "Compile the stories swirling around the entity into grounded storylines.",
            credibility_guard: "Group what sources actually say; do not inflate vague hype or off-entity noise.",
        }),
        "transfer" => Some(LensParameters {
            rail: Rail::EmotionalNews,
            operator: "The Insider",
            mandate: "Get movement predictions out quickly while preserving long-term credibility.",
            credibility_guard: "Fail closed on name-drops, stale links, weak sourcing, and misleading heat.",
        }),
        "vibe" => Some(LensParameters {
            rail: Rail::EmotionalNews,
            operator: "The Influencer",
            mandate: "Farm the engagement: find the emotion running through the entity's narratives and ride it into the felt read of the moment.",
            credibility_guard: "Separate interactable mood from durable truth; the emotion must trace to the corpus — do not invent a narrative hook.",
        }),
        "rating" => Some(LensParameters {
            rail: Rail::StatsAnalytical,
            operator: "The Scout",
            mandate: "Prepare for the entity by naming the greatest strength to stop and the greatest weakness to exploit.",
            credibility_guard: "Use supplied tiers and datapoints only; never turn average marks into strengths.",
        }),
        "momentum" => Some(LensParameters {
            rail: Rail::StatsAnalytical,
            operator: "The Analyst",
            mandate: "Read the directional force of form (PEAK/rating trajectory) and feeling (Vibe/news), then narrate the decided direction with conviction.",
            credibility_guard: "Stay detached and results-only; do not chase sentiment hype or cling to stale PEAK strength.",
        }),
        "oracle" => Some(LensParameters {
            rail: Rail::Synthesis,
            operator: "the Oracle",
            mandate: "Read the five pillar cards aloud, deliver the entity's reading in the house voice, then render the Sigil verdict grounded in the entity's own prior reads.",
            credibility_guard: "The mysticism lives in the telling, never the facts — every claim traces to a card; nothing invented; the score moves deliberately from prior verdicts.",
        }),
        "editor" => Some(LensParameters {
            rail: Rail::EmotionalNews,
            operator: "The Editor",
            mandate: "Read every arrival's full text and describe it richly for the newsroom — shape, names with descriptors, roles, result line, register, facts — so code can derive everything downstream.",
            credibility_guard: "Describe, never judge: no relevance verdicts, no invented names or results — only what the text contains, with the descriptor copied from the text.",
        }),
        "graph" => Some(LensParameters {
            rail: Rail::EmotionalNews,
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
    // vibe fixture score band (per-case boolean stand-in for the aggregate MAE axis).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_max: Option<i32>,
    /// v13 Influencer card contract: the HOOK line (card title) must materialize.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_nonempty: Option<bool>,
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
    /// Voice-direction check (OR-semantics, CASE-INSENSITIVE): at least ONE returned body contains at
    /// least ONE of these strings. Unlike `body_includes` (every string must appear), this asserts a
    /// storyline *voiced a direction at all* from a set of acceptable synonyms — the n9 fixtures use it
    /// for "voiced this as CONTINUING / HEATING / COOLING" where the exact wording is free (the voice is
    /// a draft, dialed in a later voice-tuning session). A voice-target axis, re-annotated when voice lands.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_includes_any: Option<Vec<String>>,
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
    // editor / relevance-gate rubric.
    /// THE relevance verdict — `ArticleEvidence::relevant`, the single field the whole news rail
    /// gates on. `false` pins an article the Editor must REJECT.
    ///
    /// This axis exists because the Editor ran as sole relevance judge with no eval coverage at
    /// all, and a 2026-07-26 measurement found gemma3:4b rejecting 0.9% against mistral's
    /// rank-matched 27.4% — passing 26 boxscore stubs, 18 broadcast listings and 46 odds pages
    /// with ZERO rejections. A gate nobody scores is not a gate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub article_relevant: Option<bool>,
    /// Grounding on an ACCEPTED article: each string must appear in the parsed `key_facts`
    /// (joined). Guards the other direction — a fixture set that only pinned rejections would be
    /// passed by a model that rejects everything.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_facts_include: Option<Vec<String>>,
    /// No-invention on the facts the Journalist inherits: no parsed `key_fact` may contain any of
    /// these (e.g. a team name the article never mentions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_facts_exclude: Option<Vec<String>>,
    /// The fixture prompt's VETTED entity list. `evaluate` hands it to `ArticleEvidenceParser` so
    /// the real production derivation runs — and it is load-bearing, not decoration: only vetted
    /// entities' roles count toward the verdict, because the model reliably volunteers extra
    /// people from the body and an unfiltered vote lets them overturn a correct `opponent`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reader_vetted: Option<Vec<String>>,
    /// DISCOVERY (ar7/C1): each string must appear in the parsed `relevant_entities`. This is the
    /// axis for the bleed the whole newsroom plan turns on — the Editor read 99 articles mentioning
    /// Vinicius Junior and linked him in 24 — and until ar7 the field it was supposed to name him
    /// in had **no definition anywhere in the prompt**: `relevant_entities` appeared exactly once,
    /// in the JSON template, as `"relevant_entities":["<name>", "..."]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub names_include: Option<Vec<String>>,
    /// PRECISION on the same field: none of these may appear. B4's backfill measured what an
    /// undefined field collects — `Paris` on a Tour de France story, `Moulin Rouge`, and on a
    /// mining-stock article the invented `Fortuna Düsseldorf`. Every name that RESOLVES becomes an
    /// entity link the moment B1 wires this field to the resolver, so discovery and precision have
    /// to be scored together or ar7 just trades one error class for another.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub names_exclude: Option<Vec<String>>,
    /// C2: the expected emotional register. Pinned in BOTH directions on purpose — a fixture set
    /// that only pinned non-neutral cases would be passed by a model that calls everything
    /// `outrage`, which is the same shape of failure as ar3's 99.1% `relevant:true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub register_is: Option<String>,
    // greenfield editor (ep1) rubric — the fields below score the ep1 contract's additions.
    /// ep1 topic pin (e.g. a hiring must be `roster` — the §1a ruling). Sparse use: story_type
    /// discriminates well since the ar5 collapse was fixed, so only rule-bearing cases pin it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_type_is: Option<String>,
    /// ep1 discovery kinds: each listed name must be emitted with exactly this `kind_hint`
    /// (`{"Kyle Shanahan": "person"}`). The kind gate is what routes an unknown coach to
    /// person-discovery instead of a fuzzy player match (T9), so it is scored, not assumed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_kind_is: Option<std::collections::BTreeMap<String, String>>,
    /// ep1 descriptors: each listed name must carry a non-empty `descriptor`. The descriptor is
    /// the 5.2 first-sight nomination trigger, so a bare name here is a lost discovery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_descriptor_nonempty: Option<Vec<String>>,
    /// ep1 `result_line` substring checks (verbatim-or-empty; an empty expectation is pinned by
    /// including nothing and asserting `result_line_parses: false`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_line_includes: Option<Vec<String>>,
    /// Whether the PRODUCTION `derive::parse_result_line` must parse the emitted line. `true`
    /// pins a completed result the code can consume; `false` pins that no phantom result parses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_line_parses: Option<bool>,
    /// Fixture-declared `entity_name_surfaces` rows for the resolver simulation: the eval runs
    /// the PRODUCTION grouping/kind-gate (`derive::group_hits`) against these, with
    /// case-insensitive name equality standing in for the database's `nrm()` exact match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolver_surfaces: Option<Vec<ResolverSurfaceFx>>,
    /// Names that must AUTO-LINK given the declared surfaces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolver_links_include: Option<Vec<String>>,
    /// Names that must NOT auto-link (the Paris case: the surface exists and the kind gate —
    /// fed by the model's own kind_hint — must refuse it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolver_links_exclude: Option<Vec<String>>,
    /// Names that must land in `unresolved` — the Investigator's discovery channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolver_unresolved_include: Option<Vec<String>>,
    /// Names that must be REFUSED as ambiguous (the namesake tie — never a coin flip).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolver_refused_include: Option<Vec<String>>,
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
    /// Ceiling on how many DISTINCT peers the reading may name ("the Analyst", "the Insider", …).
    ///
    /// The Oracle prompt has always said "name at most ONE peer … never a roll call of all five;
    /// the reading is yours alone", and nothing ever asserted it. The or8 gate passed 80/80 while
    /// five of six readings named two, three, or four peers — the check did not exist, so the rule
    /// was advice. A substring exclusion cannot express this: any ONE peer is legal, and only the
    /// count is wrong, so it needs its own check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_max_peers: Option<i32>,
    // momentum / trajectory reasoning rubric: PROSE ONLY.
    // momentum_score_min/max were removed in s11 — the Analyst no longer emits a score, so
    // there was nothing left for them to assert. Both numbers (direction and the ±5
    // conviction) are computed by the junction and unit-tested there, not gated here.
}

/// One fixture-declared surface row for the greenfield editor's resolver simulation — the
/// database state `derive::group_hits` is scored against.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolverSurfaceFx {
    pub name: String,
    pub entity_type: String,
    pub entity_id: i32,
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
        "oracle" => Some(Box::new(OracleTask)),
        "narratives" => Some(Box::new(NarrativeTask)),
        "transfer" => Some(Box::new(TransferTask)),
        "rating" => Some(Box::new(RatingTask)),
        "momentum" => Some(Box::new(MomentumTask)),
        "graph" => Some(Box::new(GraphTask)),
        "editor" => Some(Box::new(EditorTask)),
        _ => None,
    }
}

/// all_task_names lists the registered tasks (for usage output + unknown-task errors).
pub fn all_task_names() -> &'static [&'static str] {
    &[
        "vibe",
        "oracle",
        "narratives",
        "transfer",
        "rating",
        "momentum",
        "graph",
        "editor",
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
        Role::VibeLogic
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
            format_schema_raw: None,
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
        // Eval pins the continuity-free, memory-free prompt shape (the n8 precedent):
        // fixtures measure the fresh-signal contract, not the v12 enrichment riders — and,
        // since 7.6, not the packet block either: these fixtures are the LEGACY gate, and they
        // must keep measuring the shape the legacy rail sends.
        Ok(Some(build_sentiment_prompt(
            &e.entity_type,
            &name,
            &e.sport,
            &narratives,
            &heat,
            &[],
            None,
            None,
        )))
    }
    fn evaluate(&self, raw: &str, label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        match parse_vibe_reply(raw) {
            Ok((s, hook, v)) => {
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
                    if x.hook_nonempty == Some(true) {
                        checks.push(PropertyCheck {
                            name: "hook_nonempty".into(),
                            pass: hook.is_some(),
                            detail: format!("hook={}", hook.as_deref().unwrap_or("(absent)")),
                        });
                    }
                }
                CaseVerdict {
                    parsed: true,
                    abs_err: label.map(|l| (s as f64 - l).abs()),
                    checks,
                    display: match &hook {
                        Some(h) => format!("score={s} | {h} — {v}"),
                        None => format!("score={s} | {v}"),
                    },
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
// OracleTask — the crown: reads all five pillar cards + prior reads, then emits {reading, score}.
// (The panel SigilTask was retired in the crown fold, 2026-07-21.)
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
            format_schema_raw: None,
        }
    }
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>> {
        let name = lookup_entity_name(&hx.pool, &e.entity_type, e.entity_id, &e.sport).await?;
        let sport = e.sport.to_uppercase();
        let (_season, narratives, rating, vibe, momentum, transfers) =
            load_pillars(hx, &e.entity_type, e.entity_id, &sport).await?;
        // No-pillar path: the stage would persist a marker without a model call — no cards to read.
        if narratives.is_empty()
            && rating.is_none()
            && vibe.is_none()
            && momentum.empty()
            && transfers.is_empty()
        {
            return Ok(None);
        }
        // Deterministic convergence + omen, exactly as the live handler. prior_read = None,
        // memory = None: reproducible fixtures measure the fresh-card contract, not the
        // memory enrichment riders.
        let comparisons =
            build_pillar_divergence(&narratives, rating.as_ref(), vibe.as_ref(), &momentum);
        let convergence = pillar_convergence(&comparisons);
        let (omen, omen_reason) = compute_omen(convergence, rating.as_ref(), &momentum);
        Ok(Some(build_crown_prompt(
            &e.entity_type,
            &name,
            &e.sport,
            &narratives,
            rating.as_ref(),
            vibe.as_ref(),
            &momentum,
            &transfers,
            omen,
            &omen_reason,
            None,
            None,
            None,
        )))
    }
    fn evaluate(&self, raw: &str, label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        let Some(reply) = parse_crown_reply(raw) else {
            return CaseVerdict {
                parsed: false,
                abs_err: None,
                checks: Vec::new(),
                display: "unparseable".into(),
            };
        };
        let reading = reply.reading;
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
            if let Some(max) = x.reading_max_peers {
                let named = count_named_peers(&reading);
                checks.push(PropertyCheck {
                    name: "reading_max_peers".into(),
                    pass: named as i32 <= max,
                    detail: format!("peers={named} ≤ {max}"),
                });
            }
        }

        // The crown now emits the score too: measure it against the labeled expected score.
        let abs_err = label.map(|l| ((reply.score as f64) - l).abs());
        CaseVerdict {
            parsed: true,
            abs_err,
            checks,
            display: format!("score={} | {}", reply.score, reading),
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
            format_schema: Some(crate::junctions::journalist::narratives_format_schema()),
            format_schema_raw: None,
        }
    }
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>> {
        let name = lookup_entity_name(&hx.pool, &e.entity_type, e.entity_id, &e.sport).await?;
        // Reads use the upper-cased sport; the prompt renders the request-case value (build_narratives_request).
        let sport = e.sport.to_uppercase();
        let corpus = load_vetted_corpus(&hx.pool, hx.voice_num_ctx, &e.entity_type, e.entity_id, &sport).await?;
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
        Ok(Some(build_narratives_prompt(
            &req, &corpus, &heat, None, None, None,
        ))) // evals pin the memory-free, score-context-free, legacy-rail prompt shape
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
            // OR-semantics voice-direction check: at least one body voices at least one acceptable
            // synonym (case-insensitive — voice varies casing). One check for the whole set, so the
            // detail names which words satisfied it (or that none did).
            if let Some(any) = &x.body_includes_any {
                let lowered: Vec<String> = items.iter().map(|(_, b, _)| b.to_lowercase()).collect();
                let hit: Vec<&str> = any
                    .iter()
                    .filter(|s| {
                        let needle = s.to_lowercase();
                        lowered.iter().any(|b| b.contains(&needle))
                    })
                    .map(|s| s.as_str())
                    .collect();
                checks.push(PropertyCheck {
                    name: format!("body_includes_any:[{}]", any.join("|")),
                    pass: !hit.is_empty(),
                    detail: if hit.is_empty() {
                        "no listed synonym voiced".into()
                    } else {
                        format!("voiced {hit:?}")
                    },
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
        Role::TransferLogic
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
            format_schema_raw: None,
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
            format_schema_raw: None,
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
            crate::junctions::insider::team_relationship(&hx.pool, e.entity_id, player_id, &sport).await?;
        match build_pair_request(
            hx,
            e.entity_id,
            &team_name,
            &candidate,
            &sport,
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
            format_schema_raw: None,
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
        // with_memory=false: eval pins the memory-free prompt shape (the s12/n8 precedent).
        match build_rating_request(hx, &req, 0.0, false).await? {
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

// Momentum's prompt contract lives in `crate::junctions::analyst` (the production stage) — the eval task
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
            format_schema_raw: None,
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
        // Eval pins the memory-free prompt shape (the s5/n8 precedent): fixtures measure
        // the fresh-signal contract, not the enrichment rider.
        Ok(Some(build_momentum_prompt(
            &e.entity_type,
            &name,
            &e.sport,
            rating.as_ref(),
            vibe.as_ref(),
            &momentum,
            None,
            &[],
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

        // Contract-level invariant, asserted whether or not this case carries an `expect`: the
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

        if let Some(x) = expect {
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
            display: reply.blurb.clone(),
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

/// Case-insensitive substring match, with typographic punctuation folded to ASCII first.
///
/// The folding is not cosmetic — it is what makes a banned-phrase check real. Fixture expects are
/// hand-written with ASCII quotes (`isn't a surge`), while chat-tuned models emit the typographic
/// forms (`isn’t a surge`, U+2019). Without folding, such an exclusion can NEVER fail: it silently
/// passes on output that contains the banned phrase verbatim. That is the toothless-fixture hazard,
/// and it hid a live momentum regression through the whole of s10 — the phrase ban added in s10 was
/// reported as "10 → 0 occurrences" by a grep that could not match the model's own apostrophe.
///
/// Only quote characters are folded. Dashes are deliberately left alone: an em dash is a real
/// stylistic signal some checks may legitimately want to assert on, and folding it to `-` would
/// make those checks mean something different.
fn contains_ci(haystack: &str, needle: &str) -> bool {
    fold_quotes(haystack).contains(&fold_quotes(needle))
}

/// How many DISTINCT peers a reading names. The Oracle may name at most one, and only when that
/// card carries the turn — a roll call makes the reading a summary of the table rather than the
/// Oracle's own verdict.
///
/// Matches "the Analyst"-style references only. A bare sport word ("the scout said") would be a
/// false positive, so the definite article is required, which is how the prompt's own examples are
/// written ("the Insider's wire stirs", "the Analyst's call holds").
fn count_named_peers(reading: &str) -> usize {
    const PEERS: [&str; 5] = ["analyst", "insider", "scout", "influencer", "journalist"];
    let lower = reading.to_lowercase();
    PEERS
        .iter()
        .filter(|p| lower.contains(&format!("the {p}")))
        .count()
}

/// Phrases the momentum contract bans outright, checked on EVERY momentum reply as a single
/// invariant rather than as a per-fixture expectation.
///
/// It began as `prose_excludes` entries injected into all eight fixtures — 6 phrases x 8 fixtures =
/// 48 checks, which tripled the denominator, mostly fired where the phrase was never at risk, and
/// made scores incomparable between revisions because the denominator moved whenever the list did.
/// The ban is global, so it belongs in one place. One check per reply, with the offending phrase in
/// the detail, says exactly as much and costs 8 checks instead of 48.
///
/// Deliberately specific. Bare "the engine" is banned in the prompt but NOT here: a football READ
/// can legitimately say "the engine room of midfield", and a check that fails on correct prose
/// trains everyone to ignore it.
pub const MOMENTUM_BANNED_PHRASES: &[&str] = &[
    "isn't a surge",
    "isn't a collapse",
    "the tape calls this",
    "the engine sees this as",
    "the momentum engine",
    "the numbers say",
    "steady band",
];

/// Lowercase and fold the typographic quote characters to their ASCII equivalents.
fn fold_quotes(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{02BC}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{201F}' => '"',
            other => other,
        })
        .collect::<String>()
        .to_lowercase()
}

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
    fn role(&self) -> Role {
        Role::EmotionalNews
    }
    fn prompt_version(&self) -> &'static str {
        GRAPH_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> GenerateOptions {
        let mut o = graph_opts();
        o.temperature = Some(temperature);
        o
    }
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>> {
        if e.entity_type != "article" {
            anyhow::bail!(
                "graph evals are article-keyed: use article:<id>:<SPORT> (got {})",
                e.entity_type
            );
        }
        let sport = e.sport.to_uppercase();
        let Some((article, candidates)) =
            load_graph_article_context(&hx.pool, i64::from(e.entity_id), &sport).await?
        else {
            return Ok(None);
        };
        Ok(Some(build_graph_prompt(
            &article.source,
            &article.published,
            &article.title,
            &article.description,
            &candidates,
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

/// EditorTask — the GREENFIELD Editor's gate (contract ep1, PLAN-one-rail Phase 3.6).
///
/// Runs the production `EditorReadParser` (which derives relevance) and the production
/// derivations: `parse_result_line` on the emitted line, and `group_hits` — the resolver's
/// kind-gate/grouping core — against fixture-declared surfaces, with case-insensitive name
/// equality standing in for the database's exact `nrm()` match. So the fixtures score the same
/// code path production runs, minus only the SQL normalizer.
pub struct EditorTask;

#[async_trait]
impl LensTask for EditorTask {
    fn name(&self) -> &'static str {
        "editor"
    }
    fn role(&self) -> Role {
        Role::Editor
    }
    fn prompt_version(&self) -> &'static str {
        EDITOR_CONTRACT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> GenerateOptions {
        let mut o = editor_opts();
        o.temperature = Some(temperature);
        o
    }
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>> {
        if e.entity_type != "article" {
            anyhow::bail!(
                "editor evals are article-keyed: use article:<id>:<SPORT> (got {})",
                e.entity_type
            );
        }
        build_editor_prompt_for_eval(&hx.pool, i64::from(e.entity_id), &e.sport.to_uppercase())
            .await
    }
    fn evaluate(&self, raw: &str, _label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        let hypothesis: Vec<String> = expect
            .and_then(|x| x.reader_vetted.clone())
            .unwrap_or_default();
        let parsed = EditorReadParser {
            hypothesis: &hypothesis,
        }
        .parse(raw)
        .ok()
        .flatten();
        let Some(read): Option<EditorRead> = parsed else {
            return CaseVerdict {
                parsed: false,
                abs_err: None,
                checks: Vec::new(),
                display: "unparseable (fail-closed)".into(),
            };
        };
        let mut checks = Vec::new();
        if let Some(x) = expect {
            if let Some(want) = x.article_relevant {
                checks.push(PropertyCheck {
                    name: format!("relevant[{want}]"),
                    pass: read.relevant == want,
                    detail: format!(
                        "relevant={} page_kind={:?} roles=[{}] story_type={:?}",
                        read.relevant,
                        read.page_kind,
                        read.entity_roles
                            .iter()
                            .map(|r| format!("{}:{}", r.entity, r.role))
                            .collect::<Vec<_>>()
                            .join(", "),
                        read.story_type
                    ),
                });
            }
            let facts = read.key_facts.join(" | ");
            if let Some(incl) = &x.key_facts_include {
                for frag in incl {
                    checks.push(PropertyCheck {
                        name: format!("key_fact_present[{frag}]"),
                        pass: facts.to_lowercase().contains(&frag.to_lowercase()),
                        detail: truncate(&facts, 160),
                    });
                }
            }
            if let Some(excl) = &x.key_facts_exclude {
                for frag in excl {
                    checks.push(PropertyCheck {
                        name: format!("key_fact_absent[{frag}]"),
                        pass: !facts.to_lowercase().contains(&frag.to_lowercase()),
                        detail: truncate(&facts, 160),
                    });
                }
            }
            // Discovery, matched against the JOINED name list (the model may write a pinned
            // surname in full — the resolver sees the whole surface, so score what it sees).
            let names = read
                .names
                .iter()
                .map(|n| n.name.clone())
                .collect::<Vec<_>>()
                .join(" | ");
            if let Some(incl) = &x.names_include {
                for frag in incl {
                    checks.push(PropertyCheck {
                        name: format!("name_found[{frag}]"),
                        pass: names.to_lowercase().contains(&frag.to_lowercase()),
                        detail: truncate(&names, 200),
                    });
                }
            }
            if let Some(excl) = &x.names_exclude {
                for frag in excl {
                    checks.push(PropertyCheck {
                        name: format!("name_absent[{frag}]"),
                        pass: !names.to_lowercase().contains(&frag.to_lowercase()),
                        detail: truncate(&names, 200),
                    });
                }
            }
            // ep1 kind + descriptor axes — matched per emitted mention whose name CONTAINS the
            // expected name (same containment logic as name_found).
            if let Some(kinds) = &x.name_kind_is {
                for (who, want_kind) in kinds {
                    let found = read
                        .names
                        .iter()
                        .find(|n| n.name.to_lowercase().contains(&who.to_lowercase()));
                    checks.push(PropertyCheck {
                        name: format!("name_kind[{who}={want_kind}]"),
                        pass: found.is_some_and(|n| n.kind_hint.eq_ignore_ascii_case(want_kind)),
                        detail: found
                            .map(|n| format!("{} kind_hint={}", n.name, n.kind_hint))
                            .unwrap_or_else(|| "name not emitted".into()),
                    });
                }
            }
            if let Some(who_list) = &x.name_descriptor_nonempty {
                for who in who_list {
                    let found = read
                        .names
                        .iter()
                        .find(|n| n.name.to_lowercase().contains(&who.to_lowercase()));
                    checks.push(PropertyCheck {
                        name: format!("descriptor_nonempty[{who}]"),
                        pass: found.is_some_and(|n| !n.descriptor.trim().is_empty()),
                        detail: found
                            .map(|n| format!("{} descriptor={:?}", n.name, n.descriptor))
                            .unwrap_or_else(|| "name not emitted".into()),
                    });
                }
            }
            if let Some(incl) = &x.result_line_includes {
                for frag in incl {
                    checks.push(PropertyCheck {
                        name: format!("result_line_has[{frag}]"),
                        pass: read
                            .result_line
                            .to_lowercase()
                            .contains(&frag.to_lowercase()),
                        detail: format!("result_line={:?}", read.result_line),
                    });
                }
            }
            if let Some(want) = x.result_line_parses {
                let parsed_result = editor_derive::parse_result_line(&read.result_line);
                checks.push(PropertyCheck {
                    name: format!("result_line_parses[{want}]"),
                    pass: parsed_result.is_some() == want,
                    detail: format!(
                        "result_line={:?} parsed={:?}",
                        read.result_line, parsed_result
                    ),
                });
            }
            // Resolver simulation: production group_hits over fixture-declared surfaces.
            let needs_resolver = x.resolver_links_include.is_some()
                || x.resolver_links_exclude.is_some()
                || x.resolver_unresolved_include.is_some()
                || x.resolver_refused_include.is_some();
            if needs_resolver {
                let surfaces = x.resolver_surfaces.clone().unwrap_or_default();
                let hits: Vec<editor_derive::SurfaceHit> = read
                    .names
                    .iter()
                    .flat_map(|mention| {
                        surfaces
                            .iter()
                            .filter(|s| s.name.eq_ignore_ascii_case(&mention.name))
                            .map(|s| editor_derive::SurfaceHit {
                                name: mention.name.clone(),
                                entity_type: s.entity_type.clone(),
                                entity_id: s.entity_id,
                                sport: "FOOTBALL".to_string(),
                                norm: s.name.to_lowercase(),
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect();
                // Per-name verdict through the PRODUCTION grouping: linked | refused |
                // unresolved | not_emitted. `not_emitted` passes only never-links checks —
                // a name the model did not emit trivially cannot link.
                let verdict_for = |who: &str| -> &'static str {
                    let Some(mention) = read
                        .names
                        .iter()
                        .find(|n| n.name.to_lowercase().contains(&who.to_lowercase()))
                    else {
                        return "not_emitted";
                    };
                    let r = editor_derive::group_hits(std::slice::from_ref(mention), &hits);
                    if !r.links.is_empty() {
                        "linked"
                    } else if !r.refused_ambiguous.is_empty() {
                        "refused"
                    } else {
                        "unresolved"
                    }
                };
                for (list, want, label) in [
                    (&x.resolver_links_include, "linked", "resolver_links"),
                    (&x.resolver_unresolved_include, "unresolved", "resolver_unresolved"),
                    (&x.resolver_refused_include, "refused", "resolver_refused"),
                ] {
                    if let Some(incl) = list {
                        for who in incl {
                            let got = verdict_for(who);
                            checks.push(PropertyCheck {
                                name: format!("{label}[{who}]"),
                                pass: got == want,
                                detail: format!("{who} -> {got}"),
                            });
                        }
                    }
                }
                if let Some(excl) = &x.resolver_links_exclude {
                    for who in excl {
                        let got = verdict_for(who);
                        checks.push(PropertyCheck {
                            name: format!("resolver_never_links[{who}]"),
                            pass: got != "linked",
                            detail: format!("{who} -> {got}"),
                        });
                    }
                }
            }
            if let Some(want) = &x.story_type_is {
                checks.push(PropertyCheck {
                    name: format!("story_type[{want}]"),
                    pass: read.story_type.eq_ignore_ascii_case(want),
                    detail: format!("story_type={:?}", read.story_type),
                });
            }
            if let Some(want) = &x.register_is {
                checks.push(PropertyCheck {
                    name: format!("register[{want}]"),
                    pass: read.register.eq_ignore_ascii_case(want),
                    detail: format!(
                        "register={:?} phrase={:?}",
                        read.register, read.register_phrase
                    ),
                });
            }
            if let Some(incl) = &x.blurb_includes {
                for frag in incl {
                    checks.push(PropertyCheck {
                        name: format!("blurb_present[{frag}]"),
                        pass: read
                            .evidence_blurb
                            .to_lowercase()
                            .contains(&frag.to_lowercase()),
                        detail: truncate(&read.evidence_blurb, 160),
                    });
                }
            }
            if let Some(excl) = &x.blurb_excludes {
                for frag in excl {
                    checks.push(PropertyCheck {
                        name: format!("blurb_absent[{frag}]"),
                        pass: !read
                            .evidence_blurb
                            .to_lowercase()
                            .contains(&frag.to_lowercase()),
                        detail: truncate(&read.evidence_blurb, 160),
                    });
                }
            }
        }
        CaseVerdict {
            parsed: true,
            abs_err: None,
            checks,
            display: format!(
                "relevant={} page_kind={:?} names=[{}] result_line={:?} {} key_fact(s) story_type={:?} register={:?}",
                read.relevant,
                read.page_kind,
                truncate(
                    &read
                        .names
                        .iter()
                        .map(|n| format!("{}<{} {:?}>", n.name, n.kind_hint, n.descriptor))
                        .collect::<Vec<_>>()
                        .join(", "),
                    280
                ),
                truncate(&read.result_line, 40),
                read.key_facts.len(),
                read.story_type,
                read.register,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_resolves_known_tasks_and_rejects_unknown() {
        assert!(resolve_task("vibe").is_some());
        assert!(resolve_task("oracle").is_some());
        assert!(resolve_task("narratives").is_some());
        assert!(resolve_task("transfer").is_some());
        assert!(resolve_task("rating").is_some());
        assert!(resolve_task("momentum").is_some());
        assert!(resolve_task("nope").is_none());
        assert_eq!(resolve_task("vibe").unwrap().name(), "vibe");
        assert_eq!(resolve_task("oracle").unwrap().name(), "oracle");
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
    fn lens_parameters_capture_the_locked_cast() {
        // The cast is an identity lock (wiki/Characters.md, 2026-07-21) — a rename here is a
        // product decision, not a refactor.
        let rating = lens_parameters("rating").unwrap();
        assert_eq!(rating.rail, Rail::StatsAnalytical);
        assert_eq!(rating.operator, "The Scout");
        assert!(rating.mandate.contains("greatest strength"));

        assert_eq!(
            lens_parameters("narratives").unwrap().operator,
            "The Journalist"
        );
        assert_eq!(lens_parameters("transfer").unwrap().operator, "The Insider");
        assert_eq!(lens_parameters("vibe").unwrap().operator, "The Influencer");
        assert_eq!(lens_parameters("momentum").unwrap().operator, "The Analyst");

        let transfer = lens_parameters("transfer").unwrap();
        assert_eq!(transfer.rail, Rail::EmotionalNews);

        let oracle = lens_parameters("oracle").unwrap();
        assert_eq!(oracle.rail, Rail::Synthesis);
        assert_eq!(oracle.operator, "the Oracle");
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

    // --- crown (Oracle) eval: reading + score -------------------------------------

    const CROWN_OK: &str = r#"{"reading": "The winger's arc holds under a turning sky; the wind toward Liverpool stirs but nothing has broken. A steady hand on a rising line.", "score": 74}"#;

    #[test]
    fn crown_eval_parses_reading_and_scores_against_label() {
        let x = Expect {
            reading_min_sentences: Some(2),
            reading_includes: Some(vec!["Liverpool".into()]),
            ..Default::default()
        };
        let v = OracleTask.evaluate(CROWN_OK, Some(70.0), Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
        assert_eq!(v.abs_err, Some(4.0)); // |74 - 70|
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

    #[test]
    fn vibe_hook_nonempty_check() {
        // v13 card contract: hook_nonempty fails a hook-less (v12-shape) reply, passes a
        // three-line one.
        let x = Expect {
            hook_nonempty: Some(true),
            ..Default::default()
        };
        assert!(VibeTask
            .evaluate(
                "SCORE: 30\nHOOK: The slide is real\nVIBE: grim",
                None,
                Some(&x)
            )
            .all_checks_pass());
        assert!(!VibeTask
            .evaluate("SCORE: 30\nVIBE: grim", None, Some(&x))
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
    fn narratives_body_includes_any_is_or_and_case_insensitive() {
        // Voice-direction target: passes when ANY one synonym is voiced in ANY body, matched
        // case-insensitively; fails only when the whole set is absent.
        let reply = r#"{"narratives":[{"title":"Vale saga","body":"The Kings pursuit is still GATHERING pace after months.","articles":[1]}]}"#;
        // "gathering" (cased differently) satisfies the heating set even though "surging" is absent.
        let heating = Expect {
            body_includes_any: Some(vec!["surging".into(), "gathering".into()]),
            ..Default::default()
        };
        assert!(NarrativeTask
            .evaluate(reply, None, Some(&heating))
            .all_checks_pass());
        // None of the cooling words appear → the OR-check fails.
        let cooling = Expect {
            body_includes_any: Some(vec![
                "cooling".into(),
                "fizzled".into(),
                "gone quiet".into(),
            ]),
            ..Default::default()
        };
        assert!(!NarrativeTask
            .evaluate(reply, None, Some(&cooling))
            .all_checks_pass());
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

    // --- typographic folding in the property matcher -----------------------------

    /// The regression this exists to prevent: a banned-phrase exclusion written with an ASCII
    /// apostrophe must still fail on model output that uses U+2019. Before folding, this check
    /// passed on text containing the banned phrase verbatim — a check that cannot fail is worse
    /// than no check, because the run reports green.
    #[test]
    fn prose_excludes_matches_across_typographic_apostrophes() {
        // Real ministral-3:14b output from the momentum-s11 fixture gate (curly U+2019).
        let reply = "READ: The tape holds firm and the samples are thin. \
                     For now, this isn\u{2019}t a surge\u{2014}just a brief flash of what might come.";
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
        let reply = "READ: Harbor City\u{2019}s press is tightening cleanly across the last six.";
        let x = Expect {
            prose_includes: Some(vec!["Harbor City's press".into()]),
            ..Default::default()
        };
        let v = MomentumTask.evaluate(reply, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn fold_quotes_leaves_dashes_and_ordinary_text_alone() {
        assert_eq!(fold_quotes("A\u{2014}B"), "a\u{2014}b");
        assert_eq!(fold_quotes("\u{201C}quoted\u{201D}"), "\"quoted\"");
        assert_eq!(fold_quotes("It\u{2019}s"), "it's");
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
    fn momentum_parser_extracts_the_read() {
        // s11 contract: READ alone. Stray MOMENTUM and SCORE lines (models echoing what
        // every contract through s10 asked for) are tolerated and ignored, never content.
        let raw = "MOMENTUM: rising\nSCORE: 3\nREAD: PEAK is rising while Vibe is steady, so the current direction is modestly positive.";
        let parsed = parse_momentum_reply(raw).unwrap();
        assert!(parsed.blurb.contains("PEAK is rising"));
        assert!(!parsed.blurb.contains('3'), "SCORE leaked into the blurb: {}", parsed.blurb);
    }

    #[test]
    fn momentum_rubric_scores_prose() {
        // s11: the signed-band assertions are gone — the score is no longer the model's to
        // get wrong. `momentum_conviction_from_score` is unit-tested in the junction instead.
        let x = Expect {
            prose_includes: Some(vec!["Vibe".into()]),
            prose_excludes: Some(vec!["surging".into()]),
            ..Default::default()
        };
        let raw = "READ: Vibe is pulling the profile down despite a steadier PEAK read.";
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
        assert_eq!(fx.expect.convergence_min, None); // defaulted
                                                     // A fixture may omit expect entirely.
        let bare = r#"{"name":"n","task":"oracle","prompt_version":"or3","system":"s","user_prompt":"u","temperature":0.0}"#;
        let fx2: Fixture = serde_json::from_str(bare).unwrap();
        assert_eq!(fx2.expect.reading_min_sentences, None);
    }

    #[test]
    fn fixture_drift_flags_prompt_version_mismatch() {
        let mut fx = Fixture {
            name: "f".into(),
            task: "oracle".into(),
            prompt_version: ORACLE_PROMPT_VERSION.into(),
            system: "s".into(),
            user_prompt: "u".into(),
            temperature: 0.0,
            expect: Expect::default(),
        };
        assert!(fixture_drift(&fx, &OracleTask).is_none());
        fx.prompt_version = "or1".into();
        assert!(fixture_drift(&fx, &OracleTask).is_some());
    }

    /// Integrity guard for the on-disk narratives fixtures. `Fixture`/`Expect` have no
    /// `deny_unknown_fields`, so a misspelled expect key (e.g. `body_include_any`) parses fine and is
    /// SILENTLY dropped — a toothless fixture that looks authored. This loads the real dir (via
    /// `CARGO_MANIFEST_DIR`, so it is CWD-independent) and asserts (a) every file parses as a
    /// narratives fixture and (b) each current-version voicing fixture actually carries a voicing
    /// axis — catching a dropped field before it silently weakens the eval. It does NOT assert
    /// current-version (rot is a warn, not an error — old-version fixtures are legitimately kept
    /// until re-captured).
    #[test]
    fn narratives_fixtures_on_disk_parse_and_voicing_fixtures_carry_an_axis() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/narratives");
        let mut voiced_seen = 0;
        for entry in std::fs::read_dir(&dir).expect("read fixtures/narratives") {
            let p = entry.unwrap().path();
            if p.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            let fx: Fixture = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("fixture {} failed to parse: {e}", p.display()));
            assert_eq!(fx.task, "narratives", "{} has wrong task", p.display());
            if fx.prompt_version == NARRATIVES_PROMPT_VERSION
                && (fx.expect.body_includes_any.is_some() || fx.expect.body_excludes.is_some())
            {
                voiced_seen += 1;
            }
        }
        assert!(
            voiced_seen >= 3,
            "expected at least three current-version voicing fixtures (regenerate: cargo run --example narratives_n10_fixtures), saw {voiced_seen}"
        );
    }

    /// Integrity guard for the on-disk greenfield-editor fixtures (same serde silent-drop hazard
    /// as the narratives guard above: a misspelled expect key parses fine and silently weakens
    /// the gate). Asserts (a) every file parses, (b) task and prompt_version are the greenfield
    /// identities, (c) the set pins BOTH directions (≥2 rejects, ≥2 accepts), and (d) each of
    /// the Phase 3.6 derivation axes is exercised at least once — resolver refusal, resolver
    /// unresolved (discovery), never-links (the descriptor gate), and a parsing result_line.
    #[test]
    fn editor_fixtures_on_disk_parse_and_cover_the_ep1_axes() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/editor");
        let (mut n, mut rejects, mut accepts) = (0, 0, 0);
        let (mut refused, mut unresolved, mut never_links, mut result_parses) =
            (false, false, false, false);
        for entry in std::fs::read_dir(&dir).expect("read fixtures/editor") {
            let p = entry.unwrap().path();
            if p.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            let fx: Fixture = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("fixture {} failed to parse: {e}", p.display()));
            assert_eq!(fx.task, "editor", "{} has wrong task", p.display());
            assert_eq!(
                fx.prompt_version, EDITOR_CONTRACT_VERSION,
                "{} frozen under a different contract",
                p.display()
            );
            n += 1;
            match fx.expect.article_relevant {
                Some(false) => rejects += 1,
                Some(true) => accepts += 1,
                None => {}
            }
            refused |= fx.expect.resolver_refused_include.is_some();
            unresolved |= fx.expect.resolver_unresolved_include.is_some();
            never_links |= fx.expect.resolver_links_exclude.is_some();
            result_parses |= fx.expect.result_line_parses == Some(true);
        }
        assert!(n >= 12, "Phase 3.6 targets ≥12 fixtures, found {n}");
        assert!(
            rejects >= 2 && accepts >= 2,
            "both directions must stay pinned (rejects={rejects}, accepts={accepts})"
        );
        assert!(refused, "no fixture pins a resolver refusal (namesake tie)");
        assert!(unresolved, "no fixture pins resolver discovery (coach shape)");
        assert!(never_links, "no fixture pins the descriptor/kind gate (place collision)");
        assert!(result_parses, "no fixture pins a parsing result_line");
    }

    /// Integrity guard for the on-disk transfer fixtures (same serde silent-drop hazard as the
    /// narratives guard above). Asserts (a) every file parses as a transfer fixture and (b) each t9
    /// fixture carries a steam/fizzle BEHAVIOUR axis — `transfer_stage` and/or a confidence bound —
    /// so a dropped `transfer_stage`/`confidence_*` key can't silently gut the Phase 4 weighting
    /// check. Version rot is not asserted (old-version fixtures are legitimately kept until re-run).
    #[test]
    fn transfer_fixtures_on_disk_parse_and_current_carry_a_steam_fizzle_axis() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/transfer");
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
            if fx.prompt_version == TRANSFER_PROMPT_VERSION {
                current_seen += 1;
                assert!(
                    fx.expect.transfer_stage.is_some()
                        || fx.expect.confidence_min.is_some()
                        || fx.expect.confidence_max.is_some(),
                    "current-version fixture {} carries no steam/fizzle axis (field-name drop?)",
                    p.display()
                );
            }
        }
        assert!(
            current_seen >= 2,
            "expected the two current-version steam/fizzle fixtures (regenerate: cargo run --example transfer_t10_fixtures), saw {current_seen}"
        );
    }
}
