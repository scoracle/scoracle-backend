//! Operator compatibility entry point. Reporting-based canonical extraction is unavailable.
use anyhow::{bail, Result};

/// Refuse before any retrieval, inference, cooldown stamp or canonical write.
/// Exact quote containment does not establish a current role or affiliation.
pub fn run_factsweep() -> Result<()> {
    bail!("factsweep unavailable: no evaluated extractor for current role or affiliation; canonical facts were not changed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_without_database_or_inference() {
        assert!(run_factsweep()
            .unwrap_err()
            .to_string()
            .contains("no evaluated extractor"));
    }
}
