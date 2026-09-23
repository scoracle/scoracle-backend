//! Studio is Scoracle's in-house harness: the place where models create from prepared evidence.
//! Applications supply a model and publication adapter. Studio owns neither databases nor queues.

mod generation;
pub mod model;
pub mod plugin;
mod session;
pub mod tools;

pub use generation::{Extracted, Generation, GenerationCall, Parser, Provenance};

use anyhow::Result;
use model::{GenerateOptions, Inference};

/// One creation session using an application-selected model.
pub struct Studio<'a> {
    model: &'a dyn Inference,
}

impl<'a> Studio<'a> {
    pub fn new(model: &'a dyn Inference) -> Self {
        Self { model }
    }

    /// Configured model identity for an uncalled product marker.
    pub fn model_name(&self) -> &str {
        self.model.model()
    }

    /// Validate creation with the existing bounded surface/incomplete-output rewrites.
    /// Transport failures and durable retry policy remain with the application.
    /// A parser's `None` remains an explicit abstention for the character to handle.
    pub async fn extract<T, P: Parser<T>>(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
        parser: &P,
        correction: fn(&anyhow::Error) -> Option<String>,
    ) -> Result<Extracted<T>> {
        session::extract_with_backend(self.model, prompt, opts, parser, correction).await
    }
}
