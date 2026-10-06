//! Studio's tool vocabulary and refusal policy.
//!
//! A plugin brings its own *tools* — its readers, its source policies, its domain
//! knowledge — but never its own *workspace*. The workspace is the room's: one shared
//! budgeted fetcher with per-domain spacing, circuit breaking, Retry-After holds, and
//! provenance. The broker is the boundary between the two:
//!
//! - **Deny by default.** A plugin may call only the tool classes and domain classes its
//!   manifest declares. An undeclared reach is a hard error, not a warning.
//! - **Preparation-class browser broker.** These fetches currently run before inference.
//!   The model-directed database pilot uses explicit plugin-scoped dispatch; this
//!   browser policy does not implicitly grant it additional tools.
//! - **Recorded.** Every brokered call lands in the run's call ledger with its outcome,
//!   so provider failures stay distinguishable from model failures.
//!
//! Concrete workspace adapters live in `harness::tools`: they receive this policy plus
//! application-owned storage and provider dependencies. Studio itself remains independent of
//! SQL, HTTP, queues, and provider implementations.

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
    /// The registered RSS/news corpus.
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
        let host = reqwest::Url::parse(url).ok()?.host_str()?.to_lowercase();
        // Exact domain or subdomain — `notwikidata.org` is NOT under `wikidata.org`.
        let under = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
        if under("wikidata.org") || under("wikipedia.org") || under("wikimedia.org") {
            return Some(DomainClass::Wikimedia);
        }
        // Box-score and RSS sources are registered in configuration, not guessed from
        // the URL; the workspace callers know their source's class and pass it. URL
        // sniffing beyond Wikimedia would let a curated URL masquerade as another class.
        None
    }
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

use crate::harness::plugin::PluginManifest;
use crate::tools::fetch::{BudgetedFetcher, FetchPolicy, SourceFetch};
use anyhow::{anyhow, Result};
use sqlx::PgPool;
use std::sync::atomic::{AtomicU64, Ordering};

/// The process-wide web workspace. One `BudgetedFetcher` owns domain spacing, circuits,
/// Retry-After holds, caching, and source-document provenance for the entire fleet.
pub struct WebBroker {
    fetcher: BudgetedFetcher,
    /// Per-run ceiling on brokered calls. Zero means unlimited; handler timeout remains the
    /// outer bound.
    max_calls: u32,
}

impl WebBroker {
    pub fn new(max_calls: u32) -> Result<Self> {
        Ok(Self {
            fetcher: BudgetedFetcher::new()?,
            max_calls,
        })
    }

    /// Open one run's web scope. Reach is derived exclusively from the plugin manifest.
    pub fn scope<'a>(&'a self, pool: &'a PgPool, plugin: &'a PluginManifest) -> ScopedWeb<'a> {
        ScopedWeb {
            broker: self,
            pool,
            plugin,
            calls: AtomicU64::new(0),
        }
    }
}

/// One plugin run's access to the shared workspace.
pub struct ScopedWeb<'a> {
    broker: &'a WebBroker,
    pool: &'a PgPool,
    plugin: &'a PluginManifest,
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

    /// Fetch a Wikimedia URL under the shared workspace. The class is derived from its URL,
    /// so a Wikimedia grant cannot be spent on another host.
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

    /// Fetch an application-registered URL class. Source registry attribution is input to this
    /// adapter; the manifest gate remains the authority over whether the plugin may use it.
    pub async fn fetch_for_class(
        &self,
        class: DomainClass,
        url: &str,
        policy: &FetchPolicy,
    ) -> Result<SourceFetch> {
        self.check_grant(class, url)?;
        self.check_budget()?;
        let result = self.broker.fetcher.fetch(self.pool, url, policy).await;
        result.map_err(|e| anyhow!("{e}"))
    }

    /// Fetch one already-curated publisher article for a plugin with this grant.
    /// The provider implementation is intentionally unchanged; this boundary adds the
    /// manifest gate and run budget around it.
    pub async fn fetch_curated_article(
        &self,
        url: &str,
    ) -> Result<crate::tools::fetch::FetchedArticle> {
        self.check_grant(DomainClass::CuratedArticles, url)?;
        self.check_budget()?;
        crate::tools::fetch::fetch_article(url).await
    }
}

#[cfg(test)]
mod tests;
