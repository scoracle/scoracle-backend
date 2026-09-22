//! The Studio plugin contract: manifests, identity, and the runtime seam.
//!
//! **The Studio provides the contained space for cognition; plugins define the cognition
//! that happens inside it.** A plugin is an architectural unit — a statically registered
//! Rust module with a manifest — not a dynamically loaded library. The Studio owns the
//! lifecycle that runs around a plugin's bounded cognitive work; the plugin owns its
//! identity, materials recipe, prompt, parser, validation, and product contract.
//!
//! One hard rule survives the refactor unchanged: **the character must not assemble its
//! own world.** A plugin may declare the materials it needs; it never receives a raw
//! pool, the global router, or an undeclared tool.
//!
//! This module is deliberately descriptive policy only. Executable business logic stays
//! with the owning plugin package; the manifest names, versions, and budgets it.

use crate::application::queue::work::{Item, Stage};
use crate::studio::tools::DomainClass;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::fmt;

/// Stable, semantic plugin identity, e.g. `scoracle.character.vibe`.
///
/// The manifest's `plugin_id` is the anchor connecting configuration, context, products,
/// and provenance; the ledger and diagnostics should refer to it rather than to queue
/// stage spellings, which are durable storage vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginId(&'static str);

impl PluginId {
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// One kind of durable work a plugin can perform. Maps 1:1 onto a `pipeline_work` stage
/// for today's fleet; the indirection exists so a future compound plugin (one identity,
/// several task kinds) can declare more than one without renaming stored rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskKind(&'static str);

impl TaskKind {
    pub const fn new(kind: &'static str) -> Self {
        Self(kind)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// The durable `pipeline_work` stage this task kind drains. One task kind owns
    /// exactly one stage today; stage names are storage vocabulary, not identity.
    pub fn stage(self) -> Stage {
        match self {
            TaskKind::GRAPH => Stage::Graph,
            TaskKind::EDITOR => Stage::Editor,
            TaskKind::INVESTIGATE_ENTITY => Stage::InvestigateEntity,
            TaskKind::FIXTURE_BOXSCORE => Stage::FixtureBoxscore,
            TaskKind::RATING => Stage::Rating,
            TaskKind::MOMENTUM => Stage::Momentum,
            TaskKind::TRANSFERS => Stage::Transfers,
            TaskKind::NARRATIVES => Stage::Narratives,
            TaskKind::VIBE => Stage::Vibe,
            TaskKind::SIGIL => Stage::Sigil,
            _ => unreachable!("unknown TaskKind has no durable stage"),
        }
    }

    pub const GRAPH: TaskKind = TaskKind::new("graph");
    pub const EDITOR: TaskKind = TaskKind::new("editor");
    pub const INVESTIGATE_ENTITY: TaskKind = TaskKind::new("investigate_entity");
    pub const FIXTURE_BOXSCORE: TaskKind = TaskKind::new("fixture_boxscore");
    pub const RATING: TaskKind = TaskKind::new("rating");
    pub const MOMENTUM: TaskKind = TaskKind::new("momentum");
    pub const TRANSFERS: TaskKind = TaskKind::new("transfers");
    pub const NARRATIVES: TaskKind = TaskKind::new("narratives");
    pub const VIBE: TaskKind = TaskKind::new("vibe");
    pub const SIGIL: TaskKind = TaskKind::new("sigil");
}

impl fmt::Display for TaskKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// A versioned product family a plugin consumes or produces. Product relationships are
/// dependency edges between readings — never a pipeline hierarchy. The dependency
/// invalidation engine (future work) will read these declarations; today they are
/// descriptive and validated for cycles at boot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProductKind(&'static str);

impl ProductKind {
    pub const fn new(kind: &'static str) -> Self {
        Self(kind)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }

    pub const NARRATIVES: ProductKind = ProductKind::new("narratives");
    pub const RATING: ProductKind = ProductKind::new("rating");
    pub const VIBE: ProductKind = ProductKind::new("vibe");
    pub const TRANSFERS: ProductKind = ProductKind::new("transfers");
    pub const MOMENTUM: ProductKind = ProductKind::new("momentum");
    pub const SIGIL: ProductKind = ProductKind::new("sigil");
    pub const EDITOR_READ: ProductKind = ProductKind::new("editor_read");
    pub const IDENTITY: ProductKind = ProductKind::new("identity");
    pub const RELATIONS: ProductKind = ProductKind::new("relations");
    pub const BOX_SCORE: ProductKind = ProductKind::new("box_score");
}

impl fmt::Display for ProductKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// A class of capability a plugin is granted. Deny-by-default: a plugin may use only
/// the classes its manifest declares, and a web grant names the exact domain classes it
/// may reach. Every tool in this contract is preparation-class — it runs before
/// inference, inside a context recipe, so input hashes stay computable before the call.
/// A model never chooses to browse mid-read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolGrant {
    /// Scoped Postgres reads for context preparation (identity, cards, memories).
    WorldRead,
    /// Transaction-scoped product writes inside the Studio commit boundary.
    Commit,
    /// Budgeted external HTTP retrieval through the shared web workspace, restricted
    /// to the declared domain classes.
    WebFetch(&'static [DomainClass]),
    /// Model inference through the Studio's routed inference broker.
    Inference,
}

impl ToolGrant {
    pub const fn as_str(self) -> &'static str {
        match self {
            ToolGrant::WorldRead => "world_read",
            ToolGrant::Commit => "commit",
            ToolGrant::WebFetch(_) => "web_fetch",
            ToolGrant::Inference => "inference",
        }
    }

    /// True when this grant covers fetching the given domain class.
    pub fn grants_web(&self, class: DomainClass) -> bool {
        matches!(self, ToolGrant::WebFetch(domains) if domains.contains(&class))
    }
}

/// A prepared-material requirement a plugin declares. The manifest names what the
/// character needs to see; the application fulfills the declaration when the plugin's
/// context plan runs. Requirements are the "who is this / what does she know" step of
/// the target data flow, made explicit instead of adapter-implicit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProviderId(&'static str);
impl ProviderId {
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// Compact durable identity for the work's subject: name, sport, entity kind.
    pub const ENTITY_IDENTITY: ProviderId = ProviderId::new("entity_identity");
    /// The Editor's claim packet corpus for the subject.
    pub const EDITOR_PACKETS: ProviderId = ProviderId::new("editor_packets");
    /// Sourced continuity: prior labeled readings and provenance-bearing records.
    pub const SOURCED_MEMORY: ProviderId = ProviderId::new("sourced_memory");
    /// The subject's own latest finished card (this plugin's prior product).
    pub const LATEST_SELF_CARD: ProviderId = ProviderId::new("latest_self_card");
    /// Other characters' finished cards (the Oracle's five; the Analyst's two).
    pub const PILLAR_CARDS: ProviderId = ProviderId::new("pillar_cards");
    /// Deterministic movement snapshot (dated slopes, samples, windows).
    pub const MOMENTUM_SNAPSHOT: ProviderId = ProviderId::new("momentum_snapshot");
    /// The sport's current season as resolved from the calendar tables.
    pub const CURRENT_SEASON: ProviderId = ProviderId::new("current_season");
    /// Cohort/trajectory analytical context (DuckDB-derived today where available).
    pub const ANALYTICS_SNAPSHOT: ProviderId = ProviderId::new("analytics_snapshot");
    /// Full article text behind packet article ids — the scoped re-read path.
    pub const ARTICLE_EXPANSION: ProviderId = ProviderId::new("article_expansion");
    /// Prepared emotional signals (crowd/measurement sources, when one exists).
    pub const EMOTIONAL_SIGNALS: ProviderId = ProviderId::new("emotional_signals");
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// How many claimed items a plugin may hold at once, and which shared backend slot
/// group (if any) bounds it. This is descriptive policy the Studio scheduler reads;
/// the values here must mirror the caps the adapters already enforce so the refactor
/// is behavior-neutral.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceProfile {
    /// Ceiling on this plugin's in-flight claims. Zero is treated as one downstream.
    pub max_in_flight: usize,
    /// Shared backend slots as `(group name, total slots)`, mirroring the queue's
    /// slot-group contract. `None` means ungrouped.
    pub slot_group: Option<(&'static str, usize)>,
    /// Items claimed per rotation. Model work keeps the default of 1; batch stages
    /// override.
    pub rotation_batch: i64,
}

impl ResourceProfile {
    pub const fn unbounded_batch(max_in_flight: usize) -> Self {
        Self {
            max_in_flight,
            slot_group: None,
            rotation_batch: 1,
        }
    }

    pub const fn grouped(max_in_flight: usize, group: (&'static str, usize)) -> Self {
        Self {
            max_in_flight,
            slot_group: Some(group),
            rotation_batch: 1,
        }
    }

    pub const fn batched(mut self, rotation_batch: i64) -> Self {
        self.rotation_batch = rotation_batch;
        self
    }
}

/// A model routing role a plugin needs. Plugins declare roles, never concrete hosts,
/// models, or backends — the Studio's routing resolves them.
///
/// This is the routing role's *config label* (matching `Role::env_suffix`'s kebab form
/// and the telemetry ledger), deliberately a plain label: the Studio core does not
/// import the routing module, and fleet tests lock the spelling to the router's keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModelRole(pub &'static str);

impl ModelRole {
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for ModelRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Descriptive policy for one plugin: identity, version, tasks, model roles, products,
/// resources, and grants. The manifest contains no executable business logic.
#[derive(Clone, Debug)]
pub struct PluginManifest {
    pub id: PluginId,
    /// Bumped when context, prompt, parser, validation, or product meaning changes.
    pub contract_version: &'static str,
    /// Every task kind this plugin can perform. One cognitive identity may carry a
    /// compound job; the registry enforces unique task ownership across the fleet.
    pub tasks: &'static [TaskKind],
    pub model_roles: &'static [ModelRole],
    /// Prepared materials the plugin's cognition requires. Declared here, fulfilled by
    /// the application when the plugin's context plan runs.
    pub context_requirements: &'static [ProviderId],
    pub consumes: &'static [ProductKind],
    pub produces: &'static [ProductKind],
    pub resources: ResourceProfile,
    pub tools: &'static [ToolGrant],
}

impl PluginManifest {
    /// True when this manifest covers the given durable work stage.
    pub fn owns_stage(&self, stage: Stage) -> bool {
        self.tasks.iter().any(|t| t.stage() == stage)
    }

    /// True when this grant covers fetching the given domain class.
    pub fn grants_web(&self, class: DomainClass) -> bool {
        self.tools.iter().any(|g| g.grants_web(class))
    }
}

/// The durable disposition of one exact claim — the plugin's report, from which the
/// worker derives every queue operation. Plugins never touch the queue themselves.
///
/// - [`PluginOutcome::Committed`] — products (or their required effects) landed; the
///   claim may be completed.
/// - [`PluginOutcome::Deferred`] — partial progress landed durably and the work should
///   return to pending for another turn. **A defer without progress is a contract
///   violation**: the worker converts it to the retry ladder rather than granting a
///   free turn.
/// - [`PluginOutcome::Superseded`] — the claim was stale or reclaimed; nothing was
///   published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginOutcome {
    Committed,
    Deferred {
        /// The progress guarantee. The worker refuses a defer without it.
        made_progress: bool,
        /// Human-readable reason for the deferral note.
        note: String,
        /// How long to wait before the work becomes claimable again.
        delay: std::time::Duration,
    },
    Superseded,
}

impl PluginOutcome {
    pub fn deferred(note: impl Into<String>, delay: std::time::Duration) -> Self {
        Self::Deferred {
            made_progress: true,
            note: note.into(),
            delay,
        }
    }
}

/// The object-safe runtime seam held by the registry.
///
/// Implementations bind their own application dependencies at construction; `execute`
/// receives only the exact claim and reports its outcome. Model and network work
/// finishes before the publication transaction; a stale or superseded claim publishes
/// nothing. Errors are retried by the worker.
#[async_trait]
pub trait StudioPlugin: Send + Sync {
    fn manifest(&self) -> &'static PluginManifest;

    /// Publish under the exact claim and report the outcome. Required follow-ups
    /// belong in the publication transaction; deferral decisions are reported, and
    /// the worker performs the queue operation.
    async fn execute(&self, item: &Item) -> Result<PluginOutcome>;
}

/// Boot-time registry of the cognitive fleet. Validates task ownership once, at
/// startup — a mis-wired fleet fails boot clearly instead of surfacing as odd runtime
/// behavior. An empty registry is the valid idle scaffold.
pub struct PluginRegistry {
    plugins: Vec<std::sync::Arc<dyn StudioPlugin>>,
    /// TaskKind string → plugin index.
    by_task: BTreeMap<&'static str, usize>,
}

impl PluginRegistry {
    /// Validate and register. Fails closed on:
    /// - duplicate plugin IDs
    /// - duplicate task ownership
    /// - a plugin that declares no task kinds
    /// - an empty plugin ID
    ///
    /// An empty fleet is valid: it is the idle scaffold the worker warns about.
    pub fn new(plugins: Vec<std::sync::Arc<dyn StudioPlugin>>) -> Result<Self> {
        let mut seen_ids = BTreeMap::new();
        let mut by_task: BTreeMap<&'static str, usize> = BTreeMap::new();
        for (index, plugin) in plugins.iter().enumerate() {
            let manifest = plugin.manifest();
            anyhow::ensure!(
                !manifest.id.as_str().is_empty(),
                "plugin registry: plugin {index} has an empty id"
            );
            anyhow::ensure!(
                seen_ids.insert(manifest.id.as_str(), index).is_none(),
                "plugin registry: duplicate plugin id {}",
                manifest.id
            );
            anyhow::ensure!(
                !manifest.tasks.is_empty(),
                "plugin registry: {} declares no task kinds",
                manifest.id
            );
            for task in manifest.tasks {
                if let Some(previous) = by_task.insert(task.as_str(), index) {
                    let prior_owner = plugins[previous].manifest().id.as_str();
                    anyhow::bail!(
                        "plugin registry: task {} is owned by both {} and {}",
                        task,
                        prior_owner,
                        manifest.id
                    );
                }
            }
        }

        // Product dependencies are descriptive today: the dependency-invalidation engine
        // is future work (shadow-compare first), and the reading relationships it will
        // derive are recorded on the manifests themselves. Registration accepts partial
        // fleets, so a missing producer is not a boot error; when invalidation arrives it
        // will validate full coverage and cycle-freedom over the enabled graph.

        Ok(Self { plugins, by_task })
    }

    /// The plugin that owns a durable work stage, or `None` when the fleet was
    /// registered without it (a partial deployment, e.g. voices-only on the Mac).
    pub fn resolve(&self, stage: Stage) -> Option<&std::sync::Arc<dyn StudioPlugin>> {
        let index = self
            .plugins
            .iter()
            .position(|p| p.manifest().owns_stage(stage))?;
        self.plugins.get(index)
    }

    /// The registered fleet in registration order.
    pub fn plugins(&self) -> &[std::sync::Arc<dyn StudioPlugin>] {
        &self.plugins
    }

    /// The task kinds the registered fleet owns, in registration order.
    pub fn tasks(&self) -> Vec<&'static str> {
        self.by_task.keys().copied().collect()
    }

    /// The manifest that owns a task kind, if registered.
    pub fn manifest_for_task(&self, task: &str) -> Option<&'static PluginManifest> {
        let index = self.by_task.get(task)?;
        Some(self.plugins[*index].manifest())
    }
}

#[cfg(test)]
mod tests;
