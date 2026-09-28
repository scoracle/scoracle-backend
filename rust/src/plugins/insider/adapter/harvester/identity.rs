//! Source-only identity review after all Harvester transfer pairs settle.
use super::*;
use anyhow::{ensure, Context};
use sha2::{Digest, Sha256};

const PROMPT_VERSION: &str = TRANSFER_IDENTITY_ADJUDICATION_PROMPT_VERSION;

async fn settle(
    pool: &PgPool,
    item: &Item,
    rumor_id: i64,
    status: &str,
    reason: &str,
    model: Option<&str>,
    raw: Option<&str>,
) -> Result<PluginOutcome> {
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    let changed = sqlx::query(
        "UPDATE public.harvester_insider_identity_reviews \
         SET status=$2,reason=$3,model_version=$4,prompt_version=$5,model_raw=$6,updated_at=now() \
         WHERE transfer_rumor_id=$1 AND status='pending'",
    )
    .bind(rumor_id)
    .bind(status)
    .bind(reason)
    .bind(model)
    .bind(model.map(|_| PROMPT_VERSION))
    .bind(raw)
    .execute(&mut **publication.transaction())
    .await?;
    ensure!(
        changed.rows_affected() == 1,
        "identity review changed during claim"
    );
    publication.commit_progress().await?;
    Ok(PluginOutcome::deferred(
        "Harvester identity review settled",
        Duration::from_secs(1),
    ))
}

pub(super) async fn execute(
    pool: &PgPool,
    item: &Item,
    models: Option<&ExecutionCapabilities>,
) -> Result<Option<PluginOutcome>> {
    let backend = models
        .map(|models| models.inference(crate::plugins::graph::manifest::ROUTE))
        .transpose()?;
    execute_inner(pool, item, backend.as_deref()).await
}

async fn execute_inner(
    pool: &PgPool,
    item: &Item,
    backend: Option<&dyn Inference>,
) -> Result<Option<PluginOutcome>> {
    let row = sqlx::query(
        "SELECT r.transfer_rumor_id,r.article_id,r.player_id,r.team_id, \
                tr.is_rumor,tr.direction,tr.heat,pl.name AS player_name,t.name AS team_name, \
                c.headline,c.context_text,c.context_start,c.context_end,c.body_sha256, \
                a.title,a.full_text,COALESCE(a.source,'') AS source \
         FROM public.harvester_insider_identity_reviews r \
         JOIN public.transfer_rumors tr ON tr.id=r.transfer_rumor_id \
         JOIN public.players pl ON pl.id=r.player_id AND pl.sport=r.sport \
         JOIN public.teams t ON t.id=r.team_id AND t.sport=r.sport \
         JOIN public.harvester_classifications c ON c.id=r.classification_id \
         JOIN public.news_articles a ON a.id=r.article_id \
         WHERE r.team_id=$1 AND r.sport=$2 AND r.status='pending' \
         ORDER BY r.transfer_rumor_id LIMIT 1",
    )
    .bind(item.entity_id_i32()?)
    .bind(item.sport.to_uppercase())
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        // An applied identity commits its override and refresh request atomically.
        // A crash after that commit must still drain the materialized-view refresh.
        let pending: bool = sqlx::query_scalar(
            "SELECT COALESCE((SELECT status <> 'ready' FROM public.sport_autofill_versions \
             WHERE sport=$1),false)",
        )
        .bind(item.sport.to_uppercase())
        .fetch_one(pool)
        .await?;
        if pending {
            super::super::identity::refresh_sport_autofill_concurrently(
                pool,
                &item.sport.to_uppercase(),
                "applied_transfer_identity",
            )
            .await?;
        }
        return Ok(None);
    };
    let rumor_id: i64 = row.get("transfer_rumor_id");
    let sport = item.sport.to_uppercase();
    let article_id: i64 = row.get("article_id");
    let player_id: i32 = row.get("player_id");
    let team_id: i32 = row.get("team_id");
    let is_rumor: Option<bool> = row.get("is_rumor");
    let direction: Option<String> = row.get("direction");
    let heat: i16 = row.get("heat");
    let threshold: Option<(i16, f64)> = sqlx::query_as(
        "SELECT min_heat,min_deterministic_confidence::float8 \
         FROM public.transfer_identity_thresholds WHERE sport=$1",
    )
    .bind(&sport)
    .fetch_optional(pool)
    .await?;
    let reason = match threshold {
        None => Some("missing_identity_threshold"),
        Some(_) if is_rumor != Some(true) => Some("not_a_rumor"),
        Some(_) if direction.as_deref() != Some("incoming") => Some("not_incoming"),
        Some((min_heat, min_confidence))
            if heat < min_heat || f64::from(heat) / 100.0 < min_confidence =>
        {
            Some("below_identity_threshold")
        }
        Some(_) => None,
    };
    if let Some(reason) = reason {
        return Ok(Some(
            settle(pool, item, rumor_id, "skipped", reason, None, None).await?,
        ));
    }
    let identity = sqlx::query(
        "SELECT pci.team_id,COALESCE(t.name,'') AS team_name \
         FROM public.player_current_identity pci \
         LEFT JOIN public.teams t ON t.id=pci.team_id AND t.sport=pci.sport \
         WHERE pci.sport=$1 AND pci.player_id=$2",
    )
    .bind(&sport)
    .bind(player_id)
    .fetch_optional(pool)
    .await?;
    let Some(identity) = identity else {
        return Ok(Some(
            settle(
                pool,
                item,
                rumor_id,
                "skipped",
                "missing_current_identity",
                None,
                None,
            )
            .await?,
        ));
    };
    let old_team_id: Option<i32> = identity.get("team_id");
    if old_team_id == Some(team_id) {
        return Ok(Some(
            settle(
                pool,
                item,
                rumor_id,
                "skipped",
                "already_current_team",
                None,
                None,
            )
            .await?,
        ));
    }
    let body: Option<String> = row.get("full_text");
    let body = body.context("Harvester identity source body missing")?;
    let headline: String = row.get("headline");
    let title: String = row.get("title");
    let context: String = row.get("context_text");
    let start: i32 = row.get("context_start");
    let end: i32 = row.get("context_end");
    let hash: String = row.get("body_sha256");
    ensure!(headline == title, "Harvester identity headline drift");
    ensure!(
        hex::encode(Sha256::digest(body.as_bytes())) == hash,
        "Harvester identity body drift"
    );
    ensure!(
        start >= 0
            && end >= start
            && body.get(start as usize..end as usize) == Some(context.as_str()),
        "Harvester identity opening drift"
    );
    let source = NewsItem {
        id: article_id,
        title: headline,
        description: context,
        source: row.get("source"),
    };
    let backend = backend.context("identity adjudication model unavailable")?;
    let mut prompt = build_transfer_identity_adjudication_prompt(
        &sport,
        player_id,
        row.get("player_name"),
        old_team_id,
        identity.get("team_name"),
        team_id,
        row.get("team_name"),
        None,
        &[source],
    );
    // The legacy prompt truncates descriptions; the Harvester contract supplies
    // the complete verified opening as context for the actual adjudication.
    let opening: String = row.get("context_text");
    prompt.push_str("\nExact publisher opening (unchanged; use this as the source text):\n");
    prompt.push_str(&opening);
    let generated = crate::studio::Studio::new(backend)
        .extract(
            &prompt,
            &crate::plugins::insider::cognition::identity_options(
                &sport,
                crate::studio::model::LOCAL_STAGE_NUM_CTX,
            ),
            &TransferIdentityAdjudicationParser,
            crate::plugins::support::prompt::structured_correction,
        )
        .await?;
    let Some(adjudication) = generated.value else {
        return Ok(Some(
            settle(
                pool,
                item,
                rumor_id,
                "failed_closed",
                "invalid_adjudication_json",
                Some(&generated.model),
                Some(&generated.raw_response),
            )
            .await?,
        ));
    };
    let raw = serde_json::to_string(&adjudication)?;
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(Some(PluginOutcome::Superseded));
    };
    let result = sqlx::query(
        "SELECT application_id,status,reason FROM public.apply_transfer_identity_candidate( \
         $1,$2,$3,$4,$5,NULL,$6,$7::float8::numeric,$8::jsonb,$9,$10,$11,'rumor_threshold',$12)",
    )
    .bind(&sport)
    .bind(player_id)
    .bind(old_team_id)
    .bind(team_id)
    .bind(rumor_id)
    .bind(heat)
    .bind(f64::from(heat) / 100.0)
    .bind(&raw)
    .bind(&generated.raw_response)
    .bind(&generated.model)
    .bind(PROMPT_VERSION)
    .bind(vec![article_id])
    .fetch_one(&mut **publication.transaction())
    .await?;
    let status: String = result.get("status");
    ensure!(
        matches!(status.as_str(), "applied" | "rejected" | "failed_closed"),
        "unexpected identity application status"
    );
    let application_id: i64 = result.get("application_id");
    let changed = sqlx::query(
        "UPDATE public.harvester_insider_identity_reviews SET status=$2,reason=$3, \
         application_id=$4,model_version=$5,prompt_version=$6,model_raw=$7,updated_at=now() \
         WHERE transfer_rumor_id=$1 AND status='pending'",
    )
    .bind(rumor_id)
    .bind(&status)
    .bind(result.get::<Option<String>, _>("reason"))
    .bind(application_id)
    .bind(&generated.model)
    .bind(PROMPT_VERSION)
    .bind(&generated.raw_response)
    .execute(&mut **publication.transaction())
    .await?;
    ensure!(
        changed.rows_affected() == 1,
        "identity review changed during application"
    );
    if status == "applied" {
        let season: i32 =
            sqlx::query_scalar("SELECT current_season FROM public.sports WHERE id=$1")
                .bind(&sport)
                .fetch_one(&mut **publication.transaction())
                .await?;
        let version = crate::plugins::scout::adapter::rating_work_input_version_for_transfer(
            season,
            application_id,
        );
        for (entity_type, entity_id) in [
            ("player", Some(player_id)),
            ("team", old_team_id),
            ("team", Some(team_id)),
        ] {
            if let Some(entity_id) = entity_id {
                record_transfer_event(
                    publication.transaction(),
                    item,
                    TRANSFER_IDENTITY_APPLIED,
                    entity_type,
                    entity_id,
                    Some(&version),
                )
                .await?;
            }
        }
        sqlx::query("SELECT public.request_sport_autofill_refresh($1,$2)")
            .bind(&sport)
            .bind("applied_transfer_identity")
            .execute(&mut **publication.transaction())
            .await?;
    }
    publication.commit_progress().await?;
    Ok(Some(PluginOutcome::deferred(
        "Harvester identity review settled",
        Duration::from_secs(1),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::queue::work;
    use crate::studio::model::{GenerateOptions, GenerateResult};
    use async_trait::async_trait;

    struct Approved;
    struct Rejected;

    #[async_trait]
    impl Inference for Approved {
        async fn generate(
            &self,
            prompt: &str,
            _: &GenerateOptions,
        ) -> Result<(GenerateResult, serde_json::Value)> {
            ensure!(prompt.contains("Exact publisher opening (unchanged; use this as the source text):\nThe New Club signed Sample Player today."));
            ensure!(!prompt.contains("RSS-only claim"));
            let response = r#"{"decision":"apply","event_type":"signing","confidence":0.95,"old_team_id":9691101,"new_team_id":9691102,"reason":"The publisher reports a completed signing.","evidence_spans":["The New Club signed Sample Player today."]}"#.to_string();
            Ok((
                GenerateResult {
                    response: response.clone(),
                    thinking: String::new(),
                    model: "identity-smoke".into(),
                    total_duration: Duration::from_millis(1),
                    prompt_eval_count: 1,
                    eval_count: 1,
                    completion_reason: Some("stop".into()),
                    raw_response_body: response,
                },
                serde_json::json!({"prompt":prompt}),
            ))
        }

        fn model(&self) -> &str {
            "identity-smoke"
        }

        fn request_body(&self, prompt: &str, _: &GenerateOptions) -> serde_json::Value {
            serde_json::json!({"prompt":prompt})
        }
    }

    #[async_trait]
    impl Inference for Rejected {
        async fn generate(
            &self,
            prompt: &str,
            options: &GenerateOptions,
        ) -> Result<(GenerateResult, serde_json::Value)> {
            let (mut result, request) = Approved.generate(prompt, options).await?;
            result.response = r#"{"decision":"reject","event_type":"rumor","confidence":0.95,"old_team_id":9691101,"new_team_id":9691102,"reason":"Source does not establish a completed move.","evidence_spans":[]}"#.into();
            result.raw_response_body = result.response.clone();
            Ok((result, request))
        }

        fn model(&self) -> &str {
            "identity-smoke-reject"
        }

        fn request_body(&self, prompt: &str, options: &GenerateOptions) -> serde_json::Value {
            Approved.request_body(prompt, options)
        }
    }

    #[tokio::test]
    #[ignore = "requires isolated TEST_DATABASE_URL with migration 282"]
    async fn eligible_identity_review_applies_with_source_and_rating_fanout() -> Result<()> {
        const SPORT: &str = "NBA";
        const OLD: i32 = 9_691_101;
        const NEW: i32 = 9_691_102;
        const PLAYER: i32 = 9_691_103;
        const ARTICLE: i64 = 9_691_104;
        let pool = PgPool::connect(&std::env::var("TEST_DATABASE_URL")?).await?;
        sqlx::query(
            "DELETE FROM public.application_outbox WHERE sport=$1 AND entity_id IN ($2,$3,$4)",
        )
        .bind(SPORT)
        .bind(OLD)
        .bind(NEW)
        .bind(PLAYER)
        .execute(&pool)
        .await?;
        sqlx::query(
            "DELETE FROM public.harvester_insider_identity_reviews WHERE sport=$1 AND team_id=$2",
        )
        .bind(SPORT)
        .bind(NEW)
        .execute(&pool)
        .await?;
        sqlx::query(
            "DELETE FROM public.transfer_identity_applications WHERE sport=$1 AND player_id=$2",
        )
        .bind(SPORT)
        .bind(PLAYER)
        .execute(&pool)
        .await?;
        sqlx::query(
            "DELETE FROM public.player_current_identity_overrides WHERE sport=$1 AND player_id=$2",
        )
        .bind(SPORT)
        .bind(PLAYER)
        .execute(&pool)
        .await?;
        sqlx::query("DELETE FROM public.transfer_rumors WHERE sport=$1 AND team_id=$2")
            .bind(SPORT)
            .bind(NEW)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.news_articles WHERE id=$1")
            .bind(ARTICLE)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.pipeline_work WHERE sport=$1 AND entity_type='team' AND entity_id=$2")
            .bind(SPORT)
            .bind(NEW)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.players WHERE id=$1 AND sport=$2")
            .bind(PLAYER)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.teams WHERE id IN ($1,$2) AND sport=$3")
            .bind(OLD)
            .bind(NEW)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO public.sports(id,display_name,current_season) VALUES($1,'Basketball',2026) ON CONFLICT DO NOTHING")
            .bind(SPORT).execute(&pool).await?;
        sqlx::query("INSERT INTO public.teams(id,sport,name) VALUES($1,$3,'The Old Club'),($2,$3,'The New Club')")
            .bind(OLD).bind(NEW).bind(SPORT).execute(&pool).await?;
        sqlx::query(
            "INSERT INTO public.players(id,sport,name,team_id) VALUES($1,$2,'Sample Player',$3)",
        )
        .bind(PLAYER)
        .bind(SPORT)
        .bind(OLD)
        .execute(&pool)
        .await?;
        sqlx::query("INSERT INTO public.transfer_identity_thresholds(sport,min_heat,min_deterministic_confidence) VALUES($1,80,0.8) ON CONFLICT(sport) DO NOTHING")
            .bind(SPORT).execute(&pool).await?;
        let body = "The New Club signed Sample Player today. The deal was announced this morning.";
        sqlx::query("INSERT INTO public.news_articles(id,url_hash,url,title,source,description,full_text) VALUES($1,$2,$3,'Signing confirmed','Example Wire','RSS-only claim',$4)")
            .bind(ARTICLE).bind("identity-smoke-9691104").bind("https://example.test/identity-smoke")
            .bind(body).execute(&pool).await?;
        sqlx::query("INSERT INTO public.harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES($1,'team',$2,$3)")
            .bind(ARTICLE).bind(NEW).bind(SPORT).execute(&pool).await?;
        let classification_id: i64 = sqlx::query_scalar(
            "INSERT INTO public.harvester_classifications \
             (article_id,entity_type,entity_id,sport,contract_version,model_revision,entity_choice, \
              body_sha256,headline,model_input_start,model_input_end,model_input_text, \
              context_start,context_end,context_text,distributions,model_provenance) \
             VALUES($1,'team',$2,$3,'harvest-context-v1','identity-smoke','relevant', \
                    $4,'Signing confirmed',0,$5,$6,0,$5,$6,'{}'::jsonb,'{}'::jsonb) RETURNING id",
        )
        .bind(ARTICLE).bind(NEW).bind(SPORT)
        .bind(hex::encode(Sha256::digest(body.as_bytes())))
        .bind(body.len() as i32).bind(body).fetch_one(&pool).await?;
        let rumor_id: i64 = sqlx::query_scalar(
            "INSERT INTO public.transfer_rumors(team_id,player_id,sport,trigger_type,heat, \
             is_rumor,direction,stage,input_news_ids,subject_type) \
             VALUES($1,$2,$3,'harvester',90,true,'incoming','here_we_go',ARRAY[$4]::bigint[],'player') RETURNING id",
        )
        .bind(NEW).bind(PLAYER).bind(SPORT).bind(ARTICLE).fetch_one(&pool).await?;
        sqlx::query(
            "INSERT INTO public.harvester_insider_identity_reviews \
             (transfer_rumor_id,classification_id,article_id,team_id,player_id,sport) \
             VALUES($1,$2,$3,$4,$5,$6)",
        )
        .bind(rumor_id)
        .bind(classification_id)
        .bind(ARTICLE)
        .bind(NEW)
        .bind(PLAYER)
        .bind(SPORT)
        .execute(&pool)
        .await?;
        // Schema-only disposable databases have an unpopulated materialized view;
        // production has already populated it before concurrent refreshes.
        sqlx::query("REFRESH MATERIALIZED VIEW nba.autofill_entities")
            .execute(&pool)
            .await?;
        work::enqueue(
            &pool,
            &Item {
                stage: crate::plugins::insider::manifest::TASK,
                entity_type: "team".into(),
                entity_id: i64::from(NEW),
                sport: SPORT.into(),
                input_version: Some("identity-smoke-v1".into()),
                attempts: 0,
                claim_token: None,
            },
        )
        .await?;
        let claim = work::claim(&pool, crate::plugins::insider::manifest::TASK, 1)
            .await?
            .remove(0);
        assert!(matches!(
            execute_inner(&pool, &claim, Some(&Approved)).await?,
            Some(PluginOutcome::Deferred { .. })
        ));
        let review: (String, i64) = sqlx::query_as(
            "SELECT status,application_id FROM public.harvester_insider_identity_reviews WHERE transfer_rumor_id=$1"
        ).bind(rumor_id).fetch_one(&pool).await?;
        assert_eq!(review.0, "applied");
        let application: (String, String, serde_json::Value) = sqlx::query_as(
            "SELECT status,evidence_route,(evidence->'identity_evidence_article_ids')::jsonb \
             FROM public.transfer_identity_applications WHERE id=$1",
        )
        .bind(review.1)
        .fetch_one(&pool)
        .await?;
        assert_eq!(application.0, "applied");
        assert_eq!(application.1, "rumor_threshold");
        assert_eq!(application.2, serde_json::json!([ARTICLE]));
        use crate::plugins::scout::adapter::harvester::{structured_record, SourceKind};
        assert_eq!(
            structured_record(&pool, SPORT, "player", PLAYER, ARTICLE, SourceKind::Roster).await?,
            Some(review.1)
        );
        assert_eq!(
            structured_record(&pool, SPORT, "team", NEW, ARTICLE, SourceKind::Roster).await?,
            Some(review.1)
        );
        assert_eq!(
            structured_record(
                &pool,
                SPORT,
                "player",
                PLAYER,
                ARTICLE + 1,
                SourceKind::Roster
            )
            .await?,
            None
        );
        let current_team: Option<i32> = sqlx::query_scalar(
            "SELECT team_id FROM public.player_current_identity WHERE sport=$1 AND player_id=$2",
        )
        .bind(SPORT)
        .bind(PLAYER)
        .fetch_one(&pool)
        .await?;
        assert_eq!(current_team, Some(NEW));
        let fanout: Vec<(String, i32)> = sqlx::query_as(
            "SELECT entity_type,entity_id FROM public.application_outbox \
             WHERE sport=$1 AND kind='transfer_identity_applied' ORDER BY entity_type,entity_id",
        )
        .bind(SPORT)
        .fetch_all(&pool)
        .await?;
        assert_eq!(
            fanout,
            vec![
                ("player".into(), PLAYER),
                ("team".into(), OLD),
                ("team".into(), NEW)
            ]
        );
        assert!(execute_inner(&pool, &claim, Some(&Approved))
            .await?
            .is_none());
        let autofill_status: String =
            sqlx::query_scalar("SELECT status FROM public.sport_autofill_versions WHERE sport=$1")
                .bind(SPORT)
                .fetch_one(&pool)
                .await?;
        assert_eq!(autofill_status, "ready");
        // A later review must fail closed if the source changes after classification.
        sqlx::query(
            "UPDATE public.player_current_identity_overrides \
             SET reverted_at=now(),reverted_by='identity_smoke' \
             WHERE sport=$1 AND player_id=$2 AND reverted_at IS NULL",
        )
        .bind(SPORT)
        .bind(PLAYER)
        .execute(&pool)
        .await?;
        sqlx::query(
            "UPDATE public.harvester_insider_identity_reviews \
             SET status='pending',application_id=NULL WHERE transfer_rumor_id=$1",
        )
        .bind(rumor_id)
        .execute(&pool)
        .await?;
        sqlx::query("UPDATE public.news_articles SET full_text='Altered source body.' WHERE id=$1")
            .bind(ARTICLE)
            .execute(&pool)
            .await?;
        let drift = execute_inner(&pool, &claim, Some(&Approved))
            .await
            .unwrap_err();
        assert!(drift.to_string().contains("body drift"));
        let still_pending: String = sqlx::query_scalar(
            "SELECT status FROM public.harvester_insider_identity_reviews WHERE transfer_rumor_id=$1"
        ).bind(rumor_id).fetch_one(&pool).await?;
        assert_eq!(still_pending, "pending");
        sqlx::query("UPDATE public.news_articles SET full_text=$2 WHERE id=$1")
            .bind(ARTICLE)
            .bind(body)
            .execute(&pool)
            .await?;
        assert!(matches!(
            execute_inner(&pool, &claim, Some(&Rejected)).await?,
            Some(PluginOutcome::Deferred { .. })
        ));
        let rejected: String = sqlx::query_scalar(
            "SELECT status FROM public.harvester_insider_identity_reviews WHERE transfer_rumor_id=$1"
        ).bind(rumor_id).fetch_one(&pool).await?;
        assert_eq!(rejected, "rejected");
        let current_team: Option<i32> = sqlx::query_scalar(
            "SELECT team_id FROM public.player_current_identity WHERE sport=$1 AND player_id=$2",
        )
        .bind(SPORT)
        .bind(PLAYER)
        .fetch_one(&pool)
        .await?;
        assert_eq!(current_team, Some(OLD));
        Ok(())
    }
}
