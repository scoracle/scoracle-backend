//! The Studio tool broker: plugins declare what they may reach; the room decides how the
//! reach happens, records that it happened, and can refuse it.
//!
//! A plugin brings its own *tools* — its readers, its source policies, its domain
//! knowledge — but never its own *workspace*. The workspace is the room's: one shared
//! budgeted fetcher with per-domain spacing, circuit breaking, Retry-After holds, and
//! provenance. The broker is the boundary between the two:
//!
//! - **Deny by default.** A plugin may call only the tool classes and domain classes its
//!   manifest declares. An undeclared reach is a hard error, not a warning.
//! - **Preparation-class.** Every tool in this slice runs *before* inference, inside a
//!   context recipe. A model never chooses to browse mid-read: input hashes stay
//!   computable before the call, and the debounce economy survives.
//! - **Recorded.** Every brokered call lands in the run's call ledger with its outcome,
//!   so provider failures stay distinguishable from model failures.
//!
//! The first consumer is the shared web workspace: [`WebBroker`] wraps the existing
//! `BudgetedFetcher` (which already owns spacing, circuits, and `source_documents`
//! provenance) behind per-plugin domain-class grants. Generic HTTP mechanics live here;
//! endpoint knowledge and response parsing stay with the owning plugin.

use crate::evidence::fetch::{BudgetedFetchError, BudgetedFetcher, FetchPolicy, SourceFetch};
use crate::studio::plugin::PluginManifest;
use anyhow::{anyhow, Result};
use sqlx::PgPool;
use std::sync::atomic::{AtomicU64, Ordering};

/// A class of external domain a plugin may fetch. Deny-by-default: a plugin may fetch
/// only the classes its manifest declares, and a class grants *sources within that
/// class*, never "the web".
///
/// Adding a class is adding a vocabulary entry plus its routing in `domain_of_url`;
/// a class is not a permission to invent endpoints at runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainClass {
    /// Wikipedia/Wikidata API surfaces (`wikipedia.org`, `wikidata.org`).
    Wikimedia,
    /// The registered RSS/news corpus the Editor's funnel curates.
    NewsRss,
    /// The registered box-score sources configured in `boxscore_sources`.
    BoxscoreSources,
    /// Article bodies behind already-curated URLs (the scoped re-read path for the
    /// news characters). Not a license to follow arbitrary links.
    CuratedArticles,
}

impl DomainClass {
    pub fn as_str(self) -> &'static str {
        match self {
            DomainClass::Wikimedia => "wikimedia",
            DomainClass::NewsRss => "news_rss",
            DomainClass::BoxscoreSources => "boxscore_sources",
            DomainClass::CuratedArticles => "curated_articles",
        }
    }

    /// Route a URL to its domain class. `None` means the URL belongs to no declared
    /// class — the broker refuses it regardless of grants.
    pub fn domain_of_url(url: &str) -> Option<DomainClass> {
        let host = host_of(url)?;
        if is_under(host, "wikidata.org")
            || is_under(host, "wikipedia.org")
            || is_under(host, "wikimedia.org")
        {
            return Some(DomainClass::Wikimedia);
        }
        // Box-score and RSS sources are registered in configuration, not guessed from
        // the URL; the workspace callers know their source's class and pass it. URL
        // sniffing beyond Wikimedia would let a curated URL masquerade as another class.
        None
    }
}

/// Exact domain or subdomain — `notwikidata.org` is NOT under `wikidata.org`.
fn is_under(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn host_of(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then_some(host)
}

/// Why the broker refused a call. Distinct from a fetch failure: the reach never
/// happened, and the plugin's manifest is the thing to fix.
#[derive(Debug, PartialEq, Eq)]
pub enum ToolRefusal {
    /// The plugin's manifest does not declare this tool class.
    UndeclaredTool { tool: &'static str },
    /// The URL's domain class is not among the plugin's declared web domains.
    UndeclaredDomain { url: String, class: &'static str },
    /// The URL belongs to no declared domain class at all.
    UnknownDomain { url: String },
}

impl std::fmt::Display for ToolRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolRefusal::UndeclaredTool { tool } => {
                write!(f, "tool broker: plugin does not declare tool '{tool}'")
            }
            ToolRefusal::UndeclaredDomain { url, class } => {
                write!(
                    f,
                    "tool broker: domain class '{class}' not granted for {url}"
                )
            }
            ToolRefusal::UnknownDomain { url } => {
                write!(f, "tool broker: {url} belongs to no declared domain class")
            }
        }
    }
}

/// One recorded tool call. The broker accumulates these per run; provenance keeps
/// provider failures distinguishable from model failures.
#[derive(Clone, Debug)]
pub struct ToolCall {
    pub tool: &'static str,
    pub url: String,
    pub outcome: &'static str,
    pub elapsed_ms: u64,
}

/// The shared web workspace: one `BudgetedFetcher` per process, gated per plugin by
/// its declared domain classes and a per-run call budget.
pub struct WebBroker {
    fetcher: BudgetedFetcher,
    /// Per-run ceiling on brokered calls. Zero means unlimited (the queue's own
    /// handler timeout remains the outer bound).
    max_calls: u32,
}

impl WebBroker {
    pub fn new(max_calls: u32) -> Result<Self> {
        Ok(Self {
            fetcher: BudgetedFetcher::new()?,
            max_calls,
        })
    }

    /// Open the room's scoped web workspace for one plugin run. Refusals are computed
    /// from the plugin manifest; adapters cannot supply a narrower or broader shadow
    /// allowlist. The budget counts exactly this run's calls.
    pub fn scope<'a>(
        &'a self,
        pool: &'a PgPool,
        plugin: &'a PluginManifest,
        ledger: &'a ToolLedger,
    ) -> ScopedWeb<'a> {
        ScopedWeb {
            broker: self,
            pool,
            plugin,
            ledger,
            calls: AtomicU64::new(0),
        }
    }
}

/// Per-run call ledger. The adapter decides what to do with the record (log, attach to
/// the diagnostics payload); the broker only guarantees it is complete.
#[derive(Default)]
pub struct ToolLedger {
    calls: std::sync::Mutex<Vec<ToolCall>>,
}

impl ToolLedger {
    pub fn new() -> Self {
        Self::default()
    }

    fn record(&self, call: ToolCall) {
        self.calls.lock().expect("tool ledger poisoned").push(call);
    }

    pub fn calls(&self) -> Vec<ToolCall> {
        self.calls.lock().expect("tool ledger poisoned").clone()
    }

    pub fn len(&self) -> usize {
        self.calls.lock().expect("tool ledger poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One plugin run's scoped web reach. Holds the plugin's manifest, budget counter, and
/// ledger; every method refuses before it reaches.
pub struct ScopedWeb<'a> {
    broker: &'a WebBroker,
    pool: &'a PgPool,
    plugin: &'a PluginManifest,
    ledger: &'a ToolLedger,
    calls: AtomicU64,
}

impl<'a> ScopedWeb<'a> {
    fn check_grant(&self, class: DomainClass, url: &str) -> Result<()> {
        if !self.plugin.grants_web(class) {
            return Err(anyhow!(
                "{}",
                ToolRefusal::UndeclaredDomain {
                    url: url.to_string(),
                    class: class.as_str(),
                }
            ));
        }
        Ok(())
    }

    fn check_budget(&self) -> Result<()> {
        if self.broker.max_calls > 0 {
            let used = self.calls.fetch_add(1, Ordering::Relaxed);
            if used >= u64::from(self.broker.max_calls) {
                return Err(anyhow!(
                    "tool broker: run exceeded its web-call budget ({} calls)",
                    self.broker.max_calls
                ));
            }
        }
        Ok(())
    }

    /// Fetch a Wikimedia URL under the shared workspace. The class is derived from the
    /// URL itself, so a grant for `Wikimedia` cannot be spent on any other host.
    pub async fn fetch_wikimedia(&self, url: &str, policy: &FetchPolicy) -> Result<SourceFetch> {
        let class = DomainClass::domain_of_url(url).ok_or_else(|| {
            anyhow!(
                "{}",
                ToolRefusal::UnknownDomain {
                    url: url.to_string(),
                }
            )
        })?;
        self.fetch_for_class(class, url, policy).await
    }

    /// Fetch a URL the caller has already attributed to a registered class (box-score
    /// sources, the RSS corpus). Attribution comes from the caller's source registry —
    /// the broker verifies the grant, it does not sniff the URL.
    pub async fn fetch_for_class(
        &self,
        class: DomainClass,
        url: &str,
        policy: &FetchPolicy,
    ) -> Result<SourceFetch> {
        self.check_grant(class, url)?;
        self.check_budget()?;
        let started = std::time::Instant::now();
        let result = self.broker.fetcher.fetch(self.pool, url, policy).await;
        let elapsed = started.elapsed();
        let outcome = match &result {
            Ok(f) => {
                if f.from_cache {
                    "cache_hit"
                } else {
                    "fetched"
                }
            }
            Err(BudgetedFetchError::DomainSkipped { .. }) => "domain_skipped",
            Err(BudgetedFetchError::Http { .. }) => "http_rejected",
            Err(BudgetedFetchError::Other(_)) => "transport_failed",
        };
        self.ledger.record(ToolCall {
            tool: "web.fetch",
            url: url.to_string(),
            outcome,
            elapsed_ms: elapsed.as_millis() as u64,
        });
        result.map_err(|e| anyhow!("{e}"))
    }
}

#[cfg(test)]
mod tests;
