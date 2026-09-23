//! The Studio plugin contract: manifests, identity, and the runtime seam.
//!
//! **The Studio provides the contained space for cognition; plugins define the cognition
//! that happens inside it.** A plugin is an architectural unit — a statically registered
//! Rust module with a manifest — not a dynamically loaded library. The Studio owns the
//! lifecycle that runs around a plugin's bounded cognitive work; the plugin owns its
//! identity, materials recipe, prompt, parser, validation, and product contract.
//!
//! Cognition receives prepared material and resolved inference handles. Concrete plugin
//! adapters own typed preparation over application-supplied read/provider capabilities.
//! They write domain effects through a host-owned, claim-fenced publication transaction.
//!
//! Task ownership, claim policy, resource scheduling, inference declarations, and web grants
//! are live contracts. Preparation and product relationships stay in typed plugin code instead
//! of an unused declarative graph.

use crate::application::queue::work::{ClaimPolicy, Item, TaskKey};
use crate::runtime::route::RouteKey;
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

/// A declared capability. Concrete inference and provider handles are constructed from
/// these declarations; undeclared routes/domains are absent or refused. World reads and
/// commits remain bound by typed plugin adapters and the claim-aware publication host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolGrant {
    /// Budgeted external HTTP retrieval through the shared web workspace, restricted
    /// to the declared domain classes.
    WebFetch(&'static [DomainClass]),
    /// Model inference through the Studio's routed inference broker.
    Inference,
}

impl ToolGrant {
    /// True when this grant covers fetching the given domain class.
    pub fn grants_web(&self, class: DomainClass) -> bool {
        matches!(self, ToolGrant::WebFetch(domains) if domains.contains(&class))
    }
}

/// How many claimed items a plugin may hold at once, and which shared backend slot
/// group (if any) bounds it. This is live admission policy read by the Studio scheduler.
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

/// Live registration policy for one plugin: identity, durable task ownership, claim policy,
/// inference routes, resources, and grants. The manifest contains no business logic.
#[derive(Clone, Debug)]
pub struct PluginManifest {
    pub id: PluginId,
    /// One durable task per registration, matching the worker scheduling contract.
    /// TaskKey is the existing storage vocabulary; no arbitrary string conversion occurs.
    pub task: TaskKey,
    /// Database-level claim ordering and dependency eligibility owned by this task.
    pub claim_policy: ClaimPolicy,
    /// Declared inference routes. Registration checks the matching inference grant and
    /// the composition root resolves only these handles for the plugin.
    pub inference_routes: &'static [RouteKey],
    pub resources: ResourceProfile,
    pub tools: &'static [ToolGrant],
}

impl PluginManifest {
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
                !manifest.task.as_str().is_empty(),
                "plugin registry: {} has an empty task key",
                manifest.id
            );
            anyhow::ensure!(
                seen_ids.insert(manifest.id.as_str(), index).is_none(),
                "plugin registry: duplicate plugin id {}",
                manifest.id
            );
            anyhow::ensure!(
                manifest.tools.contains(&ToolGrant::Inference)
                    != manifest.inference_routes.is_empty(),
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
    pub fn resolve(&self, stage: TaskKey) -> Option<&std::sync::Arc<dyn StudioPlugin>> {
        let index = self.by_task.get(stage.as_str())?;
        self.plugins.get(*index)
    }

    /// The registered fleet in registration order.
    pub fn plugins(&self) -> &[std::sync::Arc<dyn StudioPlugin>] {
        &self.plugins
    }
}

#[cfg(test)]
mod tests;
