//! Article bucket classification + topic heat-rank (Wave 5 / F2).
//!
//! Scrub still has one primary job: vet linked entities. This module adds the rail split that
//! downstream stages need: every vetted article gets a transfer/non-transfer bucket, and a periodic
//! batch gives each article the size of its same-day topic cluster. The bucket path is hybrid:
//! model-emitted tags are authoritative on already-paid scrub calls; this candle classifier covers
//! auto-kept articles that skip the model.

use crate::config::ScrubConfig;
use crate::embed::{cosine_similarity, Embedder};
use crate::harness::{cluster, Harness, Vector};
use anyhow::{Context, Result};
use sqlx::Row;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use tracing::{info, warn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArticleBucket {
    Transfer,
    NonTransfer,
}

impl ArticleBucket {
    pub fn as_db(self) -> &'static str {
        match self {
            ArticleBucket::Transfer => "transfer",
            ArticleBucket::NonTransfer => "non_transfer",
        }
    }

    pub fn from_model_tag(tag: &str) -> Option<Self> {
        match tag.trim().to_lowercase().as_str() {
            "transfer" | "trade" | "transfers" | "trades" => Some(ArticleBucket::Transfer),
            "other" | "non_transfer" | "non-transfer" | "nontransfer" => {
                Some(ArticleBucket::NonTransfer)
            }
            _ => None,
        }
    }
}

pub struct BucketClassifier {
    transfer_centroid: Vector,
    non_transfer_centroid: Vector,
    cfg: ScrubConfig,
}

impl BucketClassifier {
    pub fn from_embedder(embedder: &Embedder, cfg: ScrubConfig) -> Result<Self> {
        let transfer = canonical_transfer_sentences();
        let non_transfer = canonical_non_transfer_sentences();
        let transfer_vecs = embedder
            .embed_batch(&transfer)
            .context("embed transfer bucket canon")?;
        let non_transfer_vecs = embedder
            .embed_batch(&non_transfer)
            .context("embed non-transfer bucket canon")?;
        Ok(Self {
            transfer_centroid: mean_vec(&transfer_vecs),
            non_transfer_centroid: mean_vec(&non_transfer_vecs),
            cfg,
        })
    }

    pub fn classify_vector(&self, text: &str, v: &[f32]) -> ArticleBucket {
        let contrastive = cosine_similarity(v, &self.transfer_centroid)
            - cosine_similarity(v, &self.non_transfer_centroid);
        let keyword_bonus = if keyword_hit(text, &self.cfg.bucket_keywords) {
            self.cfg.bucket_keyword_weight
        } else {
            0.0
        };
        if contrastive + keyword_bonus >= self.cfg.bucket_threshold {
            ArticleBucket::Transfer
        } else {
            ArticleBucket::NonTransfer
        }
    }
}

pub async fn classify_article(
    hx: &Harness,
    title: &str,
    description: &str,
) -> Result<ArticleBucket> {
    let text = article_text(title, description);
    let Some(classifier) = hx.bucket_classifier.as_ref() else {
        return Ok(keyword_bucket(&text, &hx.scrub.bucket_keywords));
    };
    let vectors = hx
        .embed(std::slice::from_ref(&text))
        .await
        .context("embed article for bucket classification")?;
    Ok(classifier.classify_vector(&text, &vectors[0]))
}

/// classify_with_vector buckets an article whose context is already embedded — the scrub gate hands
/// its resolve vector here so the candle fallback never re-embeds the article it just gated. The
/// resolve text form (`"title — description"`) is classifier-equivalent to the bucket text form the
/// thresholds were tuned on: measured 2026-07-13 via `examples/bucket_remeasure.rs` against the
/// GPU labels (vector cosine 0.993, AUC 0.882 vs 0.879, accuracy delta −0.002 at the live
/// threshold). `text` still feeds the keyword feature.
pub fn classify_with_vector(hx: &Harness, text: &str, v: &[f32]) -> ArticleBucket {
    match hx.bucket_classifier.as_ref() {
        Some(classifier) => classifier.classify_vector(text, v),
        None => keyword_bucket(text, &hx.scrub.bucket_keywords),
    }
}

/// The embedder-less fallback: keyword feature only.
fn keyword_bucket(text: &str, keywords: &[String]) -> ArticleBucket {
    if keyword_hit(text, keywords) {
        ArticleBucket::Transfer
    } else {
        ArticleBucket::NonTransfer
    }
}

/// Cross-pass embedding cache for `refresh_topic_heat`. The lookback window re-reads the same
/// articles pass after pass and embeddings are deterministic for unchanged text, so only
/// new/edited articles pay the CPU embedder — measured 2026-07-12, a full re-embed pass took
/// ~24 min at ~900 articles and starved the work queue. Bounded by `retain_ids` to the
/// current window (~900 × 384 floats ≈ 1.5 MB).
///
/// Backed by `topic_heat_embeddings` (mig 151): the first refresh of a process hydrates from
/// the table and every embed writes through to it, so a restart re-embeds only articles that
/// arrived while the process was down — the 2026-07-16 deploy paid a 27-minute cold pass
/// (933 articles) before its first work claim, which this removes. The table is a pure cache:
/// hydration failure (e.g. migration not yet applied) degrades to the old cold-pass behavior
/// with one WARN, and `persist` stops further table writes for the process lifetime.
pub struct TopicHeatCache {
    entries: HashMap<i64, (u64, Vector)>,
    /// One hydration attempt per process — set before the attempt so a failure never retries.
    hydrated: bool,
    /// Cleared when hydration shows the table is unusable; write-through and prune skip.
    persist: bool,
}

impl Default for TopicHeatCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            hydrated: false,
            persist: true,
        }
    }
}

impl TopicHeatCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// absorb merges one persisted row into the in-memory cache — the pure half of hydration
    /// (the SQL fetch stays in `refresh_topic_heat`). Returns false and skips rows whose bytes
    /// can't decode to `dim` floats: a corrupt row must degrade to a re-embed, never poison
    /// clustering.
    fn absorb(&mut self, article_id: i64, fingerprint: i64, bytes: &[u8], dim: usize) -> bool {
        match decode_vector(bytes, dim) {
            Some(v) => {
                self.entries.insert(article_id, (fingerprint as u64, v));
                true
            }
            None => false,
        }
    }

    /// A cached vector is valid only while the article text it embedded is unchanged.
    fn get(&self, id: i64, fingerprint: u64) -> Option<&Vector> {
        self.entries
            .get(&id)
            .filter(|(fp, _)| *fp == fingerprint)
            .map(|(_, v)| v)
    }

    fn put(&mut self, id: i64, fingerprint: u64, v: Vector) {
        self.entries.insert(id, (fingerprint, v));
    }

    /// Drop entries for articles that aged out of the lookback window.
    fn retain_ids(&mut self, keep: &HashSet<i64>) {
        self.entries.retain(|id, _| keep.contains(id));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn text_fingerprint(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// encode_vector packs an embedding as little-endian f32 bytes for mig 151's bytea column.
fn encode_vector(v: &Vector) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// decode_vector is encode_vector's inverse; None unless the bytes are exactly `dim` f32s.
fn decode_vector(bytes: &[u8], dim: usize) -> Option<Vector> {
    if dim == 0 || bytes.len() != dim * 4 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

/// One `refresh_topic_heat` pass, for the worker's log line: how much work the cache saved.
#[derive(Clone, Copy, Debug, Default)]
pub struct TopicHeatRefresh {
    /// Rows whose topic_heat was written.
    pub updated: u64,
    /// Texts sent to the CPU embedder this pass (the expensive part).
    pub embedded: usize,
    /// Texts served from the cross-pass cache.
    pub cached: usize,
}

pub async fn refresh_topic_heat(
    hx: &Harness,
    cache: &mut TopicHeatCache,
    beat: &(dyn Fn(&str) + Sync),
) -> Result<TopicHeatRefresh> {
    let Some(embedder) = hx.embedder.as_ref() else {
        return Ok(TopicHeatRefresh::default());
    };
    let model = embedder.identity.clone();
    let dim = embedder.dim;

    // One-time hydration from mig 151's persistent cache: a restart re-embeds only what
    // arrived while the process was down, not the whole hot window. `hydrated` is set
    // before the attempt so a failure (table missing, permissions) warns once and the
    // process runs on the in-memory cache alone — the pre-151 behavior.
    if !cache.hydrated {
        cache.hydrated = true;
        match sqlx::query(
            "SELECT article_id, fingerprint, embedding FROM public.topic_heat_embeddings WHERE model = $1",
        )
        .bind(&model)
        .fetch_all(&hx.pool)
        .await
        {
            Ok(rows) => {
                let total = rows.len();
                let mut absorbed = 0usize;
                for row in rows {
                    let id: i64 = row.get("article_id");
                    let fp: i64 = row.get("fingerprint");
                    let bytes: Vec<u8> = row.get("embedding");
                    if cache.absorb(id, fp, &bytes, dim) {
                        absorbed += 1;
                    }
                }
                info!(
                    absorbed,
                    skipped = total - absorbed,
                    "topic heat cache hydrated from topic_heat_embeddings"
                );
            }
            Err(e) => {
                cache.persist = false;
                warn!(
                    error = %format!("{e:#}"),
                    "topic heat cache hydration failed; in-memory cache only for this process"
                );
            }
        }
    }

    let rows = sqlx::query(
        r#"
        SELECT nae.sport, a.id, a.title, COALESCE(a.description, '') AS description,
               EXTRACT(EPOCH FROM date_trunc('day', COALESCE(a.published_at, a.fetched_at)))::bigint AS day_epoch
        FROM public.news_articles a
        JOIN public.news_article_entities nae ON nae.article_id = a.id
        WHERE nae.vetted IS TRUE
          AND COALESCE(a.published_at, a.fetched_at) > NOW() - make_interval(secs => $1)
        GROUP BY nae.sport, a.id, a.title, a.description, day_epoch
        ORDER BY nae.sport, day_epoch DESC, COALESCE(a.published_at, a.fetched_at) DESC
        "#,
    )
    .bind(hx.scrub.topic_heat_lookback.as_secs_f64())
    .fetch_all(&hx.pool)
    .await
    .context("load articles for topic heat")?;

    #[derive(Clone)]
    struct TopicArticle {
        id: i64,
        title: String,
        description: String,
    }

    let mut groups: BTreeMap<(String, i64), Vec<TopicArticle>> = BTreeMap::new();
    for row in rows {
        let sport: String = row.get("sport");
        let day_epoch: i64 = row.get("day_epoch");
        groups
            .entry((sport, day_epoch))
            .or_default()
            .push(TopicArticle {
                id: row.get("id"),
                title: row.get("title"),
                description: row.get("description"),
            });
    }

    let mut out = TopicHeatRefresh::default();
    let mut seen_ids: HashSet<i64> = HashSet::new();
    let total_groups = groups.len();
    for (group_no, ((_sport, _day), articles)) in groups.into_iter().enumerate() {
        seen_ids.extend(articles.iter().map(|a| a.id));
        let mut ids = Vec::with_capacity(articles.len());
        let mut heats = Vec::with_capacity(articles.len());
        if articles.len() == 1 {
            ids.push(articles[0].id);
            heats.push(1_i32);
        } else {
            // Serve unchanged articles from the cross-pass cache; embed only the misses.
            let mut vectors: Vec<Option<Vector>> = vec![None; articles.len()];
            let mut miss_idx: Vec<(usize, u64)> = Vec::new();
            let mut miss_texts: Vec<String> = Vec::new();
            for (i, a) in articles.iter().enumerate() {
                let text = article_text(&a.title, &a.description);
                let fp = text_fingerprint(&text);
                if let Some(v) = cache.get(a.id, fp) {
                    vectors[i] = Some(v.clone());
                    out.cached += 1;
                } else {
                    miss_idx.push((i, fp));
                    miss_texts.push(text);
                }
            }
            if !miss_texts.is_empty() {
                // The beat keeps the supervisor's watchdog reading an alive drain through a
                // long embed pass (a cold pass legitimately runs tens of minutes), and names
                // the step for the wedged-activity log.
                beat(&format!(
                    "topic-heat embed group {}/{} ({} texts, {} embedded so far)",
                    group_no + 1,
                    total_groups,
                    miss_texts.len(),
                    out.embedded
                ));
                let embedded = hx
                    .embed(&miss_texts)
                    .await
                    .context("embed topic heat batch")?;
                out.embedded += embedded.len();
                let mut new_ids: Vec<i64> = Vec::with_capacity(embedded.len());
                let mut new_fps: Vec<i64> = Vec::with_capacity(embedded.len());
                let mut new_blobs: Vec<Vec<u8>> = Vec::with_capacity(embedded.len());
                for ((i, fp), v) in miss_idx.into_iter().zip(embedded) {
                    if cache.persist {
                        new_ids.push(articles[i].id);
                        new_fps.push(fp as i64);
                        new_blobs.push(encode_vector(&v));
                    }
                    cache.put(articles[i].id, fp, v.clone());
                    vectors[i] = Some(v);
                }
                // Write-through per embed batch (not per pass): a crash mid-cold-pass keeps
                // everything embedded so far. Failures degrade to in-memory only — a cache
                // must never fail the refresh.
                if cache.persist && !new_ids.is_empty() {
                    if let Err(e) = sqlx::query(
                        r#"
                        INSERT INTO public.topic_heat_embeddings (article_id, fingerprint, model, embedding)
                        SELECT v.id, v.fp, $4, v.blob
                        FROM unnest($1::bigint[], $2::bigint[], $3::bytea[]) AS v(id, fp, blob)
                        ON CONFLICT (article_id) DO UPDATE
                            SET fingerprint = excluded.fingerprint,
                                model       = excluded.model,
                                embedding   = excluded.embedding,
                                embedded_at = now()
                        "#,
                    )
                    .bind(&new_ids)
                    .bind(&new_fps)
                    .bind(&new_blobs)
                    .bind(&model)
                    .execute(&hx.pool)
                    .await
                    {
                        warn!(
                            error = %format!("{e:#}"),
                            "topic heat write-through failed; batch stays in-memory only"
                        );
                    }
                }
            }
            let vectors: Vec<Vector> = vectors.into_iter().map(|v| v.expect("filled")).collect();
            let clusters = cluster(&vectors, hx.scrub.topic_heat_threshold);
            for c in clusters {
                let heat = c.members.len() as i32;
                for idx in c.members {
                    ids.push(articles[idx].id);
                    heats.push(heat);
                }
            }
        }
        if ids.is_empty() {
            continue;
        }
        let res = sqlx::query(
            r#"
            UPDATE public.news_articles a
               SET topic_heat = v.heat
              FROM unnest($1::bigint[], $2::int[]) AS v(id, heat)
             WHERE a.id = v.id
            "#,
        )
        .bind(&ids)
        .bind(&heats)
        .execute(&hx.pool)
        .await
        .context("update topic heat")?;
        out.updated += res.rows_affected();
    }
    cache.retain_ids(&seen_ids);
    // Prune the persistent rows to the same window (the SQL mirror of retain_ids), so the
    // table stays bounded at the hot set (~1000 rows × ~1.5 KB). By-id rather than by-age:
    // exact, and it also clears rows left behind by a model-identity change.
    if cache.persist {
        let keep: Vec<i64> = seen_ids.iter().copied().collect();
        if let Err(e) =
            sqlx::query("DELETE FROM public.topic_heat_embeddings WHERE NOT (article_id = ANY($1))")
                .bind(&keep)
                .execute(&hx.pool)
                .await
        {
            warn!(error = %format!("{e:#}"), "topic heat persistent-cache prune failed");
        }
    }
    Ok(out)
}

fn article_text(title: &str, description: &str) -> String {
    if description.is_empty() {
        title.to_string()
    } else {
        format!("{title} {description}")
    }
}

fn keyword_hit(text: &str, keywords: &[String]) -> bool {
    let lower = text.to_lowercase();
    keywords.iter().any(|k| lower.contains(k))
}

fn mean_vec(vs: &[Vector]) -> Vector {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_bucket_tags_parse() {
        assert_eq!(
            ArticleBucket::from_model_tag("transfer"),
            Some(ArticleBucket::Transfer)
        );
        assert_eq!(
            ArticleBucket::from_model_tag("other"),
            Some(ArticleBucket::NonTransfer)
        );
        assert_eq!(ArticleBucket::from_model_tag("unknown"), None);
    }

    #[test]
    fn topic_heat_cache_hits_only_on_matching_fingerprint() {
        let mut cache = TopicHeatCache::new();
        let fp = text_fingerprint("Star guard traded");
        cache.put(7, fp, vec![0.1, 0.2]);
        assert_eq!(cache.get(7, fp), Some(&vec![0.1, 0.2]));
        // Edited text → different fingerprint → miss (stale vector never served).
        assert_eq!(
            cache.get(7, text_fingerprint("Star guard traded (updated)")),
            None
        );
        assert_eq!(cache.get(8, fp), None);
    }

    #[test]
    fn topic_heat_cache_put_replaces_and_retain_evicts() {
        let mut cache = TopicHeatCache::new();
        cache.put(1, 10, vec![1.0]);
        cache.put(1, 20, vec![2.0]); // re-embedded after an edit — one entry per article
        cache.put(2, 30, vec![3.0]);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(1, 20), Some(&vec![2.0]));
        assert_eq!(cache.get(1, 10), None);
        // Article 2 aged out of the lookback window.
        cache.retain_ids(&HashSet::from([1]));
        assert_eq!(cache.len(), 1);
        assert!(cache.get(2, 30).is_none());
        assert!(!cache.is_empty());
    }

    #[test]
    fn vector_bytes_roundtrip() {
        let v: Vector = vec![0.25, -1.5, 3.75e-3, f32::MIN_POSITIVE];
        let bytes = encode_vector(&v);
        assert_eq!(bytes.len(), v.len() * 4);
        assert_eq!(decode_vector(&bytes, v.len()), Some(v));
        // Wrong dim, truncated bytes, or a zero dim never yield a vector.
        assert_eq!(decode_vector(&bytes, 3), None);
        assert_eq!(decode_vector(&bytes[..7], 2), None);
        assert_eq!(decode_vector(&[], 0), None);
    }

    #[test]
    fn absorb_hydrates_only_decodable_rows() {
        let mut cache = TopicHeatCache::new();
        let v: Vector = vec![0.5, -0.5];
        // A persisted u64 fingerprint travels as i64 with the same bit pattern.
        let fp = u64::MAX - 1;
        assert!(cache.absorb(42, fp as i64, &encode_vector(&v), 2));
        assert_eq!(cache.get(42, fp), Some(&v));
        // Corrupt bytes (wrong length for the dim) are skipped, not inserted.
        assert!(!cache.absorb(43, 1, &[0_u8; 7], 2));
        assert_eq!(cache.len(), 1);
    }
}
