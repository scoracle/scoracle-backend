//! Fresh Classifier reporting for Scout previews.
use anyhow::Result;
use sqlx::PgPool;

/// Fresh Classifier obligations are the reporting part of the daily package.
pub async fn load_scout_reports(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<crate::tools::source::SourceContext>> {
    crate::plugins::classifier::delivery::load_for_character(
        pool,
        super::manifest::MANIFEST.id.as_str(),
        entity_type,
        entity_id,
        sport,
    )
    .await
}
