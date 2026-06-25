//! L6 exploration (read-only) — REDUNDANCY CHECK: is each auto-dropped genuine link a near-duplicate
//! of another article the player KEEPS? The rigorous version of the title-match proxy, using the L4
//! Embed primitive: embed the dropped article + the player's SURVIVING vetted articles, and report
//! the max article↔article cosine. High ⇒ the story is captured elsewhere on the player's page
//! (redundant — the drop is harmless); low ⇒ the dropped article is the player's only copy of that
//! story (the drop loses truth). "Surviving" = the player's vetted articles MINUS the auto-drop band,
//! i.e. what they'd keep under the cheap gate.
//!
//! This answers the question directly: "we may be fine with the drop, because it was picked up
//! elsewhere — let's find out." Read-only: it SELECTs + embeds, writes nothing.

use anyhow::{Context, Result};
use scoracle_cognition::config::Config;
use scoracle_cognition::db;
use scoracle_cognition::embed::{cosine_similarity, Embedder};
use sqlx::{PgPool, Row};
use std::collections::HashMap;

struct FnRow {
    player_id: i32,
    sport: String,
    name: String,
    text: String,
}

struct Survivor {
    title: String,
    text: String,
}

/// A player's surviving articles, embedded once: parallel vectors + their titles.
#[derive(Default)]
struct SurvEmb {
    vecs: Vec<Vec<f32>>,
    titles: Vec<String>,
}

fn article_text(title: &str, desc: &str) -> String {
    if desc.is_empty() {
        title.to_string()
    } else {
        format!("{title} — {desc}")
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    println!("loading {} (CPU)…", cfg.embed.model_repo);
    let embedder = Embedder::from_config(&cfg.embed).context("load embedder")?;

    let fns = load_fns(&pool).await?;
    let survivors = load_survivors(&pool).await?;

    println!(
        "\nREDUNDANCY of {} auto-dropped genuine links vs each player's SURVIVING vetted articles",
        fns.len()
    );
    println!("(article↔article cosine: ≥0.85 near-dup/redundant · 0.70-0.85 related · <0.70 unique)\n");

    // Embed each player's surviving set once.
    let mut surv_vecs: HashMap<(i32, String), SurvEmb> = HashMap::new();
    for ((pid, sport), arts) in &survivors {
        let texts: Vec<String> = arts.iter().map(|a| a.text.clone()).collect();
        let vecs = if texts.is_empty() {
            Vec::new()
        } else {
            embedder.embed_batch(&texts)?
        };
        let titles: Vec<String> = arts.iter().map(|a| a.title.clone()).collect();
        surv_vecs.insert((*pid, sport.clone()), SurvEmb { vecs, titles });
    }

    let (mut redundant, mut related, mut unique, mut only) = (0u32, 0u32, 0u32, 0u32);
    let empty = SurvEmb::default();
    for f in &fns {
        let fvec = embedder.embed_one(&f.text)?;
        let SurvEmb { vecs, titles } = surv_vecs.get(&(f.player_id, f.sport.clone())).unwrap_or(&empty);
        if vecs.is_empty() {
            only += 1;
            println!("  {:18} ONLY LINK  (0 survivors)   «{}»", f.name, truncate(&f.text, 46));
            continue;
        }
        let (best_cos, best_i) = vecs
            .iter()
            .enumerate()
            .map(|(i, v)| (cosine_similarity(&fvec, v), i))
            .fold((f32::NEG_INFINITY, 0usize), |a, b| if b.0 > a.0 { b } else { a });
        let label = if best_cos >= 0.85 {
            redundant += 1;
            "REDUNDANT"
        } else if best_cos >= 0.70 {
            related += 1;
            "related  "
        } else {
            unique += 1;
            "UNIQUE   "
        };
        println!(
            "  {:18} {} max {:.3} ({} surv)  «{}» → «{}»",
            f.name,
            label,
            best_cos,
            vecs.len(),
            truncate(&f.text, 34),
            truncate(&titles[best_i], 34),
        );
    }

    println!(
        "\nsummary: {redundant} redundant · {related} related · {unique} unique · {only} only-link  (of {})",
        fns.len()
    );
    println!(
        "→ {} of {} dropped links have NO near-duplicate survivor — for those, the drop removes that story from the player's page.",
        unique + only,
        fns.len()
    );
    Ok(())
}

async fn load_fns(pool: &PgPool) -> Result<Vec<FnRow>> {
    let rows = sqlx::query(
        r#"SELECT rs.entity_id AS player_id, rs.sport, rs.name,
                  na.title, COALESCE(na.description,'') AS description
             FROM resolve_shadow rs JOIN news_articles na ON na.id = rs.article_id
            WHERE rs.band='drop' AND rs.gemma_vetted
            ORDER BY rs.name, rs.article_id"#,
    )
    .fetch_all(pool)
    .await
    .context("load fns")?;
    Ok(rows
        .iter()
        .map(|r| {
            let title: String = r.get("title");
            let desc: String = r.get("description");
            FnRow {
                player_id: r.get("player_id"),
                sport: r.get("sport"),
                name: r.get("name"),
                text: article_text(&title, &desc),
            }
        })
        .collect())
}

/// Each FN player's SURVIVING vetted articles: vetted links NOT in the auto-drop band (so it excludes
/// the dropped FN articles themselves — we never compare a dropped story to another dropped copy).
async fn load_survivors(pool: &PgPool) -> Result<HashMap<(i32, String), Vec<Survivor>>> {
    let rows = sqlx::query(
        r#"SELECT nae.entity_id AS player_id, nae.sport, na.title,
                  COALESCE(na.description,'') AS description
             FROM news_article_entities nae
             JOIN news_articles na ON na.id = nae.article_id
            WHERE nae.entity_type='player' AND nae.vetted
              AND (nae.entity_id, nae.sport) IN (
                    SELECT DISTINCT entity_id, sport FROM resolve_shadow WHERE band='drop' AND gemma_vetted)
              AND NOT EXISTS (
                    SELECT 1 FROM resolve_shadow rs
                     WHERE rs.article_id = nae.article_id AND rs.entity_id = nae.entity_id
                       AND rs.sport = nae.sport AND rs.band='drop')"#,
    )
    .fetch_all(pool)
    .await
    .context("load survivors")?;
    let mut out: HashMap<(i32, String), Vec<Survivor>> = HashMap::new();
    for r in &rows {
        let pid: i32 = r.get("player_id");
        let sport: String = r.get("sport");
        let title: String = r.get("title");
        let desc: String = r.get("description");
        out.entry((pid, sport)).or_default().push(Survivor {
            title: title.clone(),
            text: article_text(&title, &desc),
        });
    }
    Ok(out)
}
