//! Bounded headline-history ranking over a frozen Postgres source slice.
use crate::tools::{meta::EntityMeta, source::SourceContext};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgConnection, Row};
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Headline {
    pub article_id: i64,
    pub headline: String,
    pub publisher: String,
    pub published_at: String,
    pub url: String,
    pub spectrum: Value,
}

impl Headline {
    /// Writing needs dated publisher wording; full ranking metadata stays in receipts.
    pub fn for_writing(&self) -> Value {
        serde_json::json!({"headline":self.headline,"publisher":self.publisher,
            "published_at":self.published_at})
    }
}

pub async fn load_on(
    connection: &mut PgConnection,
    subject: &EntityMeta,
    fresh: &[SourceContext],
    character: &str,
) -> Result<Vec<Headline>> {
    let Some(before) = fresh.iter().filter_map(|s| s.published_at_epoch).min() else {
        return Ok(Vec::new());
    };
    let exclude: Vec<_> = fresh.iter().map(|s| s.article_id).collect();
    let rows = sqlx::query("WITH clock AS (
            SELECT LEAST(((e.cutoff_at AT TIME ZONE 'America/Detroit')::date-1)::timestamp AT TIME ZONE 'America/Detroit',to_timestamp($4::double precision)) AS before,
                COALESCE(e.frozen_at,now()) AS captured
            FROM coverage_schedule c LEFT JOIN coverage_editions e
                ON c.enabled AND c.phase='characters' AND c.edition_date=e.edition_date
        ), archive AS (
            SELECT DISTINCT ON (x.article_id) x.article_id,s.source,x.spectrum,x.gate,c.before,
                (s.source->>'published_at')::timestamptz AS published
            FROM classifier_spectra x JOIN classifier_sources s ON s.id=x.source_id CROSS JOIN clock c
            WHERE x.sport=$3 AND x.target->>'entity_type'=$1 AND x.target->>'entity_id'=$2::text
                AND x.criteria_version IN ($7,'classification-criteria-v12-newsworthy','classification-criteria-v11-targeted','classification-criteria-v10-availability','classification-criteria-v9-headlines') AND s.source->>'evidence_scope'='headline'
                AND NOT (x.article_id=ANY($5)) AND x.created_at<=c.captured
            ORDER BY x.article_id,x.id DESC
        ), eligible AS (
            SELECT DISTINCT ON (public.headline_content_key(source->>'title')) * FROM archive
            WHERE gate->>'admitted'='true' AND gate->'characters'->>$6='true'
                AND cardinality($5)<public.headline_entity_cap()
                AND NOT EXISTS(SELECT 1 FROM classifier_spectra previous
                    JOIN classifier_spectrum_deliveries receipt ON receipt.spectrum_id=previous.id
                    WHERE previous.sport=$3 AND previous.target->>'entity_type'=$1
                        AND previous.target->>'entity_id'=$2::text AND receipt.character=$6
                        AND receipt.status IN ('used','abstained')
                        AND receipt.reason IS DISTINCT FROM 'ranked_below_entity_cap'
                        AND md5(public.headline_content_key(previous.clean_text))=md5(public.headline_content_key(source->>'title'))
                        AND public.headline_content_key(previous.clean_text)=public.headline_content_key(source->>'title'))
                AND NOT EXISTS(SELECT 1 FROM classifier_spectra current
                    WHERE current.article_id=ANY($5) AND current.sport=$3
                        AND public.headline_content_key(current.clean_text)=public.headline_content_key(source->>'title'))
                AND EXISTS(SELECT 1 FROM jsonb_array_elements(public.classifier_identity_candidates($3,source->>'title','')) identity
                    WHERE identity->>'entity_type'=$1 AND identity->>'entity_id'=$2::text)
                AND published<before AND published>=before-interval '7 days'
                AND length(btrim(source->>'title'))>0 AND length(btrim(source->>'source'))>0
                AND length(btrim(source->>'url'))>0
            ORDER BY public.headline_content_key(source->>'title'),published DESC,article_id DESC
        ) SELECT article_id,source,spectrum,floor(extract(epoch FROM published))::bigint AS published_at_epoch,
            floor(extract(epoch FROM before))::bigint AS before_epoch,
            public.headline_entity_cap()-cardinality($5) AS memory_slots
            FROM eligible ORDER BY published DESC,article_id DESC LIMIT 20001")
        .bind(&subject.entity_type).bind(subject.entity_id.to_string()).bind(&subject.sport)
        .bind(before as f64).bind(exclude).bind(character).bind(super::decision::CRITERIA_VERSION)
        .fetch_all(connection).await?;
    ensure!(
        rows.len() <= 20000,
        "headline memory population exceeds bound"
    );
    let Some(first) = rows.first() else {
        return Ok(Vec::new());
    };
    let before = first.get::<i64, _>("before_epoch");
    let slots = first.get::<i32, _>("memory_slots") as usize;
    let mut candidates = Vec::new();
    for row in rows {
        let source: Value = row.get("source");
        let headline = source["title"].as_str().unwrap_or_default();
        if crate::tools::source::contains_instruction_override(headline) {
            continue;
        }
        let article_id = row.get("article_id");
        let reported_at = row.get("published_at_epoch");
        let headline = crate::tools::fetch::decode_entities(headline);
        let publisher = source["source"].as_str().unwrap_or_default().to_owned();
        candidates.push((
            Headline {
                article_id,
                headline,
                publisher,
                published_at: crate::util::utc_timestamp(reported_at),
                url: source["url"].as_str().unwrap_or_default().into(),
                spectrum: row.get("spectrum"),
            },
            reported_at,
        ));
    }
    let mut memories = select(character, before, candidates).await?;
    memories.truncate(slots);
    Ok(memories)
}

async fn select(
    character: &str,
    before: i64,
    candidates: Vec<(Headline, i64)>,
) -> Result<Vec<Headline>> {
    let criterion = match character {
        "journalist" => "subject",
        "influencer" => "emotion",
        "insider" => "movement",
        _ => anyhow::bail!("invalid headline memory character"),
    };
    ensure!(
        candidates.len() <= 20000,
        "headline memory population exceeds bound"
    );
    let mut seen = HashSet::new();
    let mut ranked = Vec::new();
    for (headline, at) in candidates {
        if crate::tools::source::is_headline_spam(&headline.headline) {
            continue;
        }
        ensure!(
            headline.article_id > 0 && seen.insert(headline.article_id),
            "invalid or duplicate headline memory observation"
        );
        let score = |key: &str| -> Result<f64> {
            let value = headline.spectrum[key]
                .as_f64()
                .ok_or_else(|| anyhow::anyhow!("missing memory score"))?;
            ensure!(
                value.is_finite() && (0.0..=100.0).contains(&value),
                "invalid memory score"
            );
            Ok(value)
        };
        let lens = score(criterion)?;
        let subject = score("subject")?;
        if at >= before.saturating_sub(7 * 86400) && at < before {
            ranked.push((lens, subject, at, headline));
        }
    }
    ranked.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then(b.1.total_cmp(&a.1))
            .then(b.2.cmp(&a.2))
            .then(b.3.article_id.cmp(&a.3.article_id))
    });
    let mut result = Vec::new();
    for (_, _, _, headline) in ranked.into_iter().take(20) {
        result.push(headline);
        if serde_json::to_vec(&result)?.len() > 6000 {
            result.pop();
            break;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::skills::influencer::request::{Assignment, Day, Parts};
    use serde_json::json;

    #[test]
    fn writing_memory_preserves_dated_evidence_without_ranking_metadata() {
        let memory = Headline {
            article_id: 7,
            headline: "Cedar denied talks".into(),
            publisher: "Wire".into(),
            published_at: "2026-10-19T12:00:00Z".into(),
            url: "https://example.invalid/7".into(),
            spectrum: json!({"movement":95}),
        };
        assert_eq!(
            memory.for_writing(),
            json!({"headline":"Cedar denied talks", "publisher":"Wire",
            "published_at":"2026-10-19T12:00:00Z"})
        );
        let retained = serde_json::to_value(&memory).unwrap();
        assert_eq!(retained["article_id"], 7);
        assert_eq!(retained["spectrum"]["movement"], 95);
        assert_eq!(retained["url"], "https://example.invalid/7");
    }

    fn records() -> Vec<Value> {
        let corpus: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/classifier/headline-memory-corpus.json"
        )))
        .unwrap();
        corpus["records"].as_array().unwrap().clone()
    }

    fn source(record: &Value) -> SourceContext {
        SourceContext {
            classification_id: 0,
            article_id: record["article_id"].as_i64().unwrap(),
            headline: record["headline"].as_str().unwrap().into(),
            context: record["headline"].as_str().unwrap().into(),
            source: record["publisher"].as_str().unwrap().into(),
            published_at_epoch: record["published_at_epoch"].as_i64(),
            classifier_world: Some(json!({"evidence_scope":"headline",
                "target_identity_resolved":true,"spectrum":record["spectrum"],"url":record["url"]})),
        }
    }

    #[tokio::test]
    async fn rust_ranking_preserves_duckdb_order_and_complete_headlines() -> Result<()> {
        let candidates: Vec<_> = records()
            .into_iter()
            .take(3)
            .enumerate()
            .map(|(i, r)| {
                (
                    Headline {
                        article_id: i as i64 + 1,
                        headline: r["headline"].as_str().unwrap().into(),
                        publisher: r["publisher"].as_str().unwrap().into(),
                        published_at: crate::util::utc_timestamp(100 + i as i64),
                        url: r["url"].as_str().unwrap().into(),
                        spectrum: [
                            json!({"subject":99,"emotion":20,"movement":80}),
                            json!({"subject":95,"emotion":99,"movement":83}),
                            json!({"subject":90,"emotion":85,"movement":99}),
                        ][i]
                            .clone(),
                    },
                    100 + i as i64,
                )
            })
            .collect();
        for (character, index) in [("journalist", 0), ("influencer", 1), ("insider", 2)] {
            let selected = select(character, 200, candidates.clone()).await?;
            assert_eq!(selected.len(), 3);
            assert_eq!(selected[0], candidates[index].0);
        }
        let mut spam = candidates[0].clone();
        spam.0.headline = "[+[LIVE@STREAMs]+] Cedar FC vs Maple United".into();
        for character in ["journalist", "influencer", "insider"] {
            assert!(select(character, 200, vec![spam.clone()]).await?.is_empty());
        }
        let mut oversized = candidates[0].clone();
        oversized.0.headline = "Complete headline ".repeat(500);
        assert!(select("journalist", 200, vec![oversized]).await?.is_empty());
        assert!(select("journalist", 200, vec![]).await?.is_empty());
        Ok(())
    }

    include!("../../../tests/internal/classifier/memories.rs");

    #[test]
    fn stored_memories_keep_provenance_while_writing_only_receives_dated_evidence() {
        let records = records();
        let memories = records[..4]
            .iter()
            .map(|r| Headline {
                article_id: r["article_id"].as_i64().unwrap(),
                headline: r["headline"].as_str().unwrap().into(),
                publisher: r["publisher"].as_str().unwrap().into(),
                published_at: r["published_at"].as_str().unwrap().into(),
                url: r["url"].as_str().unwrap().into(),
                spectrum: r["spectrum"].clone(),
            })
            .collect();
        let mut parts = Parts {
            database_record: None,
            subject: serde_json::from_value(records[0]["subject"].clone()).unwrap(),
            day: Day {
                start: 1792296000,
                end: 1792382400,
                cutoff: 1792382460,
            },
            sources: records[4..].iter().map(source).collect(),
            memories,
            excluded: vec![],
        };
        let packet: Value = serde_json::from_str(&parts.assemble()).unwrap();
        assert!(packet.get("memories").is_none());
        let retained = serde_json::to_value(&parts.memories[0]).unwrap();
        assert_eq!(retained["spectrum"], records[0]["spectrum"]);
        assert_eq!(retained["article_id"], records[0]["article_id"]);
        assert_eq!(retained["url"], records[0]["url"]);
        assert_eq!(
            packet["target"],
            json!({"name":"New York Knicks","entity_type":"team","sport":"basketball"})
        );
        assert!(packet.get("voice").is_none());
        assert!(packet["task"].is_string());
        assert_eq!(packet.as_object().unwrap().len(), 5);
        assert!(packet.get("tone").is_none());
        assert!(packet["evidence"]["reports"][0]
            .get("publisher_excerpt")
            .is_none());
        let with_memories = Assignment::from_parts(parts.clone()).unwrap().input_hash;
        parts.memories.clear();
        assert_eq!(
            with_memories,
            Assignment::from_parts(parts).unwrap().input_hash
        );
    }
}
