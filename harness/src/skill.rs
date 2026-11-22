//! Harness → plugins → skills. Plugins own capability families; skills own bounded jobs.
//! The harness schedules each skill independently and enforces its declared capabilities.
//! Domain publication stays in skills, through host-owned claim-fenced transactions.

use crate::harness::queue::work::{ClaimPolicy, Item, TaskKey};
use crate::harness::route::RouteKey;
use crate::harness::tools::DomainClass;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Strip the skill file's YAML header at compile time so only the body enters
/// model inputs and their provenance hashes.
pub(crate) const fn instructions(document: &'static str) -> &'static str {
    let [b'-', b'-', b'-', b'\n', rest @ ..] = document.as_bytes() else {
        panic!("SKILL.md must start with YAML frontmatter");
    };
    let mut rest = rest;
    loop {
        match rest {
            [b'\n', b'-', b'-', b'-', b'\n', body @ ..] => {
                let body = document
                    .split_at(document.len() - body.len())
                    .1
                    .trim_ascii();
                assert!(!body.is_empty(), "SKILL.md needs instructions");
                return body;
            }
            [_, tail @ ..] => rest = tail,
            [] => panic!("SKILL.md has unclosed frontmatter"),
        }
    }
}

/// Stable skill identity. Existing identities are retained for historical provenance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SkillId(&'static str);

impl SkillId {
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for SkillId {
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

/// How many claimed items a skill may hold at once, and which shared backend slot
/// group (if any) bounds it. This is live admission policy read by the Studio scheduler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceProfile {
    /// Ceiling on this skill's in-flight claims. Zero is treated as one downstream.
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

/// Live registration policy for one skill: identity, durable task ownership, claim policy,
/// inference routes, resources, and grants. The manifest contains no business logic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillManifest {
    pub plugin_id: &'static str,
    pub id: SkillId,
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

impl SkillManifest {
    /// True when this grant covers fetching the given domain class.
    pub fn grants_web(&self, class: DomainClass) -> bool {
        self.tools.iter().any(|g| g.grants_web(class))
    }
}

/// The adapter's report about one exact claim. Host publication receipts establish committed
/// progress or final completion; the worker handles failure and deferral.
///
/// - [`SkillOutcome::Committed`] — products (or their required effects) landed; the
///   exact claim has been completed atomically with its required effects.
/// - [`SkillOutcome::Deferred`] — partial progress landed durably and the work should
///   return to pending for another turn. **A defer without progress is a contract
///   violation**: the worker converts it to the retry ladder rather than granting a
///   free turn.
/// - [`SkillOutcome::Superseded`] — the claim was stale or reclaimed at a publication
///   boundary; that publication was skipped. Earlier progress commits may still exist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillOutcome {
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

impl SkillOutcome {
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
pub trait Skill: Send + Sync {
    fn manifest(&self) -> &'static SkillManifest;

    /// Skill-owned maintenance that the host runs independently of queue depth.
    /// The host supplies lifecycle and shutdown; each operation owns its work and
    /// cadence policy. Most skills have no scheduled operations.
    fn scheduled_operations(&self) -> Vec<std::sync::Arc<dyn ScheduledOperation>> {
        Vec::new()
    }

    /// Execute under the exact claim and report the outcome derived from host publication
    /// receipts. Required follow-ups belong in the publication transaction; deferral
    /// decisions are reported, and the worker performs deferral.
    async fn execute(&self, item: &Item) -> Result<SkillOutcome>;
}

/// One skill-owned scheduled operation. The durable host invokes it immediately
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

/// Statically linked capability family and its declared skill contracts.
#[derive(Debug)]
pub struct PluginManifest {
    pub id: &'static str,
    pub skills: &'static [&'static SkillManifest],
}

/// One plugin's enabled skills, with dependencies bound by its composition code.
pub struct Plugin {
    pub manifest: &'static PluginManifest,
    pub skills: Vec<std::sync::Arc<dyn Skill>>,
}

/// Validated enabled fleet. Flatten once at boot so plugins do not add a scheduling layer.
pub struct PluginRegistry {
    plugins: Vec<Plugin>,
    skills: Vec<std::sync::Arc<dyn Skill>>,
}

impl PluginRegistry {
    pub fn new(plugins: Vec<Plugin>) -> Result<Self> {
        let mut plugin_ids = BTreeSet::new();
        let mut skill_ids = BTreeSet::new();
        let mut tasks = BTreeMap::new();
        let mut groups = BTreeMap::new();
        let mut skills = Vec::new();
        for plugin in &plugins {
            let owner = plugin.manifest;
            anyhow::ensure!(!owner.id.is_empty(), "plugin registry: empty plugin id");
            anyhow::ensure!(
                plugin_ids.insert(owner.id),
                "duplicate plugin id {}",
                owner.id
            );
            for manifest in owner.skills {
                anyhow::ensure!(
                    manifest.plugin_id == owner.id,
                    "skill {} has wrong plugin owner",
                    manifest.id
                );
                anyhow::ensure!(
                    !manifest.id.as_str().is_empty(),
                    "{} has an empty skill id",
                    owner.id
                );
                anyhow::ensure!(
                    skill_ids.insert(manifest.id),
                    "duplicate skill id {}",
                    manifest.id
                );
                anyhow::ensure!(
                    !manifest.task.as_str().is_empty(),
                    "{} has an empty task key",
                    manifest.id
                );
                anyhow::ensure!(
                    manifest.tools.contains(&ToolGrant::Inference)
                        != manifest.inference_routes.is_empty(),
                    "{} must declare inference roles and the inference grant together",
                    manifest.id
                );
                if let Some(previous) = tasks.insert(manifest.task, manifest.id) {
                    anyhow::bail!(
                        "task {} is owned by both {} and {}",
                        manifest.task,
                        previous,
                        manifest.id
                    );
                }
                if let Some((group, budget)) = manifest.resources.slot_group {
                    anyhow::ensure!(budget > 0, "slot group {group} must have a positive budget");
                    if let Some(previous) = groups.insert(group, budget) {
                        anyhow::ensure!(
                            previous == budget,
                            "inconsistent budget for slot group {group}"
                        );
                    }
                }
            }
            let mut enabled = BTreeSet::new();
            for skill in &plugin.skills {
                let manifest = skill.manifest();
                anyhow::ensure!(
                    owner.skills.contains(&manifest),
                    "undeclared or altered skill {} for {}",
                    manifest.id,
                    owner.id
                );
                anyhow::ensure!(
                    enabled.insert(manifest.id),
                    "duplicate enabled skill {}",
                    manifest.id
                );
                skills.push(skill.clone());
            }
        }
        Ok(Self { plugins, skills })
    }

    pub fn plugins(&self) -> &[Plugin] {
        &self.plugins
    }

    /// Independently scheduled skills, in plugin and skill registration order.
    pub fn skills(&self) -> &[std::sync::Arc<dyn Skill>] {
        &self.skills
    }
}

#[cfg(test)]
pub(crate) mod tests;
