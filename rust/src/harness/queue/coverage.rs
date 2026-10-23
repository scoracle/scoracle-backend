//! The shared coverage clock runs independently of model work and queue emptiness.
use sqlx::PgPool;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tracing::{error, info};

pub(super) async fn tick(pool: &PgPool, at: Option<&str>) -> anyhow::Result<String> {
    let mut retries = 2;
    loop {
        let result: anyhow::Result<String> = async {
            let mut tx = pool.begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
                .execute(&mut *tx)
                .await?;
            // Cancel on the server before the outer timeout drops the query future.
            sqlx::query("SET LOCAL statement_timeout='110s'")
                .execute(&mut *tx)
                .await?;
            sqlx::query("SET LOCAL lock_timeout='5s'")
                .execute(&mut *tx)
                .await?;
            let (previous, edition, failures_before): (String, Option<String>, i64) = sqlx::query_as(
                "SELECT phase,edition_date::text,(SELECT count(*) FROM coverage_failures f
                 WHERE f.edition_date=c.edition_date) FROM coverage_schedule c",
            )
                .fetch_one(&mut *tx)
                .await?;
            let phase: String = sqlx::query_scalar(
                "SELECT public.coverage_tick(COALESCE($1::text::timestamptz,clock_timestamp()))",
            )
            .bind(at)
            .fetch_one(&mut *tx)
            .await?;
            if phase == "characters" {
                if previous != "characters" {
                    let headlines: Vec<(i64, String)> = sqlx::query_as(
                        "SELECT DISTINCT x.id,x.clean_text FROM coverage_assignments a
                        JOIN coverage_schedule c USING(edition_date)
                        JOIN classifier_spectra x ON x.id=ANY(a.spectrum_ids)
                        JOIN classifier_spectrum_deliveries d ON d.spectrum_id=x.id
                        WHERE c.enabled AND c.phase='characters' AND d.status='pending'",
                    )
                    .fetch_all(&mut *tx)
                    .await?;
                    let spam: Vec<i64> = headlines
                        .into_iter()
                        .filter(|(_, title)| crate::tools::source::is_headline_spam(title))
                        .map(|(id, _)| id)
                        .collect();
                    sqlx::query("SELECT public.coverage_select_headlines($1)")
                        .bind(spam)
                        .execute(&mut *tx)
                        .await?;
                }
                crate::plugins::scout::snapshot::freeze(&mut tx).await?;
            }
            let failures_after: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM coverage_failures WHERE edition_date=$1::text::date",
            )
            .bind(&edition)
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            if failures_after > failures_before {
                error!(edition=?edition, new_failures=failures_after-failures_before,
                    total_failures=failures_after, "terminal coverage failures retained for reporting; partial editions will reopen collection");
            }
            Ok(phase)
        }
        .await;
        let conflict = result
            .as_ref()
            .err()
            .and_then(|error| error.downcast_ref::<sqlx::Error>())
            .and_then(sqlx::Error::as_database_error)
            .is_some_and(|error| error.code().as_deref() == Some("40001"));
        if conflict && retries > 0 {
            retries -= 1;
            continue;
        }
        return result;
    }
}

pub(super) fn start(pool: PgPool, shutdown: Arc<AtomicBool>) -> anyhow::Result<()> {
    match std::env::var("COGNITION_COVERAGE_CONTROLLER").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("false") => return Ok(()),
        Ok("true") => {}
        _ => anyhow::bail!("COGNITION_COVERAGE_CONTROLLER must be true or false"),
    }
    tokio::spawn(async move {
        let mut previous = String::new();
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if shutdown.load(Ordering::Acquire) {
                return;
            }
            match tokio::time::timeout(Duration::from_secs(120), tick(&pool, None)).await {
                Ok(Ok(phase)) => {
                    if phase != previous {
                        info!(phase, "coverage controller phase");
                        previous = phase;
                    }
                }
                Ok(Err(e)) => error!(error = %e, "coverage tick failed; durable phase retained"),
                Err(_) => error!("coverage tick timed out; durable phase retained"),
            }
        }
    });
    Ok(())
}
