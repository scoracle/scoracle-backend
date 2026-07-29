//! remap — the B4 re-mapping backfill over the 07-27 incident cohort
//! (PLAN-ingest-simplification.md, checklist B4; HANDOFF-newsroom.md §0).
//!
//! The Editor already named the right entities on most of the articles the broken ar6 gate
//! rejected — it wrote them into `news_article_readings.evidence -> 'relevant_entities'` as NAME
//! STRINGS, and nothing ever mapped them back. This binary is that map, run offline over a pinned
//! cohort, with **zero model calls**: exact match on the normalized surface (`public.nrm`) against
//! `entity_name_surfaces`, sport-scoped, ambiguity refused.
//!
//! Two passes, deliberately staged (Scott's call 2026-07-28) so the cheap half is inspected before
//! the new half lands:
//!
//!   -pass flips   existing links at vetted = FALSE that the Editor's own names vindicate → TRUE
//!   -pass new     links the ingest query never proposed at all → INSERT, vetted = TRUE
//!
//! Usage (env from .env.local: DATABASE_PRIVATE_URL):
//!   remap [-pass flips|new] [-rollback|-apply] [-out PATH] [-since TS] [-until TS] [-max-rows N]
//!
//! DRY RUN BY DEFAULT: with no mode flag it writes the inventory TSV and touches nothing.
//! `-rollback` rehearses the real write inside a transaction it throws away; `-apply` commits.
//!
//! ## SAFETY, and the one hazard that is invisible at the call site
//!
//! ⚠️ **T10.** `enqueue_derive_on_vetted` fires `AFTER UPDATE OF vetted` and enqueues
//! **`article_read` on the ARTICLE**, keyed `input_version = 'ar:' || vetted_count`. Flipping links
//! changes that count, so the ON CONFLICT clause re-opens even articles that have already been read.
//! The "free" half of B4 — restoring links that already exist — would therefore buy ~5,400 Editor
//! re-reads through the very ar6 gate C1 is about to replace, which is precisely what "do not re-arm
//! the held articles" forbids, arriving through a side door.
//!
//! The flip pass suppresses it with `SET LOCAL session_replication_role = 'replica'` inside the
//! transaction: no lock, auto-reverts at COMMIT, and asserted before any write (see
//! `assert_triggers_suppressed`). **Do NOT reach for `ALTER TABLE ... DISABLE TRIGGER`** — that
//! takes an ACCESS EXCLUSIVE lock against a live pipeline.
//!
//! `-pass new` INSERTs, and the trigger is UPDATE-only, so it cannot fire there. The suppression is
//! applied to both passes anyway: it costs nothing, and a future trigger added to this table should
//! not silently re-arm 2,000 articles because this binary assumed the shape of a trigger list.
//!
//! Otherwise this follows `bin/bucketlabel`'s read-only posture: it NEVER claims `pipeline_work`,
//! never enqueues, and writes exactly one product column (`news_article_entities.vetted`) plus the
//! brand-new rows it reports. Idempotent — a second run flips nothing (the UPDATE re-checks
//! `vetted IS FALSE`) and inserts nothing (ON CONFLICT DO NOTHING).
//!
//! ## Why the cohort is pinned to a window rather than `vetted IS FALSE`
//!
//! `vetted IS FALSE` alone matched 24,984 articles on 2026-07-28 — 4× the intended set — because
//! normal scrub rejections have piled up since the incident and those are LEGITIMATE. The cohort is
//! articles whose Editor reading was updated inside the incident window AND which carry at least one
//! rejected link: 6,376 articles, reproducing the recorded 6,319 to within drift. A backfill written
//! against the bare predicate would un-reject four times as much as it should.

use anyhow::{bail, Context, Result};
use scoracle_cognition::config::Config;
use scoracle_cognition::db;
use sqlx::Row;
use std::collections::{BTreeMap, HashSet};
use std::io::Write;

/// The incident window, pinned. Editor readings updated in `[since, until)` — 07-27 00:00 EDT
/// through the ar7 fix at 07-28 07:04 EDT. Offsets are explicit so the answer does not depend on
/// the server's `TimeZone` (America/New_York today, which would make these agree by luck).
const DEFAULT_SINCE: &str = "2026-07-27 00:00:00-04";
const DEFAULT_UNTIL: &str = "2026-07-28 07:04:00-04";

/// The sentinel that makes an Editor-derived link greppable and reversible. Go writes 1.000
/// (primary — the entity the article was fetched for), 0.950 and 0.800; 0.990 is used by nothing
/// else, so `match_confidence = 0.990` selects exactly the rows this binary created.
///
/// It sits ABOVE 0.95 on purpose. The scrub sweep enqueues on `match_confidence < 1.0`, and the
/// Editor's co-mention candidate pool selects `< 0.95` — both also require an un-set `scrubbed_at`
/// or `vetted IS NULL`, which these rows do not have, so this is belt and braces on both counts.
const EDITOR_MATCH_CONFIDENCE: f64 = 0.990;

/// A runaway guard on `-apply`. The whole trap of B4 is a predicate that quietly matches 4× the
/// intended set, and a ceiling is what makes that failure loud instead of silent. Measured pass
/// sizes are ~9,800 (flips) and ~5,900 (new).
const DEFAULT_MAX_ROWS: usize = 20_000;

/// The cohort and its exact resolutions. Shared by the inventory and the ambiguity census so the
/// two provably see the same population.
///
/// `article_sport` takes the sport from the article's OWN links rather than from `news_articles`,
/// which has no sport column. `emitted` reads the Editor's name list; `hit` joins it to the
/// surfaces table on the normalized string — `public.nrm` on both sides, never a Rust copy of it
/// (mig 198: one normalizer, in SQL, so the index and the query provably agree).
const COHORT_CTE: &str = r#"
WITH cohort AS (
    SELECT r.article_id
      FROM public.news_article_readings r
     WHERE r.updated_at >= $1::timestamptz
       AND r.updated_at <  $2::timestamptz
       AND EXISTS (SELECT 1 FROM public.news_article_entities e
                    WHERE e.article_id = r.article_id AND e.vetted IS FALSE)
),
article_sport AS (
    SELECT DISTINCT c.article_id, e.sport
      FROM cohort c
      JOIN public.news_article_entities e ON e.article_id = c.article_id
),
emitted AS (
    SELECT a.article_id, a.sport, btrim(n) AS raw, public.nrm(n) AS norm
      FROM article_sport a
      JOIN public.news_article_readings r ON r.article_id = a.article_id
      CROSS JOIN LATERAL jsonb_array_elements_text(
             coalesce(r.evidence -> 'relevant_entities', '[]'::jsonb)) n
     WHERE public.nrm(n) <> ''
),
hit AS (
    SELECT em.article_id, em.sport, em.norm, s.entity_type, s.entity_id, min(em.raw) AS raw
      FROM emitted em
      JOIN public.entity_name_surfaces s ON s.norm = em.norm AND s.sport = em.sport
     GROUP BY 1, 2, 3, 4, 5
),
-- Ambiguity is REFUSED, not broken: a name that reaches more than one entity of the same sport
-- resolves to nothing. Measured at 0.38% of resolutions, all same-sport player namesakes.
counted AS (
    SELECT article_id, sport, norm, count(*) AS n_ent
      FROM hit GROUP BY 1, 2, 3
),
picked AS (
    SELECT h.article_id, h.sport, h.entity_type, h.entity_id, min(h.raw) AS surface
      FROM hit h JOIN counted c USING (article_id, sport, norm)
     WHERE c.n_ent = 1
     GROUP BY 1, 2, 3, 4
)
"#;

/// One resolved (article, entity) pair and what applying it would mean.
struct Pick {
    article_id: i64,
    entity_type: String,
    entity_id: i32,
    sport: String,
    /// `flip` | `new` | `already_true` | `pending_scrub` — see the CASE in `load_picks`.
    class: String,
    /// The Editor's name string that resolved here, for eyeballing the TSV.
    surface: String,
    title: String,
}

/// What a run is allowed to do. `Rollback` is the house discipline made a mode rather than a
/// separate script: it runs the REAL write, counts the rows it moved and asserts the trigger stayed
/// silent, then throws it all away. A copy of the statements in a psql scratch file would be a
/// second query claiming to be the first one, which is exactly the divergence trap A5 cost weeks to.
#[derive(PartialEq)]
enum Mode {
    DryRun,
    Rollback,
    Apply,
}

struct Args {
    pass: String,
    mode: Mode,
    out: String,
    since: String,
    until: String,
    max_rows: usize,
}

fn parse_args() -> Args {
    let mut pass = String::from("flips");
    let mut out: Option<String> = None;
    let mut mode = Mode::DryRun;
    let mut since = String::from(DEFAULT_SINCE);
    let mut until = String::from(DEFAULT_UNTIL);
    let mut max_rows = DEFAULT_MAX_ROWS;
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-pass" => {
                pass = argv[i + 1].clone();
                i += 2;
            }
            "-out" => {
                out = Some(argv[i + 1].clone());
                i += 2;
            }
            "-since" => {
                since = argv[i + 1].clone();
                i += 2;
            }
            "-until" => {
                until = argv[i + 1].clone();
                i += 2;
            }
            "-max-rows" => {
                max_rows = argv[i + 1].parse().expect("-max-rows takes an integer");
                i += 2;
            }
            // `-rollback` wins if both are given: between two readings of an operator's intent,
            // take the one that cannot damage anything.
            "-rollback" => {
                mode = Mode::Rollback;
                i += 1;
            }
            "-apply" => {
                if mode != Mode::Rollback {
                    mode = Mode::Apply;
                }
                i += 1;
            }
            other => panic!(
                "unknown arg {other}; usage: remap [-pass flips|new] [-rollback|-apply] \
                 [-out PATH] [-since TS] [-until TS] [-max-rows N]"
            ),
        }
    }
    if pass != "flips" && pass != "new" {
        panic!("-pass takes `flips` or `new`, not {pass}");
    }
    let out = out.unwrap_or_else(|| format!("planning_docs/data/remap_{pass}.tsv"));
    Args {
        pass,
        mode,
        out,
        since,
        until,
        max_rows,
    }
}

/// load_picks resolves the whole cohort once and classifies every pick against what is already in
/// `news_article_entities`. Both passes are computed on every run, so a dry run of either shows the
/// full census and the two halves can never be measured against different populations.
///
/// Note the join: `e.article_id IS NULL` (not `e.vetted IS NULL`) is what distinguishes a
/// brand-new link from an unscrubbed one — a LEFT JOIN yields NULL for `vetted` in BOTH cases, and
/// conflating them is how a census silently double-counts.
async fn load_picks(pool: &sqlx::PgPool, since: &str, until: &str) -> Result<Vec<Pick>> {
    let sql = format!(
        "{COHORT_CTE}
        SELECT p.article_id, p.entity_type, p.entity_id, p.sport, p.surface, a.title,
               CASE WHEN e.article_id IS NULL THEN 'new'
                    WHEN e.vetted IS FALSE    THEN 'flip'
                    WHEN e.vetted IS TRUE     THEN 'already_true'
                    ELSE 'pending_scrub' END AS class
          FROM picked p
          JOIN public.news_articles a ON a.id = p.article_id
          LEFT JOIN public.news_article_entities e
                 ON e.article_id = p.article_id AND e.entity_type = p.entity_type
                AND e.entity_id = p.entity_id  AND e.sport = p.sport
         ORDER BY p.article_id, p.entity_type, p.entity_id"
    );
    let rows = sqlx::query(&sql)
        .bind(since)
        .bind(until)
        .fetch_all(pool)
        .await
        .context("resolve cohort picks")?;
    Ok(rows
        .into_iter()
        .map(|r| Pick {
            article_id: r.get(0),
            entity_type: r.get(1),
            entity_id: r.get(2),
            sport: r.get(3),
            surface: r.get(4),
            title: r.get(5),
            class: r.get(6),
        })
        .collect())
}

/// load_refused is the ambiguity census — names that reached more than one entity and therefore
/// resolved to nothing. Printed rather than written: it is review material for B3/F7 (the residual
/// an alias cannot fix), not something this binary acts on.
async fn load_refused(
    pool: &sqlx::PgPool,
    since: &str,
    until: &str,
) -> Result<Vec<(String, i64, i64)>> {
    let sql = format!(
        "{COHORT_CTE}
        SELECT c.norm, max(c.n_ent) AS entities, count(*) AS instances
          FROM counted c
         WHERE c.n_ent > 1
         GROUP BY 1
         ORDER BY instances DESC, c.norm"
    );
    let rows = sqlx::query(&sql)
        .bind(since)
        .bind(until)
        .fetch_all(pool)
        .await
        .context("ambiguity census")?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get(0), r.get(1), r.get(2)))
        .collect())
}

/// assert_triggers_suppressed proves the T10 suppression took BEFORE anything is written. The
/// hazard is invisible at the call site — a failed SET would leave the flip pass quietly enqueueing
/// thousands of Editor re-reads — so it is asserted rather than assumed.
async fn assert_triggers_suppressed(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Result<()> {
    sqlx::query("SET LOCAL session_replication_role = 'replica'")
        .execute(&mut **tx)
        .await
        .context("suppress triggers (T10)")?;
    let role: String = sqlx::query_scalar("SHOW session_replication_role")
        .fetch_one(&mut **tx)
        .await
        .context("read back session_replication_role")?;
    if role != "replica" {
        bail!(
            "session_replication_role is {role:?}, not \"replica\" — refusing to write. \
             Flipping vetted with triggers live re-arms every touched article (T10)."
        );
    }
    Ok(())
}

fn write_tsv(path: &str, picks: &[&Pick]) -> Result<()> {
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::File::create(path)?;
    writeln!(
        f,
        "article_id\tentity_type\tentity_id\tsport\tclass\tsurface\ttitle"
    )?;
    for p in picks {
        writeln!(
            f,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            p.article_id,
            p.entity_type,
            p.entity_id,
            p.sport,
            p.class,
            p.surface.replace(['\t', '\n'], " "),
            p.title.replace(['\t', '\n'], " ")
        )?;
    }
    f.flush()?;
    Ok(())
}

fn census(picks: &[Pick]) -> BTreeMap<(String, String), usize> {
    let mut by_class: BTreeMap<(String, String), usize> = BTreeMap::new();
    for p in picks {
        *by_class
            .entry((p.class.clone(), p.entity_type.clone()))
            .or_default() += 1;
    }
    by_class
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args();
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, 2).await?;

    eprintln!(
        "remap: cohort = Editor readings updated in [{}, {}) carrying a rejected link",
        args.since, args.until
    );

    let picks = load_picks(&pool, &args.since, &args.until).await?;
    let refused = load_refused(&pool, &args.since, &args.until).await?;

    let by_class = census(&picks);
    let n = |class: &str, et: &str| {
        by_class
            .get(&(class.to_string(), et.to_string()))
            .copied()
            .unwrap_or(0)
    };
    eprintln!(
        "remap: {} exact resolutions — flip {} ({} team / {} player) · new {} ({} team / {} player) \
         · already_true {} · pending_scrub {}",
        picks.len(),
        n("flip", "team") + n("flip", "player"),
        n("flip", "team"),
        n("flip", "player"),
        n("new", "team") + n("new", "player"),
        n("new", "team"),
        n("new", "player"),
        n("already_true", "team") + n("already_true", "player"),
        n("pending_scrub", "team") + n("pending_scrub", "player"),
    );
    eprintln!(
        "remap: ambiguity REFUSED on {} distinct names ({} instances) — top: {}",
        refused.len(),
        refused.iter().map(|r| r.2).sum::<i64>(),
        refused
            .iter()
            .take(6)
            .map(|(name, ents, inst)| format!("{name} ({ents} entities, {inst}×)"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let want = if args.pass == "flips" { "flip" } else { "new" };
    let selected: Vec<&Pick> = picks.iter().filter(|p| p.class == want).collect();
    let articles: HashSet<i64> = selected.iter().map(|p| p.article_id).collect();

    write_tsv(&args.out, &selected)?;
    eprintln!(
        "remap: pass={} → {} links across {} articles; inventory written to {}",
        args.pass,
        selected.len(),
        articles.len(),
        args.out
    );

    if selected.is_empty() {
        eprintln!("remap: nothing to do.");
        return Ok(());
    }
    if args.mode == Mode::DryRun {
        eprintln!(
            "remap: DRY RUN — nothing written to the database. Rehearse the real write with \
             -rollback, then commit it with -apply."
        );
        return Ok(());
    }
    if selected.len() > args.max_rows {
        bail!(
            "{} rows exceeds the -max-rows ceiling of {}. The B4 cohort is ~9,800 flips / ~5,900 \
             new; a number far above that means the window is wrong and the run would un-reject \
             legitimate scrub verdicts. Check -since/-until before raising the ceiling.",
            selected.len(),
            args.max_rows
        );
    }

    // Columnar arrays, so the write touches EXACTLY the rows just inventoried and reported rather
    // than re-running the resolver against a corpus that has moved underneath it.
    let article_ids: Vec<i64> = selected.iter().map(|p| p.article_id).collect();
    let entity_types: Vec<String> = selected.iter().map(|p| p.entity_type.clone()).collect();
    let entity_ids: Vec<i32> = selected.iter().map(|p| p.entity_id).collect();
    let sports: Vec<String> = selected.iter().map(|p| p.sport.clone()).collect();

    // A DB-side mark, taken before the transaction, so the post-check below can tell a re-read this
    // run caused from one the live pipeline enqueued a minute ago.
    let mark: String = sqlx::query_scalar("SELECT now()::text")
        .fetch_one(&pool)
        .await?;

    let mut tx = pool.begin().await.context("begin remap transaction")?;
    assert_triggers_suppressed(&mut tx).await?;

    let affected = if args.pass == "flips" {
        eprintln!(
            "remap: T10 — triggers suppressed; {} article_read re-reads NOT enqueued",
            articles.len()
        );
        // `vetted IS FALSE` is re-checked here, not just in the inventory: it makes the pass
        // idempotent and refuses to overwrite a verdict something else has since changed.
        // `scrubbed_at` is deliberately left alone — it records when the incident scrub judged this
        // link, and that provenance is worth more than a fresh timestamp.
        sqlx::query(
            r#"
            UPDATE public.news_article_entities e
               SET vetted = TRUE
              FROM unnest($1::bigint[], $2::text[], $3::int[], $4::text[])
                     AS t(article_id, entity_type, entity_id, sport)
             WHERE e.article_id  = t.article_id
               AND e.entity_type = t.entity_type
               AND e.entity_id   = t.entity_id
               AND e.sport       = t.sport
               AND e.vetted IS FALSE
            "#,
        )
        .bind(&article_ids)
        .bind(&entity_types)
        .bind(&entity_ids)
        .bind(&sports)
        .execute(&mut *tx)
        .await
        .context("flip vetted FALSE -> TRUE")?
        .rows_affected()
    } else {
        // `scrubbed_at = NOW()` keeps these OUT of the scrub sweep's backlog (`scrubbed_at IS
        // NULL`). They are re-mapped, not unjudged, and the plan is explicit that the cohort must
        // not be re-judged. `title_pos` stays NULL — the ingest regex never matched these names in
        // the title, which is the whole reason the link is missing; NULL is its documented
        // "unknown", carried by 48,809 existing rows, and sorts last in the co-mention ordering.
        sqlx::query(
            r#"
            INSERT INTO public.news_article_entities
                (article_id, entity_type, entity_id, sport, match_confidence,
                 vetted, scrubbed_at, created_at)
            SELECT t.article_id, t.entity_type, t.entity_id, t.sport, $5::float8::numeric(4,3),
                   TRUE, NOW(), NOW()
              FROM unnest($1::bigint[], $2::text[], $3::int[], $4::text[])
                     AS t(article_id, entity_type, entity_id, sport)
            ON CONFLICT (article_id, entity_type, entity_id, sport) DO NOTHING
            "#,
        )
        .bind(&article_ids)
        .bind(&entity_types)
        .bind(&entity_ids)
        .bind(&sports)
        .bind(EDITOR_MATCH_CONFIDENCE)
        .execute(&mut *tx)
        .await
        .context("insert Editor-derived links")?
        .rows_affected()
    };

    eprintln!(
        "remap: {} rows written in-transaction (planned {})",
        affected,
        selected.len()
    );
    if affected as usize != selected.len() {
        eprintln!(
            "remap: NOTE — {} planned rows were already in the target state (idempotent re-run, or \
             the live pipeline moved them between inventory and write). Nothing was overwritten.",
            selected.len() - affected as usize
        );
    }

    // The T10 invariant, MEASURED rather than assumed, and measured INSIDE the transaction so it
    // sees this run's own uncommitted effect. If suppression had failed, the flip pass would have
    // left one freshly-opened `article_read` row per touched article — so a non-zero count vetoes
    // the commit. A spurious veto (the live pipeline enqueuing the same article in the same second)
    // costs a re-run; a missed re-arm costs ~5,300 gemma reads through the gate C1 is replacing.
    let rearmed: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
          FROM public.pipeline_work
         WHERE stage = 'article_read' AND entity_type = 'article'
           AND entity_id = ANY($1::int[])
           AND updated_at >= $2::timestamptz
        "#,
    )
    .bind(
        article_ids
            .iter()
            .map(|id| *id as i32)
            .collect::<Vec<i32>>(),
    )
    .bind(&mark)
    .fetch_one(&mut *tx)
    .await
    .context("verify no re-arm (T10)")?;
    if rearmed != 0 {
        tx.rollback().await.ok();
        bail!(
            "T10 — {rearmed} article_read items were opened or re-opened since {mark}. \
             ROLLED BACK; nothing was written. Suppression is asserted before the write, so this \
             is most likely the live pipeline touching the same articles concurrently — check \
             `input_version` on those rows and re-run."
        );
    }
    eprintln!("remap: T10 verified — 0 article_read items enqueued or re-opened.");

    match args.mode {
        Mode::Apply => {
            tx.commit().await.context("commit remap transaction")?;
            eprintln!("remap: APPLIED — {affected} rows committed.");
            if args.pass == "flips" {
                eprintln!(
                    "remap: {} articles are now visible to the Journalist carrying the reading \
                     they already have. Nothing was enqueued: the corpus is picked up on each \
                     entity's next narratives turn.",
                    articles.len()
                );
            }
        }
        _ => {
            tx.rollback().await.context("roll back remap rehearsal")?;
            eprintln!(
                "remap: ROLLED BACK — the write above was rehearsed and discarded. Inspect {} \
                 before re-running with -apply.",
                args.out
            );
        }
    }

    Ok(())
}
