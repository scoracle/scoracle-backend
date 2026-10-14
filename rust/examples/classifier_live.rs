//! Bounded live Classifier run on the real backlog: acquire + classify a few articles.
//! DATABASE_URL and CLASSIFIER_MODEL_DIR are required; LIMIT bounds the scope (default 5).
use anyhow::Result;
use scoracle_cognition::{
    harness::{
        plugin::StudioPlugin,
        queue::work::{self, Item},
        tools::WebBroker,
    },
    plugins::classifier::{adapter, gliner::Gliner, manifest},
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let limit: i64 = std::env::var("LIMIT").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    let gliner = Arc::new(Gliner::load(&std::env::var("CLASSIFIER_MODEL_DIR")?)?);

    let picks: Vec<(i64, String)> = sqlx::query_as(
        "SELECT DISTINCT n.id, p.sport FROM public.news_articles n
         JOIN public.harvester_query_provenance p ON p.article_id=n.id AND p.entity_type='team'
         WHERE length(coalesce(n.full_text,'')) > 400
         ORDER BY n.id LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&pool)
    .await?;
    println!("picked {} real articles", picks.len());

    for (id, sport) in &picks {
        work::enqueue(
            &pool,
            &Item {
                stage: manifest::ACQUIRE_TASK,
                entity_type: "article".into(),
                entity_id: *id,
                sport: sport.clone(),
                input_version: None,
                attempts: 0,
                claim_token: None,
            },
        )
        .await?;
    }

    let acquire = adapter::AcquireHandler::new(pool.clone(), Arc::new(WebBroker::new(0)?));
    for item in work::claim(&pool, manifest::ACQUIRE_TASK, limit).await? {
        match acquire.execute(&item).await {
            Ok(outcome) => println!("acquired {}/{} {outcome:?}", item.entity_id, item.sport),
            Err(error) => println!("acquire error {}/{}: {error:#}", item.entity_id, item.sport),
        }
    }
    for item in work::claim(&pool, manifest::TASK, limit).await? {
        match adapter::execute_spectrum(&pool, &item, &gliner).await {
            Ok(outcome) => println!("classified {}/{} {outcome:?}", item.entity_id, item.sport),
            Err(error) => println!("classify error {}/{}: {error:#}", item.entity_id, item.sport),
        }
    }

    let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT x.article_id, x.sport, x.target->>'name', x.gate::text
         FROM public.classifier_spectra x ORDER BY x.id DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&pool)
    .await?;
    println!("\n=== spectra ===");
    for (id, sport, target, gate) in rows {
        println!("{id} {sport} {target} gate={gate}");
    }
    let obligations: Vec<(String, i64)> = sqlx::query_as(
        "SELECT character, count(*) FROM public.classifier_spectrum_deliveries GROUP BY character ORDER BY character",
    )
    .fetch_all(&pool)
    .await?;
    println!("=== obligations === {obligations:?}");
    Ok(())
}
