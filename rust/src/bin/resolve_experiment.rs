//! L4 Resolve experiment — the MEASURED de-risk before trusting embeddings (Plan §1.3 / §7).
//!
//! The candle pivot's first brick is an experiment, not a leap: does a cheap CPU embedding
//! actually separate genuine entity-links from same-name false-positives? We already have the
//! labels — Gemma's scrub verdicts in `news_article_entities.vetted` (TRUE = genuinely the
//! subject, FALSE = a fuzzy-matcher false positive Gemma rejected). This harness embeds, for
//! each labeled PLAYER link, the article context (title + description) and the candidate's
//! identity card (name · nationality · current club · position — the same disambiguators the Go
//! scrub/transfer prompts lean on), takes the cosine, and measures how well that single number
//! reproduces Gemma's TRUE/FALSE call.
//!
//! Scope: SECONDARY links only (`match_confidence < 1.0`) — the fuzzy guesses where TRUE-vs-FALSE
//! is a real judgment. (Primary links are deterministically kept by the scrub, so they are
//! trivially TRUE and would inflate the result.) All 879 player FALSE links + a deterministic
//! pseudo-random sample of TRUE links.
//!
//! Outcome shapes the hybrid gate (Plan §1.3): a high AUC ⇒ embeddings can pre-filter cheaply
//! and Gemma only adjudicates the ambiguous band; a low AUC ⇒ embeddings alone don't separate
//! and the hybrid needs structured signals (the current-club tie-break) or keeps Gemma primary.
//! Either way it is a real, cheap result — the L2 "adopt only on a measured win" discipline.
//!
//! SAFETY: READ-ONLY on the live pipeline. It SELECTs the labeled set and writes only a printed
//! report + a CSV to the scratchpad. It NEVER claims `pipeline_work`, NEVER writes a product
//! table, and NEVER runs a Gemma generation (it reuses Gemma's already-recorded verdicts).
//!
//! Usage (env: DATABASE_PRIVATE_URL + COGNITION_EMBED_* optional):
//!   resolve_experiment                       # all FALSE + COGNITION_EXP_TRUE_SAMPLE (default 1500) TRUE
//!   COGNITION_EXP_TRUE_SAMPLE=3000 resolve_experiment

use anyhow::{Context, Result};
use scoracle_cognition::config::Config;
use scoracle_cognition::db;
use scoracle_cognition::embed::{cosine_similarity, Embedder};
use scoracle_cognition::harness::Vector;
use sqlx::PgPool;
use std::collections::HashMap;
use std::io::Write;

/// One labeled (article, player) link with its identity card and Gemma's verdict.
struct LabeledLink {
    article_id: i64,
    title: String,
    description: String,
    name: String,
    nationality: String,
    current_club: String,
    position: String,
    // numeric(4,3) → MUST cast ::float8 in SQL (sqlx can't scan numeric→f64 without the decimal
    // feature; the L3 landmine, carried).
    match_confidence: Option<f64>,
    vetted: bool,
}

// Manual FromRow (the crate enables sqlx without the `macros`/derive feature on purpose — the
// runtime query API needs no live DB at build time; tuples + manual impls are the house style,
// see bin/sigil_parity.rs).
impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for LabeledLink {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self {
            article_id: row.try_get("article_id")?,
            title: row.try_get("title")?,
            description: row.try_get("description")?,
            name: row.try_get("name")?,
            nationality: row.try_get("nationality")?,
            current_club: row.try_get("current_club")?,
            position: row.try_get("position")?,
            match_confidence: row.try_get("match_confidence")?,
            vetted: row.try_get("vetted")?,
        })
    }
}

const CSV_PATH: &str = concat!(
    "/tmp/claude-1000/-home-sheneveld-scoracle-scoracle-backend/",
    "34a0aab2-7a26-483a-bc91-7bc46478a6d2/scratchpad/resolve_experiment.csv"
);

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;

    let true_sample: i64 = std::env::var("COGNITION_EXP_TRUE_SAMPLE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1500);

    println!("loading labeled player links (secondary, match_confidence < 1.0)…");
    let mut links = load_false(&pool).await?;
    let n_false = links.len();
    let trues = load_true_sample(&pool, true_sample).await?;
    let n_true = trues.len();
    links.extend(trues);
    println!(
        "  {n_false} FALSE + {n_true} TRUE = {} labeled links",
        links.len()
    );
    if n_false == 0 || n_true == 0 {
        anyhow::bail!("need both classes; got {n_false} FALSE / {n_true} TRUE");
    }

    println!(
        "loading embedding model {} ({:?} pooling, CPU)…",
        cfg.embed.model_repo, cfg.embed.pooling
    );
    let embedder = Embedder::from_config(&cfg.embed).context("load embedder")?;

    // Build the two texts per link, then embed UNIQUE strings only (many links share an article,
    // and a player recurs across articles) — a big CPU saving on the i7-7700.
    let articles: Vec<String> = links
        .iter()
        .map(|l| article_text(&l.title, &l.description))
        .collect();
    let identities: Vec<String> = links
        .iter()
        .map(|l| identity_text(&l.name, &l.nationality, &l.current_club, &l.position))
        .collect();

    let mut to_embed: Vec<String> = articles.iter().chain(identities.iter()).cloned().collect();
    to_embed.sort();
    to_embed.dedup();
    println!(
        "embedding {} unique texts (from {} links × 2)…",
        to_embed.len(),
        links.len()
    );
    let vectors = embed_unique(&embedder, &to_embed)?;

    // Score each link: cosine(article, identity), split by Gemma's verdict.
    let mut pos = Vec::with_capacity(n_true); // vetted = TRUE  (genuine)
    let mut neg = Vec::with_capacity(n_false); // vetted = FALSE (impostor/noise)
    let mut csv = String::from("vetted,cosine,match_confidence,name,article_id\n");
    let mut scored: Vec<(f32, &LabeledLink)> = Vec::with_capacity(links.len());
    for (i, l) in links.iter().enumerate() {
        let a = &vectors[&articles[i]];
        let b = &vectors[&identities[i]];
        let c = cosine_similarity(a, b);
        if l.vetted {
            pos.push(c);
        } else {
            neg.push(c);
        }
        csv.push_str(&format!(
            "{},{:.4},{},{:?},{}\n",
            l.vetted,
            c,
            l.match_confidence.unwrap_or(f64::NAN),
            l.name,
            l.article_id
        ));
        scored.push((c, l));
    }
    if let Ok(mut f) = std::fs::File::create(CSV_PATH) {
        let _ = f.write_all(csv.as_bytes());
        println!("wrote per-link CSV → {CSV_PATH}");
    }

    report(&pos, &neg, &scored);
    Ok(())
}

/// article_text is what the model "reads" of the article — title, plus description when present.
/// The embedder truncates at max_tokens, so a long body is bounded.
fn article_text(title: &str, description: &str) -> String {
    if description.is_empty() {
        title.to_string()
    } else {
        format!("{title} — {description}")
    }
}

/// identity_text renders the candidate's identity card as a natural sentence (embeds better than
/// a terse `·`-joined card) — the same disambiguators the Go scrub/transfer prompts use, led by
/// current club (the strongest same-name tie-breaker).
fn identity_text(name: &str, nationality: &str, current_club: &str, position: &str) -> String {
    let descriptor = match (nationality.is_empty(), position.is_empty()) {
        (false, false) => format!("a {nationality} {position}"),
        (false, true) => format!("a {nationality} player"),
        (true, false) => format!("a {position}"),
        (true, true) => String::new(),
    };
    let mut clauses = Vec::new();
    if !descriptor.is_empty() {
        clauses.push(descriptor);
    }
    if current_club.is_empty() {
        clauses.push("current club unknown".to_string());
    } else {
        clauses.push(format!("currently at {current_club}"));
    }
    format!("{name}, {}.", clauses.join(", "))
}

/// embed_unique embeds a deduped text list in chunks (bounded batch → bounded CPU/memory) and
/// returns a text→vector map.
fn embed_unique(embedder: &Embedder, texts: &[String]) -> Result<HashMap<String, Vector>> {
    const CHUNK: usize = 48;
    let mut map = HashMap::with_capacity(texts.len());
    for chunk in texts.chunks(CHUNK) {
        let vecs = embedder.embed_batch(chunk)?;
        for (t, v) in chunk.iter().zip(vecs) {
            map.insert(t.clone(), v);
        }
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// Loaders — mirror the Go scrub/transfer loadCandidates identity-card joins.
// ---------------------------------------------------------------------------

const SELECT_LABELED: &str = r#"
    SELECT nae.article_id, na.title, COALESCE(na.description,'') AS description,
           p.name,
           COALESCE(p.nationality,'')                    AS nationality,
           COALESCE(ct.name,'')                          AS current_club,
           COALESCE(NULLIF(pos.position,'Unknown'), '')  AS position,
           nae.match_confidence::float8                  AS match_confidence,
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
    WHERE nae.entity_type = 'player' AND nae.match_confidence < 1.0
"#;

async fn load_false(pool: &PgPool) -> Result<Vec<LabeledLink>> {
    sqlx::query_as::<_, LabeledLink>(&format!("{SELECT_LABELED} AND nae.vetted IS FALSE"))
        .fetch_all(pool)
        .await
        .context("load FALSE links")
}

/// Deterministic pseudo-random TRUE sample (md5 of the link key) so the experiment is repeatable.
async fn load_true_sample(pool: &PgPool, limit: i64) -> Result<Vec<LabeledLink>> {
    sqlx::query_as::<_, LabeledLink>(&format!(
        "{SELECT_LABELED} AND nae.vetted IS TRUE \
         ORDER BY md5(nae.article_id::text || ':' || nae.entity_id::text) LIMIT $1"
    ))
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("load TRUE sample")
}

// ---------------------------------------------------------------------------
// Metrics + report.
// ---------------------------------------------------------------------------

fn mean(xs: &[f32]) -> f32 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.iter().sum::<f32>() / xs.len() as f32
}

fn median(xs: &[f32]) -> f32 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// auc is the probability a random positive (TRUE) scores above a random negative (FALSE) —
/// the rank-sum (Mann-Whitney U) estimator with tie-averaged ranks. 0.5 = no separation, 1.0 =
/// perfect. Threshold-free, so it is the headline "do embeddings separate?" number.
fn auc(pos: &[f32], neg: &[f32]) -> f64 {
    let mut all: Vec<(f32, bool)> = pos
        .iter()
        .map(|&c| (c, true))
        .chain(neg.iter().map(|&c| (c, false)))
        .collect();
    all.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let n = all.len();
    let mut ranks = vec![0.0f64; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && all[j + 1].0 == all[i].0 {
            j += 1;
        }
        let avg = ((i + 1) + (j + 1)) as f64 / 2.0;
        for r in ranks.iter_mut().take(j + 1).skip(i) {
            *r = avg;
        }
        i = j + 1;
    }
    let sum_pos: f64 = all
        .iter()
        .zip(&ranks)
        .filter(|((_, p), _)| *p)
        .map(|(_, r)| *r)
        .sum();
    let (np, nn) = (pos.len() as f64, neg.len() as f64);
    (sum_pos - np * (np + 1.0) / 2.0) / (np * nn)
}

/// best_threshold sweeps a grid and returns (threshold, precision, recall, f1, tpr, fpr) at the
/// max-F1 point (classify TRUE when cosine ≥ threshold).
fn best_threshold(pos: &[f32], neg: &[f32]) -> (f32, f32, f32, f32, f32, f32) {
    let lo = pos.iter().chain(neg).cloned().fold(f32::INFINITY, f32::min);
    let hi = pos.iter().chain(neg).cloned().fold(f32::NEG_INFINITY, f32::max);
    let mut best = (lo, 0.0, 0.0, -1.0, 0.0, 0.0);
    for k in 0..=100 {
        let t = lo + (hi - lo) * (k as f32 / 100.0);
        let tp = pos.iter().filter(|&&c| c >= t).count() as f32;
        let fn_ = pos.len() as f32 - tp;
        let fp = neg.iter().filter(|&&c| c >= t).count() as f32;
        let tn = neg.len() as f32 - fp;
        let precision = if tp + fp > 0.0 { tp / (tp + fp) } else { 0.0 };
        let recall = if tp + fn_ > 0.0 { tp / (tp + fn_) } else { 0.0 };
        let f1 = if precision + recall > 0.0 {
            2.0 * precision * recall / (precision + recall)
        } else {
            0.0
        };
        let tpr = recall;
        let fpr = if fp + tn > 0.0 { fp / (fp + tn) } else { 0.0 };
        if f1 > best.3 {
            best = (t, precision, recall, f1, tpr, fpr);
        }
    }
    best
}

fn report(pos: &[f32], neg: &[f32], scored: &[(f32, &LabeledLink)]) {
    let a = auc(pos, neg);
    let (t, prec, rec, f1, tpr, fpr) = best_threshold(pos, neg);
    println!("\n========================  RESOLVE EXPERIMENT  ========================");
    println!("signal: cosine(article context, candidate identity card), BGE-small CPU");
    println!("labels: Gemma scrub verdict (news_article_entities.vetted), secondary links\n");
    println!(
        "  TRUE  (genuine)  n={:5}   cosine mean {:.3}  median {:.3}",
        pos.len(),
        mean(pos),
        median(pos)
    );
    println!(
        "  FALSE (impostor) n={:5}   cosine mean {:.3}  median {:.3}",
        neg.len(),
        mean(neg),
        median(neg)
    );
    println!("  separation (mean TRUE − mean FALSE): {:.3}", mean(pos) - mean(neg));
    println!("\n  AUC (P[TRUE>FALSE], threshold-free): {a:.3}");
    println!(
        "  best-F1 threshold {t:.3}:  precision {prec:.3}  recall {rec:.3}  F1 {f1:.3}  (TPR {tpr:.3} / FPR {fpr:.3})"
    );
    let verdict = if a >= 0.80 {
        "STRONG separation → embeddings can pre-filter; Gemma only on the ambiguous band."
    } else if a >= 0.65 {
        "MODERATE → useful pre-filter, but the hybrid needs the structured club tie-break + a Gemma band."
    } else {
        "WEAK → embeddings alone don't separate; keep Gemma primary, embeddings as a recall aid only."
    };
    println!("\n  READ: AUC {a:.3} — {verdict}");

    // Qualitative: the hard cases — FALSE that scored HIGH (would slip a cosine pre-filter) and
    // TRUE that scored LOW (a cosine filter would wrongly drop). These show WHERE the signal fails.
    let mut hi_false: Vec<&(f32, &LabeledLink)> =
        scored.iter().filter(|(_, l)| !l.vetted).collect();
    hi_false.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let mut lo_true: Vec<&(f32, &LabeledLink)> = scored.iter().filter(|(_, l)| l.vetted).collect();
    lo_true.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    println!("\n  hardest FALSE (high cosine — would slip a pre-filter):");
    for (c, l) in hi_false.iter().take(5) {
        println!("    {:.3}  {}  · article {}", c, l.name, l.article_id);
    }
    println!("  hardest TRUE (low cosine — a filter would wrongly drop):");
    for (c, l) in lo_true.iter().take(5) {
        println!("    {:.3}  {}  · article {}", c, l.name, l.article_id);
    }
    println!("======================================================================\n");
}
