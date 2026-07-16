//! bucket_remeasure — offline re-measure for the deferred Phase-2 scrub item
//! (progress doc 2026-07-13_phase2-gate-before-hydrating.md, "Deliberately deferred"):
//! can `bucket::classify_article`'s fallback path reuse the RESOLVE context vector —
//! embedded from `"{title} — {description}"` (scrub.rs::article_text) — instead of
//! re-embedding its own `"{title} {description}"` (bucket.rs::article_text)?
//!
//! The live thresholds (bucket_threshold 0.02 + keyword_weight 0.08) were tuned on the
//! bucket text form; this harness scores BOTH text forms against the GPU labels in
//! planning_docs/data/bucket_labels.tsv (produced by bin/bucketlabel) and reports:
//!   1. faithfulness — a raw-score replica must agree with the live `classify_vector`
//!      on every article (proves the replica's raw scores describe live behavior);
//!   2. the per-article score shift and decision flips between text forms at the live
//!      operating point;
//!   3. accuracy/precision/recall vs the GPU labels for both forms, AUC, and a
//!      threshold sweep to find the em-dash form's equivalent operating point.
//!
//! SAFETY: read-only. Never claims pipeline_work, never writes any table.
//!
//! Usage (env from .env.local): cargo run --release --example bucket_remeasure
//!   [path/to/bucket_labels.tsv]   (default ../planning_docs/data/bucket_labels.tsv)

use anyhow::{bail, Context, Result};
use scoracle_cognition::bucket::{ArticleBucket, BucketClassifier};
use scoracle_cognition::config::{EmbedConfig, ScrubConfig};
use scoracle_cognition::embed::{cosine_similarity, Embedder};
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::collections::HashMap;
use std::time::Instant;

const BATCH: usize = 32;

struct Article {
    id: i64,
    label_transfer: bool, // GPU label from bucket_labels.tsv
    title: String,
    description: String,
}

/// Replica of bucket.rs::article_text (private) — the text the live classifier embeds.
fn bucket_text(title: &str, description: &str) -> String {
    if description.is_empty() {
        title.to_string()
    } else {
        format!("{title} {description}")
    }
}

/// Replica of scrub.rs::article_text (private) — the resolve context text.
fn resolve_text(title: &str, description: &str) -> String {
    if description.is_empty() {
        title.to_string()
    } else {
        format!("{title} — {description}")
    }
}

/// Replica of bucket.rs::keyword_hit (private).
fn keyword_hit(text: &str, keywords: &[String]) -> bool {
    let lower = text.to_lowercase();
    keywords.iter().any(|k| lower.contains(k))
}

/// Replica of bucket.rs::mean_vec (private).
fn mean_vec(vs: &[Vec<f32>]) -> Vec<f32> {
    let dim = vs[0].len();
    let mut m = vec![0.0_f32; dim];
    for v in vs {
        for (a, b) in m.iter_mut().zip(v) {
            *a += b;
        }
    }
    let norm = m.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm == 0.0 {
        return m;
    }
    m.iter().map(|x| x / norm).collect()
}

/// Replica of bucket.rs::canonical_transfer_sentences (private).
fn canonical_transfer_sentences() -> Vec<String> {
    [
        "Transfer rumor: the player is set to join the club in a big-money transfer deal.",
        "The club have agreed a transfer fee to sign the player this summer.",
        "The player is attracting interest from several clubs ahead of the transfer window.",
        "The club are in advanced talks over a loan move for the player.",
        "Here we go: the transfer is done and the medical is scheduled before the announcement.",
        "The player wants to leave the club and has asked for a transfer.",
        "The club have made a bid for the player, opening transfer negotiations.",
        "Exclusive: the club are targeting the midfielder as a priority signing this window.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Replica of bucket.rs::canonical_non_transfer_sentences (private).
fn canonical_non_transfer_sentences() -> Vec<String> {
    [
        "Match report: the team beat their opponents in last night's game.",
        "Game recap: the player scored 25 points as his team won at home.",
        "The player suffered an injury and is expected to miss several weeks.",
        "Season preview: analysis of the team's roster and playoff chances.",
        "The player's performance this season has been outstanding, statistics show.",
        "Betting odds and predictions for this week's fixtures.",
        "The coach spoke to the press about the team's tactics ahead of the game.",
        "Play-by-play: full box score and highlights from the game.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn embed_all(e: &Embedder, texts: &[String]) -> Result<Vec<Vec<f32>>> {
    let mut out = Vec::with_capacity(texts.len());
    for chunk in texts.chunks(BATCH) {
        out.extend(e.embed_batch(chunk)?);
    }
    Ok(out)
}

fn auc(pos: &[f32], neg: &[f32]) -> f64 {
    let (mut wins, mut total) = (0.0f64, 0.0f64);
    for p in pos {
        for n in neg {
            total += 1.0;
            if p > n {
                wins += 1.0;
            } else if p == n {
                wins += 0.5;
            }
        }
    }
    wins / total
}

fn pct(xs: &[f32], q: f64) -> f32 {
    let mut s = xs.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[((s.len() - 1) as f64 * q) as usize]
}

fn class_stats(name: &str, xs: &[f32]) {
    if xs.is_empty() {
        println!("  {name:<34} n=0");
        return;
    }
    let mean = xs.iter().sum::<f32>() / xs.len() as f32;
    println!(
        "  {name:<34} n={:<5} mean={:+.4} p10={:+.4} p50={:+.4} p90={:+.4}",
        xs.len(),
        mean,
        pct(xs, 0.10),
        pct(xs, 0.50),
        pct(xs, 0.90)
    );
}

/// accuracy / precision / recall on the transfer class at a given full-score threshold.
fn confusion(scores: &[f32], labels: &[bool], thr: f32) -> (f64, f64, f64) {
    let (mut tp, mut fp, mut tn, mut fn_) = (0f64, 0f64, 0f64, 0f64);
    for (s, l) in scores.iter().zip(labels) {
        match (*s >= thr, *l) {
            (true, true) => tp += 1.0,
            (true, false) => fp += 1.0,
            (false, false) => tn += 1.0,
            (false, true) => fn_ += 1.0,
        }
    }
    let acc = (tp + tn) / (tp + tn + fp + fn_);
    let prec = if tp + fp > 0.0 {
        tp / (tp + fp)
    } else {
        f64::NAN
    };
    let rec = if tp + fn_ > 0.0 {
        tp / (tp + fn_)
    } else {
        f64::NAN
    };
    (acc, prec, rec)
}

#[tokio::main]
async fn main() -> Result<()> {
    let tsv_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "../planning_docs/data/bucket_labels.tsv".to_string());
    let db_url = std::env::var("DATABASE_PRIVATE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .context("DATABASE_PRIVATE_URL or DATABASE_URL must be set (source .env.local)")?;

    // --- labels ------------------------------------------------------------
    let body = std::fs::read_to_string(&tsv_path)
        .with_context(|| format!("read labels TSV at {tsv_path}"))?;
    let mut labels: HashMap<i64, bool> = HashMap::new();
    let (mut n_transfer, mut n_other, mut n_skipped) = (0u32, 0u32, 0u32);
    for line in body.lines().skip(1) {
        let mut it = line.split('\t');
        let (Some(id), _sport, Some(label)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        let Ok(id) = id.parse::<i64>() else { continue };
        match label {
            "transfer" => {
                labels.insert(id, true);
                n_transfer += 1;
            }
            "other" => {
                labels.insert(id, false);
                n_other += 1;
            }
            _ => n_skipped += 1, // unparsed / error rows carry no signal
        }
    }
    println!(
        "== labels ==  usable={} (transfer={n_transfer} other={n_other}) skipped={n_skipped}  [{tsv_path}]",
        labels.len()
    );
    if labels.is_empty() {
        bail!("no usable labels");
    }

    // --- articles (read-only) ----------------------------------------------
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db_url)
        .await
        .context("connect")?;
    let ids: Vec<i64> = labels.keys().copied().collect();
    let rows = sqlx::query(
        "SELECT id, title, COALESCE(description, '') AS description
           FROM news_articles WHERE id = ANY($1) ORDER BY id",
    )
    .bind(&ids)
    .fetch_all(&pool)
    .await
    .context("load labeled articles")?;
    let articles: Vec<Article> = rows
        .into_iter()
        .map(|r| {
            let id: i64 = r.get(0);
            Article {
                id,
                label_transfer: labels[&id],
                title: r.get(1),
                description: r.get(2),
            }
        })
        .collect();
    let n_empty_desc = articles.iter().filter(|a| a.description.is_empty()).count();
    println!(
        "== articles ==  fetched={} (vanished={})  desc-empty={} (identical under both text forms)",
        articles.len(),
        labels.len() - articles.len(),
        n_empty_desc
    );

    // --- live classifier + raw-score replica --------------------------------
    let scrub_cfg = ScrubConfig::from_env()?;
    println!(
        "== config ==  bucket_threshold={} keyword_weight={} keywords={}",
        scrub_cfg.bucket_threshold,
        scrub_cfg.bucket_keyword_weight,
        scrub_cfg.bucket_keywords.len()
    );
    let t0 = Instant::now();
    let embedder = Embedder::from_config(&EmbedConfig::from_env()?)?;
    println!(
        "embedder loaded in {:.1}s, dim={}",
        t0.elapsed().as_secs_f64(),
        embedder.dim
    );
    let classifier = BucketClassifier::from_embedder(&embedder, scrub_cfg.clone())?;
    let transfer_centroid = mean_vec(&embed_all(&embedder, &canonical_transfer_sentences())?);
    let non_transfer_centroid =
        mean_vec(&embed_all(&embedder, &canonical_non_transfer_sentences())?);

    let full_score = |text: &str, v: &[f32]| -> f32 {
        let contrastive =
            cosine_similarity(v, &transfer_centroid) - cosine_similarity(v, &non_transfer_centroid);
        let bonus = if keyword_hit(text, &scrub_cfg.bucket_keywords) {
            scrub_cfg.bucket_keyword_weight
        } else {
            0.0
        };
        contrastive + bonus
    };

    // --- embed both text forms ----------------------------------------------
    let space_texts: Vec<String> = articles
        .iter()
        .map(|a| bucket_text(&a.title, &a.description))
        .collect();
    let emdash_texts: Vec<String> = articles
        .iter()
        .map(|a| resolve_text(&a.title, &a.description))
        .collect();
    let t0 = Instant::now();
    let space_v = embed_all(&embedder, &space_texts)?;
    let emdash_v = embed_all(&embedder, &emdash_texts)?;
    println!(
        "embedded {} articles x 2 forms in {:.1}s",
        articles.len(),
        t0.elapsed().as_secs_f64()
    );

    // --- faithfulness: replica must match live classify_vector on bucket text --
    let mut mismatches = 0u32;
    for ((a, text), v) in articles.iter().zip(&space_texts).zip(&space_v) {
        let live = classifier.classify_vector(text, v);
        let replica = if full_score(text, v) >= scrub_cfg.bucket_threshold {
            ArticleBucket::Transfer
        } else {
            ArticleBucket::NonTransfer
        };
        if live != replica {
            mismatches += 1;
            if mismatches <= 5 {
                println!("  FAITHFULNESS MISMATCH on [{}] {}", a.id, a.title);
            }
        }
    }
    if mismatches > 0 {
        bail!("replica disagrees with live classify_vector on {mismatches} articles — raw scores below would not describe live behavior");
    }
    println!(
        "== faithfulness ==  replica == live classify_vector on all {} articles",
        articles.len()
    );

    // --- score shift between forms (desc-nonempty only; empty desc is identical) --
    let labels_vec: Vec<bool> = articles.iter().map(|a| a.label_transfer).collect();
    let space_scores: Vec<f32> = articles
        .iter()
        .zip(&space_texts)
        .zip(&space_v)
        .map(|((_, t), v)| full_score(t, v))
        .collect();
    let emdash_scores: Vec<f32> = articles
        .iter()
        .zip(&emdash_texts)
        .zip(&emdash_v)
        .map(|((_, t), v)| full_score(t, v))
        .collect();

    let nonempty: Vec<usize> = articles
        .iter()
        .enumerate()
        .filter(|(_, a)| !a.description.is_empty())
        .map(|(i, _)| i)
        .collect();
    let vec_cos: Vec<f32> = nonempty
        .iter()
        .map(|&i| cosine_similarity(&space_v[i], &emdash_v[i]))
        .collect();
    let deltas: Vec<f32> = nonempty
        .iter()
        .map(|&i| emdash_scores[i] - space_scores[i])
        .collect();
    let deltas_pos: Vec<f32> = nonempty
        .iter()
        .filter(|&&i| labels_vec[i])
        .map(|&i| emdash_scores[i] - space_scores[i])
        .collect();
    let deltas_neg: Vec<f32> = nonempty
        .iter()
        .filter(|&&i| !labels_vec[i])
        .map(|&i| emdash_scores[i] - space_scores[i])
        .collect();
    println!(
        "\n== form shift (space -> em-dash), desc-nonempty n={} ==",
        nonempty.len()
    );
    class_stats("vector cosine(space, emdash)", &vec_cos);
    class_stats("score delta (all)", &deltas);
    class_stats("score delta (label=transfer)", &deltas_pos);
    class_stats("score delta (label=other)", &deltas_neg);

    // --- operating point ------------------------------------------------------
    let thr = scrub_cfg.bucket_threshold;
    let (acc_s, prec_s, rec_s) = confusion(&space_scores, &labels_vec, thr);
    let (acc_e, prec_e, rec_e) = confusion(&emdash_scores, &labels_vec, thr);
    println!("\n== operating point thr={thr} (vs GPU labels) ==");
    println!("  space  (live today):  acc={acc_s:.3} prec={prec_s:.3} rec={rec_s:.3}");
    println!("  emdash (reuse cand):  acc={acc_e:.3} prec={prec_e:.3} rec={rec_e:.3}");

    let mut flips: Vec<usize> = (0..articles.len())
        .filter(|&i| (space_scores[i] >= thr) != (emdash_scores[i] >= thr))
        .collect();
    flips.sort_by(|&a, &b| {
        (space_scores[a] - emdash_scores[a])
            .abs()
            .partial_cmp(&(space_scores[b] - emdash_scores[b]).abs())
            .unwrap()
            .reverse()
    });
    let to_transfer = flips.iter().filter(|&&i| emdash_scores[i] >= thr).count();
    println!(
        "  decision flips: {} of {} ({:.1}%)  [{} ->transfer, {} ->non_transfer]",
        flips.len(),
        articles.len(),
        100.0 * flips.len() as f64 / articles.len() as f64,
        to_transfer,
        flips.len() - to_transfer
    );
    for &i in flips.iter().take(10) {
        println!(
            "    [{}] {:+.4} -> {:+.4}  label={}  {}",
            articles[i].id,
            space_scores[i],
            emdash_scores[i],
            if labels_vec[i] { "transfer" } else { "other" },
            articles[i].title.chars().take(90).collect::<String>()
        );
    }

    // --- AUC + sweep -----------------------------------------------------------
    let split = |scores: &[f32]| -> (Vec<f32>, Vec<f32>) {
        let pos = scores
            .iter()
            .zip(&labels_vec)
            .filter(|(_, &l)| l)
            .map(|(s, _)| *s)
            .collect();
        let neg = scores
            .iter()
            .zip(&labels_vec)
            .filter(|(_, &l)| !l)
            .map(|(s, _)| *s)
            .collect();
        (pos, neg)
    };
    let (pos_s, neg_s) = split(&space_scores);
    let (pos_e, neg_e) = split(&emdash_scores);
    println!("\n== AUC (full score, vs GPU labels) ==");
    println!(
        "  space={:.4}  emdash={:.4}",
        auc(&pos_s, &neg_s),
        auc(&pos_e, &neg_e)
    );

    println!("\n== threshold sweep (acc / prec / rec on transfer) ==");
    println!("  thr      space                  emdash");
    for i in -4..=10 {
        let t = i as f32 * 0.01;
        let (a1, p1, r1) = confusion(&space_scores, &labels_vec, t);
        let (a2, p2, r2) = confusion(&emdash_scores, &labels_vec, t);
        let mark = if (t - thr).abs() < 1e-6 {
            "<- live"
        } else {
            ""
        };
        println!("  {t:+.2}   {a1:.3} / {p1:.3} / {r1:.3}    {a2:.3} / {p2:.3} / {r2:.3}   {mark}");
    }

    Ok(())
}
