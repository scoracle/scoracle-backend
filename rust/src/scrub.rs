//! Scrub stage handler — article bookkeeping as a `pipeline_work` stage. NO MODEL CALL.
//!
//! It claims an ARTICLE-keyed work item, admits the article's links, defers co-mentions to the
//! Editor, enqueues the read, and runs the near-verbatim novelty gate. A vetted write
//! fires the SQL trigger that enqueues downstream per-entity work.
//!
//! **This stage no longer judges relevance** (teardown plan §2.1). It used to run an
//! embedding + GPU gate here, and that gate was measured to reject ~1 in 3 links at a rate
//! UNCORRELATED with relevance — it was deleting a third of the genuinely good corpus
//! ("Arsenal sign Greek winger Christos Tzolis" was rejected). Two facts made it deletable
//! rather than repairable:
//!
//!   * Go now refuses to create a link at all unless the entity is named in the article text
//!     (`filterRSSArticles` → `MatchesEntity`, restored in `328c9c9`). Measured live: 100% of
//!     primary links carry the entity, 96.6% of them in the title itself. The question this gate
//!     existed to answer is already answered deterministically, upstream, for free.
//!   * The Editor is the sole relevance judge, and it is the only stage that reads the
//!     publisher body. It can reject a whole article after fetching (mig 190), clearing the
//!     sport's vetted links — so a false positive that survives Go still meets a judge holding
//!     real evidence, rather than a headline-only guess.
//!
//! What remains here is fact, not judgment: which links exist, which are deferred, and whether
//! this article is a near-verbatim repost of one already held.

use crate::harness::{EntityType, Harness};
use crate::stage::StageHandler;
use crate::work::{self, Item, Stage};
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::Row;

/// Go marks broad team RSS primaries at 0.95. Lower-confidence links are co-mention
/// candidates; when a primary exists, Editor owns their full-text verdict.
const PRIMARY_TEAM_CONFIDENCE_FLOOR: f64 = 0.95;

/// How many articles per entity, per sweep, are worth a model call — the read budget.
///
/// Ingest admits ~6,700 articles/day; the Editor sustains ~800/day sharing one GPU with
/// eight other stages. Something has to choose, and the choice must be made at ADMISSION rather
/// than by throttling the drain: throttle the drain and the queue simply grows while the GPU still
/// runs flat out forever. Capping what is enqueued is what lets the queue actually reach empty —
/// which is where the GPU's idle time comes from.
///
/// Budget per entity rather than a global daily quota, so a busy Arsenal news day cannot consume
/// Brentford's coverage, and so the ceiling is predictable (204 entities x 4 sweeps x K) no matter
/// how volume swings. Google's own ranking picks WHICH articles inside the budget — see
/// `feed_rank` (mig 194). Everything outside it still reaches The Journalist on its headline
/// (`narratives::article_context`); nothing is dropped, so this bounds cost, not coverage.
///
/// Ranking is per entity over the novelty lookback window (72h), not per sweep, so coverage grows
/// sub-linearly in K — an article inside several entities' budgets is still one read. Measured
/// against the live 72h corpus (12,584 articles):
///
/// | K | reads/day | coverage |
/// |---|-----------|----------|
/// | 2 |       463 |    10.8% |
/// | 4 |       701 |    16.4% |
/// | 6 |       892 |    20.9% |
/// | 8 |     1,058 |    24.7% |
///
/// The Editor sustains ~800/day on `mistral:7b`. K=4 sits deliberately under that so the queue
/// reaches empty and the GPU idles rather than running flat out — the point of having a cap at all.
/// K=8 is the ~25% target and becomes affordable once the Editor moves to a smaller model
/// (`gemma3:4b` measured 3x on decode); it is one env var, not a code change.
const DEFAULT_ARTICLE_READ_TOP_K: i64 = 4;

fn article_read_top_k() -> i64 {
    std::env::var("COGNITION_ARTICLE_READ_TOP_K")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|k| *k >= 0)
        .unwrap_or(DEFAULT_ARTICLE_READ_TOP_K)
}

/// ScrubHandler drains the article-keyed `scrub` stage.
pub struct ScrubHandler;

impl ScrubHandler {
    pub fn new() -> Self {
        ScrubHandler
    }
}

impl Default for ScrubHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// One entity currently linked to an article. The identity card (name, nationality, club,
/// position) is gone with the gate that rendered it into an embedding — nothing here needs to
/// know WHO the entity is any more, only that the link exists and what has already been ruled.
struct ScrubCandidate {
    entity_type: EntityType,
    entity_id: i32,
    confidence: f64,
    /// The link's settled verdict, or `None` when nobody has ruled yet. `Some(_)` means this link
    /// is DONE — Editor promoted/rejected a co-mention, or an earlier pass admitted it.
    /// Settled links are excluded from the write set so a re-enqueue never re-stamps a verdict
    /// already on disk; they still count toward the novelty gate's scope.
    vetted: Option<bool>,
}

#[async_trait]
impl StageHandler for ScrubHandler {
    fn stage(&self) -> Stage {
        Stage::Scrub
    }

    /// Scrub makes no model call any more — one item is a handful of indexed statements and a set
    /// intersection. Draining it in real batches keeps a broad RSS sweep from sitting in the queue
    /// behind the GPU stages; at one-per-rotation the post-teardown backlog would never clear.
    fn rotation_batch(&self) -> i64 {
        256
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        // The scrub item is article-keyed: entity_type='article', entity_id=news_articles.id.
        let article_id = item.entity_id;
        let sport = item.sport.to_uppercase();

        let row = sqlx::query(
            "SELECT title, COALESCE(description, '') AS description FROM news_articles WHERE id = $1",
        )
        .bind(article_id)
        .fetch_optional(&hx.pool)
        .await
        .context("load article")?;
        let Some(row) = row else {
            return Ok(()); // article vanished → nothing to scrub
        };
        let title: String = row.get("title");
        let description: String = row.get("description");

        let cands = load_candidates(hx, article_id, &sport).await?;
        if cands.is_empty() {
            return Ok(());
        }

        // When the Go funnel has a primary team candidate (0.95), lower-confidence co-mentions are
        // preserved for Editor's full-text verdict. If an old repair item has no
        // primary-like candidate, every candidate is scrub-owned as before.
        let context = crate::novelty::article_text(&title, &description);
        let has_primary_like = cands
            .iter()
            .any(|c| c.confidence >= PRIMARY_TEAM_CONFIDENCE_FLOOR);
        // A settled link (`vetted IS NOT NULL`) is never re-stamped. `pipeline_work` rows are
        // DELETED on completion, so the queue's idempotency key vanishes once an article is
        // scrubbed — any later team that adds a link re-enqueues the whole article. This filter
        // keeps that re-enqueue from overwriting a verdict already on disk, which now matters MORE
        // than it did: the Editor may have rejected one of these links after reading the
        // body (mig 190), and a blanket re-admit here would silently undo the only judgment in the
        // pipeline that saw real evidence. Settled links still feed the novelty gate's scope below.
        let scrub_idxs: Vec<usize> = cands
            .iter()
            .enumerate()
            .filter(|(_, c)| !has_primary_like || c.confidence >= PRIMARY_TEAM_CONFIDENCE_FLOOR)
            .filter(|(_, c)| c.vetted.is_none())
            .map(|(idx, _)| idx)
            .collect();
        let deferred_idxs: Vec<usize> = if has_primary_like {
            cands
                .iter()
                .enumerate()
                .filter(|(_, c)| c.confidence < PRIMARY_TEAM_CONFIDENCE_FLOOR)
                .map(|(idx, _)| idx)
                .collect()
        } else {
            Vec::new()
        };

        // Every scrub-owned link is admitted. This is not a lowered bar — it is the bar moving
        // upstream to where the evidence actually is: Go already proved the entity is named in
        // the article text before this link was allowed to exist, and the Editor will
        // read the body and can still reject the whole article (mig 190). Deferred co-mentions
        // stay vetted=NULL and get only scrubbed_at stamped, so maintenance will not requeue them.
        let entity_types: Vec<String> = scrub_idxs
            .iter()
            .map(|&idx| cands[idx].entity_type.as_str().to_string())
            .collect();
        let entity_ids: Vec<i32> = scrub_idxs.iter().map(|&idx| cands[idx].entity_id).collect();
        let relevants: Vec<bool> = vec![true; scrub_idxs.len()];
        let deferred_entity_types: Vec<String> = deferred_idxs
            .iter()
            .map(|&idx| cands[idx].entity_type.as_str().to_string())
            .collect();
        let deferred_entity_ids: Vec<i32> = deferred_idxs
            .iter()
            .map(|&idx| cands[idx].entity_id)
            .collect();

        // Admission counts THIS pass's keeps plus any link already settled as kept on a previous
        // pass. Without the second clause a re-enqueue that has nothing new to adjudicate would
        // read as "not admitted" and hand the novelty gate an empty scope, changing dedupe.
        let article_admitted = relevants.iter().any(|&kept| kept)
            || cands.iter().any(|c| c.vetted == Some(true));

        // The vetted membership after this scrub plus deferred co-mentions — the novelty gate's
        // conservative scope. Co-mentions are not consumer-visible until Editor promotes
        // them, but including them here avoids suppressing an article that may be unique for a
        // co-mentioned player/team once the full text is read.
        let vetted_entities: Vec<(EntityType, i32)> = if article_admitted {
            cands
                .iter()
                .enumerate()
                .filter_map(|(idx, c)| {
                    if let Some(pos) = scrub_idxs.iter().position(|&scrub_idx| scrub_idx == idx) {
                        relevants[pos].then_some((c.entity_type, c.entity_id))
                    } else if deferred_idxs.contains(&idx) {
                        Some((c.entity_type, c.entity_id))
                    } else {
                        // Settled on an earlier pass (scrub_idxs skipped it): honour the verdict
                        // on disk so the gate's scope is identical to a first-pass scrub.
                        (c.vetted == Some(true)).then_some((c.entity_type, c.entity_id))
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        apply_scrub_outcomes(
            hx,
            article_id,
            &sport,
            &entity_types,
            &entity_ids,
            &relevants,
            &deferred_entity_types,
            &deferred_entity_ids,
        )
        .await?;

        // The read budget. An article earns a model call by being one of the top `K` Google
        // returned for at least ONE of the entities it is linked to — rank 1 for Arsenal and rank
        // 40 for Chelsea is still Arsenal's top story and gets read. Everything else travels on its
        // headline. `within_read_budget` is a fact about Google's ordering, not an opinion about
        // the article, which is why this cap can sit in front of the sole relevance judge without
        // becoming a second one.
        let budgeted = article_admitted
            && within_read_budget(hx, article_id, &sport, article_read_top_k()).await?;

        if !deferred_idxs.is_empty() && budgeted {
            work::enqueue(
                &hx.pool,
                &Item {
                    stage: Stage::ArticleRead,
                    entity_type: "article".to_string(),
                    entity_id: article_id,
                    sport: sport.clone(),
                    input_version: Some(article_read_co_mentions_input_version(
                        &cands,
                        &deferred_idxs,
                    )),
                    attempts: 0,
                },
            )
            .await?;
        }

        // Near-verbatim novelty gate: suppress only a syndicated copy of coverage already held.
        // Writes `duplicate_of` only — membership is untouched, so no derive re-fires.
        crate::novelty::gate(
            hx,
            crate::novelty::ArticleNovelty {
                article_id,
                sport: &sport,
                context: &context,
                vetted_entities: &vetted_entities,
            },
        )
        .await
        .context("novelty gate")?;
        Ok(())
    }
}

fn article_read_co_mentions_input_version(cands: &[ScrubCandidate], idxs: &[usize]) -> String {
    let mut parts: Vec<String> = idxs
        .iter()
        .map(|&idx| {
            let c = &cands[idx];
            format!(
                "{}:{}:{}",
                c.entity_type.as_str(),
                c.entity_id,
                c.confidence
            )
        })
        .collect();
    parts.sort();
    format!(
        "arcm:{}:{}",
        parts.len(),
        crate::util::hash_components(&parts.join(","))
    )
}

/// load_candidates returns every entity linked to the article. The four identity-card joins
/// (players, teams, `player_current_identity`, current club) are gone with the embedding gate that
/// consumed them — this is now a single-table read. `match_confidence` is cast `::float8` (sqlx has
/// no numeric→f64 without the decimal feature — the L5 landmine).
async fn load_candidates(
    hx: &Harness,
    article_id: i64,
    sport: &str,
) -> Result<Vec<ScrubCandidate>> {
    let rows = sqlx::query(
        r#"
        SELECT nae.entity_type, nae.entity_id,
               nae.match_confidence::float8 AS confidence,
               nae.vetted                   AS vetted
        FROM news_article_entities nae
        WHERE nae.article_id = $1 AND nae.sport = $2
        ORDER BY nae.match_confidence DESC, nae.entity_type, nae.entity_id
        "#,
    )
    .bind(article_id)
    .bind(sport)
    .fetch_all(&hx.pool)
    .await
    .context("load candidates")?;

    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        let et: String = r.get("entity_type");
        let Some(entity_type) = EntityType::from_db_str(&et) else {
            continue; // unknown entity_type → skip (defensive)
        };
        out.push(ScrubCandidate {
            entity_type,
            entity_id: r.get("entity_id"),
            confidence: r.get("confidence"),
            vetted: r.get("vetted"),
        });
    }
    Ok(out)
}

/// within_read_budget reports whether this article is among the `k` best-ranked canonical articles
/// Google returned for any entity it is vetted against, inside the current lookback.
///
/// `feed_rank` NULL sorts LAST: a pre-migration backlog article carries no ranking evidence and
/// must never displace a fresh Google top hit. `k <= 0` disables reading entirely (every article
/// passes through on its headline) — a deliberate escape hatch for giving the GPU a full stop
/// without stopping ingest or losing coverage.
async fn within_read_budget(hx: &Harness, article_id: i64, sport: &str, k: i64) -> Result<bool> {
    if k <= 0 {
        return Ok(false);
    }
    let within: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM news_article_entities me
            WHERE me.article_id = $1 AND me.sport = $2 AND me.vetted IS TRUE
              AND $1 IN (
                  SELECT ranked.article_id
                  FROM (
                      SELECT peer.article_id,
                             ROW_NUMBER() OVER (
                                 ORDER BY a.feed_rank ASC NULLS LAST, a.fetched_at DESC, a.id
                             ) AS pos
                      FROM news_article_entities peer
                      JOIN news_articles a ON a.id = peer.article_id
                      WHERE peer.entity_type = me.entity_type
                        AND peer.entity_id  = me.entity_id
                        AND peer.sport      = me.sport
                        AND peer.vetted IS TRUE
                        AND a.duplicate_of IS NULL
                        AND a.fetched_at >= NOW() - make_interval(secs => $3)
                  ) ranked
                  WHERE ranked.pos <= $4
              )
        )
        "#,
    )
    .bind(article_id)
    .bind(sport)
    .bind(hx.scrub.novelty_lookback.as_secs_f64())
    .bind(k)
    .fetch_one(&hx.pool)
    .await
    .context("evaluate article read budget")?;
    Ok(within)
}

/// apply_scrub_outcomes admits the scrub-owned links and marks deferred co-mentions as handed to
/// Editor. Admissions write `vetted`; deferred co-mentions keep `vetted=NULL` but get
/// `scrubbed_at=NOW()` so the maintenance sweep does not treat them as unprocessed work.
async fn apply_scrub_outcomes(
    hx: &Harness,
    article_id: i64,
    sport: &str,
    entity_types: &[String],
    entity_ids: &[i32],
    relevants: &[bool],
    deferred_entity_types: &[String],
    deferred_entity_ids: &[i32],
) -> Result<()> {
    if !entity_types.is_empty() {
        sqlx::query(
            r#"
            UPDATE news_article_entities n
               SET vetted = v.relevant, scrubbed_at = NOW()
              FROM unnest($2::text[], $3::int[], $4::bool[]) AS v(entity_type, entity_id, relevant)
             WHERE n.article_id = $1 AND n.sport = $5
               AND n.entity_type = v.entity_type AND n.entity_id = v.entity_id
            "#,
        )
        .bind(article_id)
        .bind(entity_types)
        .bind(entity_ids)
        .bind(relevants)
        .bind(sport)
        .execute(&hx.pool)
        .await
        .context("apply scrub verdicts")?;
    }
    if !deferred_entity_types.is_empty() {
        sqlx::query(
            r#"
            UPDATE news_article_entities n
               SET scrubbed_at = COALESCE(scrubbed_at, NOW())
              FROM unnest($2::text[], $3::int[]) AS v(entity_type, entity_id)
             WHERE n.article_id = $1 AND n.sport = $4
               AND n.entity_type = v.entity_type AND n.entity_id = v.entity_id
               AND n.vetted IS NULL
            "#,
        )
        .bind(article_id)
        .bind(deferred_entity_types)
        .bind(deferred_entity_ids)
        .bind(sport)
        .execute(&hx.pool)
        .await
        .context("defer co-mentions to article_read")?;
    }
    Ok(())
}
