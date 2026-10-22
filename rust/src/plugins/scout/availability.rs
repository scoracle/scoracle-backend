//! Availability-only headline evidence and transactional delivery outcomes.
use crate::tools::{meta::EntityMeta, source::SourceContext};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Availability {
    pub sources: Vec<SourceContext>,
    pub held: Vec<SourceContext>,
    pub redundant: Vec<SourceContext>,
}
impl Availability {
    pub async fn load(connection: &mut PgConnection, subject: &EntityMeta) -> Result<Self> {
        let sources = crate::plugins::classifier::delivery::load_for_character_on(
            connection,
            super::manifest::MANIFEST.id.as_str(),
            &subject.entity_type,
            subject.entity_id,
            &subject.sport,
        )
        .await?;
        Ok(Self::pack(sources, crate::plugins::influencer::now()))
    }
    pub fn pack(sources: Vec<SourceContext>, cutoff: i64) -> Self {
        let mut result = Self::default();
        let mut seen = std::collections::HashSet::new();
        for source in sources {
            if !source.published_at_epoch.is_some_and(|at| at <= cutoff)
                || !source.classifier_world.as_ref().is_some_and(|w| {
                    w["target_identity_resolved"] == true
                        && w["gate"]["characters"]["scout"] == true
                })
                || crate::plugins::influencer::fresh::source_disposition(&source.context).is_some()
            {
                result.held.push(source);
            } else if !seen.insert(crate::tools::fresh::evidence_key(
                &source.source,
                &source.context,
            )) {
                result.redundant.push(source);
            } else {
                result.sources.push(source);
            }
        }
        if result.sources.len() > 8
            || serde_json::to_vec(&result.sources).map_or(true, |v| v.len() > 4000)
        {
            result.held.append(&mut result.sources);
        }
        result
    }
    pub fn for_writing(&self) -> serde_json::Value {
        serde_json::json!(self
            .sources
            .iter()
            .map(|s| serde_json::json!({
                "headline":s.headline,"publisher":s.source,
                "published_at":s.published_at_epoch.map(crate::util::utc_timestamp)
            }))
            .collect::<Vec<_>>())
    }
    pub async fn settle(
        &self,
        connection: &mut PgConnection,
        item: &crate::harness::queue::work::Item,
        used: bool,
    ) -> Result<()> {
        let all: Vec<_> = self
            .sources
            .iter()
            .chain(&self.held)
            .chain(&self.redundant)
            .cloned()
            .collect();
        if all.is_empty() {
            return Ok(());
        }
        // Source locks and identity checks are the same ones used by the other headline consumers.
        // The caller supplies its claim-fenced transaction through this connection.
        let current = crate::plugins::classifier::delivery::load_for_character_on(
            connection,
            super::manifest::MANIFEST.id.as_str(),
            &item.entity_type,
            item.entity_id_i32()?,
            &item.sport,
        )
        .await?;
        anyhow::ensure!(
            all.iter().all(|s| current.contains(s)),
            "Scout availability source changed"
        );
        for (sources, status, reason) in [
            (&self.sources, if used { "used" } else { "abstained" }, None),
            (
                &self.held,
                "held",
                Some("unusable_or_oversized_availability"),
            ),
            (&self.redundant, "redundant", Some("known_copy")),
        ] {
            let ids: Vec<_> = sources.iter().map(|s| s.classification_id).collect();
            crate::plugins::classifier::delivery::settle(
                connection,
                super::manifest::MANIFEST.id.as_str(),
                &ids,
                status,
                reason,
                serde_json::json!({"contract":crate::plugins::classifier::AVAILABILITY_CONTRACT}),
            )
            .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_current_resolved_availability_enters_a_bounded_profile() {
        let source = |id| SourceContext {
            classification_id: id,
            article_id: id,
            headline: "Kai Rowan may return".into(),
            context: "Kai Rowan may return".into(),
            source: "Wire".into(),
            published_at_epoch: Some(100),
            classifier_world: Some(
                serde_json::json!({"target_identity_resolved":true,"gate":{"characters":{"scout":true}}}),
            ),
        };
        let mut future = source(3);
        future.published_at_epoch = Some(201);
        let mut irrelevant = source(4);
        irrelevant.classifier_world.as_mut().unwrap()["gate"]["characters"]["scout"] =
            serde_json::json!(false);
        let a = Availability::pack(vec![source(1), source(2), future, irrelevant], 200);
        assert_eq!(a.sources.len(), 1);
        assert_eq!(a.redundant.len(), 1);
        assert_eq!(a.held.len(), 2);
        assert_eq!(a.for_writing()[0]["headline"], "Kai Rowan may return");
    }
}

#[tokio::test]
#[ignore = "requires disposable classifier_test_coverage database; run serially"]
async fn availability_routes_freezes_and_settles_without_inventing_a_profile() -> anyhow::Result<()>
{
    use crate::harness::{plugin::StudioPlugin, queue::work};
    use crate::plugins::{classifier, scout};
    let pool = crate::harness::dbtest::pool("disposable availability database").await;
    let db: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(
        db.starts_with("classifier_test_coverage"),
        "disposable database required"
    );
    sqlx::raw_sql("UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL;
      DELETE FROM pipeline_work WHERE sport='ZZ_AVAIL'; DELETE FROM application_outbox WHERE sport='ZZ_AVAIL';
      DELETE FROM stat_summaries WHERE sport='ZZ_AVAIL'; DELETE FROM news_articles WHERE id=923001;
      DELETE FROM entity_name_surfaces WHERE sport='ZZ_AVAIL';
      INSERT INTO sports(id,display_name,current_season) VALUES('ZZ_AVAIL','Availability fixture',2026) ON CONFLICT DO NOTHING;
      INSERT INTO teams(id,sport,name) VALUES(923001,'ZZ_AVAIL','Cedar FC') ON CONFLICT DO NOTHING;
      INSERT INTO entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('team',923001,'ZZ_AVAIL','cedar fc','name');
      INSERT INTO news_articles(id,url,url_hash,title,source,published_at) VALUES(923001,'https://example.invalid/availability',md5('availability'),'Cedar FC denies injury in availability fixture','Wire',now()-interval '1 minute');
      INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES(923001,'team',923001,'ZZ_AVAIL');
      SELECT classifier_enqueue_admission(923001,'ZZ_AVAIL');").execute(&pool).await?;
    let model = classifier::tests::Model::new()?;
    let handler = classifier::adapter::ClassifierHandler::new(pool.clone(), model.decision()?);
    for item in work::claim(&pool, classifier::manifest::TASK, 1000).await? {
        if item.sport == "ZZ_AVAIL" {
            handler.execute(&item).await?;
        } else {
            work::release(&pool, &item).await?;
        }
    }
    let mut claim = None;
    for item in work::claim(&pool, scout::manifest::TASK, 1000).await? {
        if item.sport == "ZZ_AVAIL" {
            claim = Some(item);
        } else {
            work::release(&pool, &item).await?;
        }
    }
    let claim = claim.ok_or_else(|| anyhow::anyhow!("missing Scout availability claim"))?;
    assert!(claim
        .input_version
        .as_deref()
        .unwrap()
        .starts_with(classifier::AVAILABILITY_CONTRACT));
    let req = scout::prompt::RatingReq {
        entity_type: "team".into(),
        entity_id: 923001,
        sport: "ZZ_AVAIL".into(),
        season: Some(2026),
        entity_name: "Cedar FC".into(),
        trigger_type: "daily".into(),
    };
    let mut tx = pool.begin().await?;
    let prepared = scout::prompt::build_rating_request_on(&mut tx, 7900, &req, 0.0, false).await?;
    let scout::prompt::RatingBuild::NoStats { availability, .. } = prepared else {
        panic!("headline invented a profile")
    };
    assert_eq!(availability.sources.len(), 1);
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/quality/rating/synthetic-strong.json"
    ))?;
    let mut parts: scout::prompt::Parts = serde_json::from_value(fixture["parts"].clone())?;
    parts.availability = availability.clone();
    let before = parts.input_components(serde_json::json!({"availability":availability}));
    let measurements = serde_json::to_value(&parts.profile)?;
    parts.availability.sources[0].published_at_epoch = Some(1);
    let after = parts.input_components(serde_json::json!({"availability":parts.availability}));
    assert_ne!(
        crate::util::hash_components(&before),
        crate::util::hash_components(&after)
    );
    assert_eq!(serde_json::to_value(&parts.profile)?, measurements);
    tx.rollback().await?;
    let mut publication =
        crate::harness::queue::publication::ClaimPublication::begin(&pool, &claim)
            .await?
            .unwrap();
    classifier::delivery::validate_for_publication(
        publication.transaction(),
        scout::manifest::MANIFEST.id.as_str(),
        "team",
        923001,
        "ZZ_AVAIL",
        &availability.sources,
    )
    .await?;
    availability
        .settle(publication.transaction(), &claim, false)
        .await?;
    publication.commit_final().await?;
    let status:String=sqlx::query_scalar("SELECT d.status FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.sport='ZZ_AVAIL' AND d.character='scout'").fetch_one(&pool).await?;
    assert_eq!(status, "abstained");
    let products: i64 =
        sqlx::query_scalar("SELECT count(*) FROM stat_summaries WHERE sport='ZZ_AVAIL'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(products, 0);
    sqlx::raw_sql("DELETE FROM pipeline_work WHERE sport='ZZ_AVAIL'; DELETE FROM application_outbox WHERE sport='ZZ_AVAIL';").execute(&pool).await?;
    Ok(())
}
