//! candle_probe — offline measurement harness for the F2/F8 candle extensions in
//! planning_docs/DATA_FLOW_FRICTION_PRUNE_PLAN.md.
//!
//! Tests, against LIVE labeled data (read-only):
//!   1. F2 bucket classification — cosine of article (title+description) vs a
//!      "transfer prototype" embedding, three prototype strategies:
//!      P1 single canonical sentence
//!      P2 mean of a small canonical sentence set
//!      P3 data-driven mean of held-out positive articles
//!      Ground truth: POS = articles cited by is_rumor=TRUE transfer_rumors rows;
//!      BROAD-NEG = vetted articles never cited by any rumor row;
//!      HARD-NEG = articles cited ONLY by is_rumor=FALSE rows (the model looked and
//!      said no — the F8 "ambiguous middle").
//!   2. F2 topic heat-rank — harness::cluster over one real day's vetted titles at
//!      several thresholds; prints cluster-size distribution + sample clusters.
//!   3. CPU throughput — articles/sec through embed_batch (the F2 cost axis).
//!
//! SAFETY: read-only. Never claims pipeline_work, never writes any table.
//!
//! Usage (env from .env.local): cargo run --release --example candle_probe

use anyhow::{Context, Result};
use scoracle_cognition::config::EmbedConfig;
use scoracle_cognition::embed::{cosine_similarity, Embedder};
use scoracle_cognition::harness::cluster;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::time::Instant;

const POS_SAMPLE: i64 = 300;
const BROAD_NEG_SAMPLE: i64 = 300;
const HARD_NEG_SAMPLE: i64 = 200;
/// Held-out positives that form the P3 data-driven prototype (never scored).
const PROTO_HOLDOUT: usize = 64;
const BATCH: usize = 32;

struct Article {
    id: i64,
    title: String,
    text: String,
}

fn article_rows(rows: Vec<sqlx::postgres::PgRow>) -> Vec<Article> {
    rows.into_iter()
        .map(|r| {
            let title: String = r.get(1);
            let desc: String = r.get(2);
            let text = if desc.trim().is_empty() {
                title.clone()
            } else {
                format!("{title}. {desc}")
            };
            Article {
                id: r.get(0),
                title,
                text,
            }
        })
        .collect()
}

fn embed_all(e: &Embedder, texts: &[String]) -> Result<(Vec<Vec<f32>>, f64)> {
    let start = Instant::now();
    let mut out = Vec::with_capacity(texts.len());
    for chunk in texts.chunks(BATCH) {
        out.extend(e.embed_batch(chunk)?);
    }
    Ok((out, start.elapsed().as_secs_f64()))
}

fn mean_vec(vs: &[Vec<f32>]) -> Vec<f32> {
    let dim = vs[0].len();
    let mut m = vec![0.0f32; dim];
    for v in vs {
        for (a, b) in m.iter_mut().zip(v) {
            *a += b;
        }
    }
    let norm = m.iter().map(|x| x * x).sum::<f32>().sqrt();
    m.iter().map(|x| x / norm).collect()
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
    let mean = xs.iter().sum::<f32>() / xs.len() as f32;
    println!(
        "  {name:<28} n={:<4} mean={:.3} p10={:.3} p50={:.3} p90={:.3}",
        xs.len(),
        mean,
        pct(xs, 0.10),
        pct(xs, 0.50),
        pct(xs, 0.90)
    );
}

fn sweep(pos: &[f32], neg: &[f32]) {
    println!("  thr    pos-recall  neg-FPR   precision(pos vs neg)");
    for t10 in (30..=90).step_by(5) {
        let t = t10 as f32 / 100.0;
        let tp = pos.iter().filter(|&&x| x >= t).count() as f64;
        let fp = neg.iter().filter(|&&x| x >= t).count() as f64;
        let recall = tp / pos.len() as f64;
        let fpr = fp / neg.len() as f64;
        let prec = if tp + fp > 0.0 {
            tp / (tp + fp)
        } else {
            f64::NAN
        };
        println!("  {t:.2}   {recall:>8.3}   {fpr:>7.3}   {prec:>7.3}");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let db_url = std::env::var("DATABASE_PRIVATE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .context("DATABASE_PRIVATE_URL or DATABASE_URL must be set (source .env.local)")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db_url)
        .await
        .context("connect")?;

    println!("== loading embedder (candle, CPU) ==");
    let t0 = Instant::now();
    let embedder = Embedder::from_config(&EmbedConfig::from_env()?)?;
    println!(
        "loaded in {:.1}s, dim={}",
        t0.elapsed().as_secs_f64(),
        embedder.dim
    );

    // --- labeled sets -----------------------------------------------------
    let pos_rows = sqlx::query(
        "WITH pos AS (SELECT DISTINCT unnest(input_news_ids) AS nid
                        FROM transfer_rumors WHERE is_rumor IS TRUE)
         SELECT a.id, a.title, coalesce(a.description,'')
           FROM news_articles a JOIN pos ON pos.nid = a.id
          ORDER BY md5(a.id::text) LIMIT $1",
    )
    .bind(POS_SAMPLE + PROTO_HOLDOUT as i64)
    .fetch_all(&pool)
    .await?;
    let broad_rows = sqlx::query(
        "WITH cited AS (SELECT DISTINCT unnest(input_news_ids) AS nid FROM transfer_rumors)
         SELECT a.id, a.title, coalesce(a.description,'')
           FROM news_articles a
          WHERE EXISTS (SELECT 1 FROM news_article_entities nae
                         WHERE nae.article_id = a.id AND nae.vetted IS TRUE)
            AND NOT EXISTS (SELECT 1 FROM cited WHERE cited.nid = a.id)
          ORDER BY md5(a.id::text) LIMIT $1",
    )
    .bind(BROAD_NEG_SAMPLE)
    .fetch_all(&pool)
    .await?;
    let hard_rows = sqlx::query(
        "WITH pos AS (SELECT DISTINCT unnest(input_news_ids) AS nid
                        FROM transfer_rumors WHERE is_rumor IS TRUE),
              neg AS (SELECT DISTINCT unnest(input_news_ids) AS nid
                        FROM transfer_rumors WHERE is_rumor IS FALSE)
         SELECT a.id, a.title, coalesce(a.description,'')
           FROM news_articles a JOIN neg ON neg.nid = a.id
          WHERE NOT EXISTS (SELECT 1 FROM pos WHERE pos.nid = a.id)
          ORDER BY md5(a.id::text) LIMIT $1",
    )
    .bind(HARD_NEG_SAMPLE)
    .fetch_all(&pool)
    .await?;

    let mut positives = article_rows(pos_rows);
    let broad_neg = article_rows(broad_rows);
    let hard_neg = article_rows(hard_rows);
    let proto_pool: Vec<Article> = positives
        .drain(..PROTO_HOLDOUT.min(positives.len()))
        .collect();
    println!(
        "\n== samples ==  pos={} broad_neg={} hard_neg={} proto_holdout={}",
        positives.len(),
        broad_neg.len(),
        hard_neg.len(),
        proto_pool.len()
    );

    // --- embed everything, measuring throughput ---------------------------
    let texts = |v: &[Article]| v.iter().map(|a| a.text.clone()).collect::<Vec<_>>();
    let (pos_v, tp) = embed_all(&embedder, &texts(&positives))?;
    let (broad_v, tb) = embed_all(&embedder, &texts(&broad_neg))?;
    let (hard_v, th) = embed_all(&embedder, &texts(&hard_neg))?;
    let (proto_v, _) = embed_all(&embedder, &texts(&proto_pool))?;
    let n_total = pos_v.len() + broad_v.len() + hard_v.len();
    let t_total = tp + tb + th;
    println!(
        "embed throughput: {} articles in {:.1}s = {:.0} articles/sec (CPU)",
        n_total,
        t_total,
        n_total as f64 / t_total
    );

    // --- prototypes --------------------------------------------------------
    let p1 = embedder.embed_one(
        "Transfer rumor: the player is set to join the club in a big-money transfer deal.",
    )?;
    let canon: Vec<String> = [
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
    .collect();
    let (canon_v, _) = embed_all(&embedder, &canon)?;
    let p2 = mean_vec(&canon_v);
    let p3 = mean_vec(&proto_v);

    for (name, proto) in [
        ("P1 single sentence", &p1),
        ("P2 canonical-set mean", &p2),
        ("P3 data-driven mean", &p3),
    ] {
        let score = |vs: &[Vec<f32>]| {
            vs.iter()
                .map(|v| cosine_similarity(v, proto))
                .collect::<Vec<f32>>()
        };
        let sp = score(&pos_v);
        let sb = score(&broad_v);
        let sh = score(&hard_v);
        println!("\n== prototype: {name} ==");
        class_stats("positives (rumor-cited)", &sp);
        class_stats("broad negatives (uncited)", &sb);
        class_stats("hard negatives (is_rumor=F)", &sh);
        println!(
            "  AUC pos-vs-broad = {:.3}   AUC pos-vs-hard = {:.3}",
            auc(&sp, &sb),
            auc(&sp, &sh)
        );
        println!("  threshold sweep (pos vs broad):");
        sweep(&sp, &sb);

        if name.starts_with("P3") {
            // Eyeball the failure tails for the strongest prototype.
            let mut worst_pos: Vec<(f32, &Article)> = sp.iter().copied().zip(&positives).collect();
            worst_pos.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            println!("  lowest-scoring POSITIVES (false-negative risk):");
            for (s, a) in worst_pos.iter().take(8) {
                println!(
                    "    {s:.3}  [{}] {}",
                    a.id,
                    a.title.chars().take(100).collect::<String>()
                );
            }
            let mut worst_neg: Vec<(f32, &Article)> = sb.iter().copied().zip(&broad_neg).collect();
            worst_neg.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            println!("  highest-scoring BROAD NEGATIVES (false-positive risk):");
            for (s, a) in worst_neg.iter().take(8) {
                println!(
                    "    {s:.3}  [{}] {}",
                    a.id,
                    a.title.chars().take(100).collect::<String>()
                );
            }
        }
    }

    // --- contrastive scoring: cos(transfer proto) - cos(non-transfer proto) --
    // Single-prototype cosine compresses all sports news into a tight cone; the
    // contrastive form subtracts the common "sports news" component.
    let non_transfer_canon: Vec<String> = [
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
    .collect();
    let (nt_canon_v, _) = embed_all(&embedder, &non_transfer_canon)?;
    let p2_neg = mean_vec(&nt_canon_v);

    // Data-driven negative centroid from held-out broad negatives.
    let neg_holdout = 64.min(broad_neg.len() / 2);
    let p3_neg = mean_vec(&broad_v[..neg_holdout]);
    let broad_eval = &broad_v[neg_holdout..];
    let broad_eval_articles = &broad_neg[neg_holdout..];

    let contrastive = |vs: &[Vec<f32>], pp: &[f32], pn: &[f32]| {
        vs.iter()
            .map(|v| cosine_similarity(v, pp) - cosine_similarity(v, pn))
            .collect::<Vec<f32>>()
    };
    for (name, pp, pn) in [
        ("S2 canonical contrastive (P2 - nonT)", &p2, &p2_neg),
        ("S3 data-driven contrastive (P3 - negC)", &p3, &p3_neg),
    ] {
        let sp = contrastive(&pos_v, pp, pn);
        let sb = contrastive(broad_eval, pp, pn);
        let sh = contrastive(&hard_v, pp, pn);
        println!("\n== contrastive: {name} ==");
        class_stats("positives", &sp);
        class_stats("broad negatives (eval)", &sb);
        class_stats("hard negatives", &sh);
        println!(
            "  AUC pos-vs-broad = {:.3}   AUC pos-vs-hard = {:.3}",
            auc(&sp, &sb),
            auc(&sp, &sh)
        );
        println!("  thr     pos-recall  neg-FPR   precision");
        for i in -6..=8 {
            let t = i as f32 * 0.02;
            let tp = sp.iter().filter(|&&x| x >= t).count() as f64;
            let fp = sb.iter().filter(|&&x| x >= t).count() as f64;
            let recall = tp / sp.len() as f64;
            let fpr = fp / sb.len() as f64;
            let prec = if tp + fp > 0.0 {
                tp / (tp + fp)
            } else {
                f64::NAN
            };
            println!("  {t:>5.2}   {recall:>8.3}   {fpr:>7.3}   {prec:>7.3}");
        }
        if name.starts_with("S3") {
            let mut worst_pos: Vec<(f32, &Article)> = sp.iter().copied().zip(&positives).collect();
            worst_pos.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            println!("  lowest-scoring POSITIVES:");
            for (s, a) in worst_pos.iter().take(6) {
                println!(
                    "    {s:>6.3}  [{}] {}",
                    a.id,
                    a.title.chars().take(100).collect::<String>()
                );
            }
            let mut worst_neg: Vec<(f32, &Article)> =
                sb.iter().copied().zip(broad_eval_articles).collect();
            worst_neg.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            println!("  highest-scoring BROAD NEGATIVES:");
            for (s, a) in worst_neg.iter().take(6) {
                println!(
                    "    {s:>6.3}  [{}] {}",
                    a.id,
                    a.title.chars().take(100).collect::<String>()
                );
            }
        }
    }

    // --- dump per-article scores for hand-label evaluation ------------------
    if let Ok(path) = std::env::var("CANDLE_PROBE_DUMP") {
        use std::io::Write;
        let mut f = std::fs::File::create(&path)?;
        writeln!(f, "id\tset\tcos_p2\ts2\ts3\ttitle")?;
        let dump =
            |f: &mut std::fs::File, set: &str, arts: &[Article], vs: &[Vec<f32>]| -> Result<()> {
                for (a, v) in arts.iter().zip(vs) {
                    writeln!(
                        f,
                        "{}\t{}\t{:.4}\t{:.4}\t{:.4}\t{}",
                        a.id,
                        set,
                        cosine_similarity(v, &p2),
                        cosine_similarity(v, &p2) - cosine_similarity(v, &p2_neg),
                        cosine_similarity(v, &p3) - cosine_similarity(v, &p3_neg),
                        a.title.replace('\t', " ")
                    )?;
                }
                Ok(())
            };
        dump(&mut f, "pos", &positives, &pos_v)?;
        dump(&mut f, "broad", broad_eval_articles, broad_eval)?;
        dump(&mut f, "hard", &hard_neg, &hard_v)?;
        println!("\n(dumped per-article scores to {path})");
    }

    // --- keyword baseline: does candle beat a trivial regex? ----------------
    let kw = [
        "transfer",
        "trade",
        "sign",
        "signing",
        "signs",
        "deal",
        "bid",
        "loan",
        "rumor",
        "rumour",
        "free agency",
        "free agent",
        "extension",
        "contract",
        "linked",
        "target",
        "interest",
        "waive",
        "swap",
        "acquire",
        "move to",
    ];
    let kw_score = |arts: &[Article]| {
        arts.iter()
            .map(|a| {
                let t = a.text.to_lowercase();
                if kw.iter().any(|k| t.contains(k)) {
                    1.0f32
                } else {
                    0.0f32
                }
            })
            .collect::<Vec<f32>>()
    };
    let kp = kw_score(&positives);
    let kb = kw_score(broad_eval_articles);
    let kh = kw_score(&hard_neg);
    println!("\n== keyword baseline ==");
    println!(
        "  pos hit-rate={:.3}  broad-neg hit-rate={:.3}  hard-neg hit-rate={:.3}",
        kp.iter().sum::<f32>() / kp.len() as f32,
        kb.iter().sum::<f32>() / kb.len() as f32,
        kh.iter().sum::<f32>() / kh.len() as f32
    );
    println!(
        "  AUC pos-vs-broad = {:.3}   AUC pos-vs-hard = {:.3}",
        auc(&kp, &kb),
        auc(&kp, &kh)
    );

    // --- F2 topic clustering on one real day -------------------------------
    let day_row = sqlx::query(
        "SELECT nae.sport, date_trunc('day', a.published_at)::text AS d, count(DISTINCT a.id) AS c
           FROM news_articles a
           JOIN news_article_entities nae ON nae.article_id = a.id AND nae.vetted IS TRUE
          WHERE a.published_at IS NOT NULL
          GROUP BY 1, 2 ORDER BY 2 DESC, 3 DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await?;
    let sport: String = day_row.get(0);
    let day: String = day_row.get(1);
    let day_rows = sqlx::query(
        "SELECT DISTINCT a.id, a.title, ''
           FROM news_articles a
           JOIN news_article_entities nae ON nae.article_id = a.id AND nae.vetted IS TRUE
          WHERE nae.sport = $1 AND date_trunc('day', a.published_at) = $2::timestamptz
          ORDER BY a.id",
    )
    .bind(&sport)
    .bind(day)
    .fetch_all(&pool)
    .await?;
    let day_articles = article_rows(day_rows);
    println!(
        "\n== F2 topic clustering ==  sport={sport} day-articles={}",
        day_articles.len()
    );
    let (day_v, td) = embed_all(&embedder, &texts(&day_articles))?;
    println!("  day embed cost: {:.2}s for {} titles", td, day_v.len());
    for thr in [0.60f32, 0.70, 0.75, 0.80, 0.85] {
        let t0 = Instant::now();
        let cs = cluster(&day_v, thr);
        let singles = cs.iter().filter(|c| c.members.len() == 1).count();
        let maxc = cs.iter().map(|c| c.members.len()).max().unwrap_or(0);
        println!(
            "  thr={thr:.2}: {} clusters, {} singletons, max size {} ({:.0}ms)",
            cs.len(),
            singles,
            maxc,
            t0.elapsed().as_secs_f64() * 1000.0
        );
    }
    let cs = cluster(&day_v, 0.75);
    let mut by_size: Vec<_> = cs.iter().collect();
    by_size.sort_by_key(|c| std::cmp::Reverse(c.members.len()));
    println!("  top clusters at thr=0.75:");
    for c in by_size.iter().take(3) {
        println!("    -- topic_heat={} --", c.members.len());
        for &m in c.members.iter().take(6) {
            println!(
                "       [{}] {}",
                day_articles[m].id,
                day_articles[m].title.chars().take(110).collect::<String>()
            );
        }
    }

    Ok(())
}
