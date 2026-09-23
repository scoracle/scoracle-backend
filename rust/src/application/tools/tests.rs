//! Tool broker contract tests: deny-by-default grants, URL-derived Wikimedia routing,
//! per-run budget, and the call ledger. No network.

use super::*;
use crate::application::fleet::{EDITOR, FIXTURE_BOXSCORE, INVESTIGATOR};
use crate::application::tools::{ScopedWeb, ToolLedger, WebBroker};
use crate::evidence::fetch::FetchPolicy;
use crate::studio::plugin::PluginManifest;

fn scope<'a>(
    broker: &'a WebBroker,
    pool: &'a sqlx::PgPool,
    plugin: &'a PluginManifest,
    ledger: &'a ToolLedger,
) -> ScopedWeb<'a> {
    broker.scope(pool, plugin, ledger)
}

fn lazy_pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://localhost/unused")
        .unwrap()
}

/// No cache probe: granted-path tests reach the transport (which refuses on the unroutable
/// host) without ever touching Postgres.
fn no_cache_policy() -> FetchPolicy {
    FetchPolicy::new(std::time::Duration::from_secs(2), std::time::Duration::ZERO)
}

#[test]
fn wikimedia_urls_route_to_the_wikimedia_class() {
    assert_eq!(
        DomainClass::domain_of_url("https://www.wikidata.org/w/api.php?action=wbsearchentities"),
        Some(DomainClass::Wikimedia)
    );
    assert_eq!(
        DomainClass::domain_of_url("https://en.wikipedia.org/wiki/Thing"),
        Some(DomainClass::Wikimedia)
    );
    assert_eq!(
        DomainClass::domain_of_url("https://upload.wikimedia.org/x.png"),
        Some(DomainClass::Wikimedia)
    );
}

#[test]
fn non_wikimedia_urls_are_not_sniffed_into_a_class() {
    // Registered classes (RSS, box scores, curated articles) are attributed by the
    // caller's source registry, not inferred from the URL — a curated host must not
    // masquerade as another class.
    assert_eq!(DomainClass::domain_of_url("https://example.com/feed"), None);
    assert_eq!(
        DomainClass::domain_of_url("https://notwikidata.org/w/api.php"),
        None
    );
    assert_eq!(DomainClass::domain_of_url("not a url"), None);
}

#[tokio::test]
async fn undeclared_domain_is_refused_before_any_reach() {
    let broker = WebBroker::new(4).unwrap();
    let pool = lazy_pool();
    let ledger = ToolLedger::new();
    let web = scope(&broker, &pool, &EDITOR, &ledger);

    let err = web
        .fetch_for_class(
            DomainClass::BoxscoreSources,
            "https://127.0.0.1:9/x",
            &no_cache_policy(),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not granted"), "{err}");
    // Refusal is not a call: the ledger stays empty.
    assert!(ledger.is_empty());
}

#[tokio::test]
async fn granted_class_passes_the_grant_check() {
    // The grant check passes; the lazy pool's fetch fails with a transport error, which
    // proves the call was brokered (and recorded) rather than refused at the door.
    let broker = WebBroker::new(4).unwrap();
    let pool = lazy_pool();
    let ledger = ToolLedger::new();
    let web = scope(&broker, &pool, &FIXTURE_BOXSCORE, &ledger);

    let result = web
        .fetch_for_class(
            DomainClass::BoxscoreSources,
            "https://127.0.0.1:9/x",
            &no_cache_policy(),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(ledger.len(), 1);
    assert_eq!(ledger.calls()[0].tool, "web.fetch");
}

#[tokio::test]
async fn wikimedia_grant_cannot_be_spent_on_other_hosts() {
    let broker = WebBroker::new(4).unwrap();
    let pool = lazy_pool();
    let ledger = ToolLedger::new();
    let web = scope(&broker, &pool, &INVESTIGATOR, &ledger);

    // fetch_wikimedia derives the class from the URL; a non-Wikimedia URL is refused
    // before any reach.
    let err = web
        .fetch_wikimedia("https://example.com/not-wiki", &no_cache_policy())
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("no declared domain class"),
        "{err}"
    );
    assert!(ledger.is_empty());
}

#[tokio::test]
async fn the_run_budget_stops_further_calls() {
    let broker = WebBroker::new(1).unwrap();
    let pool = lazy_pool();
    let ledger = ToolLedger::new();
    let web = scope(&broker, &pool, &FIXTURE_BOXSCORE, &ledger);

    // First call is brokered (fails on the lazy pool's transport, but passes the gate).
    let _ = web
        .fetch_for_class(
            DomainClass::BoxscoreSources,
            "https://127.0.0.1:9/1",
            &no_cache_policy(),
        )
        .await;
    // Second call hits the budget before any reach.
    let err = web
        .fetch_for_class(
            DomainClass::BoxscoreSources,
            "https://127.0.0.1:9/2",
            &no_cache_policy(),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("web-call budget"), "{err}");
    assert_eq!(ledger.len(), 1);
}

#[tokio::test]
async fn zero_budget_means_unbounded() {
    let broker = WebBroker::new(0).unwrap();
    let pool = lazy_pool();
    let ledger = ToolLedger::new();
    let web = scope(&broker, &pool, &FIXTURE_BOXSCORE, &ledger);

    for i in 0..3 {
        let _ = web
            .fetch_for_class(
                DomainClass::BoxscoreSources,
                &format!("https://127.0.0.1:9/{i}"),
                &no_cache_policy(),
            )
            .await;
    }
    assert_eq!(ledger.len(), 3);
}

#[test]
fn refusals_display_the_fix_not_the_failure() {
    let undeclared = ToolRefusal::UndeclaredTool { tool: "web.fetch" };
    assert!(undeclared.to_string().contains("does not declare"));
    let unknown = ToolRefusal::UnknownDomain {
        url: "https://x.test/a".into(),
    };
    assert!(unknown.to_string().contains("no declared domain class"));
}
