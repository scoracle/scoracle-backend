//! Application-selected model routes and generation limits. No storage or queue access.
use crate::runtime::route::{RouteKey, Router};
use crate::studio::model::Inference;
use crate::studio::plugin::{PluginManifest, ToolGrant};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

pub struct Models {
    pub router: Router,
    /// Cooperative ceiling for multi-call assignments; zero means unbounded.
    pub handler_budget: Duration,
    pub voice_num_ctx: i32,
}

/// Capabilities resolved for one plugin at composition time. Unlike [`Models`], this
/// value cannot discover another plugin's route or reach the global router.
#[derive(Clone)]
pub struct ExecutionCapabilities {
    inference: HashMap<RouteKey, Arc<dyn Inference>>,
    handler_budget: Duration,
    pub voice_num_ctx: i32,
}

/// Absolute timing scope for one plugin invocation. A zero configured budget means
/// unbounded and therefore carries no deadline.
#[derive(Clone, Copy, Debug)]
pub struct RunDeadline {
    started: std::time::Instant,
    budget: Duration,
}

impl RunDeadline {
    pub(crate) fn starting_at(started: std::time::Instant, budget: Duration) -> Self {
        Self { started, budget }
    }

    pub fn deadline(self) -> Option<std::time::Instant> {
        (!self.budget.is_zero()).then(|| self.started + self.budget)
    }

    pub fn fraction(self, fraction: f64) -> Option<std::time::Instant> {
        (!self.budget.is_zero()).then(|| self.started + self.budget.mul_f64(fraction))
    }

    pub fn elapsed(self) -> Duration {
        self.started.elapsed()
    }
}

impl ExecutionCapabilities {
    pub fn inference(&self, route: RouteKey) -> Result<Arc<dyn Inference>> {
        self.inference.get(&route).cloned().ok_or_else(|| {
            anyhow!(
                "inference capability for route '{}' was not supplied",
                route.as_str()
            )
        })
    }

    pub fn begin_run(&self) -> RunDeadline {
        RunDeadline::starting_at(std::time::Instant::now(), self.handler_budget)
    }
}

impl Models {
    /// Construct the exact inference surface declared by a plugin manifest.
    /// Declaration checks happen here, where concrete handles cross into plugin code.
    pub fn capabilities(&self, manifest: &PluginManifest) -> Result<ExecutionCapabilities> {
        if manifest.inference_routes.is_empty() {
            anyhow::ensure!(
                !manifest.tools.contains(&ToolGrant::Inference),
                "{} declares inference without a route",
                manifest.id
            );
        } else {
            anyhow::ensure!(
                manifest.tools.contains(&ToolGrant::Inference),
                "{} declares routes without inference capability",
                manifest.id
            );
        }
        let mut inference = HashMap::new();
        for &route in manifest.inference_routes {
            inference.insert(route, self.router.for_route(route));
        }
        Ok(ExecutionCapabilities {
            inference,
            handler_budget: self.handler_budget,
            voice_num_ctx: self.voice_num_ctx,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::config::{Backend, ModelSpec, RouteConfig};

    fn models() -> Models {
        let roles = crate::application::fleet::inference_routes()
            .into_iter()
            .map(|route| {
                (
                    route,
                    ModelSpec {
                        backend: Backend::Ollama,
                        model: "test-model".to_string(),
                        base_url: "http://127.0.0.1:1".to_string(),
                        think: None,
                    },
                )
            })
            .collect();
        Models {
            router: Router::from_config(
                &RouteConfig {
                    roles,
                    candidates: HashMap::new(),
                    backend_concurrency: HashMap::new(),
                },
                Duration::from_secs(1),
                1,
            )
            .unwrap(),
            handler_budget: Duration::from_secs(30),
            voice_num_ctx: 4096,
        }
    }

    #[test]
    fn production_capability_scope_refuses_another_plugins_route() {
        let capabilities = models()
            .capabilities(&crate::plugins::editor::manifest::MANIFEST)
            .unwrap();
        assert!(capabilities
            .inference(crate::plugins::editor::manifest::ROUTE)
            .is_ok());
        let error = capabilities
            .inference(crate::plugins::scout::manifest::ROUTE)
            .err()
            .expect("undeclared route must be absent");
        assert!(error.to_string().contains("was not supplied"));
    }

    #[test]
    fn insider_receives_its_declared_adjudication_route() {
        let capabilities = models()
            .capabilities(&crate::plugins::insider::manifest::MANIFEST)
            .unwrap();
        assert!(capabilities
            .inference(crate::plugins::insider::manifest::ROUTE)
            .is_ok());
        assert!(capabilities
            .inference(crate::plugins::graph::manifest::ROUTE)
            .is_ok());
    }
}
