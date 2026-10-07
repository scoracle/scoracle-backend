//! Operator entry point for the Investigator's non-queue person-metadata sweep.

use anyhow::{anyhow, ensure, Result};
use scoracle_cognition::plugins::investigator::adapter::run_factsweep;

fn parse_args(mut it: impl Iterator<Item = String>) -> Result<()> {
    let mut sport = String::new();
    let mut limit: i64 = 60;
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "-sport" => sport = it.next().ok_or_else(|| anyhow!("-sport needs a value"))?,
            "-limit" => {
                limit = it
                    .next()
                    .ok_or_else(|| anyhow!("-limit needs a value"))?
                    .parse()?
            }
            "-person" => {
                it.next()
                    .ok_or_else(|| anyhow!("-person needs a value"))?
                    .parse::<i32>()?;
            }
            "-dry-run" => {}
            other => return Err(anyhow!("unknown flag {other:?}")),
        }
    }
    if sport.is_empty() {
        return Err(anyhow!("-sport is required"));
    }
    ensure!(!sport.trim().is_empty(), "factsweep sport is required");
    ensure!(limit >= 0, "factsweep limit cannot be negative");
    Ok(())
}

fn main() -> Result<()> {
    parse_args(std::env::args().skip(1))?;
    run_factsweep()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_compatibility_flags_and_refuses_both_modes() {
        for flags in [
            vec!["-sport", "football", "-person", "57"],
            vec!["-sport", "football", "-person", "57", "-dry-run"],
        ] {
            parse_args(flags.into_iter().map(str::to_owned)).unwrap();
            assert!(run_factsweep()
                .unwrap_err()
                .to_string()
                .contains("no evaluated extractor"));
        }
        for flags in [
            vec![],
            vec!["-sport"],
            vec!["-sport", " "],
            vec!["-sport", "football", "-limit", "-1"],
            vec!["-sport", "football", "-limit", "bad"],
            vec!["-sport", "football", "-person", "bad"],
            vec!["-sport", "football", "-person", "2147483648"],
            vec!["-sport", "football", "-unknown"],
        ] {
            assert!(parse_args(flags.into_iter().map(str::to_owned)).is_err());
        }
    }
}
