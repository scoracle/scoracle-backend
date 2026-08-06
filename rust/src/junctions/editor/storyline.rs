//! The Desk, part one — storyline assembly (PLAN-one-rail Phase 6.1, §1b).
//!
//! Deterministic code, zero model tokens. Every read the Editor commits is scored against the
//! open storylines of its sport and either ATTACHES to the best one or OPENS a new one. The
//! join key is **entities + story type + time** — free-text story names are banned from
//! matching (D3's lesson), which is why `storylines.title` is display-only and never read here.
//!
//! **T3 is the law of this module.** Two articles that share entities and disagree about the
//! facts are the SAME STORY WITH A DIFFERENT CLAIM: they attach to one storyline, and the
//! packet compiler keeps both claims, attributed. Nothing in the scoring rule looks at whether
//! members agree — the disagreement IS the story, and collapsing it would be the one failure
//! this design exists to prevent.
//!
//! The clock is a PARAMETER (`as_of_epoch`), not `now()`, because 6.2's backfill replays the
//! shadow corpus in arrival order: scored against wall-clock now, every week-old read would see
//! every storyline as equally recent and the 14-day candidate window would never close. Live
//! callers pass `None` and get `NOW()`.

use super::derive::Resolved;
use super::{EditorEntityRole, EditorRead, NameMention};
use anyhow::{Context, Result};
use sqlx::{PgConnection, PgPool, Row};
use tracing::debug;

/// Candidate storylines must have been touched within this window (§1b).
pub const CANDIDATE_WINDOW_DAYS: i64 = 14;
/// A candidate touched this recently earns the recency bonus.
pub const RECENCY_BONUS_HOURS: i64 = 48;
/// **People are the join; a club alone is a coincidence.** A shared person is worth two points
/// of overlap, a shared club one.
///
/// This is not a taste call. Go queries the news one ranked query PER TEAM, so the club is on
/// the hypothesis list of every article of its day and resolves as a link in most of them — a
/// shared club says only "both stories happened at Real Madrid", which is true of twenty
/// unrelated stories every Tuesday. A shared person says the two articles are about the same
/// human being doing the same thing.
pub const PERSON_OVERLAP_POINTS: i32 = 2;
pub const TEAM_OVERLAP_POINTS: i32 = 1;
/// Attach to the top scorer **above** this threshold (§1b); everything else opens a new
/// storyline.
///
/// With the weights above, 3 is exactly the line the 6.5 hand count draws. A same-day article
/// sharing one PERSON scores 2 + 1 (type) + 1 (recency) = 4 and attaches; a same-day article
/// sharing only the CLUB scores 1 + 1 + 1 = 3 and opens its own storyline. That is what keeps
/// Lee Kang-in — a PSG player whose article also links Real Madrid as an interested party —
/// out of Real Madrid's transfer saga, while twenty pieces about the same Vinicius story land
/// in one place.
pub const ATTACH_THRESHOLD: i32 = 3;

/// How the membership edge was written (`storyline_articles.attach_method`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachMethod {
    Auto,
    Backfill,
}

impl AttachMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            AttachMethod::Auto => "auto",
            AttachMethod::Backfill => "backfill",
        }
    }
}

/// One scored candidate — the pure half of the attachment rule.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Candidate {
    pub storyline_id: i64,
    /// DISTINCT active `player`/`person` participants shared with this read's resolved links.
    pub person_overlap: i32,
    /// DISTINCT active `team` participants shared with this read's resolved links.
    pub team_overlap: i32,
    /// The storyline's dominant member story type (mode; NULL for a storyline whose members
    /// never carried one).
    pub dominant_type: Option<String>,
    /// `as_of - storylines.last_seen_at`, in seconds. Never negative for a candidate (the
    /// query refuses storylines from the future — see [`candidates`]).
    pub age_secs: i64,
}

/// score is §1b's rule, entire: weighted entity overlap + 1 for a matching story type + 1 for
/// recency.
///
/// Overlap dominates on purpose. Type and recency are single points because they are cheap
/// coincidences — half the football stories on a Tuesday are `transfer`, and everything in the
/// candidate set is by definition within 14 days.
pub fn score(candidate: &Candidate, story_type: &str) -> i32 {
    let type_match = !story_type.trim().is_empty()
        && candidate
            .dominant_type
            .as_deref()
            .is_some_and(|d| d.eq_ignore_ascii_case(story_type.trim()));
    let recent = candidate.age_secs <= RECENCY_BONUS_HOURS * 3_600;
    candidate.person_overlap * PERSON_OVERLAP_POINTS
        + candidate.team_overlap * TEAM_OVERLAP_POINTS
        + i32::from(type_match)
        + i32::from(recent)
}

/// pick returns the winning (storyline_id, score) above the threshold, or None to open a new
/// storyline. Ties break toward the FRESHEST candidate, then the lowest id (the established
/// storyline) — deterministic, so a replay of the same corpus assembles the same storylines.
pub fn pick(candidates: &[Candidate], story_type: &str) -> Option<(i64, i32)> {
    candidates
        .iter()
        .map(|c| (c, score(c, story_type)))
        .filter(|(_, s)| *s > ATTACH_THRESHOLD)
        .max_by(|(a, sa), (b, sb)| {
            sa.cmp(sb)
                .then(b.age_secs.cmp(&a.age_secs))
                .then(b.storyline_id.cmp(&a.storyline_id))
        })
        .map(|(c, s)| (c.storyline_id, s))
}

/// The outcome of one attach, for logging and the backfill's counters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub storyline_id: i64,
    pub opened: bool,
    pub score: i32,
    pub candidates: usize,
}

/// attach_read runs the §1b rule for one committed read, on the caller's connection.
///
/// It opens NO transaction of its own: the live path wraps one attach ([`attach_in_tx`]), and
/// 6.2's backfill wraps a whole batch so a rehearsal can be rolled back with the invariants
/// asserted inside it (the `remap -rollback` habit). Every write below belongs to whatever the
/// caller committed to.
///
/// `Ok(None)` means the read carries no join key and is deliberately left unattached: an
/// article whose names resolved to NOTHING has no entity to join on, and a storyline with no
/// participants can never be matched into, nor fanned out from (mig 206 routes on
/// `storyline_entities`). Those reads stay in `editor_reads` as archive and reach the
/// Investigator through the nomination sweep instead.
///
/// Already-attached reads return `Ok(None)` too: membership is settled once. A re-read after a
/// body change updates the read, never the story it belongs to.
#[allow(clippy::too_many_arguments)]
pub async fn attach_read(
    conn: &mut PgConnection,
    sport: &str,
    article_id: i64,
    title: &str,
    read: &EditorRead,
    resolved: &Resolved,
    method: AttachMethod,
    as_of_epoch: Option<i64>,
) -> Result<Option<Attachment>> {
    if resolved.links.is_empty() {
        return Ok(None);
    }
    if already_attached(&mut *conn, article_id).await? {
        return Ok(None);
    }

    let entity_types: Vec<String> = resolved.links.iter().map(|l| l.entity_type.clone()).collect();
    let entity_ids: Vec<i32> = resolved.links.iter().map(|l| l.entity_id).collect();
    let roles = link_roles(&read.names, &read.entity_roles, resolved);

    let candidates = candidates(
        &mut *conn,
        sport,
        &entity_types,
        &entity_ids,
        as_of_epoch,
    )
    .await?;
    let chosen = pick(&candidates, &read.story_type);
    let tx = &mut *conn;

    let (storyline_id, opened, score) = match chosen {
        Some((id, score)) => {
            sqlx::query(
                r#"
                UPDATE public.storylines
                   SET last_seen_at = GREATEST(last_seen_at, COALESCE(to_timestamp($2), NOW()))
                 WHERE id = $1
                "#,
            )
            .bind(id)
            .bind(as_of_epoch.map(|e| e as f64))
            .execute(&mut *tx)
            .await
            .with_context(|| format!("touch storyline {id}"))?;
            (id, false, score)
        }
        None => {
            let id: i64 = sqlx::query_scalar(
                r#"
                INSERT INTO public.storylines
                    (sport, title, status, first_seen_at, last_seen_at)
                VALUES ($1, NULLIF($2, ''), 'open',
                        COALESCE(to_timestamp($3), NOW()), COALESCE(to_timestamp($3), NOW()))
                RETURNING id
                "#,
            )
            .bind(sport.to_uppercase())
            .bind(title)
            .bind(as_of_epoch.map(|e| e as f64))
            .fetch_one(&mut *tx)
            .await
            .context("open storyline")?;
            (id, true, 0)
        }
    };

    sqlx::query(
        r#"
        INSERT INTO public.storyline_articles
            (storyline_id, article_id, attached_at, attach_method)
        VALUES ($1, $2, COALESCE(to_timestamp($3), NOW()), $4)
        ON CONFLICT (storyline_id, article_id) DO NOTHING
        "#,
    )
    .bind(storyline_id)
    .bind(article_id)
    .bind(as_of_epoch.map(|e| e as f64))
    .bind(method.as_str())
    .execute(&mut *tx)
    .await
    .with_context(|| format!("attach article {article_id} to storyline {storyline_id}"))?;

    // D5: an entity's part in a storyline has its own lifespan. A fresh mention bumps
    // `last_seen_at`; it never resurrects an edge a resolution closed (`left_at` stays put —
    // reopening it would undo the close-in-one-stroke this table exists for).
    sqlx::query(
        r#"
        INSERT INTO public.storyline_entities
            (storyline_id, entity_type, entity_id, sport, role, joined_at, last_seen_at)
        -- DISTINCT ON: two emitted names can resolve to ONE entity (a name and an alias on the
        -- same norm), and Postgres refuses an ON CONFLICT that would touch a row twice in one
        -- statement. The strongest role among the duplicates is the one that survives.
        SELECT DISTINCT ON (m.entity_type, m.entity_id)
               $1, m.entity_type, m.entity_id, $2, NULLIF(m.role, ''),
               COALESCE(to_timestamp($6), NOW()), COALESCE(to_timestamp($6), NOW())
          FROM unnest($3::text[], $4::int[], $5::text[]) AS m(entity_type, entity_id, role)
         ORDER BY m.entity_type, m.entity_id,
                  CASE m.role WHEN 'subject' THEN 0 WHEN 'opponent' THEN 1
                              WHEN 'passing_mention' THEN 2 ELSE 3 END
        ON CONFLICT (storyline_id, entity_type, entity_id, sport) DO UPDATE SET
            last_seen_at = GREATEST(public.storyline_entities.last_seen_at,
                                    EXCLUDED.last_seen_at),
            role = COALESCE(public.storyline_entities.role, EXCLUDED.role)
        "#,
    )
    .bind(storyline_id)
    .bind(sport.to_uppercase())
    .bind(&entity_types)
    .bind(&entity_ids)
    .bind(&roles)
    .bind(as_of_epoch.map(|e| e as f64))
    .execute(&mut *tx)
    .await
    .with_context(|| format!("upsert storyline entities {storyline_id}"))?;

    // `updated_at` is deliberately NOT touched: the cutover's coverage clause (§2.1) measures
    // an editor_reads row's age against ingest, and the Desk bumping it would flatter that
    // number with work the Editor did not do.
    sqlx::query(
        r#"
        UPDATE public.editor_reads
           SET storyline_id = $2
         WHERE article_id = $1
        "#,
    )
    .bind(article_id)
    .bind(storyline_id)
    .execute(&mut *tx)
    .await
    .with_context(|| format!("stamp editor_reads.storyline_id {article_id}"))?;

    debug!(
        article_id,
        storyline_id,
        opened,
        score,
        candidates = candidates.len(),
        story_type = %read.story_type,
        method = method.as_str(),
        "storyline attach"
    );
    Ok(Some(Attachment {
        storyline_id,
        opened,
        score,
        candidates: candidates.len(),
    }))
}

/// attach_in_tx is one attach, atomic: the membership edge, the participant edges and the
/// read's stamp land together or not at all.
#[allow(clippy::too_many_arguments)]
pub async fn attach_in_tx(
    pool: &PgPool,
    sport: &str,
    article_id: i64,
    title: &str,
    read: &EditorRead,
    resolved: &Resolved,
    method: AttachMethod,
    as_of_epoch: Option<i64>,
) -> Result<Option<Attachment>> {
    let mut tx = pool
        .begin()
        .await
        .with_context(|| format!("begin storyline attach {article_id}"))?;
    let outcome = attach_read(
        &mut tx,
        sport,
        article_id,
        title,
        read,
        resolved,
        method,
        as_of_epoch,
    )
    .await?;
    tx.commit()
        .await
        .with_context(|| format!("commit storyline attach {article_id}"))?;
    Ok(outcome)
}

/// attach_best_effort is the live call site's wrapper: the read is already persisted and the
/// Desk is downstream bookkeeping, so a failure here must never re-run the model call.
pub async fn attach_best_effort(
    pool: &PgPool,
    sport: &str,
    article_id: i64,
    title: &str,
    read: &EditorRead,
    resolved: &Resolved,
) {
    if let Err(e) = attach_in_tx(
        pool,
        sport,
        article_id,
        title,
        read,
        resolved,
        AttachMethod::Auto,
        None,
    )
    .await
    {
        tracing::warn!(
            article_id,
            error = %format!("{e:#}"),
            "storyline attach failed (read already persisted; continuing)"
        );
    }
}

async fn already_attached(conn: &mut PgConnection, article_id: i64) -> Result<bool> {
    let hit: Option<i64> = sqlx::query_scalar(
        "SELECT storyline_id FROM public.editor_reads WHERE article_id = $1 AND storyline_id IS NOT NULL",
    )
    .bind(article_id)
    .fetch_optional(conn)
    .await
    .context("check existing storyline attachment")?;
    Ok(hit.is_some())
}

/// candidates is §1b's candidate set: OPEN storylines in this sport that share at least one
/// ACTIVE participant with the read and were touched inside the 14-day window.
///
/// The dominant type is a mode over member reads, computed here rather than cached on the
/// storyline: a cached rollup is a second writer of the same fact, and this query runs against
/// a handful of candidates, not the table.
async fn candidates(
    conn: &mut PgConnection,
    sport: &str,
    entity_types: &[String],
    entity_ids: &[i32],
    as_of_epoch: Option<i64>,
) -> Result<Vec<Candidate>> {
    let rows = sqlx::query(
        r#"
        WITH clock AS (SELECT COALESCE(to_timestamp($4), NOW()) AS as_of),
        mine AS (
            -- DISTINCT: two emitted names can resolve to one entity (name + alias), and a
            -- doubled row would double that entity's weight in the overlap.
            SELECT DISTINCT * FROM unnest($2::text[], $3::int[]) AS m(entity_type, entity_id)
        ),
        cand AS (
            SELECT s.id,
                   count(DISTINCT (se.entity_type, se.entity_id))
                     FILTER (WHERE se.entity_type <> 'team')::int AS person_overlap,
                   count(DISTINCT (se.entity_type, se.entity_id))
                     FILTER (WHERE se.entity_type =  'team')::int AS team_overlap,
                   EXTRACT(EPOCH FROM (c.as_of - s.last_seen_at))::bigint AS age_secs
              FROM public.storylines s
              CROSS JOIN clock c
              JOIN public.storyline_entities se
                ON se.storyline_id = s.id AND se.left_at IS NULL
              JOIN mine m
                ON m.entity_type = se.entity_type AND m.entity_id = se.entity_id
             WHERE s.sport = $1
               AND s.status = 'open'
               AND s.last_seen_at >  c.as_of - make_interval(days => $5::int)
               AND s.last_seen_at <= c.as_of
             GROUP BY s.id, s.last_seen_at, c.as_of
        )
        SELECT cand.id, cand.person_overlap, cand.team_overlap, cand.age_secs,
               (SELECT er.read ->> 'story_type'
                  FROM public.storyline_articles sa
                  JOIN public.editor_reads er ON er.article_id = sa.article_id
                 WHERE sa.storyline_id = cand.id
                   AND COALESCE(er.read ->> 'story_type', '') <> ''
                 GROUP BY er.read ->> 'story_type'
                 ORDER BY count(*) DESC, er.read ->> 'story_type' ASC
                 LIMIT 1) AS dominant_type
          FROM cand
         ORDER BY cand.id
        "#,
    )
    .bind(sport.to_uppercase())
    .bind(entity_types)
    .bind(entity_ids)
    .bind(as_of_epoch.map(|e| e as f64))
    .bind(CANDIDATE_WINDOW_DAYS as i32)
    .fetch_all(conn)
    .await
    .context("load storyline candidates")?;

    Ok(rows
        .into_iter()
        .map(|r| Candidate {
            storyline_id: r.get("id"),
            person_overlap: r.get("person_overlap"),
            team_overlap: r.get("team_overlap"),
            dominant_type: r.get("dominant_type"),
            age_secs: r.get("age_secs"),
        })
        .collect())
}

/// The ep1 role vocabulary, strongest first — the order `link_roles` resolves ties with.
const ROLE_STRENGTH: &[&str] = &["subject", "opponent", "passing_mention", "absent"];

/// link_roles recovers each resolved link's ep1 role for `storyline_entities.role`.
///
/// `Resolved.links` carries entity ids and the surface they matched, not the emitted name — the
/// §1a shape, which is persisted and must not grow a field for the Desk's convenience. The
/// mapping is recoverable instead: `group_hits` walks `names[]` in order and files each mention
/// into exactly one of links / unresolved / refused_ambiguous, and the last two carry the name.
/// So the names that appear in NEITHER are the linked ones, in link order.
///
/// If that reconstruction does not line up exactly (a duplicated name string, a future change
/// to the resolver), every role comes back empty rather than misattributed: a NULL role costs a
/// label, a wrong role would put a passing mention in a story's subject seat.
pub fn link_roles(
    names: &[NameMention],
    entity_roles: &[EditorEntityRole],
    resolved: &Resolved,
) -> Vec<String> {
    let empty = vec![String::new(); resolved.links.len()];
    let linked: Vec<&NameMention> = names
        .iter()
        .filter(|n| {
            !resolved
                .unresolved
                .iter()
                .any(|u| u.name.eq_ignore_ascii_case(&n.name))
                && !resolved
                    .refused_ambiguous
                    .iter()
                    .any(|r| r.name.eq_ignore_ascii_case(&n.name))
        })
        .collect();
    if linked.len() != resolved.links.len() {
        return empty;
    }
    linked
        .into_iter()
        .map(|mention| {
            entity_roles
                .iter()
                .filter(|r| r.entity.trim().eq_ignore_ascii_case(mention.name.trim()))
                .filter_map(|r| {
                    let role = r.role.trim().to_lowercase();
                    ROLE_STRENGTH
                        .iter()
                        .position(|s| *s == role)
                        .map(|rank| (rank, role))
                })
                .min_by_key(|(rank, _)| *rank)
                .map(|(_, role)| role)
                .unwrap_or_default()
        })
        .collect()
}

/// mark_dormant is 6.4's lifecycle sweep: an open storyline nobody has added to for 14 days is
/// over in every sense that matters to the Desk. Resolution stays a downstream act
/// ([`resolve_storyline`]); dormancy is just the candidate set staying honest.
pub async fn mark_dormant(pool: &PgPool) -> Result<u64> {
    let n = sqlx::query(
        r#"
        UPDATE public.storylines
           SET status = 'dormant'
         WHERE status = 'open'
           AND last_seen_at < NOW() - make_interval(days => $1::int)
        "#,
    )
    .bind(CANDIDATE_WINDOW_DAYS as i32)
    .execute(pool)
    .await
    .context("storyline dormancy sweep")?
    .rows_affected();
    Ok(n)
}

/// resolve_storyline is D5, in one stroke: name who the story resolved FOR, then close every
/// other participant's edge as `not_the_outcome` in the same transaction.
///
/// Wired now, invoked later: the transfer chain is the first caller (Phase 7). Nothing in
/// Phase 6 records a resolution, and inventing one from article prose would be exactly the
/// model-renders-a-verdict move T2 forbids.
pub async fn resolve_storyline(
    pool: &PgPool,
    storyline_id: i64,
    winner: (&str, i32),
    resolution: &serde_json::Value,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .with_context(|| format!("begin storyline resolve {storyline_id}"))?;

    sqlx::query(
        r#"
        UPDATE public.storylines
           SET status = 'resolved', resolved_at = NOW(), resolution = $2::jsonb
         WHERE id = $1
        "#,
    )
    .bind(storyline_id)
    .bind(resolution)
    .execute(&mut *tx)
    .await
    .with_context(|| format!("resolve storyline {storyline_id}"))?;

    sqlx::query(
        r#"
        UPDATE public.storyline_entities
           SET left_at = NOW(), exit_reason = 'not_the_outcome'
         WHERE storyline_id = $1
           AND left_at IS NULL
           AND NOT (entity_type = $2 AND entity_id = $3)
        "#,
    )
    .bind(storyline_id)
    .bind(winner.0)
    .bind(winner.1)
    .execute(&mut *tx)
    .await
    .with_context(|| format!("close storyline edges {storyline_id}"))?;

    tx.commit()
        .await
        .with_context(|| format!("commit storyline resolve {storyline_id}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::junctions::editor::derive::{RefusedName, ResolvedLink};
    use std::collections::BTreeSet;

    // ── the offline replay (6.5) ────────────────────────────────────────────────────────────
    //
    // The attachment rule's DB half is one GROUP BY; its JUDGMENT is `score`/`pick`, and that is
    // what these fixtures exercise. `Sim` mirrors the candidate query exactly — shared active
    // participants split person/team, the member-type mode, the age of the last touch — so a
    // canned day of reads assembles here the way it assembles on archbox.

    const HOUR: i64 = 3_600;

    #[derive(Clone, Debug)]
    struct SimStory {
        id: i64,
        entities: BTreeSet<(String, i32)>,
        types: Vec<String>,
        last_seen: i64,
        members: usize,
    }

    struct Canned {
        links: Vec<(&'static str, i32)>,
        story_type: &'static str,
        at: i64,
    }

    fn read(links: &[(&'static str, i32)], story_type: &'static str, at_hours: i64) -> Canned {
        Canned {
            links: links.to_vec(),
            story_type,
            at: at_hours * HOUR,
        }
    }

    /// The mode of a storyline's member types — the SQL's `ORDER BY count(*) DESC, story_type ASC`.
    fn dominant(types: &[String]) -> Option<String> {
        let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
        for t in types {
            *counts.entry(t.as_str()).or_default() += 1;
        }
        counts
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0)))
            .map(|(t, _)| t.to_string())
    }

    fn replay(reads: &[Canned]) -> Vec<SimStory> {
        let mut stories: Vec<SimStory> = Vec::new();
        for r in reads {
            let mine: BTreeSet<(String, i32)> = r
                .links
                .iter()
                .map(|(t, id)| (t.to_string(), *id))
                .collect();
            let candidates: Vec<Candidate> = stories
                .iter()
                .filter(|s| {
                    s.last_seen <= r.at && r.at - s.last_seen < CANDIDATE_WINDOW_DAYS * 24 * HOUR
                })
                .map(|s| {
                    let shared: Vec<&(String, i32)> = s.entities.intersection(&mine).collect();
                    Candidate {
                        storyline_id: s.id,
                        person_overlap: shared.iter().filter(|(t, _)| t != "team").count() as i32,
                        team_overlap: shared.iter().filter(|(t, _)| t == "team").count() as i32,
                        dominant_type: dominant(&s.types),
                        age_secs: r.at - s.last_seen,
                    }
                })
                .filter(|c| c.person_overlap + c.team_overlap > 0)
                .collect();

            match pick(&candidates, r.story_type) {
                Some((id, _)) => {
                    let s = stories.iter_mut().find(|s| s.id == id).expect("picked id");
                    s.entities.extend(mine);
                    s.types.push(r.story_type.to_string());
                    s.last_seen = s.last_seen.max(r.at);
                    s.members += 1;
                }
                None => stories.push(SimStory {
                    id: stories.len() as i64 + 1,
                    entities: mine,
                    types: vec![r.story_type.to_string()],
                    last_seen: r.at,
                    members: 1,
                }),
            }
        }
        stories
    }

    fn story_of(stories: &[SimStory], entity: (&str, i32)) -> Vec<i64> {
        stories
            .iter()
            .filter(|s| s.entities.contains(&(entity.0.to_string(), entity.1)))
            .map(|s| s.id)
            .collect()
    }

    /// The Real-Madrid-day shape (6.5), by hand: five clusters, one club, one afternoon.
    fn real_madrid_day() -> Vec<Canned> {
        const RM: (&str, i32) = ("team", 3468);
        const VINICIUS: (&str, i32) = ("player", 1);
        const RODRI: (&str, i32) = ("player", 2);
        const DIOMANDE: (&str, i32) = ("player", 3);
        const LEE: (&str, i32) = ("player", 4);
        const PSG: (&str, i32) = ("team", 5);
        const ALVAREZ: (&str, i32) = ("player", 6);
        const ATLETICO: (&str, i32) = ("team", 7);
        const LEIPZIG: (&str, i32) = ("team", 8);
        vec![
            read(&[RM, VINICIUS], "transfer", 0),
            read(&[RM, VINICIUS], "transfer", 1),
            // The same people the same afternoon: one story, even as the type wobbles between
            // an outlet calling it a transfer and another calling it a contract.
            read(&[RM, VINICIUS], "contract", 2),
            read(&[RM, RODRI], "performance", 3),
            read(&[RM, RODRI], "performance", 4),
            // The hand count's hard case: a PSG player whose article ALSO links Real Madrid as
            // an interested party. One shared club is not a shared story.
            read(&[LEE, PSG, RM], "transfer", 5),
            read(&[LEE, PSG], "transfer", 6),
            read(&[DIOMANDE, LEIPZIG], "transfer", 7),
            read(&[ALVAREZ, ATLETICO, RM], "transfer", 8),
            read(&[ALVAREZ, ATLETICO], "transfer", 9),
        ]
    }

    #[test]
    fn lee_kang_in_does_not_land_in_real_madrids_storyline() {
        let stories = replay(&real_madrid_day());
        let vinicius = story_of(&stories, ("player", 1));
        let lee = story_of(&stories, ("player", 4));
        assert_eq!(vinicius.len(), 1, "Vinicius belongs to one story");
        assert_eq!(lee.len(), 1, "Lee Kang-in belongs to one story");
        assert_ne!(
            vinicius[0], lee[0],
            "a shared Real Madrid link is not a shared story (the 6.5 hand count)"
        );
    }

    #[test]
    fn the_hand_counted_day_assembles_into_its_five_clusters() {
        let stories = replay(&real_madrid_day());
        assert_eq!(
            stories.len(),
            5,
            "Vinicius, Rodri, Lee Kang-in, Diomande, Álvarez — five stories, not one Real \
             Madrid blob and not ten singletons: {stories:#?}"
        );
        // Every cluster kept its own follow-up coverage, and the ten reads are all placed.
        let members: Vec<usize> = stories.iter().map(|s| s.members).collect();
        assert_eq!(
            members,
            vec![3, 2, 2, 1, 2],
            "Vinicius 3 (the type wobble stayed in the story), Rodri 2, Lee Kang-in 2, \
             Diomande 1, Álvarez 2: {stories:#?}"
        );
        assert_eq!(members.iter().sum::<usize>(), 10, "no read went unplaced");
        // The club appears in four of them — a channel, not a story.
        assert!(story_of(&stories, ("team", 3468)).len() >= 4);
    }

    #[test]
    fn one_hot_story_gathers_its_whole_days_coverage() {
        // Twenty outlets on the same transfer: the 6.7 band expects the top cluster around
        // 15–25:1, and it can only get there if same-people coverage keeps attaching.
        let reads: Vec<Canned> = (0..20)
            .map(|i| read(&[("team", 3468), ("player", 1)], "transfer", i))
            .collect();
        let stories = replay(&reads);
        assert_eq!(stories.len(), 1);
        assert_eq!(stories[0].members, 20);
    }

    #[test]
    fn a_contradiction_attaches_rather_than_forking() {
        // T3: "agreement reached" and "yet to reach agreement" are the same story with a
        // different claim. Nothing in the rule looks at whether members agree — by construction,
        // two reads with the same people land in the same storyline, and the packet keeps both
        // claims (see `packet::tests`).
        let stories = replay(&[
            read(&[("team", 3468), ("player", 1)], "transfer", 0),
            read(&[("team", 3468), ("player", 1)], "transfer", 1),
        ]);
        assert_eq!(stories.len(), 1);
        assert_eq!(stories[0].members, 2);
    }

    #[test]
    fn a_cold_storyline_falls_out_of_the_candidate_window() {
        let stories = replay(&[
            read(&[("team", 3468), ("player", 1)], "transfer", 0),
            // 15 days later: the 14-day window has closed, so this opens a new story even
            // though it is about exactly the same people.
            read(&[("team", 3468), ("player", 1)], "transfer", 15 * 24),
        ]);
        assert_eq!(stories.len(), 2);
    }

    // ── the scoring rule itself ─────────────────────────────────────────────────────────────

    fn candidate(person: i32, team: i32, dominant: Option<&str>, age_secs: i64) -> Candidate {
        Candidate {
            storyline_id: 1,
            person_overlap: person,
            team_overlap: team,
            dominant_type: dominant.map(str::to_string),
            age_secs,
        }
    }

    #[test]
    fn a_shared_club_alone_never_clears_the_bar() {
        let c = candidate(0, 1, Some("transfer"), HOUR);
        assert_eq!(score(&c, "transfer"), 3, "1 club + type + recency");
        assert!(pick(&[c], "transfer").is_none(), "3 is not above 3");
    }

    #[test]
    fn a_shared_person_clears_the_bar_on_its_own() {
        let c = candidate(1, 0, None, HOUR);
        assert_eq!(score(&c, "transfer"), 3, "2 for the person + recency");
        // …but only with something else behind it: a person alone, stale and off-type, waits.
        assert!(pick(&[c], "transfer").is_none());
        let fresh_and_on_type = candidate(1, 0, Some("transfer"), HOUR);
        assert_eq!(score(&fresh_and_on_type, "transfer"), 4);
        assert_eq!(pick(&[fresh_and_on_type], "transfer"), Some((1, 4)));
    }

    #[test]
    fn an_empty_story_type_earns_no_match_point() {
        // A read whose story_type is empty must not "match" a storyline whose dominant type is
        // also unknown — that would be two absences agreeing.
        let c = candidate(1, 0, None, HOUR);
        assert_eq!(score(&c, ""), 3);
    }

    #[test]
    fn ties_break_to_the_freshest_then_the_newest_storyline() {
        let older = Candidate {
            storyline_id: 7,
            person_overlap: 1,
            team_overlap: 1,
            dominant_type: Some("transfer".into()),
            age_secs: 40 * HOUR,
        };
        let fresher = Candidate {
            storyline_id: 3,
            age_secs: 2 * HOUR,
            ..older.clone()
        };
        assert_eq!(pick(&[older.clone(), fresher.clone()], "transfer"), Some((3, 5)));
        // Same score AND same age: the LOWEST id wins — the established storyline, and a
        // deterministic answer either way, which is what a replay of the corpus needs.
        let same_age = Candidate {
            storyline_id: 9,
            ..fresher.clone()
        };
        assert_eq!(pick(&[same_age, fresher], "transfer"), Some((3, 5)));
    }

    // ── link → role reconstruction ──────────────────────────────────────────────────────────

    fn mention(name: &str, kind: &str) -> NameMention {
        NameMention {
            name: name.to_string(),
            kind_hint: kind.to_string(),
            descriptor: String::new(),
        }
    }

    fn link(entity_type: &str, entity_id: i32) -> ResolvedLink {
        ResolvedLink {
            entity_type: entity_type.to_string(),
            entity_id,
            sport: "FOOTBALL".to_string(),
            via_surface: String::new(),
        }
    }

    #[test]
    fn roles_line_up_with_the_links_the_resolver_kept() {
        let names = vec![
            mention("Real Madrid", "club"),
            mention("Some Nobody", "person"),
            mention("Vinicius", "person"),
        ];
        let entity_roles = vec![
            EditorEntityRole {
                entity: "Real Madrid".into(),
                role: "subject".into(),
            },
            EditorEntityRole {
                entity: "Vinicius".into(),
                role: "passing_mention".into(),
            },
        ];
        let resolved = Resolved {
            links: vec![link("team", 3468), link("player", 1)],
            unresolved: vec![mention("Some Nobody", "person")],
            refused_ambiguous: vec![],
        };
        assert_eq!(
            link_roles(&names, &entity_roles, &resolved),
            vec!["subject".to_string(), "passing_mention".to_string()]
        );
    }

    #[test]
    fn an_unreconstructable_mapping_yields_no_roles_rather_than_wrong_ones() {
        let names = vec![mention("Vinicius", "person")];
        let resolved = Resolved {
            // Two links from one name: the reconstruction does not line up, so every role comes
            // back empty instead of putting a passing mention in the subject's seat.
            links: vec![link("player", 1), link("player", 2)],
            unresolved: vec![],
            refused_ambiguous: vec![],
        };
        assert_eq!(link_roles(&names, &[], &resolved), vec!["", ""]);
    }

    #[test]
    fn a_refused_tie_is_not_counted_as_a_link() {
        let names = vec![mention("Vinicius", "person"), mention("Rodrigo", "person")];
        let resolved = Resolved {
            links: vec![link("player", 1)],
            unresolved: vec![],
            refused_ambiguous: vec![RefusedName {
                name: "Rodrigo".into(),
                kind_hint: "person".into(),
                descriptor: String::new(),
                candidates: vec![],
            }],
        };
        let roles = link_roles(
            &names,
            &[EditorEntityRole {
                entity: "Vinicius".into(),
                role: "subject".into(),
            }],
            &resolved,
        );
        assert_eq!(roles, vec!["subject".to_string()]);
    }
}
