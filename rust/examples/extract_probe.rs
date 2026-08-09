//! extract_probe — measure fetch::extract_article_text against the old whole-page clean_html
//! on real publisher URLs. Offline except for the fetches; no DB, no model.
//!
//!     cargo run --example extract_probe -- <url> [<url> ...]
use scoracle_cognition::fetch::{clean_html, count_words, extract_article_text};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let urls: Vec<String> = std::env::args().skip(1).collect();
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; ScoracleBot/1.0; +https://scoracle.com)")
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let (mut tot_old, mut tot_new) = (0usize, 0usize);
    for url in &urls {
        let html = match client.get(url).send().await {
            Ok(r) => r.text().await.unwrap_or_default(),
            Err(e) => {
                println!("{url}\n  FETCH FAILED: {e}");
                continue;
            }
        };
        let old = clean_html(&html);
        let new = extract_article_text(&html);
        tot_old += old.len();
        tot_new += new.len();
        println!(
            "{url}\n  old {:>6} ch / {:>5} w   new {:>6} ch / {:>5} w   saved {:>5.1}%",
            old.len(),
            count_words(&old),
            new.len(),
            count_words(&new),
            100.0 * (old.len() as f64 - new.len() as f64) / old.len().max(1) as f64
        );
    }
    println!(
        "\nTOTAL old {tot_old} ch -> new {tot_new} ch = {:.1}% of the article budget reclaimed",
        100.0 * (tot_old as f64 - tot_new as f64) / tot_old.max(1) as f64
    );
    Ok(())
}
