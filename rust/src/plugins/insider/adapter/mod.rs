//! Insider publication from verified Harvester source packages.

use crate::application::models::ExecutionCapabilities;
use crate::application::queue::publication::ClaimPublication;
use crate::application::queue::work::Item;
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::PgPool;

pub(crate) mod source;
pub(crate) use source::preview;

pub(crate) const TRANSFER_PUBLISHED: &str = "transfer_published";

pub(crate) async fn record_transfer_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    item: &Item,
    kind: &'static str,
    entity_type: &str,
    entity_id: i32,
    input_version: Option<&str>,
) -> Result<()> {
    crate::application::queue::outbox::record(
        tx,
        item,
        crate::application::queue::outbox::NewEvent {
            kind,
            entity_type,
            entity_id,
            source_input_version: input_version,
        },
    )
    .await
    .context("record transfer publication")
}

pub struct TransferHandler {
    pool: PgPool,
    models: ExecutionCapabilities,
}

impl TransferHandler {
    pub fn new(pool: PgPool, models: ExecutionCapabilities) -> Self {
        Self { pool, models }
    }
}

#[async_trait]
impl StudioPlugin for TransferHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &crate::plugins::insider::manifest::MANIFEST
    }

    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        if item
            .input_version
            .as_deref()
            .is_some_and(|version| version.starts_with("harvest-context-v"))
        {
            return source::execute(&self.pool, &self.models, item).await;
        }
        // Retire work created by the old periodic pipeline.
        let Some(publication) = ClaimPublication::begin(&self.pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        publication.commit_final().await?;
        Ok(PluginOutcome::Committed)
    }
}
