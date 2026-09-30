//! Count-only probe of the optional production Chrome fallback on low-content pages.
//! DATABASE_PRIVATE_URL=... cargo run --release --example harvester_chrome_probe -- RUN_ID [LIMIT]
//! Publisher URLs and text stay in this process on the database host.
use anyhow::{Context, Result};
use scoracle_cognition::evidence::fetch::{count_words, fetch_article};
use sqlx::PgPool;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(
        (2..=3).contains(&args.len()),
        "usage: harvester_chrome_probe RUN_ID [LIMIT]"
    );
    let run_id: i64 = args[1].parse().context("invalid run ID")?;
    let limit: i64 = args
        .get(2)
        .map(String::as_str)
        .unwrap_or("3")
        .parse()
        .context("invalid limit")?;
    anyhow::ensure!((1..=5).contains(&limit), "limit must be 1 through 5");
    let pool = PgPool::connect(&std::env::var("DATABASE_PRIVATE_URL")?).await?;
    let urls: Vec<String> = sqlx::query_scalar(
        "SELECT a.url FROM public.news_articles a \
         JOIN public.harvester_acquisitions h ON h.article_id=a.id \
         WHERE h.status='low_content' AND EXISTS ( \
           SELECT 1 FROM public.harvester_query_provenance p \
           JOIN public.pipeline_runs r ON r.id=$1 AND r.job='pipeline' \
           WHERE p.article_id=a.id AND p.last_seen_at BETWEEN r.started_at AND r.finished_at) \
         ORDER BY a.id LIMIT $2",
    )
    .bind(run_id)
    .bind(limit)
    .fetch_all(&pool)
    .await?;
    std::env::set_var("ARTICLE_READ_CHROME_ENABLED", "1");
    let mut recovered = 0;
    let mut still_low = 0;
    let mut fetch_errors = 0;
    let mut total_words = 0;
    for url in &urls {
        match fetch_article(url).await {
            Ok(fetched) => {
                let words = count_words(&fetched.text);
                if words >= 20 {
                    recovered += 1;
                    total_words += words;
                } else {
                    still_low += 1;
                }
            }
            Err(_) => fetch_errors += 1,
        }
    }
    println!(
        "tested={} recovered={} still_low={} fetch_errors={} mean_recovered_words={}",
        urls.len(),
        recovered,
        still_low,
        fetch_errors,
        if recovered > 0 {
            total_words / recovered
        } else {
            0
        }
    );
    Ok(())
}
