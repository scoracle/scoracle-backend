//! Rust stats-rail commentary batch.
//!
//! Step 3 replacement for `go/cmd/statcommentary`: rating is not a pipeline_work
//! stage, so this bin enumerates entities and calls the L12 rating core directly.

use anyhow::{anyhow, Context, Result};
use scoracle_cognition::config::Config;
use scoracle_cognition::harness::Harness;
use scoracle_cognition::ollama::OllamaClient;
use scoracle_cognition::rating::{
    generate_rating, persist_stat_summary, RatingOutput, RatingReq, RATING_TEMPERATURE,
};
use scoracle_cognition::route::Router;
use scoracle_cognition::{db, vibe};
use sqlx::{PgPool, Postgres, Row};
use std::time::Duration;

#[derive(Clone, Debug)]
struct Args {
    mode: String,
    entity_type: String,
    entity_id: i32,
    season: Option<i32>,
    sport: String,
    trigger: String,
    persist: bool,
    skip_unchanged: bool,
    throttle_ms: u64,
    limit: usize,
}

#[derive(Clone, Debug)]
struct Target {
    entity_type: String,
    entity_id: i32,
    season: i32,
    sport: String,
}

#[derive(Default)]
struct Counts {
    ok: usize,
    unchanged: usize,
    no_stats: usize,
    failed: usize,
    scanned: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let cfg = Config::from_env()?;

    if args.mode != "single" {
        let ollama =
            OllamaClient::new(&cfg.ollama_base_url, &cfg.ollama_model, cfg.ollama_timeout)?;
        ollama
            .ping()
            .await
            .context("ollama must be reachable for statcommentary batch")?;
    }

    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    let harness = Harness {
        pool,
        router: Router::from_config(&cfg.route, cfg.ollama_timeout, cfg.ollama_max_concurrent)?,
        embedder: None,
        resolve: cfg.resolve.clone(),
    };

    match args.mode.as_str() {
        "single" => run_single(&harness, &args).await,
        "nightly" => run_corpus(&harness, &cfg.database_url, &args, true).await,
        "backfill" => run_corpus(&harness, &cfg.database_url, &args, false).await,
        other => Err(anyhow!(
            "unknown -mode {other:?}; valid: single | nightly | backfill"
        )),
    }
}

async fn run_single(hx: &Harness, args: &Args) -> Result<()> {
    if args.entity_id <= 0 || args.sport.is_empty() {
        return Err(anyhow!("-entity-id and -sport are required in single mode"));
    }
    let sport = args.sport.to_uppercase();
    let name =
        vibe::lookup_entity_name(&hx.pool, &args.entity_type, args.entity_id, &sport).await?;
    let req = RatingReq {
        entity_type: args.entity_type.clone(),
        entity_id: args.entity_id,
        entity_name: name.clone(),
        sport: sport.clone(),
        season: args.season,
        trigger_type: args.trigger.clone(),
    };
    let out = generate_rating(hx, &req, RATING_TEMPERATURE, args.skip_unchanged).await?;
    if args.persist && !out.skipped_unchanged {
        persist_rating(hx, &req, &out).await?;
    }
    let mode = if args.persist { "persisted" } else { "dry-run" };
    println!(
        "statcommentary single: {name} {}/{} {sport} season {} — {mode}{}",
        args.entity_type,
        args.entity_id,
        out.season,
        outcome_label(&out),
    );
    if let Some(body) = &out.body {
        println!("\n{body}");
    }
    Ok(())
}

async fn run_corpus(hx: &Harness, db_url: &str, args: &Args, nightly: bool) -> Result<()> {
    let lock = JobLock::acquire(&hx.pool, db_url, "statcommentary").await?;
    let Some(lock) = lock else {
        println!("statcommentary: another run holds the advisory lock; exiting cleanly");
        return Ok(());
    };

    let sports = selected_sports(&args.sport);
    let mut targets = Vec::new();
    for sport in &sports {
        let mut ts = if nightly {
            let season = current_season(&hx.pool, sport).await?;
            enum_current_season(&hx.pool, sport, season).await?
        } else {
            enum_missing(&hx.pool, sport).await?
        };
        targets.append(&mut ts);
    }

    println!(
        "statcommentary: starting mode={} sports={:?} targets={} limit={}",
        if nightly { "nightly" } else { "backfill" },
        sports,
        targets.len(),
        args.limit
    );

    let mut c = Counts::default();
    for t in targets {
        if args.limit > 0 && c.ok >= args.limit {
            println!(
                "statcommentary: generation limit reached limit={} scanned={}",
                args.limit, c.scanned
            );
            break;
        }
        c.scanned += 1;
        match run_target(hx, &t, nightly).await {
            Ok(out) if out.skipped_unchanged => c.unchanged += 1,
            Ok(out) if out.skipped_no_stats => {
                persist_target(hx, &t, &out).await?;
                c.no_stats += 1;
            }
            Ok(out) => {
                persist_target(hx, &t, &out).await?;
                c.ok += 1;
            }
            Err(e) => {
                c.failed += 1;
                eprintln!(
                    "statcommentary: generate failed sport={} {}/{} season={} error={e:#}",
                    t.sport, t.entity_type, t.entity_id, t.season
                );
            }
        }
        if c.scanned % 25 == 0 {
            println!(
                "statcommentary: progress scanned={} ok={} unchanged={} no_stats={} failed={}",
                c.scanned, c.ok, c.unchanged, c.no_stats, c.failed
            );
        }
        if args.throttle_ms > 0 {
            tokio::time::sleep(Duration::from_millis(args.throttle_ms)).await;
        }
    }
    lock.release().await?;

    println!(
        "statcommentary: complete scanned={} ok={} unchanged={} no_stats={} failed={}",
        c.scanned, c.ok, c.unchanged, c.no_stats, c.failed
    );
    if c.failed > 0 && c.ok == 0 && c.unchanged == 0 && c.no_stats == 0 {
        return Err(anyhow!("all {} attempted generation(s) failed", c.failed));
    }
    if c.failed > 0 {
        return Err(anyhow!("partial: {} generation(s) failed", c.failed));
    }
    Ok(())
}

async fn run_target(hx: &Harness, t: &Target, skip_unchanged: bool) -> Result<RatingOutput> {
    let name = vibe::lookup_entity_name(&hx.pool, &t.entity_type, t.entity_id, &t.sport).await?;
    let req = RatingReq {
        entity_type: t.entity_type.clone(),
        entity_id: t.entity_id,
        entity_name: name,
        sport: t.sport.clone(),
        season: Some(t.season),
        trigger_type: "periodic".to_string(),
    };
    generate_rating(hx, &req, RATING_TEMPERATURE, skip_unchanged).await
}

async fn persist_target(hx: &Harness, t: &Target, out: &RatingOutput) -> Result<()> {
    persist_stat_summary(
        &hx.pool,
        &t.entity_type,
        t.entity_id,
        &t.sport,
        "periodic",
        &serde_json::json!({}),
        out,
    )
    .await
}

async fn persist_rating(hx: &Harness, req: &RatingReq, out: &RatingOutput) -> Result<()> {
    persist_stat_summary(
        &hx.pool,
        &req.entity_type,
        req.entity_id,
        &req.sport,
        &req.trigger_type,
        &serde_json::json!({}),
        out,
    )
    .await
}

async fn current_season(pool: &PgPool, sport: &str) -> Result<i32> {
    sqlx::query_scalar("SELECT current_season FROM public.sports WHERE id = $1")
        .bind(sport)
        .fetch_one(pool)
        .await
        .with_context(|| format!("current season {sport}"))
}

async fn enum_current_season(pool: &PgPool, sport: &str, season: i32) -> Result<Vec<Target>> {
    let rows = sqlx::query(enum_current_season_sql())
        .bind(sport)
        .bind(season)
        .fetch_all(pool)
        .await
        .with_context(|| format!("enumerate current season {sport}/{season}"))?;
    scan_targets(rows, sport)
}

fn enum_current_season_sql() -> &'static str {
    r#"
        WITH candidates AS (
            SELECT et, id, season, updated_at FROM (
                SELECT 'player'::text AS et, player_id AS id, season, updated_at,
                       row_number() OVER (
                           PARTITION BY player_id
                           ORDER BY (COALESCE(league_id, 0) = 0) DESC,
                                    jsonb_array_length(COALESCE(rating_breakdown, '[]'::jsonb)) DESC,
                                    COALESCE(league_id, 0) ASC
                       ) AS rn
                FROM player_stats
                WHERE sport = $1 AND season = $2 AND rating_composite_score IS NOT NULL
            ) p
            WHERE rn = 1
            UNION ALL
            SELECT et, id, season, updated_at FROM (
                SELECT 'team'::text AS et, team_id AS id, season, updated_at,
                       row_number() OVER (
                           PARTITION BY team_id
                           ORDER BY (COALESCE(league_id, 0) = 0) DESC,
                                    jsonb_array_length(COALESCE(rating_breakdown, '[]'::jsonb)) DESC,
                                    COALESCE(league_id, 0) ASC
                       ) AS rn
                FROM team_stats
                WHERE sport = $1 AND season = $2 AND rating_composite_score IS NOT NULL
            ) t
            WHERE rn = 1
        ),
        latest AS (
            SELECT DISTINCT ON (entity_type, entity_id, sport, season)
                   entity_type, entity_id, sport, season, generated_at, input_hash
            FROM stat_summaries
            WHERE sport = $1 AND season = $2
            ORDER BY entity_type, entity_id, sport, season, generated_at DESC
        )
        SELECT c.et, c.id, c.season
        FROM candidates c
        LEFT JOIN latest s
          ON s.entity_type = c.et
         AND s.entity_id = c.id
         AND s.sport = $1
         AND s.season = c.season
        WHERE s.generated_at IS NULL
           OR NULLIF(s.input_hash, '') IS NULL
           OR c.updated_at > s.generated_at
        ORDER BY (s.generated_at IS NOT NULL) ASC, c.et, c.id
        "#
}

async fn enum_missing(pool: &PgPool, sport: &str) -> Result<Vec<Target>> {
    let rows = sqlx::query(
        r#"
        SELECT c.et, c.id, c.season FROM (
            SELECT 'player'::text AS et, player_id AS id, season FROM player_stats
             WHERE sport = $1 AND rating_composite_score IS NOT NULL
             GROUP BY player_id, season
            UNION ALL
            SELECT 'team'::text, team_id, season FROM team_stats
             WHERE sport = $1 AND rating_composite_score IS NOT NULL
             GROUP BY team_id, season
        ) c
        WHERE NOT EXISTS (
            SELECT 1 FROM stat_summaries s
             WHERE s.entity_type = c.et AND s.entity_id = c.id AND s.sport = $1 AND s.season = c.season
        )
        ORDER BY c.season DESC, c.et, c.id
        "#,
    )
    .bind(sport)
    .fetch_all(pool)
    .await
    .with_context(|| format!("enumerate missing {sport}"))?;
    scan_targets(rows, sport)
}

fn scan_targets(rows: Vec<sqlx::postgres::PgRow>, sport: &str) -> Result<Vec<Target>> {
    rows.into_iter()
        .map(|r| {
            Ok(Target {
                entity_type: r.try_get::<String, _>(0)?,
                entity_id: r.try_get::<i32, _>(1)?,
                season: r.try_get::<i32, _>(2)?,
                sport: sport.to_string(),
            })
        })
        .collect()
}

struct JobLock {
    conn: sqlx::pool::PoolConnection<Postgres>,
    key: String,
}

impl JobLock {
    async fn acquire(pool: &PgPool, _db_url: &str, key: &str) -> Result<Option<Self>> {
        let mut conn = pool.acquire().await.context("acquire advisory-lock conn")?;
        let acquired: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtext($1)::bigint)")
                .bind(key)
                .fetch_one(&mut *conn)
                .await
                .with_context(|| format!("acquire advisory lock {key}"))?;
        Ok(acquired.then(|| Self {
            conn,
            key: key.to_string(),
        }))
    }

    async fn release(mut self) -> Result<()> {
        let unlocked: bool = sqlx::query_scalar("SELECT pg_advisory_unlock(hashtext($1)::bigint)")
            .bind(&self.key)
            .fetch_one(&mut *self.conn)
            .await
            .with_context(|| format!("release advisory lock {}", self.key))?;
        if !unlocked {
            eprintln!("statcommentary: advisory lock was not held at release");
        }
        Ok(())
    }
}

fn selected_sports(raw: &str) -> Vec<String> {
    let s = raw.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("all") {
        vec!["NBA".to_string(), "NFL".to_string(), "FOOTBALL".to_string()]
    } else {
        vec![s.to_uppercase()]
    }
}

fn outcome_label(out: &RatingOutput) -> &'static str {
    if out.skipped_no_stats {
        " (no usable rating profile)"
    } else if out.skipped_unchanged {
        " (unchanged)"
    } else {
        ""
    }
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args> {
    let mut out = Args {
        mode: "single".to_string(),
        entity_type: "player".to_string(),
        entity_id: 0,
        season: None,
        sport: String::new(),
        trigger: "manual".to_string(),
        persist: false,
        skip_unchanged: false,
        throttle_ms: 0,
        limit: 0,
    };
    let mut it = args.peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-mode" => out.mode = next_value(&mut it, "-mode")?,
            "-entity-type" => out.entity_type = next_value(&mut it, "-entity-type")?.to_lowercase(),
            "-entity-id" => out.entity_id = next_value(&mut it, "-entity-id")?.parse()?,
            "-season" => {
                let season: i32 = next_value(&mut it, "-season")?.parse()?;
                out.season = (season > 0).then_some(season);
            }
            "-sport" => out.sport = next_value(&mut it, "-sport")?,
            "-trigger" => out.trigger = next_value(&mut it, "-trigger")?,
            "-persist" => out.persist = true,
            "-skip-unchanged" => out.skip_unchanged = true,
            "-throttle-ms" => out.throttle_ms = next_value(&mut it, "-throttle-ms")?.parse()?,
            "-limit" => out.limit = next_value(&mut it, "-limit")?.parse()?,
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(anyhow!("unknown argument {other:?}; pass --help for usage")),
        }
    }
    if out.entity_type != "player" && out.entity_type != "team" {
        return Err(anyhow!("-entity-type must be player or team"));
    }
    Ok(out)
}

fn next_value(
    it: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    flag: &str,
) -> Result<String> {
    it.next().ok_or_else(|| anyhow!("{flag} requires a value"))
}

fn print_help() {
    println!(
        "statcommentary -mode single|nightly|backfill [options]\n\
         single:   -entity-type player|team -entity-id ID -sport SPORT [-season YEAR] [-persist] [-skip-unchanged]\n\
         nightly:  -mode nightly [-sport NBA|NFL|FOOTBALL|all] [-limit N] [-throttle-ms N]\n\
         backfill: -mode backfill [-sport NBA|NFL|FOOTBALL|all] [-limit N] [-throttle-ms N]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_season_enum_prefilters_latest_changed_or_missing_hash() {
        let sql = enum_current_season_sql();

        assert!(sql.contains("row_number() OVER"));
        assert!(sql.contains("PARTITION BY player_id"));
        assert!(sql.contains("PARTITION BY team_id"));
        assert!(sql.contains("WHERE rn = 1"));
        assert!(sql.contains("(COALESCE(league_id, 0) = 0) DESC"));
        assert!(sql.contains("jsonb_array_length(COALESCE(rating_breakdown, '[]'::jsonb)) DESC"));

        assert!(sql.contains("SELECT DISTINCT ON (entity_type, entity_id, sport, season)"));
        assert!(sql.contains("ORDER BY entity_type, entity_id, sport, season, generated_at DESC"));
        assert!(sql.contains("NULLIF(s.input_hash, '') IS NULL"));
        assert!(sql.contains("c.updated_at > s.generated_at"));
    }
}
