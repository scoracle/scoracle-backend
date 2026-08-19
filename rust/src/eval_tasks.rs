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
//! `rating` on `Role::StatsLogic` (The Scout), `momentum` on `Role::MomentumLogic`
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
use crate::harness::{Harness, Parser};
use crate::junctions::analyst::{
    build_momentum_prompt, parse_momentum_reply, MOMENTUM_NUM_PREDICT, MOMENTUM_PROMPT_VERSION,
    MOMENTUM_SYSTEM_PROMPT,
};
use crate::junctions::editor::{
    build_editor_prompt_for_eval, derive as editor_derive, editor_opts, EditorRead,
    EditorReadParser, EDITOR_CONTRACT_VERSION,
};
use crate::junctions::graph::{
    build_graph_prompt, graph_opts, load_graph_article_context, GraphCandidate, GraphParser,
    GRAPH_PROMPT_VERSION,
};
use crate::junctions::influencer::{
    build_sentiment_prompt, load_latest_narratives, parse_vibe_reply, VIBE_NUM_PREDICT,
    VIBE_PROMPT_VERSION, VIBE_SYSTEM_PROMPT,
};
use crate::junctions::insider::{
    build_pair_request, load_candidates, transfer_system_prompt, PairBuild, TransferParser,
    TRANSFER_DEFAULT_MIN_ARTICLES, TRANSFER_NUM_PREDICT, TRANSFER_PROMPT_VERSION,
};
use crate::junctions::investigator::prompt::{
    prose_opts, ProseReadParser, INVESTIGATOR_PROSE_CONTRACT_VERSION,
};
use crate::junctions::journalist::{
    build_narratives_prompt, load_packet_corpus, NarrativesParser, NarrativesReq,
    NARRATIVES_PROMPT_VERSION,
    NARRATIVES_SYSTEM_PROMPT,
};
use crate::junctions::oracle::{
    build_crown_prompt, build_pillar_divergence, compute_omen, count_sentences, load_pillars,
    oracle_format_schema, parse_crown_reply, pillar_convergence, ORACLE_NUM_PREDICT,
    ORACLE_PROMPT_VERSION, ORACLE_SYSTEM_PROMPT,
};
use crate::junctions::scout::{
    build_rating_request, RatingBuild, RatingReply, RatingReq, RATING_NUM_PREDICT,
    RATING_PROMPT_VERSION, RATING_SYSTEM_PROMPT,
};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::util::truncate;
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
/// three internal seats (editor, investigator, graph). `operator` carries the character identity
/// (the cast locked in wiki/Characters.md, 2026-07-21); the junction's system prompt is that
/// character's voice, so a voice change is a prompt change, never a rename here.
///
/// There is no `rail` here any more. It was product taxonomy from the two-rail era — a guess that
/// lenses would eventually route by model family — and its own doc admitted roles were the serving
/// primitive "until evals prove a split". The split never came: routing is per-`Role`
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
            mandate: "Farm the engagement: find the emotion running through the entity's narratives and ride it into the felt read of the moment.",
            credibility_guard: "Separate interactable mood from durable truth; the emotion must trace to the corpus — do not invent a narrative hook.",
        }),
        "rating" => Some(LensParameters {
            operator: "The Scout",
            mandate: "Prepare for the entity by naming the greatest strength to stop and the greatest weakness to exploit.",
            credibility_guard: "Use supplied tiers and datapoints only; never turn average marks into strengths.",
        }),
        "momentum" => Some(LensParameters {
            operator: "The Analyst",
            mandate: "Read the directional force of form (the rating trajectory) and feeling (the news mood), then narrate the decided direction with conviction.",
            credibility_guard: "Stay detached and results-only; do not chase sentiment hype or cling to stale profile strength.",
        }),
        "oracle" => Some(LensParameters {
            operator: "the Oracle",
            mandate: "Read the five pillar cards, deliver the entity's reading in the house voice, then render the Sigil verdict — the score this spread has earned (blind to memories since or9).",
            credibility_guard: "The mysticism lives in the telling, never the facts — every claim traces to a card shown; nothing invented; no internal field or product names.",
        }),
        "editor" => Some(LensParameters {
            operator: "The Editor",
            mandate: "Read every arrival's full text and describe it richly for the newsroom — shape, names with descriptors, roles, result line, register, facts — so code can derive everything downstream.",
            credibility_guard: "Describe, never judge: no relevance verdicts, no invented names or results — only what the text contains, with the descriptor copied from the text.",
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
    // vibe fixture score band (per-case boolean stand-in for the aggregate MAE axis).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_max: Option<i32>,
    // (The v13/v17 `hook_nonempty`/`hook_max_words`/`hook_excludes` axes retired 08-19: the
    // hook contract is a GLOBAL invariant — one `hook_contract` check per reply via
    // `guards::hook_violation`, the same rule `VibeParser` enforces in production.)
    /// momentum s14: the contract's "emit NO number" rule, gated — no ASCII digit anywhere in
    /// the READ. (The decided-direction line hands the model a signed score; echoing it is the
    /// exact violation this catches.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prose_no_digits: Option<bool>,
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
    /// Citation (n18, OR-semantics, case-insensitive): at least one returned body names at least
    /// one of these publications — the fixture lists its corpus's `[source]` tags. The register
    /// weaves the name into prose ("first reported by ESPN"); this axis only asserts a name
    /// appears, never how.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources_any: Option<Vec<String>>,
    /// Edition budget (n18): total sentences across ALL returned bodies must not exceed this.
    /// Counted crudely (terminal .!? runs) — a ceiling against padding, not a style meter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_sentences_max: Option<i32>,
    /// The Journalist's card_score (n12 busyness verdict, 1-99): the reply must carry one inside
    /// this band. Authored in the n17 pass — the field had been gate-invisible since n12 (the
    /// D-T45 rule). A missing card_score FAILS any band check: the fixture asserting the band is
    /// asserting the verdict exists at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_score_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_score_max: Option<i32>,
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
    // (The `reading_max_peers` axis retired 08-19: every oracle fixture carried `1`, i.e. the
    // rule was global — it is now one unconditional invariant check per reading, the same rule
    // `CrownParser` rejects on. Its history — "the or8 gate passed 80/80 while five of six
    // readings named two, three, or four peers; a rule measured by nothing is advice" — lives
    // with `guards::count_named_peers`.)
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
    /// Registry key (`"vibe"`, `"oracle"`) — also the `fixtures/<name>/` dir.
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
        "investigator" => Some(Box::new(InvestigatorTask)),
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
        "investigator",
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
                // Contract-level invariants (the MOMENTUM_BANNED_PHRASES shape, folded 08-19):
                // the HOOK contract and the body's global bans are enforced in production by
                // `VibeParser`'s guards — the gate asserts the SAME rules, one check each,
                // instead of the per-fixture `hook_*` expect entries they replaced. (Those
                // axes carried the v17 D-T45 gate growth; the invariants inherit that duty.)
                let hook_rule = hook.as_deref().map(crate::guards::hook_violation);
                checks.push(PropertyCheck {
                    name: "hook_contract".into(),
                    pass: matches!(hook_rule, Some(None)),
                    detail: match (&hook, hook_rule.flatten()) {
                        (None, _) => "hook=MISSING".into(),
                        (Some(h), Some(rule)) => format!("{rule} (hook={h:?})"),
                        (Some(_), None) => String::new(),
                    },
                });
                let banned = crate::guards::first_banned_phrase(&v, crate::guards::VIBE_BODY_BANS);
                checks.push(PropertyCheck {
                    name: "no_banned_phrases".into(),
                    pass: banned.is_none(),
                    detail: banned.map_or_else(String::new, |p| format!("found {p:?}")),
                });
                checks.push(product_name_check(&v));
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
                    for s in x.prose_includes.iter().flatten() {
                        checks.push(PropertyCheck {
                            name: format!("prose_includes:{s}"),
                            pass: contains_ci(&v, s),
                            detail: String::new(),
                        });
                    }
                    for s in x.prose_excludes.iter().flatten() {
                        checks.push(PropertyCheck {
                            name: format!("prose_excludes:{s}"),
                            pass: !contains_ci(&v, s),
                            detail: String::new(),
                        });
                    }
                    let word_count = v.split_whitespace().count() as i32;
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
                        let total = sentence_runs(&v);
                        checks.push(PropertyCheck {
                            name: "total_sentences_le".into(),
                            pass: total <= max,
                            detail: format!("sentences={total} ≤ {max}"),
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
        let (omen, omen_reason) = compute_omen(convergence, &momentum);
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

        // Contract-level invariants on every reading (the momentum no_banned_phrases shape).
        // (1) Product names: Scott, 2026-08-10 — "if it references another Character, it should
        // be their name and not PEAK or Vibe". Case-sensitive; see PRODUCT_NAME_BANS.
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
        // (3) The reading's global vocabulary bans — internal metric names, mechanism words,
        // the verdict formula — one check per reply via the SAME list `CrownParser` enforces
        // in production (`guards::ORACLE_READING_BANS`). Folded 08-19 from per-fixture
        // `reading_excludes` entries; the fixture axis now carries only spread-contextual
        // exclusions (the wrong omen words).
        let banned = crate::guards::first_banned_phrase(&reading, crate::guards::ORACLE_READING_BANS);
        checks.push(PropertyCheck {
            name: "no_banned_phrases".into(),
            pass: banned.is_none(),
            detail: banned.map_or_else(String::new, |p| format!("found {p:?}")),
        });
        // (4) The peer roll-call cap (or10: "name at most ONE peer … never a roll call") — was
        // a per-fixture `reading_max_peers: 1` on every oracle fixture, i.e. global; now one
        // invariant, same rule `CrownParser` rejects on.
        let named = count_named_peers(&reading);
        checks.push(PropertyCheck {
            name: "reading_max_peers".into(),
            pass: named <= 1,
            detail: format!("peers={named} ≤ 1"),
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
            // The production envelope, not the legacy 16384/4000 pair: an eval generating in a
            // window the live stage never runs would measure the wrong thing — and asking the
            // pinned runner for 16384 evicts it besides.
            num_predict: crate::junctions::journalist::NARRATIVES_NUM_PREDICT_PACKET,
            num_ctx: crate::route::VOICE_NUM_CTX_PACKET,
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
        // The PACKET corpus — the one production reads. This used `load_vetted_corpus` until the
        // Phase 9 rail prune, which meant the narratives eval was scoring a prompt the live stage
        // no longer builds: the same "measures the wrong thing" trap the fixtures' frozen system
        // prompt had (see `eval --live-system`).
        let (corpus, _exclusions, _framing) =
            load_packet_corpus(&hx.pool, &e.entity_type, e.entity_id, &sport, &name).await?;
        // No corpus ⇒ the stage writes the NULL-narrative marker without a model call — nothing to score.
        if corpus.is_empty() {
            return Ok(None);
        }
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
            &req, &corpus, None, None, None,
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
            // Citation OR-check (n18): any body names any listed publication, case-insensitive.
            if let Some(srcs) = &x.sources_any {
                let lowered: Vec<String> = items.iter().map(|(_, b, _)| b.to_lowercase()).collect();
                let hit: Vec<&str> = srcs
                    .iter()
                    .filter(|s| {
                        let needle = s.to_lowercase();
                        lowered.iter().any(|b| b.contains(&needle))
                    })
                    .map(|s| s.as_str())
                    .collect();
                checks.push(PropertyCheck {
                    name: format!("sources_any:[{}]", srcs.join("|")),
                    pass: !hit.is_empty(),
                    detail: if hit.is_empty() {
                        "no body cites any listed publication".to_string()
                    } else {
                        format!("cited {}", hit.join(", "))
                    },
                });
            }
            // Edition-budget ceiling (n18): terminal-punctuation runs across all bodies.
            if let Some(max) = x.total_sentences_max {
                let total: i32 = items.iter().map(|(_, b, _)| sentence_runs(b)).sum();
                checks.push(PropertyCheck {
                    name: "total_sentences_le".into(),
                    pass: total <= max,
                    detail: format!("sentences={total} ≤ {max}"),
                });
            }
            // card_score band (one check per bound, mirroring narratives_min/max). A reply with
            // no card_score fails the bound outright — asserting a band asserts presence.
            let score_detail = || match doc.card_score() {
                Some(s) => format!("card_score={s}"),
                None => "card_score=MISSING".to_string(),
            };
            if let Some(min) = x.card_score_min {
                checks.push(PropertyCheck {
                    name: "card_score_ge".into(),
                    pass: doc.card_score().is_some_and(|s| i32::from(s) >= min),
                    detail: format!("{} ≥ {min}", score_detail()),
                });
            }
            if let Some(max) = x.card_score_max {
                checks.push(PropertyCheck {
                    name: "card_score_le".into(),
                    pass: doc.card_score().is_some_and(|s| i32::from(s) <= max),
                    detail: format!("{} ≤ {max}", score_detail()),
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
            crate::junctions::insider::team_relationship(&hx.pool, e.entity_id, player_id, &sport)
                .await?;
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
// RatingTask — stats/analytical rail, identity specificity + prose richness.
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
        // Shape-only parse (NOT `RatingParser`): the gate must see a guard-violating body's
        // prose and score it red on the invariant checks — production's guards would reject it
        // before any check could run. Same lists either way (`crate::guards`).
        let body = crate::junctions::scout::parse_rating_body(raw);
        if body.trim().is_empty() {
            return CaseVerdict {
                parsed: false,
                abs_err: None,
                checks: Vec::new(),
                display: "unparseable".into(),
            };
        }
        let reply = RatingReply { body };
        let mut checks = Vec::new();
        let word_count = reply.body.split_whitespace().count() as i32;

        // Contract-level invariant, asserted whether or not this case carries an `expect` (the
        // momentum no_banned_phrases shape): product names are banned from every brief, not from
        // the fixtures that happened to trip one. Case-sensitive — see PRODUCT_NAME_BANS.
        checks.push(product_name_check(&reply.body));
        // The brief's decoration bans (` · ` bullets, `**`) — folded 08-19 from per-fixture
        // `prose_excludes` entries; same list `RatingParser` rejects on in production.
        let banned = crate::guards::first_banned_phrase(&reply.body, crate::guards::RATING_BODY_BANS);
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
            // s14 gate growth (D-T45): the "emit NO number" rule and the 8-sentence allowance
            // had no check of any kind.
            if x.prose_no_digits == Some(true) {
                let digit = reply.blurb.chars().find(|c| c.is_ascii_digit());
                checks.push(PropertyCheck {
                    name: "prose_no_digits".into(),
                    pass: digit.is_none(),
                    detail: digit.map_or_else(String::new, |d| format!("found digit {d:?}")),
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
use crate::guards::{contains_ci, count_named_peers};
pub use crate::guards::{MOMENTUM_BANNED_PHRASES, PRODUCT_NAME_BANS};

// (sentence_runs folded into `guards::count_sentences` 08-19 — one counter for every prose
// lens; the crude version miscounted decimals as sentence stops.)
fn sentence_runs(text: &str) -> i32 {
    crate::guards::count_sentences(text) as i32
}

/// One shared invariant check over a served-prose field: the first product name found, as a
/// `PropertyCheck` every wired seat pushes unconditionally. For rating the check runs on the
/// parsed BODY only: the structural "PEAK: <label>" marker line is stripped by `RatingParser`
/// and never serves. (The list itself lives in [`crate::guards::PRODUCT_NAME_BANS`] — production
/// enforces the same vocabulary.)
fn product_name_check(prose: &str) -> PropertyCheck {
    let named = crate::guards::first_product_name(prose);
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
                    (
                        &x.resolver_unresolved_include,
                        "unresolved",
                        "resolver_unresolved",
                    ),
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

// ---------------------------------------------------------------------------
// investigator — the prose-triage contract (`ip1`), fixture-driven (D-T46)
// ---------------------------------------------------------------------------

pub struct InvestigatorTask;

#[async_trait]
impl LensTask for InvestigatorTask {
    fn name(&self) -> &'static str {
        "investigator"
    }
    fn role(&self) -> Role {
        Role::Investigator
    }
    fn prompt_version(&self) -> &'static str {
        INVESTIGATOR_PROSE_CONTRACT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> GenerateOptions {
        let mut o = prose_opts();
        o.temperature = Some(temperature);
        o
    }
    /// Fixture-driven on purpose: the production prompt is built from a LIVE Wikipedia
    /// search + summary fetch for a candidate row, which is exactly what a frozen fixture
    /// exists to pin down. Capture new fixtures from `acquisition_runs.query_plan` (the
    /// prose arm records every page it read) rather than re-fetching a moving encyclopedia.
    async fn build_prompt(&self, _hx: &Harness, _e: &EntitySpec) -> Result<Option<String>> {
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
        assert_eq!(rating.operator, "The Scout");
        assert!(rating.mandate.contains("greatest strength"));

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
        assert_eq!(lens_parameters("editor").unwrap().operator, "The Editor");
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
            .evaluate("SCORE: 30\nHOOK: The slide is real\nVIBE: grim", None, Some(&x))
            .all_checks_pass());
        assert!(!VibeTask
            .evaluate("SCORE: 70\nHOOK: The room is up\nVIBE: bright", None, Some(&x))
            .all_checks_pass());
    }

    #[test]
    fn vibe_hook_contract_is_a_global_invariant() {
        // The hook contract is unconditional (08-19): present, ≤12 words, no colon or
        // question mark — the same `guards::hook_violation` rule `VibeParser` enforces.
        // No expect needed: a hook-less (v12-shape) reply fails, a clean three-line passes.
        assert!(VibeTask
            .evaluate("SCORE: 30\nHOOK: The slide is real\nVIBE: grim", None, None)
            .all_checks_pass());
        assert!(!VibeTask
            .evaluate("SCORE: 30\nVIBE: grim", None, None)
            .all_checks_pass());
        assert!(!VibeTask
            .evaluate("SCORE: 30\nHOOK: Breaking: a move\nVIBE: grim", None, None)
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

    // (fold_for_match / contains_ci tests moved to `guards::tests` with the functions.)

    // --- rating / stats-lens rubric ---------------------------------------------

    const RATING_REPLY: &str = "PEAK: Rim protection\nAn elite rim protector who grades at the 94th percentile in blocks and anchors the paint without fouling. The profile is thinner as a creator, but the defensive identity is clear and valuable.";

    #[test]
    fn rating_rubric_scores_peak_specificity_and_prose_richness() {
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
        let v = RatingTask.evaluate(RATING_REPLY, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn rating_product_name_ban_is_case_sensitive_and_body_scoped() {
        // The marker line's own "PEAK:" is stripped by the parser and must not trip the ban;
        // lowercase "peak" is honest English and must not trip it either.
        let clean = "PEAK: Rim protection\nStill at the peak of his powers: an elite rim protector at the 94th percentile in blocks who anchors the paint without fouling, and the defensive identity is clear.";
        let v = RatingTask.evaluate(clean, None, None);
        assert!(
            v.checks.iter().all(|c| c.pass),
            "clean body tripped: {:?}",
            v.checks
        );
        // An echoed product name in the body is exactly what the check exists to catch.
        let echo = "PEAK: Rim protection\nHis PEAK skill is rim protection and the staff must scheme away from it, forcing the ball to the perimeter.";
        let v = RatingTask.evaluate(echo, None, None);
        let ban = v
            .checks
            .iter()
            .find(|c| c.name == "no_product_names")
            .expect("invariant check present");
        assert!(!ban.pass, "echoed PEAK not caught: {:?}", v.checks);
    }

    #[test]
    fn rating_rubric_catches_generic_peak_and_thin_prose() {
        let x = Expect {
            // s19: prose-anchored — the include names a skill the thin body lacks, the
            // exclude names a phrase the thin body contains.
            skill_includes: Some(vec!["Rim protection".into()]),
            skill_excludes: Some(vec!["Average".into()]),
            prose_min_words: Some(20),
            ..Default::default()
        };
        let v = RatingTask.evaluate("PEAK: No standout skill\nAverage profile.", None, Some(&x));
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
        // s11 contract: READ alone. Stray MOMENTUM and SCORE lines (models echoing what
        // every contract through s10 asked for) are tolerated and ignored, never content.
        let raw = "MOMENTUM: rising\nSCORE: 3\nREAD: PEAK is rising while Vibe is steady, so the current direction is modestly positive.";
        let parsed = parse_momentum_reply(raw).unwrap();
        assert!(parsed.blurb.contains("PEAK is rising"));
        assert!(
            !parsed.blurb.contains('3'),
            "SCORE leaked into the blurb: {}",
            parsed.blurb
        );
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
        let raw = "READ: The mood around the club is pulling the profile down despite steadier recent form.";
        let v = MomentumTask.evaluate(raw, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn momentum_product_names_trip_the_invariant() {
        // The s14-era register itself: exactly what the s15 contract inverts.
        let raw = "READ: Vibe is pulling the profile down despite a steadier PEAK read.";
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
                fx.prompt_version,
                EDITOR_CONTRACT_VERSION,
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
        assert!(
            unresolved,
            "no fixture pins resolver discovery (coach shape)"
        );
        assert!(
            never_links,
            "no fixture pins the descriptor/kind gate (place collision)"
        );
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
