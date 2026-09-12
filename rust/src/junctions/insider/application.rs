//! Applies vetted transfer facts to identity and downstream bookkeeping.

use super::{
    build_transfer_identity_adjudication_prompt, transfer_identity_adjudication_system_prompt,
    NewsItem, Outcome, TransferCandidate, TransferIdentityAdjudicationParser, TransferRow,
    TRANSFER_IDENTITY_ADJUDICATION_PROMPT_VERSION, TRANSFER_IDENTITY_ADJUDICATION_SCHEMA_RAW,
    TRANSFER_PROMPT_VERSION,
};
use crate::harness::Harness;
use crate::ollama::GenerateOptions;
use crate::route::Role;
use anyhow::{anyhow, Context, Result};
use sqlx::{PgPool, Row};
use tracing::warn;

#[derive(Clone, Debug)]
pub(super) struct TransferIdentityThreshold {
    min_heat: i16,
    min_deterministic_confidence: f64,
}

/// Bank a served verdict as a junction-origin narrative event. Extraction-only feedback queries
/// cannot use these events as corroboration.
pub(super) async fn bank_transfer_junction_event(
    pool: &PgPool,
    sport: &str,
    player_id: i32,
    team_id: i32,
    stage: &str,
    model: &str,
    news_ids: &[i64],
) -> Result<()> {
    let Some(anchor) = news_ids.iter().copied().min() else {
        return Ok(());
    };
    let (predicate, confidence) = match stage {
        "here_we_go" => ("trade_confirmed", "confirmed"),
        "advanced_talks" | "concrete_interest" => ("trade_rumor", "reported"),
        _ => ("trade_rumor", "speculative"),
    };
    sqlx::query(
        r#"
        INSERT INTO narrative_events
            (sport, subject_type, subject_id, predicate, object_type, object_id,
             sentiment, confidence, article_id, event_date, source, model_version,
             prompt_version, origin)
        SELECT $1, 'player', $2, $3, 'team', $4,
               NULL, $5, a.id, NOW(), a.source, $6, $7, 'junction'
        FROM news_articles a WHERE a.id = $8
        ON CONFLICT (article_id, sport, subject_type, subject_id, predicate,
                     COALESCE(object_type, ''), COALESCE(object_id, 0), origin)
        DO UPDATE SET confidence = EXCLUDED.confidence, event_date = NOW(),
                      model_version = EXCLUDED.model_version, extracted_at = NOW()
        "#,
    )
    .bind(sport)
    .bind(player_id)
    .bind(predicate)
    .bind(team_id)
    .bind(confidence)
    .bind(model)
    .bind(TRANSFER_PROMPT_VERSION)
    .bind(anchor)
    .execute(pool)
    .await
    .context("bank transfer junction event")?;
    Ok(())
}

pub(super) async fn load_transfer_identity_threshold(
    pool: &PgPool,
    sport: &str,
) -> Result<Option<TransferIdentityThreshold>> {
    let row = sqlx::query(
        r#"
        SELECT min_heat,
               min_deterministic_confidence::float8 AS min_deterministic_confidence
        FROM public.transfer_identity_thresholds
        WHERE sport = $1
        "#,
    )
    .bind(sport)
    .fetch_optional(pool)
    .await
    .context("load transfer identity threshold")?;

    Ok(row.map(|r| TransferIdentityThreshold {
        min_heat: r.get("min_heat"),
        min_deterministic_confidence: r.get("min_deterministic_confidence"),
    }))
}

pub(super) fn identity_apply_deterministic_score(heat: i16) -> (i16, f64) {
    (heat, f64::from(heat) / 100.0)
}

async fn current_identity_team(
    pool: &PgPool,
    sport: &str,
    player_id: i32,
) -> Result<(Option<i32>, String)> {
    let row = sqlx::query(
        r#"
        SELECT pci.team_id, COALESCE(t.name, '') AS team_name
        FROM public.player_current_identity pci
        LEFT JOIN public.teams t ON t.id = pci.team_id AND t.sport = pci.sport
        WHERE pci.sport = $1 AND pci.player_id = $2
        "#,
    )
    .bind(sport)
    .bind(player_id)
    .fetch_one(pool)
    .await
    .context("load current identity for transfer apply")?;

    Ok((row.get("team_id"), row.get("team_name")))
}

#[allow(clippy::too_many_arguments)]
async fn record_transfer_identity_failure(
    pool: &PgPool,
    sport: &str,
    player_id: i32,
    old_team_id: Option<i32>,
    new_team_id: i32,
    source_rumor_id: i64,
    deterministic_heat: i16,
    deterministic_confidence: f64,
    raw: &str,
    model: &str,
    reason: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        SELECT public.record_transfer_identity_adjudication_failure(
            $1,$2,$3,$4,$5,NULL,$6,$7::float8::numeric,$8,$9,$10,$11
        )
        "#,
    )
    .bind(sport)
    .bind(player_id)
    .bind(old_team_id)
    .bind(new_team_id)
    .bind(source_rumor_id)
    .bind(deterministic_heat)
    .bind(deterministic_confidence)
    .bind(raw)
    .bind(model)
    .bind(TRANSFER_IDENTITY_ADJUDICATION_PROMPT_VERSION)
    .bind(reason)
    .execute(pool)
    .await
    .context("record transfer identity adjudication failure")?;
    Ok(())
}

fn autofill_view_for_sport(sport: &str) -> Result<&'static str> {
    match sport {
        "NBA" => Ok("nba.autofill_entities"),
        "NFL" => Ok("nfl.autofill_entities"),
        "FOOTBALL" => Ok("football.autofill_entities"),
        _ => Err(anyhow!("unsupported sport for autofill refresh: {sport}")),
    }
}

pub(super) async fn refresh_sport_autofill_concurrently(
    pool: &PgPool,
    sport: &str,
    reason: &str,
) -> Result<()> {
    let view = autofill_view_for_sport(sport)?;
    sqlx::query("SELECT public.request_sport_autofill_refresh($1, $2)")
        .bind(sport)
        .bind(reason)
        .execute(pool)
        .await
        .context("mark sport autofill refreshing")?;

    if let Err(err) = sqlx::query(&format!("REFRESH MATERIALIZED VIEW CONCURRENTLY {view}"))
        .execute(pool)
        .await
    {
        let _ = sqlx::query("SELECT public.fail_sport_autofill_refresh($1, $2)")
            .bind(sport)
            .bind(err.to_string())
            .execute(pool)
            .await;
        return Err(err).context("refresh sport autofill concurrently");
    }

    let total: i32 = match sqlx::query_scalar(&format!("SELECT COUNT(*)::int FROM {view}"))
        .fetch_one(pool)
        .await
    {
        Ok(total) => total,
        Err(err) => {
            let _ = sqlx::query("SELECT public.fail_sport_autofill_refresh($1, $2)")
                .bind(sport)
                .bind(err.to_string())
                .execute(pool)
                .await;
            return Err(err).context("count refreshed sport autofill entities");
        }
    };

    sqlx::query("SELECT public.complete_sport_autofill_refresh($1, $2, $3)")
        .bind(sport)
        .bind(total)
        .bind(reason)
        .execute(pool)
        .await
        .context("complete sport autofill refresh")?;
    Ok(())
}

async fn sport_autofill_refresh_pending(pool: &PgPool, sport: &str) -> Result<bool> {
    let pending: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT status <> 'ready' FROM public.sport_autofill_versions WHERE sport = $1), false)",
    )
    .bind(sport)
    .fetch_one(pool)
    .await
    .context("check sport autofill refresh status")?;
    Ok(pending)
}

/// Apply an eligible transfer and report whether the team drain should refresh autofill.
#[allow(clippy::too_many_arguments)]
pub(super) async fn maybe_apply_transfer_identity(
    hx: &Harness,
    team_id: i32,
    team_name: &str,
    candidate: &TransferCandidate,
    sport: &str,
    heat: i16,
    news: &[NewsItem],
    persisted_rumor_id: i64,
    row: &TransferRow,
    outcome: Outcome,
    threshold: Option<&TransferIdentityThreshold>,
) -> Result<bool> {
    if outcome != Outcome::Rumor || row.is_rumor != Some(true) {
        return Ok(false);
    }
    if row.direction.as_deref() != Some("incoming") {
        return Ok(false);
    }

    let (identity_heat, deterministic_confidence) = identity_apply_deterministic_score(heat);
    let Some(threshold) = threshold else {
        warn!(
            sport,
            "transfers: missing identity threshold config; skipping apply"
        );
        return Ok(false);
    };
    if identity_heat < threshold.min_heat
        || deterministic_confidence < threshold.min_deterministic_confidence
    {
        return Ok(false);
    }

    let (old_team_id, old_team_name) =
        current_identity_team(&hx.pool, sport, candidate.player_id).await?;
    if old_team_id == Some(team_id) {
        return sport_autofill_refresh_pending(&hx.pool, sport).await;
    }

    let prompt = build_transfer_identity_adjudication_prompt(
        sport,
        candidate.player_id,
        &candidate.player_name,
        old_team_id,
        &old_team_name,
        team_id,
        team_name,
        news,
    );
    let opts = GenerateOptions {
        system: Some(transfer_identity_adjudication_system_prompt(sport)),
        temperature: Some(0.0),
        num_predict: 700,
        num_ctx: crate::route::LOCAL_STAGE_NUM_CTX,
        json_mode: false,
        format_schema: Some(
            serde_json::from_str(TRANSFER_IDENTITY_ADJUDICATION_SCHEMA_RAW)
                .expect("identity adjudication schema is valid JSON"),
        ),
        format_schema_raw: Some(TRANSFER_IDENTITY_ADJUDICATION_SCHEMA_RAW.to_string()),
    };
    let backend = hx.router.for_role(Role::EmotionalNews);
    let model_configured = backend.model().to_string();
    let generated = match hx
        .extract(
            Role::EmotionalNews,
            &prompt,
            &opts,
            &TransferIdentityAdjudicationParser,
        )
        .await
    {
        Ok(extracted) => extracted,
        Err(error) => {
            warn!(
                team = team_id,
                player = candidate.player_id,
                error = %error,
                "transfers: identity adjudication generate failed; fail closed"
            );
            record_transfer_identity_failure(
                &hx.pool,
                sport,
                candidate.player_id,
                old_team_id,
                team_id,
                persisted_rumor_id,
                identity_heat,
                deterministic_confidence,
                "",
                &model_configured,
                "identity adjudication generate failed",
            )
            .await?;
            return Ok(false);
        }
    };

    let Some(adjudication) = generated.value else {
        record_transfer_identity_failure(
            &hx.pool,
            sport,
            candidate.player_id,
            old_team_id,
            team_id,
            persisted_rumor_id,
            identity_heat,
            deterministic_confidence,
            &generated.raw_response,
            &generated.model,
            "invalid identity adjudication JSON",
        )
        .await?;
        return Ok(false);
    };

    let raw =
        serde_json::to_string(&adjudication).context("serialize transfer identity adjudication")?;
    let result = sqlx::query(
        r#"
        SELECT application_id, override_id, status, reason
        FROM public.apply_transfer_identity_candidate(
            $1,$2,$3,$4,$5,NULL,$6,$7::float8::numeric,$8::jsonb,$9,$10,$11
        )
        "#,
    )
    .bind(sport)
    .bind(candidate.player_id)
    .bind(old_team_id)
    .bind(team_id)
    .bind(persisted_rumor_id)
    .bind(identity_heat)
    .bind(deterministic_confidence)
    .bind(&raw)
    .bind(&raw)
    .bind(&generated.model)
    .bind(TRANSFER_IDENTITY_ADJUDICATION_PROMPT_VERSION)
    .fetch_one(&hx.pool)
    .await
    .context("apply transfer identity candidate")?;

    let status: String = result.get("status");
    if status != "applied" {
        return Ok(false);
    }

    let application_id: i64 = result.get("application_id");
    let override_id: Option<i64> = result.get("override_id");
    warn!(
        application_id,
        override_id,
        team = team_id,
        player = candidate.player_id,
        "transfers: applied current identity override"
    );
    sqlx::query("SELECT public.request_sport_autofill_refresh($1, $2)")
        .bind(sport)
        .bind("applied_transfer_identity")
        .execute(&hx.pool)
        .await
        .context("mark sport autofill refreshing")?;
    if let Err(error) = crate::junctions::scout::enqueue_rating_for_applied_transfer(
        &hx.pool,
        sport,
        candidate.player_id,
        old_team_id,
        Some(team_id),
        application_id,
    )
    .await
    {
        warn!(
            application_id,
            player = candidate.player_id,
            "transfers: applied move did not reach the Scout: {error:#}"
        );
    }
    Ok(true)
}
