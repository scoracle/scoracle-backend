//! L4 Resolve EVAL — offline end-to-end validation of the REAL hybrid `resolve_set` gate.
//!
//! Where `resolve_experiment` measured the raw cosine signal, this exercises the actual primitive:
//! for a sample of articles it loads their labeled secondary player links, calls
//! `Harness::resolve_set` (cheap CPU band → Gemma adjudicates the ambiguous middle → fail-closed),
//! and reports two things:
//!   * **agreement vs Gemma** — how faithfully the hybrid reproduces the production scrub's
//!     `news_article_entities.vetted` verdicts (accuracy / precision / recall / F1); and
//!   * **the live split** — what fraction of candidates the cheap band auto-decided vs sent to
//!     Gemma (the GPU saving, in situ — the experiment's ~55% projection, now actually run).
//!
//! It proves the gate WORKS end-to-end (embed → band → adjudicate → fail-closed) on real data, and
//! that the savings are real. Needs Ollama up for the ambiguous band. READ-ONLY on the pipeline:
//! it SELECTs the labeled set and calls embed + generate; it never writes a table or the queue.
//!
//! Usage (env: DATABASE_PRIVATE_URL + OLLAMA_* + COGNITION_EMBED_*/COGNITION_RESOLVE_* optional):
//!   resolve_eval                          # COGNITION_EVAL_ARTICLES (default 40) sampled articles

use anyhow::{Context, Result};
use scoracle_cognition::config::Config;
use scoracle_cognition::db;
use scoracle_cognition::embed::{cosine_similarity, Embedder};
use scoracle_cognition::harness::{Candidate, EntityType, Harness, IdentityCard};
use scoracle_cognition::resolve::identity_text;
use scoracle_cognition::route::{Role, Router};
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::io::Write;

/// One labeled secondary player link of a sampled article.
struct LinkRow {
    article_id: i64,
    title: String,
    description: String,
    entity_id: i32,
    name: String,
    nationality: String,
    current_club: String,
    position: String,
    vetted: bool,
}

impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for LinkRow {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self {
            article_id: row.try_get("article_id")?,
            title: row.try_get("title")?,
            description: row.try_get("description")?,
            entity_id: row.try_get("entity_id")?,
            name: row.try_get("name")?,
            nationality: row.try_get("nationality")?,
            current_club: row.try_get("current_club")?,
            position: row.try_get("position")?,
            vetted: row.try_get("vetted")?,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    let sample: i64 = std::env::var("COGNITION_EVAL_ARTICLES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(40);

    println!("loading model {} (CPU)…", cfg.embed.model_repo);
    let embedder = Embedder::from_config(&cfg.embed).context("load embedder")?;
    let hx = Harness {
        pool,
        router: Router::from_config(&cfg.route, cfg.ollama_timeout)?,
        embedder: Some(embedder),
        resolve: cfg.resolve.clone(),
    };

    let articles = load_sample(&hx.pool, sample).await?;
    println!(
        "resolve_eval — {} articles, band keep≥{:.2}/drop<{:.2}, Gemma role {}\n",
        articles.len(),
        cfg.resolve.keep_threshold,
        cfg.resolve.drop_threshold,
        Role::EmotionalNews.as_str(),
    );

    // Confusion vs Gemma's vetted labels + the live auto/Gemma split.
    let (mut tp, mut fp, mut tn, mut fn_) = (0u32, 0u32, 0u32, 0u32);
    let (mut auto, mut gemma) = (0u32, 0u32);
    let mut disagreements = Vec::new();

    for (idx, art) in articles.iter().enumerate() {
        let context = article_text(&art.title, &art.description);
        let candidates: Vec<Candidate> = art.links.iter().map(to_candidate).collect();
        let labels: Vec<bool> = art.links.iter().map(|l| l.vetted).collect();

        // Count the band split independently (one cheap embed) so we can report the live GPU
        // saving — what resolve_set decided WITHOUT Gemma vs the ambiguous middle it routed.
        let mut texts = vec![context.clone()];
        texts.extend(candidates.iter().map(identity_text));
        let vecs = hx.embed(&texts).await?;
        let (ctx, ids) = vecs.split_first().expect("≥1 vector");
        for v in ids {
            let c = cosine_similarity(ctx, v);
            if c >= cfg.resolve.keep_threshold || c < cfg.resolve.drop_threshold {
                auto += 1;
            } else {
                gemma += 1;
            }
        }

        // The primitive under test.
        let resolutions = hx.resolve_set(Role::EmotionalNews, &context, &candidates).await?;
        for (k, res) in resolutions.iter().enumerate() {
            match (res.kept, labels[k]) {
                (true, true) => tp += 1,
                (true, false) => fp += 1,
                (false, false) => tn += 1,
                (false, true) => fn_ += 1,
            }
            if res.kept != labels[k] {
                disagreements.push(format!(
                    "    {} (article {}) — hybrid {}, Gemma {}",
                    art.links[k].name,
                    art.article_id,
                    if res.kept { "KEEP" } else { "drop" },
                    if labels[k] { "KEEP" } else { "drop" },
                ));
            }
        }
        print!("\r  {}/{} articles…", idx + 1, articles.len());
        let _ = std::io::stdout().flush();
    }
    println!("\r{:30}", " ");

    report(tp, fp, tn, fn_, auto, gemma, &disagreements);
    Ok(())
}

fn article_text(title: &str, description: &str) -> String {
    if description.is_empty() {
        title.to_string()
    } else {
        format!("{title} — {description}")
    }
}

fn to_candidate(l: &LinkRow) -> Candidate {
    let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
    Candidate {
        entity_type: EntityType::Player,
        entity_id: l.entity_id,
        name: l.name.clone(),
        identity: IdentityCard {
            nationality: opt(&l.nationality),
            current_club: opt(&l.current_club),
            position: opt(&l.position),
        },
    }
}

/// One sampled article + its labeled secondary player links.
struct Article {
    article_id: i64,
    title: String,
    description: String,
    links: Vec<LinkRow>,
}

async fn load_sample(pool: &PgPool, sample: i64) -> Result<Vec<Article>> {
    // Deterministic pseudo-random sample of articles that have ≥1 labeled secondary player link.
    let ids: Vec<i64> = {
        use sqlx::Row;
        sqlx::query(
            "SELECT z.article_id FROM ( \
                SELECT DISTINCT nae.article_id \
                FROM news_article_entities nae \
                WHERE nae.entity_type='player' AND nae.vetted IS NOT NULL AND nae.match_confidence < 1.0 \
             ) z ORDER BY md5(z.article_id::text) LIMIT $1",
        )
        .bind(sample)
        .fetch_all(pool)
        .await
        .context("sample article ids")?
        .iter()
        .map(|r| r.get::<i64, _>("article_id"))
        .collect()
    };

    let rows = sqlx::query_as::<_, LinkRow>(
        r#"
        SELECT nae.article_id, na.title, COALESCE(na.description,'') AS description,
               nae.entity_id, p.name,
               COALESCE(p.nationality,'')                    AS nationality,
               COALESCE(ct.name,'')                          AS current_club,
               COALESCE(NULLIF(pos.position,'Unknown'), '')  AS position,
               nae.vetted
        FROM news_article_entities nae
        JOIN news_articles na ON na.id = nae.article_id
        JOIN players p ON p.id = nae.entity_id AND p.sport = nae.sport
        LEFT JOIN public.player_current_team pct ON pct.player_id = p.id AND pct.sport = p.sport
        LEFT JOIN teams ct ON ct.id = pct.team_id AND ct.sport = p.sport
        LEFT JOIN LATERAL (
            SELECT ps.position FROM player_stats ps
            WHERE ps.player_id = p.id AND ps.sport = p.sport
            ORDER BY ps.season DESC NULLS LAST LIMIT 1
        ) pos ON true
        WHERE nae.entity_type='player' AND nae.vetted IS NOT NULL AND nae.match_confidence < 1.0
          AND nae.article_id = ANY($1)
        "#,
    )
    .bind(&ids)
    .fetch_all(pool)
    .await
    .context("load sample links")?;

    // Group by article (BTreeMap → deterministic order).
    let mut by_article: BTreeMap<i64, Article> = BTreeMap::new();
    for r in rows {
        let entry = by_article.entry(r.article_id).or_insert_with(|| Article {
            article_id: r.article_id,
            title: r.title.clone(),
            description: r.description.clone(),
            links: Vec::new(),
        });
        entry.links.push(r);
    }
    Ok(by_article.into_values().collect())
}

#[allow(clippy::too_many_arguments)]
fn report(tp: u32, fp: u32, tn: u32, fn_: u32, auto: u32, gemma: u32, disagreements: &[String]) {
    let total = tp + fp + tn + fn_;
    let f = |x: u32, d: u32| if d > 0 { x as f64 / d as f64 } else { 0.0 };
    let accuracy = f(tp + tn, total);
    let precision = f(tp, tp + fp);
    let recall = f(tp, tp + fn_);
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };
    println!("======================  RESOLVE EVAL (real gate)  ======================");
    println!("agreement with Gemma's vetted labels over {total} candidates:");
    println!("  accuracy  {accuracy:.3}   precision {precision:.3}   recall {recall:.3}   F1 {f1:.3}");
    println!("  confusion: TP {tp}  FP {fp}  TN {tn}  FN {fn_}");
    let split_total = auto + gemma;
    println!(
        "\nlive band split over {split_total} candidates (the GPU saving, in situ):"
    );
    println!(
        "  auto-decided (no Gemma): {auto} ({:.0}%)   →  Gemma adjudicated: {gemma} ({:.0}%)",
        100.0 * f(auto, split_total),
        100.0 * f(gemma, split_total),
    );
    println!(
        "  ⇒ the hybrid spent Gemma on {:.0}% of candidates the all-Gemma scrub would have — {:.0}% GPU saved.",
        100.0 * f(gemma, split_total),
        100.0 * f(auto, split_total),
    );
    if !disagreements.is_empty() {
        println!("\ndisagreements with Gemma ({}):", disagreements.len());
        for d in disagreements.iter().take(12) {
            println!("{d}");
        }
    }
    println!("========================================================================");
}
