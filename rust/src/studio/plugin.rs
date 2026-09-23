//! The Studio plugin contract: manifests, identity, and the runtime seam.
//!
//! **The Studio provides the contained space for cognition; plugins define the cognition
//! that happens inside it.** A plugin is an architectural unit — a statically registered
//! Rust module with a manifest — not a dynamically loaded library. The Studio owns the
//! lifecycle that runs around a plugin's bounded cognitive work; the plugin owns its
//! identity, materials recipe, prompt, parser, validation, and product contract.
//!
//! Cognition receives prepared material. Concrete plugin adapters currently bind a pool
//! and router and prepare that material. They write domain effects through a host-owned,
//! claim-fenced publication transaction. Narrowing adapter dependencies remains incremental;
//! manifest declarations alone do not enforce capability isolation.
//!
//! Task ownership, resource scheduling, and inference-declaration consistency are live
//! contracts. Context and product metadata remain descriptive. See the architecture plan
//! in `docs/plugin-architecture-plan.md` for the remaining boundaries.

use crate::application::queue::work::{Item, Stage};
use crate::runtime::route::Role;
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

/// A versioned product family a plugin consumes or produces. Product relationships are
/// dependency edges between readings — never a pipeline hierarchy. The dependency
/// declarations are descriptive today. Registration does not check graph coverage or
/// cycles, and these declarations do not determine downstream scheduling.
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

/// A declared capability. The web broker checks domain grants on calls routed through
/// it; direct adapter access is not constrained by this enum. Registration checks that
/// inference grants and roles are declared together. World reads and commits remain
/// descriptive until the host supplies scoped handles. Retrieval currently occurs during
/// preparation, before inference, so the input hash is available before the model call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolGrant {
    /// Scoped Postgres reads for context preparation (identity, cards, memories).
    WorldRead,
    /// Product writes; currently enforced by claim-aware application adapters.
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
/// character needs to see. This is descriptive metadata; application preparation code
/// selects the actual materials. There is no context-plan executor.
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

/// Descriptive policy for one plugin: identity, version, tasks, model roles, products,
/// resources, and grants. The manifest contains no executable business logic.
#[derive(Clone, Debug)]
pub struct PluginManifest {
    pub id: PluginId,
    /// Descriptive compatibility label. Prompt and output-schema versions retain their
    /// separate owners and meanings; this field does not drive invalidation.
    pub contract_version: &'static str,
    /// One durable task per registration, matching the worker scheduling contract.
    /// Stage is the existing storage vocabulary; no arbitrary string conversion occurs.
    pub task: Stage,
    /// Declared inference routes. Registration checks the matching inference grant;
    /// adapters still select their routes until scoped inference is introduced.
    pub model_roles: &'static [Role],
    /// Descriptive prepared-material requirements; not executed by the registry.
    pub context_requirements: &'static [ProviderId],
    pub consumes: &'static [ProductKind],
    pub produces: &'static [ProductKind],
    pub resources: ResourceProfile,
    pub tools: &'static [ToolGrant],
}

impl PluginManifest {
    /// True when this manifest covers the given durable work stage.
    pub fn owns_stage(&self, stage: Stage) -> bool {
        self.task == stage
    }

    /// True when this grant covers fetching the given domain class.
    pub fn grants_web(&self, class: DomainClass) -> bool {
        self.tools.iter().any(|g| g.grants_web(class))
    }
}

/// The adapter's report about one exact claim. Host publication receipts establish committed
/// progress or final completion; the worker handles failure and deferral.
///
/// - [`PluginOutcome::Committed`] — products (or their required effects) landed; the
///   exact claim has been completed atomically with its required effects.
/// - [`PluginOutcome::Deferred`] — partial progress landed durably and the work should
///   return to pending for another turn. **A defer without progress is a contract
///   violation**: the worker converts it to the retry ladder rather than granting a
///   free turn.
/// - [`PluginOutcome::Superseded`] — the claim was stale or reclaimed at a publication
///   boundary; that publication was skipped. Earlier progress commits may still exist.
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
/// finishes before each publication transaction; each boundary checks the exact claim.
/// Earlier progress commits survive later errors, which are retried by the worker.
#[async_trait]
pub trait StudioPlugin: Send + Sync {
    fn manifest(&self) -> &'static PluginManifest;

    /// Plugin-owned maintenance that the host runs independently of queue depth.
    /// The host supplies lifecycle and shutdown; each operation owns its work and
    /// cadence policy. Most plugins have no scheduled operations.
    fn scheduled_operations(&self) -> Vec<std::sync::Arc<dyn ScheduledOperation>> {
        Vec::new()
    }

    /// Execute under the exact claim and report the outcome derived from host publication
    /// receipts. Required follow-ups belong in the publication transaction; deferral
    /// decisions are reported, and the worker performs deferral.
    async fn execute(&self, item: &Item) -> Result<PluginOutcome>;
}

/// One plugin-owned scheduled operation. The durable host invokes it immediately
/// at startup and then sleeps for the returned delay. Operations are best-effort:
/// they log their own domain failures and choose the next cadence without affecting
/// claimed work.
#[async_trait]
pub trait ScheduledOperation: Send + Sync {
    /// Stable diagnostic identity; this is not a durable queue key.
    fn name(&self) -> &'static str;

    /// Run one pass and return the delay before the next pass.
    async fn run(&self, cause: &'static str) -> std::time::Duration;
}

/// Boot-time registry of the cognitive fleet. Validates task ownership once, at
/// startup — a mis-wired fleet fails boot clearly instead of surfacing as odd runtime
/// behavior. An empty registry is the valid idle scaffold.
pub struct PluginRegistry {
    plugins: Vec<std::sync::Arc<dyn StudioPlugin>>,
    /// Durable stage string → plugin index.
    by_task: BTreeMap<&'static str, usize>,
}

impl PluginRegistry {
    /// Validate and register. Fails closed on:
    /// - duplicate plugin IDs
    /// - duplicate task ownership
    /// - inference roles without a grant, or an inference grant without roles
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
                manifest.tools.contains(&ToolGrant::Inference) == !manifest.model_roles.is_empty(),
                "plugin registry: {} must declare inference roles and the inference grant together",
                manifest.id
            );
            if let Some(previous) = by_task.insert(manifest.task.as_str(), index) {
                let prior_owner = plugins[previous].manifest().id.as_str();
                anyhow::bail!(
                    "plugin registry: task {} is owned by both {} and {}",
                    manifest.task,
                    prior_owner,
                    manifest.id
                );
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
        let index = self.by_task.get(stage.as_str())?;
        self.plugins.get(*index)
    }

    /// The registered fleet in registration order.
    pub fn plugins(&self) -> &[std::sync::Arc<dyn StudioPlugin>] {
        &self.plugins
    }

    /// The durable task names the registered fleet owns, sorted by name.
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
