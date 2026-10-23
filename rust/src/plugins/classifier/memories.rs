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
                AND x.criteria_version IN ($7,'classification-criteria-v9-headlines') AND s.source->>'evidence_scope'='headline'
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
    use crate::plugins::influencer::prompt::{Assignment, Day, Parts};
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
        let corpus: Value = serde_json::from_str(include_str!(
            "../../../fixtures/classifier/headline-memory-corpus.json"
        ))
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

    #[tokio::test]
    #[ignore = "requires SCORACLE_MEMORY_STUDY_BIN"]
    async fn rust_sort_matches_existing_duckdb_for_ties_and_window_edges() -> Result<()> {
        let before = 800000;
        let candidates: Vec<_> = (1..=30)
            .map(|id| {
                let at = match id {
                    1 => before,
                    2 => before - 7 * 86400 - 1,
                    3 => before - 7 * 86400,
                    _ => before - id / 2,
                };
                (
                    Headline {
                        article_id: id,
                        headline: format!("Report {id}"),
                        publisher: "Wire".into(),
                        published_at: crate::util::utc_timestamp(at),
                        url: format!("https://example.invalid/{id}"),
                        spectrum: json!({"subject":id%3+90,"emotion":id%4+80,"movement":id%5+70}),
                    },
                    at,
                )
            })
            .collect();
        for character in ["journalist", "influencer", "insider"] {
            let request = json!({"kind":"headline_memory","version":"headline-spectrum-memory-v1",
                "character":character,"from":before-7*86400,"before":before,"limit":20,
                "observations":candidates.iter().map(|(h,at)| json!({"article_id":h.article_id,"reported_at":at,"spectrum":h.spectrum})).collect::<Vec<_>>()});
            let expected: Vec<i64> = crate::tools::memories::run(&request).await?;
            let actual: Vec<i64> = select(character, before, candidates.clone())
                .await?
                .into_iter()
                .map(|h| h.article_id)
                .collect();
            assert_eq!(actual, expected);
        }
        Ok(())
    }

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
        assert_eq!(packet["memories"][0]["headline"], records[0]["headline"]);
        assert_eq!(
            packet["memories"][0]["published_at"],
            records[0]["published_at"]
        );
        assert_eq!(packet["memories"][0]["publisher"], records[0]["publisher"]);
        assert_eq!(packet["memories"][0].as_object().unwrap().len(), 3);
        let retained = serde_json::to_value(&parts.memories[0]).unwrap();
        assert_eq!(retained["spectrum"], records[0]["spectrum"]);
        assert_eq!(retained["article_id"], records[0]["article_id"]);
        assert_eq!(retained["url"], records[0]["url"]);
        assert_eq!(
            packet["meta"],
            json!({"name":"New York Knicks","entity_type":"team","sport":"basketball"})
        );
        assert!(packet.get("voice").is_none());
        assert!(packet.get("task").is_none());
        assert_eq!(packet.as_object().unwrap().len(), 3);
        assert!(packet["fresh"]["reports"][0]
            .get("publisher_text")
            .is_none());
        let with_memories = Assignment::from_parts(parts.clone()).unwrap().input_hash;
        parts.memories.clear();
        assert_ne!(
            with_memories,
            Assignment::from_parts(parts).unwrap().input_hash
        );
    }

    #[tokio::test]
    #[ignore = "requires a disposable classifier_test database via TEST_DATABASE_URL"]
    async fn stored_headline_archive_respects_character_day_and_freeze() -> anyhow::Result<()> {
        let pool = crate::harness::dbtest::pool("disposable classifier_test database").await;
        super::super::plumbing_tests::setup_disposable(&pool).await?;
        let mut tx = pool.begin().await?;
        let records = records();
        let subject: EntityMeta = serde_json::from_value(records[0]["subject"].clone())?;
        sqlx::query("INSERT INTO teams(id,sport,name) VALUES(20,'NBA','New York Knicks') ON CONFLICT DO NOTHING").execute(&mut *tx).await?;
        sqlx::query("INSERT INTO entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('team',20,'NBA','new york knicks','name'),('team',20,'NBA','knicks','alias') ON CONFLICT DO NOTHING").execute(&mut *tx).await?;
        let mut spectrum_ids = Vec::new();
        for record in &records {
            let id = record["article_id"].as_i64().unwrap();
            let headline = record["headline"].as_str().unwrap();
            let url = record["url"].as_str().unwrap();
            sqlx::query("INSERT INTO news_articles(id,url_hash,url,title,source,published_at) VALUES($1,$2,$3,$4,$5,$6::text::timestamptz)")
                .bind(id).bind(super::super::hash(url)).bind(url).bind(headline)
                .bind(record["publisher"].as_str().unwrap()).bind(record["published_at"].as_str().unwrap()).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO news_article_entities(article_id,entity_type,entity_id,sport) VALUES($1,'team',20,'NBA')").bind(id).execute(&mut *tx).await?;
            let identities: Value =
                sqlx::query_scalar("SELECT classifier_identity_candidates('NBA',$1,'')")
                    .bind(headline)
                    .fetch_one(&mut *tx)
                    .await?;
            let retained = json!({"article_id":id,"body":"","title":headline,"source":record["publisher"],
                "published_at":record["published_at"],"url":url,"query_entities":[subject],"identity_candidates":identities,"evidence_scope":"headline"});
            let native =
                super::super::decision::Measurement::from_receipt(record["receipt"].clone())?;
            let source_id:i64=sqlx::query_scalar("INSERT INTO classifier_sources(article_id,sport,input_hash,body_sha256,source,discovery_version) VALUES($1,'NBA',$2,$3,$4,classifier_discovery_version($1,'NBA')) RETURNING id")
                .bind(id).bind(&native.request_hash).bind(super::super::hash("")).bind(retained).fetch_one(&mut *tx).await?;
            let gate = json!({"coverage":"complete","admitted":super::super::decision::admitted(&native.spectrum)?,
                "characters":super::super::decision::characters(&native.spectrum)?});
            let spectrum_id:i64=sqlx::query_scalar("INSERT INTO classifier_spectra(source_id,article_id,sport,target,model,criteria_version,scrub_version,clean_text,clean_sha256,spectrum,gate,request_hash,receipt,created_at)
                VALUES($1,$2,'NBA',$3,$4,$5,'rss-headline-spectrum-v1',$6,$7,$8,$9,$10,$11,to_timestamp(1792382459)) RETURNING id")
                .bind(source_id).bind(id).bind(json!({"entity_type":"team","entity_id":20,"sport":"NBA"}))
                .bind(native.model).bind(super::super::decision::CRITERIA_VERSION).bind(headline).bind(super::super::hash(headline))
                .bind(json!(native.spectrum)).bind(gate).bind(native.request_hash).bind(native.receipt).fetch_one(&mut *tx).await?;
            sqlx::query("INSERT INTO classifier_source_heads(article_id,sport,source_id) VALUES($1,'NBA',$2)").bind(id).bind(source_id).execute(&mut *tx).await?;
            if spectrum_ids.len() >= 4 {
                sqlx::query("INSERT INTO classifier_spectrum_deliveries(spectrum_id,character) VALUES($1,'journalist')").bind(spectrum_id).execute(&mut *tx).await?;
            }
            spectrum_ids.push(spectrum_id);
        }
        sqlx::query("INSERT INTO coverage_editions(edition_date,cutoff_at,drain_deadline,frozen_at,status)
            VALUES('2026-10-19',to_timestamp(1792382400),to_timestamp(1792383300),to_timestamp(1792382460),'characters')
            ON CONFLICT(edition_date) DO UPDATE SET frozen_at=EXCLUDED.frozen_at").execute(&mut *tx).await?;
        sqlx::query("UPDATE coverage_schedule SET enabled=true,enrolled_at=to_timestamp(1792296000),phase='characters',edition_date='2026-10-19'").execute(&mut *tx).await?;
        sqlx::query("INSERT INTO coverage_assignments(edition_date,stage,entity_type,entity_id,sport,spectrum_ids)
            VALUES('2026-10-19','narratives','team',20,'NBA',$1)").bind(&spectrum_ids[4..]).execute(&mut *tx).await?;
        let fresh = super::super::delivery::load_for_character_on(
            &mut tx,
            crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
            "team",
            20,
            "NBA",
        )
        .await?;
        assert_eq!(fresh.len(), 2);
        assert!(fresh.iter().all(|s| s.context == s.headline));
        let memories = load_on(&mut tx, &subject, &fresh, "journalist").await?;
        assert_eq!(memories.len(), 4);
        sqlx::query("SAVEPOINT reviewed_memory")
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO classifier_spectrum_deliveries(spectrum_id,character,status) VALUES($1,'journalist','used')")
            .bind(spectrum_ids[0]).execute(&mut *tx).await?;
        let unread = load_on(&mut tx, &subject, &fresh, "journalist").await?;
        assert_eq!(unread.len(), 3);
        assert!(unread
            .iter()
            .all(|m| m.article_id != records[0]["article_id"].as_i64().unwrap()));
        sqlx::query("ROLLBACK TO SAVEPOINT reviewed_memory")
            .execute(&mut *tx)
            .await?;
        let mut full_packet = fresh.clone();
        full_packet.resize(39, fresh[0].clone());
        assert_eq!(
            load_on(&mut tx, &subject, &full_packet, "journalist")
                .await?
                .len(),
            1
        );
        full_packet.push(fresh[0].clone());
        assert!(load_on(&mut tx, &subject, &full_packet, "journalist")
            .await?
            .is_empty());
        let mut assignment = crate::plugins::journalist::fresh::prepare(
            subject.clone(),
            fresh
                .iter()
                .map(crate::plugins::journalist::fresh::CorpusItem::from)
                .collect(),
            &[],
            1792382460,
        )?;
        assignment.memories = memories.clone();
        let packet: Value =
            serde_json::from_str(&crate::plugins::journalist::prompt::prompt(&assignment))?;
        assert_eq!(packet["fresh"].as_array().unwrap().len(), 2);
        assert_eq!(packet["memories"].as_array().unwrap().len(), 4);
        for report in packet["fresh"].as_array().unwrap() {
            assert!(report.get("publisher_excerpt").is_none());
            assert!(fresh.iter().any(|s| report["headline"] == s.headline));
        }
        let influencer = crate::plugins::influencer::fresh::load(
            &mut tx,
            subject.clone(),
            Day {
                start: 1792296000,
                end: 1792382400,
                cutoff: 1792382460,
            },
        )
        .await?;
        assert!(
            influencer.sources.is_empty(),
            "non-emotional headlines cannot leak from Journalist's selection"
        );
        assert!(memories
            .iter()
            .all(|m| !fresh.iter().any(|s| s.article_id == m.article_id)));
        assert!(memories.iter().all(|m| m.published_at.ends_with('Z')));
        assert!(load_on(&mut tx, &subject, &fresh, "influencer")
            .await?
            .is_empty());
        sqlx::query(
            "UPDATE classifier_spectra SET created_at=to_timestamp(1792382461) WHERE id=$1",
        )
        .bind(spectrum_ids[0])
        .execute(&mut *tx)
        .await?;
        let frozen = load_on(&mut tx, &subject, &fresh, "journalist").await?;
        assert_eq!(frozen.len(), 3);
        assert!(frozen
            .iter()
            .all(|m| m.article_id != records[0]["article_id"].as_i64().unwrap()));
        sqlx::query("INSERT INTO pipeline_work(stage,entity_type,entity_id,sport,input_version)
            VALUES('classifier','article',$1,'NBA',classifier_discovery_version($1,'NBA')) ON CONFLICT DO NOTHING")
            .bind(records[0]["article_id"].as_i64().unwrap()).execute(&mut *tx).await?;
        let offered: i64 = sqlx::query_scalar("SELECT classifier_replay_admission(1)")
            .fetch_one(&mut *tx)
            .await?;
        assert_eq!(
            offered, 0,
            "historical replay must leave room for existing runnable work"
        );
        tx.rollback().await?;
        Ok(())
    }
}
