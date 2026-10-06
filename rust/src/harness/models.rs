//! Application-selected model routes and generation limits. No storage or queue access.
use crate::harness::model::Inference;
use crate::harness::plugin::{PluginManifest, ToolGrant};
use crate::harness::route::{RouteKey, Router};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;

pub struct Models {
    pub router: Router,
    pub voice_num_ctx: i32,
}

/// Capabilities resolved for one plugin at composition time. Unlike [`Models`], this
/// value cannot discover another plugin's route or reach the global router.
#[derive(Clone)]
pub struct ExecutionCapabilities {
    inference: HashMap<RouteKey, Arc<dyn Inference>>,
    pub voice_num_ctx: i32,
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
            voice_num_ctx: self.voice_num_ctx,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::config::{Backend, ModelSpec, RouteConfig};
    use std::time::Duration;

    fn models() -> Models {
        let roles = crate::harness::fleet::inference_routes()
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
            voice_num_ctx: 4096,
        }
    }

    #[test]
    fn production_capability_scope_refuses_another_plugins_route() {
        let capabilities = models()
            .capabilities(&crate::plugins::analyst::manifest::MANIFEST)
            .unwrap();
        assert!(capabilities
            .inference(crate::plugins::analyst::manifest::ROUTE)
            .is_ok());
        let error = capabilities
            .inference(crate::plugins::scout::manifest::ROUTE)
            .err()
            .expect("undeclared route must be absent");
        assert!(error.to_string().contains("was not supplied"));
    }

    #[test]
    fn insider_receives_only_its_single_route() {
        let capabilities = models()
            .capabilities(&crate::plugins::insider::manifest::MANIFEST)
            .unwrap();
        assert!(capabilities
            .inference(crate::plugins::insider::manifest::ROUTE)
            .is_ok());
        assert!(capabilities
            .inference(crate::plugins::graph::manifest::ROUTE)
            .is_err());
    }
}
