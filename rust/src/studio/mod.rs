//! Studio is Scoracle's in-house harness: the place where models create from prepared evidence.
//! Applications supply a model and publication adapter. Studio owns neither databases nor queues.

pub mod analyst;
pub mod form;
mod generation;
pub mod guards;
pub mod influencer;
pub mod insider;
pub mod journalist;
pub mod model;
pub mod oracle;
pub mod scout;
mod session;

pub use generation::{Extracted, Generation, GenerationCall, Parser, Provenance};

use anyhow::Result;
use async_trait::async_trait;
use model::{GenerateOptions, Inference};

/// One creation session using an application-selected model.
pub struct Studio<'a> {
    model: &'a dyn Inference,
}

impl<'a> Studio<'a> {
    pub fn new(model: &'a dyn Inference) -> Self {
        Self { model }
    }

    /// Validate creation with the existing bounded surface/incomplete-output rewrites.
    /// Transport failures and durable retry policy remain with the application.
    /// A parser's `None` remains an explicit abstention for the character to handle.
    pub async fn extract<T, P: Parser<T>>(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
        parser: &P,
    ) -> Result<Extracted<T>> {
        session::extract_with_backend(self.model, prompt, opts, parser).await
    }
}

/// The application publishes a validated product and returns its own receipt.
/// Durability, idempotency, and queue completion remain the adapter's responsibility.
#[async_trait]
pub trait Publisher<T: Sync>: Sync {
    type Receipt;

    async fn publish(&self, output: &Generation<T>) -> Result<Self::Receipt>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome<R> {
    NoMaterial,
    Published(R),
}

pub mod editor;
