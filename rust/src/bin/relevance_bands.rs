//! relevance_bands — offline banding analysis for the scrub relevance gate (rebuild plan, Phase C).
//!
//! Two questions, both answered against verdicts already on disk, with no prod risk:
//!
//!   1. **Of the candidates that reach the GPU, what fraction does the model KEEP?** The live gate
//!      is asymmetric (`resolve.rs`): `cosine >= keep_threshold` auto-keeps with no model call, and
//!      only the model may reject. The database records the merged outcome, so CPU fast-path keeps
//!      and GPU keeps cannot be told apart from SQL. Recomputing the cosine separates them.
//!   2. **How much of the proxy's weakness is the input pair rather than the model?** Run 1 found
//!      every cosine crushed into 0.55–0.64 with only 0.063 separation, and only 1.4% of candidates
//!      clearing the keep line. A team's identity text is `"Arsenal (team)"` — two tokens against a
//!      thirty-word article. This binary embeds several richer identity renderings side by side over
//!      the same articles, so "the proxy is weak" and "the proxy was starved" are distinguishable.
//!
//! SEPARATION (mean cosine of kept minus mean cosine of rejected) is the metric that matters, not
//! mean cosine. Adding league/country tokens lifts every comparison — including the rejections —
//! because thousands of articles mention the Premier League. A variant that raises both means
//! equally has added noise, not signal, and the sweep will show it as unchanged erasure.
//!
//! SAFETY: strictly read-only. No writes, no `pipeline_work`, no Ollama, no network beyond the
//! one-time HuggingFace model fetch the embedder already caches. Embeddings run on the CPU.
//!
//!   cargo run --release --bin relevance_bands -- [--sport FOOTBALL] [--limit 20000]

use anyhow::{Context, Result};
use scoracle_cognition::config::{EmbedConfig, ResolveConfig};
use scoracle_cognition::embed::{cosine_similarity, Embedder};
use scoracle_cognition::novelty::article_text;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::collections::HashMap;

/// BGE_QUERY_PREFIX — bge-small-en-v1.5 is trained for ASYMMETRIC retrieval (short query vs long
/// passage) and its model card instructs prepending this to the QUERY side. Production omits it and
/// embeds `"Arsenal (team)"` raw against a full article, which is exactly the s2p shape the prefix
/// exists for. Testing it as a variant rather than assuming either way.
const BGE_QUERY_PREFIX: &str = "Represent this sentence for searching relevant passages: ";

/// The identity renderings under test. V0 is exactly what production embeds today.
const VARIANTS: &[&str] = &[
    "V0 current: \"Name (team)\"",
    "V0P V0 + BGE query prefix",
    "V1 + aliases",
    "V2 + aliases, city, country, sport",
    "V3 + aliases, city, country, sport, league, venue",
];

struct Decided {
    context: String,
    identities: Vec<String>, // one per VARIANTS entry, same order
    vetted: bool,
    lang: &'static str,
    article_id: i64,
    title_pos: Option<i32>, // char offset of the team in the HEADLINE; None = description-only
    n_links: i32,           // teams linked to this same article — the roundup detector
}

/// Stopword sets for a deliberately small language guess — the corpus turned out to be only 24%
/// English, so every result is reported per-language as well as overall.
const STOPWORDS: &[(&str, &[&str])] = &[
    ("en", &["the", "and", "for", "with", "after", "from", "his", "has", "that", "will", "over", "against", "into"]),
    ("es", &["el", "la", "los", "las", "por", "para", "con", "del", "que", "una", "sus", "está", "más", "según"]),
    ("fr", &["le", "les", "des", "une", "pour", "avec", "dans", "sur", "qui", "est", "aux", "son", "plus", "après"]),
    ("de", &["der", "die", "das", "und", "für", "mit", "von", "den", "ist", "auf", "dem", "ein", "nicht", "nach"]),
    ("it", &["il", "lo", "gli", "che", "per", "con", "del", "della", "una", "sono", "più", "dopo", "nel", "sul"]),
    ("pt", &["o", "os", "as", "por", "para", "com", "do", "da", "dos", "das", "uma", "não", "mais", "após"]),
    ("nl", &["de", "het", "een", "van", "voor", "met", "niet", "aan", "zijn", "naar", "over", "door", "maar", "ook"]),
];

fn guess_lang(text: &str) -> &'static str {
    let lower = text.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.len() < 4 {
        return "unknown";
    }
    let mut best = ("unknown", 0usize);
    let mut tied = false;
    for (lang, words) in STOPWORDS {
        let n = tokens.iter().filter(|t| words.contains(t)).count();
        if n > best.1 {
            best = (lang, n);
            tied = false;
        } else if n == best.1 && n > 0 {
            tied = true;
        }
    }
    if best.1 == 0 || tied {
        "unknown"
    } else {
        best.0
    }
}

/// The natural-language sport phrase. Articles say "the Premier League club" and "the NBA team",
/// never "FOOTBALL", so the card should read the way the corpus reads.
fn sport_phrase(sport: &str) -> &'static str {
    match sport {
        "FOOTBALL" => "soccer football club",
        "NBA" => "basketball team",
        "NFL" => "American football team",
        _ => "team",
    }
}

/// Renders one team under every variant. Aliases are capped: the tail of a long alias list is
/// abbreviations and noise, and a 256-token budget spent on junk crowds out the discriminative
/// tokens (name, city, venue).
fn build_identities(
    name: &str,
    sport: &str,
    aliases: &[String],
    city: &str,
    country: &str,
    league: &str,
    venue: &str,
) -> Vec<String> {
    let alias_list: Vec<&str> = aliases
        .iter()
        .map(|a| a.trim())
        .filter(|a| !a.is_empty() && !a.eq_ignore_ascii_case(name))
        .take(4)
        .collect();
    let alias_clause = if alias_list.is_empty() {
        String::new()
    } else {
        format!(", also known as {}", alias_list.join(", "))
    };

    let mut place = Vec::new();
    if !city.is_empty() {
        place.push(city.to_string());
    }
    if !country.is_empty() && country != city {
        place.push(country.to_string());
    }
    let place_clause = if place.is_empty() {
        String::new()
    } else {
        format!(" from {}", place.join(", "))
    };

    let v0 = format!("{name} (team)");
    let v0p = format!("{BGE_QUERY_PREFIX}{name} (team)");
    let v1 = format!("{name}{alias_clause}.");
    let v2 = format!(
        "{name}{alias_clause}, a {}{place_clause}.",
        sport_phrase(sport)
    );
    let mut v3 = format!(
        "{name}{alias_clause}, a {}{place_clause}",
        sport_phrase(sport)
    );
    if !league.is_empty() {
        v3.push_str(&format!(", playing in the {league}"));
    }
    if !venue.is_empty() {
        v3.push_str(&format!(", home stadium {venue}"));
    }
    v3.push('.');

    vec![v0, v0p, v1, v2, v3]
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut sport_filter: Option<String> = None;
    let mut limit: i64 = 20000;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--sport" => {
                sport_filter = args.get(i + 1).cloned();
                i += 2;
            }
            "--limit" => {
                limit = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(limit);
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
    let keep_t = ResolveConfig::from_env()
        .map(|c| c.keep_threshold)
        .unwrap_or(0.75);

    // The 0.95 band is team-primaries only (Go sets it via isTeamEntity), so this pulls team
    // metadata directly rather than going through the player-shaped IdentityCard.
    let rows = sqlx::query(
        r#"
        SELECT t.name                                        AS name,
               nae.sport                                     AS sport,
               COALESCE(t.search_aliases, ARRAY[]::text[])   AS aliases,
               COALESCE(t.city, '')                          AS city,
               COALESCE(t.country, '')                       AS country,
               COALESCE(l.name, '')                          AS league,
               COALESCE(t.venue_name, '')                    AS venue,
               nae.vetted                                    AS vetted,
               nae.article_id                                AS article_id,
               nae.title_pos::int                            AS title_pos,
               (SELECT count(*) FROM news_article_entities x
                 WHERE x.article_id = nae.article_id AND x.match_confidence = 0.95)::int AS n_links,
               a.title                                       AS title,
               COALESCE(a.description, '')                   AS description
        FROM news_article_entities nae
        JOIN news_articles a ON a.id = nae.article_id
        JOIN teams t ON t.id = nae.entity_id AND t.sport = nae.sport
        LEFT JOIN leagues l ON l.id = t.league_id
        WHERE nae.match_confidence = 0.95
          AND nae.entity_type = 'team'
          AND nae.vetted IS NOT NULL
          AND ($1::text IS NULL OR nae.sport = $1)
        ORDER BY nae.scrubbed_at DESC
        LIMIT $2
        "#,
    )
    .bind(sport_filter.as_deref())
    .bind(limit)
    .fetch_all(&pool)
    .await
    .context("load decided links")?;

    println!("loaded {} decided team links", rows.len());
    if rows.is_empty() {
        return Ok(());
    }

    let mut decided = Vec::with_capacity(rows.len());
    for r in &rows {
        let name: String = r.get("name");
        let sport: String = r.get("sport");
        let aliases: Vec<String> = r.get("aliases");
        let title: String = r.get("title");
        let description: String = r.get("description");
        let context = article_text(&title, &description);
        let lang = guess_lang(&context);
        decided.push(Decided {
            identities: build_identities(
                &name,
                &sport,
                &aliases,
                &r.get::<String, _>("city"),
                &r.get::<String, _>("country"),
                &r.get::<String, _>("league"),
                &r.get::<String, _>("venue"),
            ),
            context,
            vetted: r.get("vetted"),
            lang,
            article_id: r.get("article_id"),
            title_pos: r.get("title_pos"),
            n_links: r.get("n_links"),
        });
    }

    println!("\n=== identity rendering samples ===");
    if let Some(d) = decided.first() {
        for (v, id) in VARIANTS.iter().zip(&d.identities) {
            println!("  {v}\n     {id}");
        }
    }

    // Embed every distinct string once. Article contexts dominate the count; identity strings
    // collapse to ~204 teams x 4 variants, so testing four variants costs almost nothing extra.
    let embedder = Embedder::from_config(&EmbedConfig::from_env()?).context("load embedder")?;
    let mut uniq: Vec<String> = Vec::new();
    let mut idx: HashMap<String, usize> = HashMap::new();
    let mut intern = |s: &String, uniq: &mut Vec<String>, idx: &mut HashMap<String, usize>| {
        if !idx.contains_key(s) {
            idx.insert(s.clone(), uniq.len());
            uniq.push(s.clone());
        }
    };
    for d in &decided {
        intern(&d.context, &mut uniq, &mut idx);
        for id in &d.identities {
            intern(id, &mut uniq, &mut idx);
        }
    }
    println!("\nembedding {} distinct strings on CPU...", uniq.len());
    let mut vectors = Vec::with_capacity(uniq.len());
    for (n, chunk) in uniq.chunks(64).enumerate() {
        vectors.extend(embedder.embed_batch(chunk).context("embed batch")?);
        if n % 25 == 0 {
            println!("  {} / {}", vectors.len(), uniq.len());
        }
    }

    for (vi, vname) in VARIANTS.iter().enumerate() {
        let scored: Vec<(f32, bool, &'static str)> = decided
            .iter()
            .map(|d| {
                let c = cosine_similarity(
                    &vectors[idx[&d.context]],
                    &vectors[idx[&d.identities[vi]]],
                );
                (c, d.vetted, d.lang)
            })
            .collect();
        report(vname, &scored, keep_t);
    }

    // ---- CONTRASTIVE scoring: the embedder used as a RANKER, not a classifier ----
    //
    // An article about football scores high against EVERY club — it is full of football words. That
    // shared baseline is what crushes every cosine into 0.55-0.64 and makes an absolute threshold
    // meaningless. The question was never "how similar is this article to Arsenal", it is "is this
    // article about Arsenal RATHER THAN the other 203 clubs". Two parameter-light ways to ask it:
    //
    //   z-score — how many standard deviations the target sits above this article's mean similarity
    //             to the whole club universe. Removes the article's own genericness.
    //   rank    — where the target lands when every club is sorted by similarity to this article.
    //             Scale-free by construction, so it cannot be broken by a shifted cosine band.
    //
    // Retrieval is what embedding models are actually good at. We were using a ranker as a
    // classifier, which is the mismatch worth testing before declaring the candle useless.
    println!("\n\n########## CONTRASTIVE SCORING (embedder as ranker) ##########");
    for (vi, vname) in VARIANTS.iter().enumerate() {
        let mut universe: Vec<usize> = decided.iter().map(|d| idx[&d.identities[vi]]).collect();
        universe.sort_unstable();
        universe.dedup();

        // Per distinct article: mean/sd of similarity across every club card, plus the sorted
        // similarities so a target's rank is a lookup rather than a rescan.
        let mut stats: HashMap<usize, (f32, f32, Vec<f32>)> = HashMap::new();
        for d in &decided {
            let ci = idx[&d.context];
            if stats.contains_key(&ci) {
                continue;
            }
            let sims: Vec<f32> = universe
                .iter()
                .map(|&u| cosine_similarity(&vectors[ci], &vectors[u]))
                .collect();
            let n = sims.len().max(1) as f32;
            let mean = sims.iter().sum::<f32>() / n;
            let sd = (sims.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / n).sqrt().max(1e-6);
            let mut sorted = sims.clone();
            sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
            stats.insert(ci, (mean, sd, sorted));
        }

        let z_scored: Vec<(f32, bool, &'static str)> = decided
            .iter()
            .map(|d| {
                let ci = idx[&d.context];
                let (mean, sd, _) = &stats[&ci];
                let c = cosine_similarity(&vectors[ci], &vectors[idx[&d.identities[vi]]]);
                ((c - mean) / sd, d.vetted, d.lang)
            })
            .collect();

        let rank_scored: Vec<(f32, bool, &'static str)> = decided
            .iter()
            .map(|d| {
                let ci = idx[&d.context];
                let (_, _, sorted) = &stats[&ci];
                let c = cosine_similarity(&vectors[ci], &vectors[idx[&d.identities[vi]]]);
                // 1.0 = top of the universe, 0.0 = bottom. Scale-free.
                let better = sorted.iter().filter(|&&s| s > c).count() as f32;
                (1.0 - better / sorted.len().max(1) as f32, d.vetted, d.lang)
            })
            .collect();

        let m = |v: &[(f32, bool, &'static str)], want: bool| -> f32 {
            let xs: Vec<f32> = v.iter().filter(|(_, k, _)| *k == want).map(|(c, _, _)| *c).collect();
            if xs.is_empty() { 0.0 } else { xs.iter().sum::<f32>() / xs.len() as f32 }
        };
        println!(
            "\n  {vname}  (club universe = {} cards)",
            universe.len()
        );
        println!(
            "    absolute cosine : kept {:.3}  rejected {:.3}  separation {:.3}",
            m(&decided.iter().map(|d| (cosine_similarity(&vectors[idx[&d.context]], &vectors[idx[&d.identities[vi]]]), d.vetted, d.lang)).collect::<Vec<_>>(), true),
            m(&decided.iter().map(|d| (cosine_similarity(&vectors[idx[&d.context]], &vectors[idx[&d.identities[vi]]]), d.vetted, d.lang)).collect::<Vec<_>>(), false),
            separation(&decided.iter().map(|d| (cosine_similarity(&vectors[idx[&d.context]], &vectors[idx[&d.identities[vi]]]), d.vetted, d.lang)).collect::<Vec<_>>())
        );
        println!(
            "    z-score         : kept {:+.3} rejected {:+.3} separation {:.3}",
            m(&z_scored, true), m(&z_scored, false), separation(&z_scored)
        );
        println!(
            "    rank percentile : kept {:.3}  rejected {:.3}  separation {:.3}",
            m(&rank_scored, true), m(&rank_scored, false), separation(&rank_scored)
        );
    }

    println!("\n\n########## SUMMARY: separation by variant ##########");
    println!("  variant                                              all      en   non-en   auto-keep@0.75");
    for (vi, vname) in VARIANTS.iter().enumerate() {
        let scored: Vec<(f32, bool, &'static str)> = decided
            .iter()
            .map(|d| {
                let c = cosine_similarity(
                    &vectors[idx[&d.context]],
                    &vectors[idx[&d.identities[vi]]],
                );
                (c, d.vetted, d.lang)
            })
            .collect();
        let ak = scored.iter().filter(|(c, _, _)| *c >= keep_t).count();
        println!(
            "  {:<50} {:.3}   {:.3}   {:.3}   {:>5.1}%",
            vname,
            separation(&scored),
            separation(&scored.iter().copied().filter(|(_, _, l)| *l == "en").collect::<Vec<_>>()),
            separation(&scored.iter().copied().filter(|(_, _, l)| *l != "en" && *l != "unknown").collect::<Vec<_>>()),
            pct(ak, scored.len())
        );
    }
    println!("\n  Separation is the number that matters. A variant that lifts mean cosine on BOTH\n  kept and rejected has added shared vocabulary, not discriminating signal.");

    // ---- The classifier: combine the cheap CPU cues instead of thresholding any one of them ----
    println!("\n\n########## CPU CLASSIFIER (cosine + title position + roundup) ##########");
    println!("Train/test split is by ARTICLE (id % 10 < 7), never by link, so links from the same");
    println!("article cannot leak across the split and inflate the result.");
    let nf = FEATURES.len();
    for (vi, vname) in VARIANTS.iter().enumerate() {
        let mut universe: Vec<usize> = decided.iter().map(|d| idx[&d.identities[vi]]).collect();
        universe.sort_unstable();
        universe.dedup();
        let mut cstats: HashMap<usize, (f32, f32, Vec<f32>)> = HashMap::new();
        for d in &decided {
            let ci = idx[&d.context];
            if cstats.contains_key(&ci) { continue; }
            let sims: Vec<f32> = universe.iter().map(|&u| cosine_similarity(&vectors[ci], &vectors[u])).collect();
            let n = sims.len().max(1) as f32;
            let mean = sims.iter().sum::<f32>() / n;
            let sd = (sims.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / n).sqrt().max(1e-6);
            let mut sorted = sims.clone();
            sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
            cstats.insert(ci, (mean, sd, sorted));
        }
        let mut rows: Vec<([f64; 6], bool, bool)> = decided
            .iter()
            .map(|d| {
                let ci = idx[&d.context];
                let c = cosine_similarity(&vectors[ci], &vectors[idx[&d.identities[vi]]]);
                let (mean, sd, sorted) = &cstats[&ci];
                let z = (c - mean) / sd;
                let better = sorted.iter().filter(|&&s| s > c).count() as f32;
                let rank = 1.0 - better / sorted.len().max(1) as f32;
                let is_train = (d.article_id % 10) < 7;
                (features(c, d.title_pos, d.n_links, z, rank), d.vetted, is_train)
            })
            .collect();
        standardize(&mut rows, nf);
        let w = fit_logistic(&rows, nf);

        println!("\n\n===== {vname} =====");
        print!("  weights (standardized):  bias {:+.3}", w[nf]);
        for (j, f) in FEATURES.iter().enumerate() {
            print!("   {f} {:+.3}", w[j]);
        }
        println!();

        let test: Vec<(f64, bool)> = rows
            .iter()
            .filter(|(_, _, tr)| !*tr)
            .map(|(x, y, _)| (predict(&w, x, nf), *y))
            .collect();
        sweep_scores("MODEL (all features)", &test);

        // Cosine-only baseline fit the same way, on the same split — otherwise "the model is
        // better" could just be the split or the standardization talking.
        let mut cos_rows: Vec<([f64; 6], bool, bool)> = rows
            .iter()
            .map(|(x, y, tr)| ([x[0], 0.0, 0.0, 0.0, 0.0, 0.0], *y, *tr))
            .collect();
        let wc = fit_logistic(&cos_rows, nf);
        for r in cos_rows.iter_mut() {
            r.0 = [r.0[0], 0.0, 0.0, 0.0, 0.0, 0.0];
        }
        let base: Vec<(f64, bool)> = cos_rows
            .iter()
            .filter(|(_, _, tr)| !*tr)
            .map(|(x, y, _)| (predict(&wc, x, nf), *y))
            .collect();
        sweep_scores("BASELINE (cosine only)", &base);
    }

    Ok(())
}

/// FEATURES — an automation of what a person does scanning a Google News results page. You can spot
/// the noise instantly because you are holding the search intent in your head; each of these is one
/// of the cues you use without naming it:
///
///   * `cosine`     — does the blurb read like it is about this club? (needs a rich card, V1+)
///   * `in_title`   — is the club named in the HEADLINE, or only buried in the snippet?
///   * `title_early`— does it LEAD the headline, or trail at the end of a list?
///   * `log_links`  — is this a story, or a roundup naming eight clubs? (3.6% keep at 8 links)
///
/// Independently each is mediocre; the point is that they fail on different articles. `title_pos`
/// rescues the low-cosine article whose headline leads with the club — precisely the case that
/// erased a player at L5 and got auto-drop banned.
const FEATURES: &[&str] = &["cosine", "in_title", "title_early", "log_links", "zscore", "rank"];

fn features(cosine: f32, title_pos: Option<i32>, n_links: i32, z: f32, rank: f32) -> [f64; 6] {
    let in_title = if title_pos.is_some() { 1.0 } else { 0.0 };
    let title_early = match title_pos {
        Some(p) => 1.0 - (p.clamp(0, 120) as f64 / 120.0),
        None => 0.0,
    };
    [
        cosine as f64,
        in_title,
        title_early,
        (n_links.max(1) as f64).ln(),
        z as f64,
        rank as f64,
    ]
}

/// Standardize with TRAIN-set statistics only — using the whole set would leak test information
/// into the scaling and flatter the result.
fn standardize(rows: &mut [([f64; 6], bool, bool)], n: usize) -> ([f64; 6], [f64; 6]) {
    let mut mean = [0.0; 6];
    let mut sd = [0.0; 6];
    let train: Vec<&([f64; 6], bool, bool)> = rows.iter().filter(|(_, _, tr)| *tr).collect();
    let m = train.len().max(1) as f64;
    for j in 0..n {
        mean[j] = train.iter().map(|(x, _, _)| x[j]).sum::<f64>() / m;
        let var = train.iter().map(|(x, _, _)| (x[j] - mean[j]).powi(2)).sum::<f64>() / m;
        sd[j] = var.sqrt().max(1e-9);
    }
    for (x, _, _) in rows.iter_mut() {
        for j in 0..n {
            x[j] = (x[j] - mean[j]) / sd[j];
        }
    }
    (mean, sd)
}

/// Plain batch logistic regression — deliberately the smallest model that can combine these cues.
/// Coefficients stay inspectable, training is seconds of CPU on verdicts the GPU already produced,
/// and it can be refit nightly. No download, no inference cost, no new dependency.
fn fit_logistic(rows: &[([f64; 6], bool, bool)], n: usize) -> Vec<f64> {
    let mut w = vec![0.0f64; n + 1]; // + bias
    let train: Vec<&([f64; 6], bool, bool)> = rows.iter().filter(|(_, _, tr)| *tr).collect();
    let m = train.len().max(1) as f64;
    let (lr, l2, iters) = (0.5, 1e-4, 3000);
    for _ in 0..iters {
        let mut grad = vec![0.0f64; n + 1];
        for (x, y, _) in &train {
            let mut z = w[n];
            for j in 0..n {
                z += w[j] * x[j];
            }
            let p = 1.0 / (1.0 + (-z).exp());
            let err = p - if *y { 1.0 } else { 0.0 };
            for j in 0..n {
                grad[j] += err * x[j];
            }
            grad[n] += err;
        }
        for j in 0..=n {
            let reg = if j < n { l2 * w[j] } else { 0.0 };
            w[j] -= lr * (grad[j] / m + reg);
        }
    }
    w
}

fn predict(w: &[f64], x: &[f64; 6], n: usize) -> f64 {
    let mut z = w[n];
    for j in 0..n {
        z += w[j] * x[j];
    }
    1.0 / (1.0 + (-z).exp())
}

/// Sweeps a rejection threshold on held-out data and prints the only two numbers that decide
/// whether this can own exclusion: model calls saved, and share of the genuine corpus erased.
fn sweep_scores(label: &str, scores: &[(f64, bool)]) {
    let total = scores.len();
    let kept_total = scores.iter().filter(|(_, v)| *v).count();
    if total == 0 || kept_total == 0 {
        println!("  {label}: (insufficient data)");
        return;
    }
    println!(
        "\n  {label}  —  held-out n={total}, genuine links={kept_total}"
    );
    println!("    reject below   auto-dropped   correct    ERASED (of real corpus)   calls saved");
    for t in [0.02f64, 0.05, 0.10, 0.15, 0.20, 0.25, 0.30, 0.40, 0.50] {
        let dropped: Vec<&(f64, bool)> = scores.iter().filter(|(p, _)| *p < t).collect();
        let erased = dropped.iter().filter(|(_, v)| *v).count();
        println!(
            "    {:>10.2}   {:>10}   {:>7} ({:>5.1}%)   {:>7} ({:>6.2}%)          {:>6.1}%",
            t,
            dropped.len(),
            dropped.len() - erased,
            pct(dropped.len() - erased, dropped.len().max(1)),
            erased,
            pct(erased, kept_total),
            pct(dropped.len(), total)
        );
    }
}

fn separation(scored: &[(f32, bool, &'static str)]) -> f32 {
    let m = |want: bool| -> f32 {
        let v: Vec<f32> = scored
            .iter()
            .filter(|(_, k, _)| *k == want)
            .map(|(c, _, _)| *c)
            .collect();
        if v.is_empty() {
            0.0
        } else {
            v.iter().sum::<f32>() / v.len() as f32
        }
    };
    m(true) - m(false)
}

fn report(label: &str, scored: &[(f32, bool, &'static str)], keep_t: f32) {
    println!("\n\n########## {label} ##########");
    let total = scored.len();
    if total == 0 {
        println!("(empty)");
        return;
    }
    let kept_total = scored.iter().filter(|(_, v, _)| *v).count();
    let mean = |want: bool| -> f32 {
        let v: Vec<f32> = scored
            .iter()
            .filter(|(_, k, _)| *k == want)
            .map(|(c, _, _)| *c)
            .collect();
        if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 }
    };
    println!(
        "decided {total}  kept {kept_total} ({:.1}%)   mean cosine kept {:.3} / rejected {:.3}  SEPARATION {:.3}",
        pct(kept_total, total),
        mean(true),
        mean(false),
        mean(true) - mean(false)
    );

    let above = scored.iter().filter(|(c, _, _)| *c >= keep_t).count();
    let above_kept = scored.iter().filter(|(c, v, _)| *c >= keep_t && *v).count();
    let below: Vec<&(f32, bool, &str)> = scored.iter().filter(|(c, _, _)| *c < keep_t).collect();
    let below_kept = below.iter().filter(|(_, v, _)| *v).count();
    println!(
        "auto-keep @ {keep_t}: {above} ({:.1}% of all), precision {:.1}%   |   reaching GPU: {} ({:.1}%), of which model keeps {:.1}%",
        pct(above, total),
        pct(above_kept, above.max(1)),
        below.len(),
        pct(below.len(), total),
        pct(below_kept, below.len().max(1))
    );

    println!("\n  t     auto-dropped   correct    ERASED (was kept)   calls saved");
    let mut t = 0.35f32;
    while t <= keep_t + 0.0001 {
        let dropped: Vec<&(f32, bool, &str)> = below.iter().copied().filter(|(c, _, _)| *c < t).collect();
        let erased = dropped.iter().filter(|(_, v, _)| *v).count();
        println!(
            "  {:.2}  {:>10}   {:>7} ({:>5.1}%)   {:>7} ({:>5.2}%)   {:>6} ({:.1}%)",
            t,
            dropped.len(),
            dropped.len() - erased,
            pct(dropped.len() - erased, dropped.len().max(1)),
            erased,
            pct(erased, kept_total.max(1)),
            dropped.len(),
            pct(dropped.len(), below.len().max(1))
        );
        t += 0.05;
    }
}

fn pct(n: usize, d: usize) -> f64 {
    if d == 0 {
        0.0
    } else {
        n as f64 * 100.0 / d as f64
    }
}
