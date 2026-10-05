//! Operator compatibility entry point. Reporting-based canonical extraction is unavailable.
use anyhow::{bail, ensure, Result};

#[derive(Clone, Debug)]
pub struct FactsweepRequest {
    pub sport: String,
    pub limit: i64,
    pub person: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FactsweepRunContext {
    Commit,
    DryRun,
}

/// Refuse before any retrieval, inference, cooldown stamp or canonical write.
/// Exact quote containment does not establish a current role or affiliation.
pub fn run_factsweep(request: &FactsweepRequest, _context: FactsweepRunContext) -> Result<()> {
    ensure!(
        !request.sport.trim().is_empty(),
        "factsweep sport is required"
    );
    ensure!(request.limit >= 0, "factsweep limit cannot be negative");
    bail!("factsweep unavailable: no evaluated extractor for current role or affiliation; canonical facts were not changed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_operator_modes_refuse_without_database_or_inference() {
        let request = FactsweepRequest {
            sport: "FOOTBALL".into(),
            limit: 60,
            person: Some(57),
        };
        for context in [FactsweepRunContext::Commit, FactsweepRunContext::DryRun] {
            assert!(run_factsweep(&request, context)
                .unwrap_err()
                .to_string()
                .contains("no evaluated extractor"));
        }
    }
}
