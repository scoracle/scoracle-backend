//! topology_probe — print (and optionally ping) the model topology the env resolves to.
//!
//! The measured check BEFORE wiring, per house culture. Reads the same `COGNITION_ROUTE_*` /
//! `COGNITION_BACKEND_CONCURRENCY` env the service reads and prints the resolved role → model@host
//! table plus each host's concurrency budget. Touches NO database and claims NO work, so it is
//! safe to run against production while the service is up.
//!
//!   set -a && source .env.local && set +a
//!   cargo run --example topology_probe          # resolve and print
//!   cargo run --example topology_probe -- --ping  # also check every host answers
//!
//! `--ping` is the one that matters for the Archbox/Mac split: the likeliest failure mode is the
//! remote box being asleep, and this turns that into one line here rather than an hour of failed
//! items later.

use anyhow::Result;
use scoracle_cognition::config::RouteConfig;
use scoracle_cognition::ollama::OllamaClient;
use scoracle_cognition::route::Role;
use std::collections::BTreeMap;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    let ping = std::env::args().any(|a| a == "--ping");
    let base = std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".into());
    let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "mistral:7b".into());
    let default_permits: usize = std::env::var("OLLAMA_MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1);
    let timeout = Duration::from_secs(
        std::env::var("OLLAMA_TIMEOUT_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60),
    );

    let cfg = RouteConfig::from_env(&model, &base);

    // Group roles by host so the split reads as "what runs where", not a flat list.
    let mut by_host: BTreeMap<&str, Vec<(Role, &str)>> = BTreeMap::new();
    for role in Role::all() {
        let spec = &cfg.roles[&role];
        by_host
            .entry(spec.base_url.as_str())
            .or_default()
            .push((role, spec.model.as_str()));
    }

    println!("default model : {model}");
    println!("default host  : {base}");
    println!("hosts         : {}", by_host.len());
    if by_host.len() == 1 {
        println!("                (single-host deploy — no topology split configured)");
    }
    println!();

    for (host, mut roles) in by_host {
        let permits = cfg
            .backend_concurrency
            .get(host)
            .copied()
            .unwrap_or(default_permits);
        let budget_src = if cfg.backend_concurrency.contains_key(host) {
            "COGNITION_BACKEND_CONCURRENCY"
        } else {
            "OLLAMA_MAX_CONCURRENT (default)"
        };
        print!("HOST {host}   max_concurrent={permits}  [{budget_src}]");
        if ping {
            let client = OllamaClient::new(host, &model, timeout)?;
            match client.ping().await {
                Ok(()) => print!("  ✓ reachable"),
                Err(e) => print!("  ✗ UNREACHABLE: {e}"),
            }
        }
        println!();

        roles.sort_by_key(|(r, _)| r.as_str());
        // Distinct models on one host are what force eviction when they do not co-fit; call it
        // out, since one-model-per-machine is the whole point of the split.
        let mut models: Vec<&str> = roles.iter().map(|(_, m)| *m).collect();
        models.sort_unstable();
        models.dedup();
        for (role, m) in &roles {
            println!("     {:<16} {}", role.as_str(), m);
        }
        if models.len() > 1 {
            println!(
                "     ⚠ {} distinct models on this host ({}) — they will contend for its memory",
                models.len(),
                models.join(", ")
            );
        }
        println!();
    }

    let challengers: Vec<String> = Role::all()
        .into_iter()
        .filter_map(|r| {
            cfg.candidates
                .get(&r)
                .map(|s| format!("{}={}@{}", r.as_str(), s.model, s.base_url))
        })
        .collect();
    if !challengers.is_empty() {
        println!("eval challengers (never served): {}", challengers.join(" "));
    }
    Ok(())
}
