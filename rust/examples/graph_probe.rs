//! graph_probe — the measured probe for the typed narrative extraction primitive
//! (Plan - Narrative Graph, build order step 1: eyeball extraction quality on real
//! articles BEFORE wiring any stage). Pulls recent scrub-vetted articles with ≥2
//! vetted entities, runs the closed-candidate-list extraction, and prints every
//! relation/person the parser accepts — plus fail-closed and timing counts.
//!
//! SAFETY: read-only. Never claims pipeline_work, never writes any table.
//!
//! Usage (env from .env.local):
//!   cargo run --release --example graph_probe            # default 12 articles
//!   GRAPH_PROBE_ARTICLES=20 GRAPH_PROBE_SPORT=FOOTBALL \
//!   cargo run --release --example graph_probe

use anyhow::{Context, Result};
use scoracle_cognition::config::Config;
use scoracle_cognition::db;
use scoracle_cognition::graph::{
    build_graph_prompt, graph_opts, load_graph_article_context, GraphParser,
    GRAPH_PROMPT_VERSION,
};
use scoracle_cognition::harness::Harness;
use scoracle_cognition::ollama::OllamaClient;
use scoracle_cognition::route::{Role, Router};
use sqlx::Row;

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;

    let ollama = OllamaClient::new(&cfg.ollama_base_url, &cfg.ollama_model, cfg.ollama_timeout)?;
    ollama
        .ping()
        .await
        .context("ollama must be reachable for the graph probe")?;

    let harness = Harness {
        pool,
        router: Router::from_config(&cfg.route, cfg.ollama_timeout, 1)?,
        embedder: None,
        resolve: cfg.resolve.clone(),
        scrub: cfg.scrub.clone(),
    };

    let n_articles: i64 = std::env::var("GRAPH_PROBE_ARTICLES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12);
    let sport = std::env::var("GRAPH_PROBE_SPORT").unwrap_or_else(|_| "FOOTBALL".to_string());

    // Recent vetted articles with ≥2 vetted entities — the graph-worthy corpus.
    let rows = sqlx::query(
        r#"
        SELECT a.id, a.source, a.published_at::date::text, a.title,
               COALESCE(a.description, '')
        FROM news_articles a
        WHERE EXISTS (
            SELECT 1 FROM news_article_entities e
            WHERE e.article_id = a.id AND e.sport = $1 AND e.vetted IS TRUE
            HAVING count(*) >= 2
        )
        ORDER BY a.published_at DESC
        LIMIT $2
        "#,
    )
    .bind(&sport)
    .bind(n_articles)
    .fetch_all(&harness.pool)
    .await?;

    println!(
        "graph_probe — prompt_version={GRAPH_PROMPT_VERSION}, sport={sport}, {} article(s)\n",
        rows.len()
    );

    let (mut ok, mut failed_closed, mut total_rel, mut total_per, mut total_ms) = (0, 0, 0, 0, 0);
    for row in rows {
        let article_id: i64 = row.get(0);
        let source: Option<String> = row.get(1);
        let published: String = row.get(2);
        let title: String = row.get(3);
        let description: String = row.get(4);

        // Shared deterministic prefix (single-homed in graph.rs since the eval lens +
        // stage handler landed): article meta + vetted candidates with identity cards.
        let Some((_article, candidates)) =
            load_graph_article_context(&harness.pool, article_id, &sport).await?
        else {
            println!("── article {article_id} — no vetted candidates; skipped\n");
            continue;
        };

        let prompt = build_graph_prompt(
            source.as_deref().unwrap_or("unknown"),
            &published,
            &title,
            &description,
            &candidates,
        );
        let parser = GraphParser {
            candidates: &candidates,
        };
        let extracted = harness
            .extract(Role::EmotionalNews, &prompt, &graph_opts(), &parser)
            .await?;
        total_ms += extracted.wall_ms;

        println!("── article {article_id} [{published}] {title}");
        for c in &candidates {
            println!("   entity: {}", c.descriptor);
        }
        match extracted.value {
            None => {
                failed_closed += 1;
                println!("   ✗ FAIL-CLOSED (unparseable reply)");
            }
            Some(g) => {
                ok += 1;
                total_rel += g.relations.len();
                total_per += g.persons.len();
                if g.relations.is_empty() && g.persons.is_empty() {
                    println!("   (no relations, no persons — clean empty extraction)");
                }
                for r in &g.relations {
                    println!(
                        "   rel: {}/{} --{}({})--> {} sentiment={:?}",
                        r.subject_type,
                        r.subject_id,
                        r.predicate,
                        r.confidence,
                        match (&r.object_type, r.object_id) {
                            (Some(t), Some(i)) => format!("{t}/{i}"),
                            _ => "(unary)".to_string(),
                        },
                        r.sentiment
                    );
                }
                for p in &g.persons {
                    println!(
                        "   person: {} [{}] team_context={:?}",
                        p.name, p.kind, p.team_context_id
                    );
                }
            }
        }
        println!("   ({} ms)\n", extracted.wall_ms);
    }

    println!(
        "summary: {ok} parsed / {failed_closed} fail-closed; {total_rel} relations, {total_per} persons; avg {} ms/article",
        if ok + failed_closed > 0 {
            total_ms / (ok + failed_closed) as u64
        } else {
            0
        }
    );
    Ok(())
}
