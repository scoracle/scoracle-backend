//! Classifier integration tests use the real migrated schema.
use anyhow::{ensure, Result};

pub(crate) async fn setup_disposable(pool: &sqlx::PgPool) -> Result<()> {
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await?;
    ensure!(
        database.starts_with("classifier_test"),
        "use a disposable classifier_test database"
    );
    let migrated: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version='312_classifier_subject_reader')")
        .fetch_one(pool).await?;
    ensure!(
        migrated,
        "restore sql/schema and apply all migrations before running Classifier tests"
    );
    Ok(())
}
