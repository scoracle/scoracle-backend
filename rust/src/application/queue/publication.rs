//! Host-owned claim-fenced publication transactions.
//!
//! Plugins prepare model and network results before entering this boundary. The host
//! opens the transaction, locks the exact lease and input revision, and coordinates
//! either a durable progress checkpoint or final claim completion. Plugin adapters
//! receive a borrowed transaction for their domain SQL; they cannot commit it.

use crate::application::queue::work::{self, Item};
use anyhow::{ensure, Context, Result};
use sqlx::{PgPool, Postgres, Transaction};

/// A claim-fenced publication transaction owned by the durable host.
pub(crate) struct ClaimPublication<'a> {
    tx: Transaction<'a, Postgres>,
    item: &'a Item,
}

/// Durable evidence that a progress checkpoint committed under the exact claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProgressReceipt(());

/// Durable evidence that the exact claim and all required effects committed together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FinalReceipt(());

impl<'a> ClaimPublication<'a> {
    /// Begin a short publication transaction and fence it to the exact lease and input
    /// revision. `None` means the claim was superseded before any domain effect ran.
    pub(crate) async fn begin(pool: &'a PgPool, item: &'a Item) -> Result<Option<Self>> {
        let mut tx = pool.begin().await.context("begin claim publication")?;
        if !work::lock_claim(&mut tx, item).await? {
            tx.rollback()
                .await
                .context("close superseded claim publication")?;
            return Ok(None);
        }
        Ok(Some(Self { tx, item }))
    }

    /// Borrow the transaction for plugin-owned domain SQL. Publication coordination
    /// remains with the host because a borrow cannot commit or roll back the transaction.
    pub(crate) fn transaction(&mut self) -> &mut Transaction<'a, Postgres> {
        &mut self.tx
    }

    /// Commit a durable checkpoint while leaving the exact claim active. The worker may
    /// subsequently defer it; a later failure does not erase this committed progress.
    pub(crate) async fn commit_progress(self) -> Result<ProgressReceipt> {
        self.tx
            .commit()
            .await
            .context("commit claim progress publication")?;
        Ok(ProgressReceipt(()))
    }

    /// Atomically commit plugin effects and completion of the exact claim.
    pub(crate) async fn commit_final(mut self) -> Result<FinalReceipt> {
        ensure!(
            work::complete_in_transaction(&mut self.tx, self.item).await?,
            "claim changed while its publication transaction held the row lock"
        );
        self.tx
            .commit()
            .await
            .context("commit final claim publication")?;
        Ok(FinalReceipt(()))
    }
}
