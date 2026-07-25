//! relevance_bands — offline banding analysis for the scrub relevance gate (rebuild plan, Phase C).
//!
//! It answers the one question that decides whether the candle can own exclusion on the CPU:
//!
//!   **Of the candidates that reach the GPU, what fraction does the model KEEP?**
//!
//! The live gate is asymmetric (`resolve.rs`): `cosine >= keep_threshold` auto-keeps with no model
//! call; everything below goes to the model, and the model is the only thing allowed to reject. So
//! from the database alone the keeps are CPU fast-path keeps and GPU keeps mixed together, and the
//! two cannot be separated. This binary separates them by recomputing the cosine the live gate used
//! and cross-tabbing it against the verdict that was actually written.
//!
//! If the model keeps ~nothing below the keep line, the GPU adjudication is redundant — moving
//! exclusion to the CPU is a refactor with identical output, not a gamble. If it keeps a meaningful
//! share, the cosine cannot do the job and a better instrument (cross-encoder) must land first.
//!
//! It also sweeps candidate drop thresholds so "be lenient" becomes a number: for each threshold,
//! how many model calls it would save, and how many genuine links it would have erased.
//!
//! SAFETY: strictly read-only. No writes, no `pipeline_work`, no Ollama, no network beyond the
//! one-time HuggingFace model fetch the embedder already caches. Embeddings run on the CPU.
//!
//!   cargo run --bin relevance_bands -- [--sport FOOTBALL] [--limit 20000] [--confidence 0.95]

use anyhow::{Context, Result};
use scoracle_cognition::config::{EmbedConfig, ResolveConfig};
use scoracle_cognition::embed::{cosine_similarity, Embedder};
use scoracle_cognition::harness::{Candidate, EntityType, IdentityCard};
use scoracle_cognition::novelty::article_text;
use scoracle_cognition::resolve::identity_text;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::collections::HashMap;

/// One already-decided link: the exact text pair the live gate embedded, plus the verdict on disk.
struct Decided {
    context: String,
    identity: String,
    vetted: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Env comes from the shell (source .env.local), same as the other bins.
    let mut sport: Option<String> = None;
    let mut limit: i64 = 20000;
    let mut confidence: f64 = 0.95;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--sport" => {
                sport = args.get(i + 1).cloned();
                i += 2;
            }
            "--limit" => {
                limit = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(limit);
                i += 2;
            }
            "--confidence" => {
                confidence = args
                    .get(i + 1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(confidence);
                i += 2;
            }
            other => anyhow::bail!("unknown arg {other}"),
        }
    }

    let db = std::env::var("DATABASE_PRIVATE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .context("DATABASE_PRIVATE_URL / DATABASE_URL required")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&db)
        .await
        .context("connect db")?;

    let resolve_cfg = ResolveConfig::from_env().unwrap_or(ResolveConfig {
        keep_threshold: 0.75,
        drop_threshold: 0.60,
    });

    // Every settled link in the band, with the identity-card columns `load_candidates` reads so the
    // identity text is byte-identical to what the live gate embedded.
    let rows = sqlx::query(
        r#"
        SELECT nae.entity_type,
               nae.entity_id,
               COALESCE(p.name, t.name, '')                  AS name,
               COALESCE(p.nationality, '')                   AS nationality,
               COALESCE(ct.name, '')                         AS current_club,
               COALESCE(NULLIF(pci.position, 'Unknown'), '') AS position,
               nae.vetted                                    AS vetted,
               a.title                                       AS title,
               COALESCE(a.description, '')                   AS description
        FROM news_article_entities nae
        JOIN news_articles a ON a.id = nae.article_id
        LEFT JOIN players p ON nae.entity_type = 'player' AND p.id = nae.entity_id AND p.sport = nae.sport
        LEFT JOIN teams   t ON nae.entity_type = 'team'   AND t.id = nae.entity_id AND t.sport = nae.sport
        LEFT JOIN public.player_current_identity pci ON nae.entity_type = 'player' AND pci.player_id = nae.entity_id AND pci.sport = nae.sport
        LEFT JOIN teams ct ON ct.id = pci.team_id AND ct.sport = nae.sport
        WHERE nae.match_confidence = $1
          AND nae.vetted IS NOT NULL
          AND ($2::text IS NULL OR nae.sport = $2)
        ORDER BY nae.scrubbed_at DESC
        LIMIT $3
        "#,
    )
    .bind(confidence)
    .bind(sport.as_deref())
    .bind(limit)
    .fetch_all(&pool)
    .await
    .context("load decided links")?;

    println!("loaded {} decided links (confidence {confidence})", rows.len());
    if rows.is_empty() {
        return Ok(());
    }

    let mut decided = Vec::with_capacity(rows.len());
    for r in &rows {
        let et: String = r.get("entity_type");
        let entity_type = match et.as_str() {
            "player" => EntityType::Player,
            "team" => EntityType::Team,
            _ => continue,
        };
        let name: String = r.get("name");
        if name.is_empty() {
            continue; // purged entity — the live gate would not have had a card either
        }
        let opt = |s: String| if s.is_empty() { None } else { Some(s) };
        let cand = Candidate {
            entity_type,
            entity_id: r.get("entity_id"),
            name,
            identity: IdentityCard {
                nationality: opt(r.get("nationality")),
                current_club: opt(r.get("current_club")),
                position: opt(r.get("position")),
            },
        };
        let title: String = r.get("title");
        let description: String = r.get("description");
        decided.push(Decided {
            context: article_text(&title, &description),
            identity: identity_text(&cand),
            vetted: r.get("vetted"),
        });
    }

    // Embed each distinct string once — articles repeat across links, identity cards repeat hard.
    let embedder = Embedder::from_config(&EmbedConfig::from_env()?).context("load embedder")?;
    let mut uniq: Vec<String> = Vec::new();
    let mut idx: HashMap<String, usize> = HashMap::new();
    for d in &decided {
        for s in [&d.context, &d.identity] {
            if !idx.contains_key(s) {
                idx.insert(s.clone(), uniq.len());
                uniq.push(s.clone());
            }
        }
    }
    println!("embedding {} distinct strings on CPU...", uniq.len());
    let mut vectors = Vec::with_capacity(uniq.len());
    for (n, chunk) in uniq.chunks(64).enumerate() {
        vectors.extend(embedder.embed_batch(chunk).context("embed batch")?);
        if n % 20 == 0 {
            println!("  {} / {}", vectors.len(), uniq.len());
        }
    }

    let scored: Vec<(f32, bool)> = decided
        .iter()
        .map(|d| {
            let c = cosine_similarity(&vectors[idx[&d.context]], &vectors[idx[&d.identity]]);
            (c, d.vetted)
        })
        .collect();

    let total = scored.len();
    let kept_total = scored.iter().filter(|(_, v)| *v).count();
    println!("\n=== overall ===");
    println!(
        "decided {total}  kept {kept_total} ({:.1}%)  rejected {} ({:.1}%)",
        pct(kept_total, total),
        total - kept_total,
        pct(total - kept_total, total)
    );

    // THE decisive split: above the keep line the CPU decided alone; below it the model decided.
    let keep_t = resolve_cfg.keep_threshold;
    let above: Vec<&(f32, bool)> = scored.iter().filter(|(c, _)| *c >= keep_t).collect();
    let below: Vec<&(f32, bool)> = scored.iter().filter(|(c, _)| *c < keep_t).collect();
    let below_kept = below.iter().filter(|(_, v)| *v).count();
    let above_kept = above.iter().filter(|(_, v)| *v).count();

    println!("\n=== the decisive number (keep_threshold = {keep_t}) ===");
    println!(
        "ABOVE keep line (CPU auto-kept, no model call): {} ({:.1}% of all)  of which vetted=true {} ({:.1}%)",
        above.len(),
        pct(above.len(), total),
        above_kept,
        pct(above_kept, above.len())
    );
    println!(
        "BELOW keep line (went to the GPU):              {} ({:.1}% of all)  of which the MODEL KEPT {} ({:.1}%)",
        below.len(),
        pct(below.len(), total),
        below_kept,
        pct(below_kept, below.len())
    );
    println!(
        "\n  → the GPU keeps {:.1}% of what it sees. Those {} links are what a CPU-only\n    no-gate would have to preserve; everything else it sees is a rejection it could\n    have made for free.",
        pct(below_kept, below.len()),
        below_kept
    );

    // Threshold sweep: turn "be lenient" into a number.
    println!("\n=== drop-threshold sweep (auto-reject when cosine < t) ===");
    println!("  t     auto-dropped   correct (was rejected)   ERASED (was kept)   model calls saved");
    let mut t = 0.05f32;
    while t <= keep_t + 0.0001 {
        let dropped: Vec<&(f32, bool)> = below.iter().copied().filter(|(c, _)| *c < t).collect();
        let erased = dropped.iter().filter(|(_, v)| *v).count();
        let correct = dropped.len() - erased;
        println!(
            "  {:.2}  {:>10}   {:>10} ({:>5.1}%)      {:>8} ({:>5.2}%)     {:>6} ({:.1}% of gate)",
            t,
            dropped.len(),
            correct,
            pct(correct, dropped.len().max(1)),
            erased,
            pct(erased, kept_total.max(1)),
            dropped.len(),
            pct(dropped.len(), below.len().max(1))
        );
        t += 0.05;
    }
    println!("\n  ERASED% is measured against ALL genuine links, i.e. the share of the real corpus\n  that threshold would silently delete. That is the number with no recovery path.");

    Ok(())
}

fn pct(n: usize, d: usize) -> f64 {
    if d == 0 {
        0.0
    } else {
        n as f64 * 100.0 / d as f64
    }
}
