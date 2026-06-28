//! L5 Resolve SHADOW — the at-scale, read-only validation of the hybrid `resolve_set` gate.
//!
//! L4 proved the gate works end-to-end but on only 43 candidates. This runs it over the WHOLE
//! labeled secondary-link corpus (thousands) and records every verdict beside Gemma's production
//! `vetted` label in `resolve_shadow` (mig 108), so the live agreement — and the Álvarez
//! current-club-lag FN — can be measured AT SCALE before any pipeline wiring. It is the disciplined
//! first increment of "wire the hybrid gate into the live scrub": SHADOW first, confirm at scale,
//! settle the FN, THEN flip.
//!
//! READ-ONLY on the pipeline: it SELECTs the labeled corpus + embeds + (on a bounded sample) calls
//! Gemma, and writes ONLY to `resolve_shadow`. It NEVER UPDATEs `news_article_entities.vetted` (so
//! the mig-103 enqueue trigger never fires off it) and NEVER claims `pipeline_work`. Go's
//! maintenance scrub stays the live owner of the vetted flag.
//!
//! Two measurements, by design:
//!   * **Auto-decide safety (full universe, ZERO Gemma).** For the keep/drop bands the hybrid verdict
//!     IS the cheap cosine decision — so auto-keep precision and, critically, the auto-drop FN rate
//!     (genuine links the cheap band would wrongly drop — the Álvarez risk) are full-corpus numbers
//!     that need no GPU. Sliced by the recent-mover diagnostics so the band-vs-refresh fork is settled
//!     with data. This is the statistically robust signal.
//!   * **Real-gate agreement (bounded sample, with Gemma).** The actual `Harness::resolve_set` (embed
//!     band → Gemma adjudicates the ambiguous middle → fail-closed) over a bounded sample of articles
//!     with an ambiguous link, vs Gemma's labels — confirming the end-to-end primitive at a
//!     much-larger-than-43 scale. The Gemma calls are the only slow part, so they are budgeted.
//!
//! Usage (env: DATABASE_PRIVATE_URL + OLLAMA_* + COGNITION_EMBED_*/COGNITION_RESOLVE_* optional):
//!   resolve_shadow
//!     COGNITION_SHADOW_ARTICLES=0      # max articles to embed (0 = the whole corpus; default 0)
//!     COGNITION_SHADOW_ADJUDICATE=250  # max articles to send the ambiguous band to Gemma (default 250)

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

/// One labeled secondary player link of an article, plus the recent-mover diagnostics.
struct LinkRow {
    article_id: i64,
    title: String,
    description: String,
    sport: String,
    entity_id: i32,
    name: String,
    nationality: String,
    current_club: String,
    position: String,
    vetted: bool,
    in_transfer_rumors: bool,
    career_teams: i16,
}

impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for LinkRow {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self {
            article_id: row.try_get("article_id")?,
            title: row.try_get("title")?,
            description: row.try_get("description")?,
            sport: row.try_get("sport")?,
            entity_id: row.try_get("entity_id")?,
            name: row.try_get("name")?,
            nationality: row.try_get("nationality")?,
            current_club: row.try_get("current_club")?,
            position: row.try_get("position")?,
            vetted: row.try_get("vetted")?,
            in_transfer_rumors: row.try_get("in_transfer_rumors")?,
            career_teams: row.try_get("career_teams")?,
        })
    }
}

/// The band the cheap cosine pre-filter assigns — mirrors `resolve::classify` (kept in sync; the
/// bin inlines it like `resolve_eval` does, so the gate's policy stays the one place it is defined).
fn band_of(cosine: f32, keep: f32, drop: f32) -> &'static str {
    if cosine >= keep {
        "keep"
    } else if cosine < drop {
        "drop"
    } else {
        "ambiguous"
    }
}

/// One assembled shadow row, ready to persist.
struct ShadowRow {
    article_id: i64,
    sport: String,
    entity_id: i32,
    name: String,
    gemma_vetted: bool,
    cosine: f32,
    band: &'static str,
    auto_verdict: bool,
    hybrid_verdict: Option<bool>,
    decided_by: &'static str,
    in_transfer_rumors: bool,
    career_teams: i16,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    let max_articles: i64 = env_i64("COGNITION_SHADOW_ARTICLES", 0); // 0 = all
    let adjudicate_budget: usize = env_i64("COGNITION_SHADOW_ADJUDICATE", 250).max(0) as usize;

    println!("loading model {} (CPU)…", cfg.embed.model_repo);
    let embedder = Embedder::from_config(&cfg.embed).context("load embedder")?;
    let hx = Harness {
        pool,
        router: Router::from_config(&cfg.route, cfg.ollama_timeout, 1)?, // offline; single-flight
        embedder: Some(embedder),
        resolve: cfg.resolve.clone(),
    };
    let (keep, drop) = (cfg.resolve.keep_threshold, cfg.resolve.drop_threshold);
    let adjudicator = hx.router.for_role(Role::EmotionalNews).model().to_string();

    let articles = load_corpus(&hx.pool, max_articles).await?;
    let total_links: usize = articles.iter().map(|a| a.links.len()).sum();
    println!(
        "resolve_shadow — {} articles / {} secondary links, band keep≥{keep:.2}/drop<{drop:.2}, \
         embed {}, adjudicate ≤{adjudicate_budget} articles via {adjudicator}\n",
        articles.len(),
        total_links,
        cfg.embed.model_repo,
    );

    // Fresh run for this band config (idempotent re-run).
    sqlx::query("DELETE FROM resolve_shadow WHERE keep_threshold = $1 AND drop_threshold = $2")
        .bind(keep)
        .bind(drop)
        .execute(&hx.pool)
        .await
        .context("clear prior rows for this band")?;

    // Full-universe accumulators (the robust, zero-Gemma signal).
    let mut auc = AucAccumulator::default();
    let (mut n_keep, mut n_drop, mut n_amb) = (0u32, 0u32, 0u32);
    // Auto-decide confusion (band != ambiguous), the production risk.
    let (mut ak_tp, mut ak_fp) = (0u32, 0u32); // auto-keep: kept & vetted / kept & !vetted
    let (mut ad_tn, mut ad_fn) = (0u32, 0u32); // auto-drop: dropped & !vetted / dropped & vetted (the FN)
    let mut auto_drop_fns: Vec<FnRow> = Vec::new();
    // Bounded real-gate agreement (with Gemma).
    let (mut h_tp, mut h_fp, mut h_tn, mut h_fn) = (0u32, 0u32, 0u32, 0u32);
    let mut adjudicated_articles = 0usize;
    let mut adjudicated_links = 0usize;
    let mut hybrid_disagreements: Vec<String> = Vec::new();

    for (idx, art) in articles.iter().enumerate() {
        let context = article_text(&art.title, &art.description);
        let candidates: Vec<Candidate> = art.links.iter().map(to_candidate).collect();

        // One embed call: [context, identity_0, …]. The per-link cosine the band reads.
        let mut texts = Vec::with_capacity(candidates.len() + 1);
        texts.push(context.clone());
        texts.extend(candidates.iter().map(identity_text));
        let vecs = hx.embed(&texts).await?;
        let (ctx, ids) = vecs.split_first().expect("≥1 vector");

        let cosines: Vec<f32> = ids.iter().map(|v| cosine_similarity(ctx, v)).collect();
        let bands: Vec<&'static str> = cosines.iter().map(|&c| band_of(c, keep, drop)).collect();
        let has_ambiguous = bands.contains(&"ambiguous");

        // Decide whether to spend Gemma on this article's ambiguous band (budget-bounded).
        let adjudicate = has_ambiguous && adjudicated_articles < adjudicate_budget;
        let resolutions = if adjudicate {
            adjudicated_articles += 1;
            Some(
                hx.resolve_set(Role::EmotionalNews, &context, &candidates)
                    .await?,
            )
        } else {
            None
        };

        let mut rows = Vec::with_capacity(art.links.len());
        for (i, link) in art.links.iter().enumerate() {
            let band = bands[i];
            let auto_verdict = band == "keep";
            auc.push(cosines[i], link.vetted);
            match band {
                "keep" => n_keep += 1,
                "drop" => n_drop += 1,
                _ => n_amb += 1,
            }

            // Auto-decide confusion (only the bands the cheap filter settles WITHOUT a model).
            if band == "keep" {
                if link.vetted {
                    ak_tp += 1
                } else {
                    ak_fp += 1
                }
            } else if band == "drop" {
                if link.vetted {
                    ad_fn += 1; // a genuine link the cheap band would wrongly drop — the Álvarez risk
                    auto_drop_fns.push(FnRow {
                        name: link.name.clone(),
                        article_id: link.article_id,
                        cosine: cosines[i],
                        in_transfer_rumors: link.in_transfer_rumors,
                        career_teams: link.career_teams,
                    });
                } else {
                    ad_tn += 1
                }
            }

            // The final hybrid verdict + its provenance.
            let (hybrid_verdict, decided_by) = match (band, &resolutions) {
                ("keep", _) => (Some(true), "keep_band"),
                ("drop", _) => (Some(false), "drop_band"),
                ("ambiguous", Some(res)) => (Some(res[i].kept), "gemma"),
                ("ambiguous", None) => (None, "pending"),
                _ => unreachable!("band is keep|drop|ambiguous"),
            };

            // Bounded real-gate agreement: count only the links of adjudicated articles, so the
            // confusion reflects the actual end-to-end primitive (auto bands + Gemma middle).
            if let Some(v) = hybrid_verdict {
                if adjudicate {
                    adjudicated_links += 1;
                    match (v, link.vetted) {
                        (true, true) => h_tp += 1,
                        (true, false) => h_fp += 1,
                        (false, false) => h_tn += 1,
                        (false, true) => h_fn += 1,
                    }
                    if v != link.vetted {
                        hybrid_disagreements.push(format!(
                            "    {} (article {}, cos {:.3}, band {}) — hybrid {}, Gemma {}{}",
                            link.name,
                            link.article_id,
                            cosines[i],
                            band,
                            if v { "KEEP" } else { "drop" },
                            if link.vetted { "KEEP" } else { "drop" },
                            mover_tag(link.in_transfer_rumors, link.career_teams),
                        ));
                    }
                }
            }

            rows.push(ShadowRow {
                article_id: link.article_id,
                sport: link.sport.clone(),
                entity_id: link.entity_id,
                name: link.name.clone(),
                gemma_vetted: link.vetted,
                cosine: cosines[i],
                band,
                auto_verdict,
                hybrid_verdict,
                decided_by,
                in_transfer_rumors: link.in_transfer_rumors,
                career_teams: link.career_teams,
            });
        }

        let model_version = adjudicate.then(|| adjudicator.clone());
        insert_rows(&hx.pool, &rows, keep, drop, &cfg.embed.model_repo, model_version.as_deref())
            .await?;

        print!("\r  {}/{} articles… ({adjudicated_articles} adjudicated)", idx + 1, articles.len());
        let _ = std::io::stdout().flush();
    }
    println!("\r{:60}", " ");

    report(Report {
        articles: articles.len(),
        total_links,
        keep,
        drop,
        auc: auc.value(),
        n_keep,
        n_drop,
        n_amb,
        ak_tp,
        ak_fp,
        ad_tn,
        ad_fn,
        auto_drop_fns: &auto_drop_fns,
        adjudicated_articles,
        adjudicated_links,
        h_tp,
        h_fp,
        h_tn,
        h_fn,
        hybrid_disagreements: &hybrid_disagreements,
    });
    Ok(())
}

fn mover_tag(in_rumors: bool, career_teams: i16) -> String {
    if in_rumors || career_teams > 1 {
        format!(
            "  [recent-mover: {}{}{}]",
            if in_rumors { "in-transfer-rumors" } else { "" },
            if in_rumors && career_teams > 1 { ", " } else { "" },
            if career_teams > 1 { format!("{career_teams} career teams") } else { String::new() },
        )
    } else {
        String::new()
    }
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

/// One article + its labeled secondary player links.
struct Article {
    title: String,
    description: String,
    links: Vec<LinkRow>,
}

/// load_corpus pulls EVERY labeled secondary player link (`match_confidence < 1.0`, `vetted`
/// labeled) with its identity card and the two recent-mover diagnostics. The CTEs pre-aggregate the
/// mover signals once (cheap) rather than per-row. Ordered by article so the grouping is stable.
async fn load_corpus(pool: &PgPool, max_articles: i64) -> Result<Vec<Article>> {
    // The article id set (bounded by max_articles when > 0), deterministic order.
    let ids: Vec<i64> = {
        use sqlx::Row;
        let q = if max_articles > 0 {
            "SELECT DISTINCT article_id FROM news_article_entities \
             WHERE entity_type='player' AND vetted IS NOT NULL AND match_confidence < 1.0 \
             ORDER BY article_id LIMIT $1"
        } else {
            "SELECT DISTINCT article_id FROM news_article_entities \
             WHERE entity_type='player' AND vetted IS NOT NULL AND match_confidence < 1.0 \
             ORDER BY article_id"
        };
        let mut query = sqlx::query(q);
        if max_articles > 0 {
            query = query.bind(max_articles);
        }
        query
            .fetch_all(pool)
            .await
            .context("load article ids")?
            .iter()
            .map(|r| r.get::<i64, _>("article_id"))
            .collect()
    };

    let rows = sqlx::query_as::<_, LinkRow>(
        r#"
        WITH tr AS (
            SELECT DISTINCT player_id, sport FROM transfer_rumors
        ),
        ct AS (
            SELECT player_id, sport, count(DISTINCT team_id) AS career_teams
              FROM player_stats WHERE team_id IS NOT NULL
             GROUP BY player_id, sport
        )
        SELECT nae.article_id, na.title, COALESCE(na.description,'') AS description, nae.sport,
               nae.entity_id, p.name,
               COALESCE(p.nationality,'')                    AS nationality,
               COALESCE(ct2.name,'')                         AS current_club,
               COALESCE(NULLIF(pos.position,'Unknown'), '')  AS position,
               nae.vetted,
               (tr.player_id IS NOT NULL)                    AS in_transfer_rumors,
               COALESCE(ct.career_teams, 0)::smallint        AS career_teams
        FROM news_article_entities nae
        JOIN news_articles na ON na.id = nae.article_id
        JOIN players p ON p.id = nae.entity_id AND p.sport = nae.sport
        LEFT JOIN public.player_current_team pct ON pct.player_id = p.id AND pct.sport = p.sport
        LEFT JOIN teams ct2 ON ct2.id = pct.team_id AND ct2.sport = p.sport
        LEFT JOIN LATERAL (
            SELECT ps.position FROM player_stats ps
            WHERE ps.player_id = p.id AND ps.sport = p.sport
            ORDER BY ps.season DESC NULLS LAST LIMIT 1
        ) pos ON true
        LEFT JOIN tr ON tr.player_id = nae.entity_id AND tr.sport = nae.sport
        LEFT JOIN ct ON ct.player_id = nae.entity_id AND ct.sport = nae.sport
        WHERE nae.entity_type='player' AND nae.vetted IS NOT NULL AND nae.match_confidence < 1.0
          AND nae.article_id = ANY($1)
        ORDER BY nae.article_id, nae.entity_id
        "#,
    )
    .bind(&ids)
    .fetch_all(pool)
    .await
    .context("load corpus links")?;

    let mut by_article: BTreeMap<i64, Article> = BTreeMap::new();
    for r in rows {
        let entry = by_article.entry(r.article_id).or_insert_with(|| Article {
            title: r.title.clone(),
            description: r.description.clone(),
            links: Vec::new(),
        });
        entry.links.push(r);
    }
    Ok(by_article.into_values().collect())
}

/// insert_rows persists one article's shadow rows in a single UNNEST insert. ONLY write path of
/// this bin — it touches `resolve_shadow` and nothing else (no vetted UPDATE, no queue).
async fn insert_rows(
    pool: &PgPool,
    rows: &[ShadowRow],
    keep: f32,
    drop: f32,
    embed_model: &str,
    model_version: Option<&str>,
) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let article_ids: Vec<i64> = rows.iter().map(|r| r.article_id).collect();
    let sports: Vec<String> = rows.iter().map(|r| r.sport.clone()).collect();
    let entity_ids: Vec<i32> = rows.iter().map(|r| r.entity_id).collect();
    let names: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
    let vetted: Vec<bool> = rows.iter().map(|r| r.gemma_vetted).collect();
    let cosines: Vec<f32> = rows.iter().map(|r| r.cosine).collect();
    let bands: Vec<String> = rows.iter().map(|r| r.band.to_string()).collect();
    let autos: Vec<bool> = rows.iter().map(|r| r.auto_verdict).collect();
    let hybrids: Vec<Option<bool>> = rows.iter().map(|r| r.hybrid_verdict).collect();
    let decided: Vec<String> = rows.iter().map(|r| r.decided_by.to_string()).collect();
    let in_rumors: Vec<bool> = rows.iter().map(|r| r.in_transfer_rumors).collect();
    let career: Vec<i16> = rows.iter().map(|r| r.career_teams).collect();

    sqlx::query(
        r#"
        INSERT INTO resolve_shadow
            (article_id, sport, entity_type, entity_id, name, gemma_vetted, cosine, band,
             auto_verdict, hybrid_verdict, decided_by, in_transfer_rumors, career_teams,
             keep_threshold, drop_threshold, embed_model, model_version)
        SELECT u.article_id, u.sport, 'player', u.entity_id, u.name, u.vetted, u.cosine, u.band,
               u.auto_verdict, u.hybrid_verdict, u.decided_by, u.in_transfer_rumors, u.career_teams,
               $13, $14, $15, $16
        FROM unnest($1::bigint[], $2::text[], $3::int[], $4::text[], $5::bool[], $6::real[],
                    $7::text[], $8::bool[], $9::bool[], $10::text[], $11::bool[], $12::smallint[])
            AS u(article_id, sport, entity_id, name, vetted, cosine, band, auto_verdict,
                 hybrid_verdict, decided_by, in_transfer_rumors, career_teams)
        "#,
    )
    .bind(&article_ids)
    .bind(&sports)
    .bind(&entity_ids)
    .bind(&names)
    .bind(&vetted)
    .bind(&cosines)
    .bind(&bands)
    .bind(&autos)
    .bind(&hybrids)
    .bind(&decided)
    .bind(&in_rumors)
    .bind(&career)
    .bind(keep)
    .bind(drop)
    .bind(embed_model)
    .bind(model_version)
    .execute(pool)
    .await
    .context("insert shadow rows")?;
    Ok(())
}

/// An auto-dropped genuine link (the Álvarez failure mode) — the FN slice the report breaks down.
struct FnRow {
    name: String,
    article_id: i64,
    cosine: f32,
    in_transfer_rumors: bool,
    career_teams: i16,
}

impl FnRow {
    fn is_mover(&self) -> bool {
        self.in_transfer_rumors || self.career_teams > 1
    }
}

/// AucAccumulator computes ROC-AUC = P(cosine of a TRUE link > cosine of a FALSE link) via the rank
/// (Mann–Whitney) identity, so it needs no threshold sweep. Ties count as 0.5.
#[derive(Default)]
struct AucAccumulator {
    pos: Vec<f32>,
    neg: Vec<f32>,
}

impl AucAccumulator {
    fn push(&mut self, cosine: f32, vetted: bool) {
        if vetted {
            self.pos.push(cosine)
        } else {
            self.neg.push(cosine)
        }
    }
    fn value(&self) -> f64 {
        if self.pos.is_empty() || self.neg.is_empty() {
            return f64::NAN;
        }
        // Rank-sum: concatenate, sort, assign average ranks, sum the positives' ranks.
        let mut all: Vec<(f32, bool)> = self
            .pos
            .iter()
            .map(|&c| (c, true))
            .chain(self.neg.iter().map(|&c| (c, false)))
            .collect();
        all.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut rank_sum_pos = 0f64;
        let mut i = 0usize;
        while i < all.len() {
            let mut j = i + 1;
            while j < all.len() && all[j].0 == all[i].0 {
                j += 1;
            }
            // Tie group [i, j): average rank (1-indexed).
            let avg_rank = ((i + 1) + j) as f64 / 2.0;
            for item in &all[i..j] {
                if item.1 {
                    rank_sum_pos += avg_rank;
                }
            }
            i = j;
        }
        let (np, nn) = (self.pos.len() as f64, self.neg.len() as f64);
        (rank_sum_pos - np * (np + 1.0) / 2.0) / (np * nn)
    }
}

#[allow(clippy::too_many_arguments)]
struct Report<'a> {
    articles: usize,
    total_links: usize,
    keep: f32,
    drop: f32,
    auc: f64,
    n_keep: u32,
    n_drop: u32,
    n_amb: u32,
    ak_tp: u32,
    ak_fp: u32,
    ad_tn: u32,
    ad_fn: u32,
    auto_drop_fns: &'a [FnRow],
    adjudicated_articles: usize,
    adjudicated_links: usize,
    h_tp: u32,
    h_fp: u32,
    h_tn: u32,
    h_fn: u32,
    hybrid_disagreements: &'a [String],
}

fn report(r: Report) {
    let pct = |x: u32, d: u32| if d > 0 { 100.0 * x as f64 / d as f64 } else { 0.0 };
    let ratio = |x: u32, d: u32| if d > 0 { x as f64 / d as f64 } else { 0.0 };
    let auto = r.n_keep + r.n_drop;

    println!("======================  RESOLVE SHADOW (at scale)  ======================");
    println!(
        "corpus: {} articles / {} secondary links · band keep≥{:.2}/drop<{:.2}",
        r.articles, r.total_links, r.keep, r.drop
    );
    println!("cosine separation (full universe): ROC-AUC {:.3}", r.auc);
    println!();
    println!("band split over {} links (the GPU saving at scale):", r.total_links);
    println!(
        "  auto-keep {} ({:.0}%)   auto-drop {} ({:.0}%)   → ambiguous→Gemma {} ({:.0}%)",
        r.n_keep,
        pct(r.n_keep, r.total_links as u32),
        r.n_drop,
        pct(r.n_drop, r.total_links as u32),
        r.n_amb,
        pct(r.n_amb, r.total_links as u32),
    );
    println!(
        "  ⇒ the hybrid auto-decides {:.0}% of links with NO Gemma call — {:.0}% GPU saved vs all-Gemma.",
        pct(auto, r.total_links as u32),
        pct(auto, r.total_links as u32),
    );
    println!();
    println!("AUTO-DECIDE SAFETY (full universe, zero Gemma — the production risk):");
    println!(
        "  auto-KEEP: {} links → precision {:.3} ({} agree with Gemma, {} false-keep)",
        r.n_keep,
        ratio(r.ak_tp, r.ak_tp + r.ak_fp),
        r.ak_tp,
        r.ak_fp,
    );
    println!(
        "  auto-DROP: {} links → {} genuine links wrongly dropped (FN rate {:.3}); {} correct drops",
        r.n_drop,
        r.ad_fn,
        ratio(r.ad_fn, r.ad_fn + r.ad_tn),
        r.ad_tn,
    );
    // The Álvarez slice: how many auto-drop FNs are recent movers (→ band-widening recoverable)
    // vs not (→ identity-refresh needed).
    let movers = r.auto_drop_fns.iter().filter(|f| f.is_mover()).count();
    let non_movers = r.auto_drop_fns.len() - movers;
    println!(
        "  auto-drop FN breakdown: {} recent-movers (band-widening recoverable), {} non-movers (identity-refresh)",
        movers, non_movers
    );
    if !r.auto_drop_fns.is_empty() {
        println!("  worst auto-drop FNs (lowest cosine first):");
        let mut sorted: Vec<&FnRow> = r.auto_drop_fns.iter().collect();
        sorted.sort_by(|a, b| a.cosine.partial_cmp(&b.cosine).unwrap_or(std::cmp::Ordering::Equal));
        for f in sorted.iter().take(15) {
            println!(
                "    {} (article {}, cos {:.3}){}",
                f.name,
                f.article_id,
                f.cosine,
                mover_tag(f.in_transfer_rumors, f.career_teams),
            );
        }
    }
    println!();
    println!(
        "REAL-GATE AGREEMENT (bounded sample: {} articles / {} links, embed band + Gemma middle):",
        r.adjudicated_articles, r.adjudicated_links
    );
    let total = r.h_tp + r.h_fp + r.h_tn + r.h_fn;
    let precision = ratio(r.h_tp, r.h_tp + r.h_fp);
    let recall = ratio(r.h_tp, r.h_tp + r.h_fn);
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };
    println!(
        "  accuracy {:.3}   precision {:.3}   recall {:.3}   F1 {:.3}",
        ratio(r.h_tp + r.h_tn, total),
        precision,
        recall,
        f1,
    );
    println!("  confusion: TP {}  FP {}  TN {}  FN {}", r.h_tp, r.h_fp, r.h_tn, r.h_fn);
    if !r.hybrid_disagreements.is_empty() {
        println!("  real-gate disagreements ({}):", r.hybrid_disagreements.len());
        for d in r.hybrid_disagreements.iter().take(15) {
            println!("{d}");
        }
    }
    println!("=========================================================================");
}

fn env_i64(key: &str, default: i64) -> i64 {
    std::env::var(key).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}
