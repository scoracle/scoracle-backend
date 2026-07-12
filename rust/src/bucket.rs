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
        return Ok(if keyword_hit(&text, &hx.scrub.bucket_keywords) {
            ArticleBucket::Transfer
        } else {
            ArticleBucket::NonTransfer
        });
    };
    let vectors = hx
        .embed(std::slice::from_ref(&text))
        .await
        .context("embed article for bucket classification")?;
    Ok(classifier.classify_vector(&text, &vectors[0]))
}

/// Cross-pass embedding cache for `refresh_topic_heat`. The lookback window re-reads the same
/// articles pass after pass and embeddings are deterministic for unchanged text, so only
/// new/edited articles pay the CPU embedder — measured 2026-07-12, a full re-embed pass took
/// ~24 min at ~900 articles and starved the work queue. In-memory only (the worker owns one
/// for its lifetime); a restart pays one full pass and is warm from then on. Bounded by
/// `retain_ids` to the current window (~900 × 384 floats ≈ 1.5 MB).
#[derive(Default)]
pub struct TopicHeatCache {
    entries: HashMap<i64, (u64, Vector)>,
}

impl TopicHeatCache {
    pub fn new() -> Self {
        Self::default()
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

pub async fn refresh_topic_heat(hx: &Harness, cache: &mut TopicHeatCache) -> Result<TopicHeatRefresh> {
    if hx.embedder.is_none() {
        return Ok(TopicHeatRefresh::default());
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
    for ((_sport, _day), articles) in groups {
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
                let embedded = hx
                    .embed(&miss_texts)
                    .await
                    .context("embed topic heat batch")?;
                out.embedded += embedded.len();
                for ((i, fp), v) in miss_idx.into_iter().zip(embedded) {
                    cache.put(articles[i].id, fp, v.clone());
                    vectors[i] = Some(v);
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
        assert_eq!(cache.get(7, text_fingerprint("Star guard traded (updated)")), None);
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
}
