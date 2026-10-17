//! Freeze Scout's selected measurements in the edition-opening transaction.
use super::prompt::{build_rating_request_on, RatingBuild, RatingReq, RATING_TEMPERATURE};
use crate::harness::queue::work::Item;
use anyhow::{ensure, Context, Result};
use sqlx::{PgConnection, PgPool};

pub(crate) async fn freeze(connection: &mut PgConnection) -> Result<()> {
    let subjects: Vec<(String, i32, String, Option<i32>, String)> = sqlx::query_as(
        "SELECT a.entity_type,a.entity_id::int,a.sport,s.current_season,
            COALESCE(t.name,p.name,a.entity_type||' '||a.entity_id)
        FROM public.coverage_assignments a JOIN public.coverage_schedule c USING(edition_date)
        LEFT JOIN public.sports s ON s.id=a.sport
        LEFT JOIN public.teams t ON a.entity_type='team' AND t.id=a.entity_id AND t.sport=a.sport
        LEFT JOIN public.players p ON a.entity_type='player' AND p.id=a.entity_id AND p.sport=a.sport
        WHERE c.enabled AND c.phase='characters' AND a.stage='rating' AND a.prepared_input IS NULL",
    ).fetch_all(&mut *connection).await?;
    for (entity_type, entity_id, sport, season, entity_name) in subjects {
        let req = RatingReq {
            entity_type,
            entity_id,
            sport,
            season,
            entity_name,
            trigger_type: "daily".into(),
        };
        let prepared =
            build_rating_request_on(connection, 0, &req, RATING_TEMPERATURE, false).await?;
        sqlx::query(
            "UPDATE public.coverage_assignments a SET prepared_input=$4::text::jsonb
            FROM public.coverage_schedule c WHERE a.edition_date=c.edition_date
            AND a.stage='rating' AND a.entity_type=$1 AND a.entity_id=$2 AND a.sport=$3",
        )
        .bind(&req.entity_type)
        .bind(req.entity_id)
        .bind(&req.sport)
        .bind(serde_json::to_string(&prepared)?)
        .execute(&mut *connection)
        .await?;
    }
    Ok(())
}

pub(crate) async fn load(pool: &PgPool, item: &Item, num_ctx: i32) -> Result<Option<RatingBuild>> {
    let (enabled, prepared): (bool, Option<String>) = sqlx::query_as(
        "SELECT c.enabled,a.prepared_input::text FROM public.coverage_schedule c
        LEFT JOIN public.coverage_assignments a ON a.edition_date=c.edition_date
            AND a.stage='rating' AND a.entity_type=$1 AND a.entity_id=$2 AND a.sport=$3
            AND a.input_version IS NOT DISTINCT FROM $4::text",
    )
    .bind(&item.entity_type)
    .bind(item.entity_id)
    .bind(&item.sport)
    .bind(&item.input_version)
    .fetch_one(pool)
    .await?;
    if !enabled {
        return Ok(None);
    }
    let mut prepared: RatingBuild =
        serde_json::from_str(&prepared.context("daily Scout measurements were not frozen")?)?;
    if let RatingBuild::Ready(assignment) = &mut prepared {
        ensure!(assignment.built_prompt == assignment.parts.render(), "frozen Scout prompt contract changed; finish or expire this edition before deploying a new contract");
        assignment.opts = assignment
            .parts
            .generation_options(num_ctx, RATING_TEMPERATURE);
    }
    Ok(Some(prepared))
}
