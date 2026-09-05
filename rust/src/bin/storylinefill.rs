//! storylinefill — the one-shot storyline backfill (PLAN-one-rail Phase 6.2).
//!
//! Replays the shadow period's `editor_reads` through the §1b attachment rule, OLDEST FIRST, so
//! the storylines form in the order the news arrived rather than in whatever order a scan
//! returns. Every membership edge it writes is stamped `attach_method='backfill'`, so the live
//! rail's work and this repair are never confused for one another afterwards.
//!
//! ```text
//!   storylinefill [-rollback|-apply] [-limit N] [-batch N] [-sport FOOTBALL]
//! ```
//!
//! `-rollback` (the default) rehearses the write inside ONE transaction, asserts the invariants
//! INSIDE it, prints the shape of what it built, and throws it away — the `remap -rollback`
//! habit, and the only honest way to look at a 12k-row write before making it. `-apply` commits,
//! in batches, so the live Editor is never locked out of `storylines` for the length of a
//! 12,000-article replay.
//!
//! The clock is the ARTICLE's, never the wall: each read is attached `as_of` its publish time
//! (fetch time when the feed carried none). Replaying a week-old corpus against `NOW()` would
//! hand every candidate the recency bonus and collapse a week into one day's storylines.
//!
//! Zero model calls. Zero packets: this bin never inserts one, so mig 206's fan-out cannot fire
//! from a backfill (see `editor::packet`'s fan-out note).

use anyhow::{Context, Result};
use scoracle_cognition::db;
use scoracle_cognition::junctions::editor::derive::Resolved;
use scoracle_cognition::junctions::editor::storyline::{self, AttachMethod};
use scoracle_cognition::junctions::editor::EditorRead;
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Rollback,
    Apply,
}

struct Args {
    mode: Mode,
    /// Reads to process. 0 = every unattached read (the real backfill).
    limit: i64,
    /// Rows per committed transaction under `-apply`.
    batch: i64,
    sport: Option<String>,
}

/// One read to replay, loaded in arrival order.
struct Pending {
    article_id: i64,
    title: String,
    sport: String,
    as_of: i64,
    read: EditorRead,
    resolved: Resolved,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args()?;
    let database_url = std::env::var("DATABASE_PRIVATE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .context("DATABASE_PRIVATE_URL or DATABASE_URL must be set")?;
    let pool = db::build_pool(&database_url, 4).await?;

    let pending = load_pending(&pool, args.limit, args.sport.as_deref()).await?;
    eprintln!(
        "storylinefill: {} unattached successful reads to replay{}",
        pending.len(),
        args.sport
            .as_deref()
            .map(|s| format!(" (sport {s})"))
            .unwrap_or_default()
    );
    if pending.is_empty() {
        return Ok(());
    }
    eprintln!(
        "storylinefill: window {} → {} (article clock)",
        pending.first().map(|p| p.as_of).unwrap_or(0),
        pending.last().map(|p| p.as_of).unwrap_or(0),
    );

    match args.mode {
        Mode::Rollback => rehearse(&pool, &pending).await,
        Mode::Apply => apply(&pool, &pending, args.batch).await,
    }
}

/// The rehearsal: one transaction, invariants asserted inside it, then rolled back.
async fn rehearse(pool: &PgPool, pending: &[Pending]) -> Result<()> {
    let mut tx = pool.begin().await.context("begin rehearsal")?;
    let before = counts(&mut tx).await?;
    let mut opened = 0usize;
    let mut attached = 0usize;
    let mut skipped = 0usize;

    for p in pending {
        match storyline::attach_read(
            &mut tx,
            &p.sport,
            p.article_id,
            &p.title,
            &p.read,
            &p.resolved,
            AttachMethod::Backfill,
            Some(p.as_of),
        )
        .await?
        {
            Some(a) if a.opened => opened += 1,
            Some(_) => attached += 1,
            None => skipped += 1,
        }
    }

    let after = counts(&mut tx).await?;
    report(pending.len(), opened, attached, skipped, before, after);
    print_shape(&mut tx).await?;

    // The invariants, measured INSIDE the transaction so they see this run's own uncommitted
    // effect. Each one is a way the replay could be silently wrong rather than loudly broken.
    let unstamped: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*) FROM public.editor_reads er
         WHERE er.article_id = ANY($1::bigint[]) AND er.storyline_id IS NULL
        "#,
    )
    .bind(pending.iter().map(|p| p.article_id).collect::<Vec<i64>>())
    .fetch_one(&mut *tx)
    .await
    .context("invariant: every replayed read is stamped")?;

    let orphan_storylines: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*) FROM public.storylines s
         WHERE NOT EXISTS (SELECT 1 FROM public.storyline_entities se
                            WHERE se.storyline_id = s.id)
        "#,
    )
    .fetch_one(&mut *tx)
    .await
    .context("invariant: no storyline without participants")?;

    let wrong_method: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*) FROM public.storyline_articles sa
         WHERE sa.article_id = ANY($1::bigint[]) AND sa.attach_method <> 'backfill'
        "#,
    )
    .bind(pending.iter().map(|p| p.article_id).collect::<Vec<i64>>())
    .fetch_one(&mut *tx)
    .await
    .context("invariant: replayed edges are stamped backfill")?;

    tx.rollback().await.context("roll back rehearsal")?;

    eprintln!(
        "storylinefill: invariants — unstamped reads {unstamped} (want 0, minus {skipped} skipped), \
         participant-less storylines {orphan_storylines} (want 0), \
         non-backfill edges among replayed articles {wrong_method} (want 0)"
    );
    eprintln!(
        "storylinefill: ROLLED BACK — nothing was written. Re-run with -apply to commit this shape."
    );
    if unstamped as usize != skipped || orphan_storylines != 0 {
        eprintln!(
            "storylinefill: WARNING — an invariant is off its expected value. Do NOT -apply \
             until that is understood."
        );
    }
    Ok(())
}

/// The commit path: same replay, committed in batches so the live Editor keeps its turn.
async fn apply(pool: &PgPool, pending: &[Pending], batch: i64) -> Result<()> {
    let before = counts_pool(pool).await?;
    let mut opened = 0usize;
    let mut attached = 0usize;
    let mut skipped = 0usize;

    for (n, chunk) in pending.chunks(batch.max(1) as usize).enumerate() {
        let mut tx = pool.begin().await.context("begin backfill batch")?;
        for p in chunk {
            match storyline::attach_read(
                &mut tx,
                &p.sport,
                p.article_id,
                &p.title,
                &p.read,
                &p.resolved,
                AttachMethod::Backfill,
                Some(p.as_of),
            )
            .await?
            {
                Some(a) if a.opened => opened += 1,
                Some(_) => attached += 1,
                None => skipped += 1,
            }
        }
        tx.commit().await.context("commit backfill batch")?;
        eprintln!(
            "storylinefill: batch {} committed ({} reads; opened {opened}, attached {attached})",
            n + 1,
            chunk.len()
        );
    }

    let after = counts_pool(pool).await?;
    report(pending.len(), opened, attached, skipped, before, after);
    let mut conn = pool.acquire().await.context("acquire for shape report")?;
    print_shape(&mut conn).await?;
    eprintln!("storylinefill: APPLIED.");
    Ok(())
}

/// The reads still waiting for a story, oldest first. Only successful reads with at least one
/// resolved link can attach — the join key is entities (§1b).
async fn load_pending(pool: &PgPool, limit: i64, sport: Option<&str>) -> Result<Vec<Pending>> {
    let rows = sqlx::query(
        r#"
        SELECT er.article_id, a.title, er.read, er.resolved,
               EXTRACT(EPOCH FROM COALESCE(a.published_at, a.fetched_at))::bigint AS as_of,
               upper(COALESCE(er.resolved -> 'links' -> 0 ->> 'sport', '')) AS sport
          FROM public.editor_reads er
          JOIN public.news_articles a ON a.id = er.article_id
         WHERE er.status = 'success'
           AND er.storyline_id IS NULL
           AND jsonb_array_length(COALESCE(er.resolved -> 'links', '[]'::jsonb)) > 0
           AND ($2::text IS NULL
                OR upper(COALESCE(er.resolved -> 'links' -> 0 ->> 'sport', '')) = upper($2))
         ORDER BY COALESCE(a.published_at, a.fetched_at) ASC, er.article_id ASC
         LIMIT CASE WHEN $1 <= 0 THEN NULL ELSE $1 END
        "#,
    )
    .bind(limit)
    .bind(sport)
    .fetch_all(pool)
    .await
    .context("load unattached editor reads")?;

    let mut pending = Vec::with_capacity(rows.len());
    for r in rows {
        let article_id: i64 = r.get("article_id");
        let envelope: serde_json::Value = r.get("read");
        let read: EditorRead = match serde_json::from_value(envelope) {
            Ok(read) => read,
            Err(e) => {
                eprintln!("storylinefill: article {article_id} read did not parse ({e}); skipping");
                continue;
            }
        };
        let resolved_json: serde_json::Value = r.get("resolved");
        let resolved = match resolved_from_json(&resolved_json) {
            Some(resolved) => resolved,
            None => {
                eprintln!("storylinefill: article {article_id} resolved did not parse; skipping");
                continue;
            }
        };
        pending.push(Pending {
            article_id,
            title: r.get("title"),
            sport: r.get("sport"),
            as_of: r.get::<Option<i64>, _>("as_of").unwrap_or(0),
            read,
            resolved,
        });
    }
    Ok(pending)
}

/// Rebuilds a [`Resolved`] from the persisted `editor_reads.resolved` shape (§1a). Only the two
/// buckets the attachment rule reads are reconstructed in full — links (the join key) and the
/// name buckets `link_roles` needs to line links up with `names[]`.
fn resolved_from_json(v: &serde_json::Value) -> Option<Resolved> {
    use scoracle_cognition::junctions::editor::derive::{RefusedName, ResolvedLink};
    use scoracle_cognition::junctions::editor::NameMention;

    let links = v.get("links")?.as_array()?;
    let mut out = Resolved::default();
    for l in links {
        out.links.push(ResolvedLink {
            entity_type: l.get("entity_type")?.as_str()?.to_string(),
            entity_id: i32::try_from(l.get("entity_id")?.as_i64()?).ok()?,
            sport: l.get("sport")?.as_str()?.to_string(),
            via_surface: l
                .get("via_surface")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string(),
        });
    }
    for u in v
        .get("unresolved")
        .and_then(|u| u.as_array())
        .into_iter()
        .flatten()
    {
        out.unresolved.push(NameMention {
            name: u
                .get("name")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string(),
            kind_hint: u
                .get("kind_hint")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string(),
            descriptor: u
                .get("descriptor")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string(),
        });
    }
    for rf in v
        .get("refused_ambiguous")
        .and_then(|u| u.as_array())
        .into_iter()
        .flatten()
    {
        out.refused_ambiguous.push(RefusedName {
            name: rf
                .get("name")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string(),
            kind_hint: rf
                .get("kind_hint")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string(),
            descriptor: rf
                .get("descriptor")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string(),
            candidates: Vec::new(),
        });
    }
    Some(out)
}

async fn counts(conn: &mut sqlx::PgConnection) -> Result<(i64, i64)> {
    let storylines: i64 = sqlx::query_scalar("SELECT count(*) FROM public.storylines")
        .fetch_one(&mut *conn)
        .await
        .context("count storylines")?;
    let edges: i64 = sqlx::query_scalar("SELECT count(*) FROM public.storyline_articles")
        .fetch_one(&mut *conn)
        .await
        .context("count storyline_articles")?;
    Ok((storylines, edges))
}

async fn counts_pool(pool: &PgPool) -> Result<(i64, i64)> {
    let mut conn = pool.acquire().await.context("acquire for counts")?;
    counts(&mut conn).await
}

fn report(
    total: usize,
    opened: usize,
    attached: usize,
    skipped: usize,
    before: (i64, i64),
    after: (i64, i64),
) {
    let placed = opened + attached;
    eprintln!(
        "storylinefill: replayed {total} reads — opened {opened}, attached {attached}, \
         skipped {skipped}; storylines {} → {}, membership edges {} → {}",
        before.0, after.0, before.1, after.1
    );
    if placed > 0 {
        eprintln!(
            "storylinefill: {:.1}% attached to an existing storyline, {:.2} articles per storyline",
            100.0 * attached as f64 / placed as f64,
            after.1 as f64 / after.0.max(1) as f64,
        );
    }
}

/// The distribution 6.7 reads: how big the biggest clusters are, and how the long tail sits.
async fn print_shape(conn: &mut sqlx::PgConnection) -> Result<()> {
    let rows = sqlx::query(
        r#"
        SELECT s.id, s.sport, count(sa.article_id)::bigint AS members,
               COALESCE(s.title, '') AS title
          FROM public.storylines s
          LEFT JOIN public.storyline_articles sa ON sa.storyline_id = s.id
         GROUP BY s.id, s.sport, s.title
         ORDER BY members DESC, s.id
         LIMIT 10
        "#,
    )
    .fetch_all(&mut *conn)
    .await
    .context("storyline shape")?;
    eprintln!("storylinefill: biggest storylines —");
    for r in rows {
        eprintln!(
            "  #{:<6} {:<9} {:>4} articles  {}",
            r.get::<i64, _>("id"),
            r.get::<String, _>("sport"),
            r.get::<i64, _>("members"),
            truncate(&r.get::<String, _>("title"), 70),
        );
    }

    let hist = sqlx::query(
        r#"
        SELECT members, count(*)::bigint AS storylines FROM (
            SELECT count(sa.article_id)::bigint AS members
              FROM public.storylines s
              LEFT JOIN public.storyline_articles sa ON sa.storyline_id = s.id
             GROUP BY s.id
        ) x GROUP BY members ORDER BY members
        "#,
    )
    .fetch_all(&mut *conn)
    .await
    .context("storyline histogram")?;
    let mut buckets: BTreeMap<&'static str, i64> = BTreeMap::new();
    for r in hist {
        let members: i64 = r.get("members");
        let n: i64 = r.get("storylines");
        let bucket = match members {
            0..=1 => "1",
            2..=3 => "2-3",
            4..=9 => "4-9",
            10..=24 => "10-24",
            _ => "25+",
        };
        *buckets.entry(bucket).or_default() += n;
    }
    eprintln!("storylinefill: articles-per-storyline — {buckets:?}");
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "…"
}

fn parse_args() -> Result<Args> {
    let mut saw_rollback = false;
    let mut saw_apply = false;
    let mut limit = 0i64;
    let mut batch = 500i64;
    let mut sport = None;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-rollback" | "--rollback" => saw_rollback = true,
            "-apply" | "--apply" => saw_apply = true,
            "-limit" | "--limit" => {
                limit = it
                    .next()
                    .context("-limit needs a value")?
                    .parse()
                    .context("-limit must be an integer")?
            }
            "-batch" | "--batch" => {
                batch = it
                    .next()
                    .context("-batch needs a value")?
                    .parse()
                    .context("-batch must be an integer")?
            }
            "-sport" | "--sport" => sport = Some(it.next().context("-sport needs a value")?),
            other => anyhow::bail!(
                "unknown arg {other}; usage: storylinefill [-rollback|-apply] [-limit N] \
                 [-batch N] [-sport FOOTBALL]"
            ),
        }
    }
    // `-rollback` wins if both are given (the remap habit: between two readings of an operator's
    // intent, take the one that writes nothing).
    let mode = if saw_apply && !saw_rollback {
        Mode::Apply
    } else {
        Mode::Rollback
    };
    Ok(Args {
        mode,
        limit,
        batch,
        sport,
    })
}
