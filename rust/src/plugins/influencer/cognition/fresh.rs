//! Fresh source presentation. Durable IDs and classification stay in provenance.
use crate::plugins::harvester::delivery::SourceContext;
use crate::plugins::support::source::Reporting;

pub(super) fn prepare(source: &SourceContext) -> Reporting<'_> {
    Reporting::new(&source.source, source.published_at_epoch, &source.context)
}

/// Source-owned title; articulation cannot invent a headline claim.
pub(super) fn title(source: &SourceContext, entity_name: &str) -> String {
    let headline = source.headline.trim();
    if !headline.is_empty()
        && headline.chars().count() <= crate::plugins::support::form::HOOK_MAX_CHARS
    {
        headline.to_string()
    } else {
        entity_name.to_string()
    }
}

/// Model-selected read, scoped to the application-assigned subject.
pub(super) async fn read(
    pool: &sqlx::PgPool,
    subject: &crate::plugins::meta::EntityMeta,
) -> anyhow::Result<serde_json::Value> {
    let sources = crate::plugins::harvester::delivery::load_for_character(
        pool,
        crate::plugins::influencer::manifest::MANIFEST.id.as_str(),
        &subject.entity_type,
        subject.entity_id,
        &subject.sport,
    )
    .await?;
    let Some(source) = sources.last() else {
        return Ok(serde_json::json!({"status":"unavailable","reason":"no_pending_source"}));
    };
    let now = crate::plugins::influencer::adapter::harvester::now();
    if let Some(reason) = super::source_disposition(
        &source.context,
        source.published_at_epoch.unwrap_or(now),
        now,
    ) {
        return Ok(serde_json::json!({"status":"unavailable","reason":reason}));
    }
    Ok(serde_json::json!({
        "status":"available", "article_id":source.article_id,
        "source":prepare(source)
    }))
}
