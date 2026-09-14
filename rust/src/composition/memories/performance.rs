//! Comparable production keeps its measurement identity and denominator. This
//! prepares evidence for a role comparison; it never assigns a role or verdict.

use serde_json::{json, Value};

pub(super) fn with_rates(mut snapshot: Value, sport: &str, entity_type: &str) -> Value {
    // Football's cumulative counts share the minutes in the same source row.
    // NBA provider values may already be per-game: never normalize them again.
    if sport != "FOOTBALL" || entity_type != "player" {
        return snapshot;
    }
    let minutes = snapshot["recorded_sample"]["minutes_played"]
        .as_f64()
        .filter(|n| n.is_finite() && *n > 0.0);
    if let Some(measures) = snapshot["measurements"].as_array_mut() {
        for measure in measures {
            if measure["unit"] == "cumulative_total" {
                let rate = measure["value"]
                    .as_f64()
                    .zip(minutes)
                    .filter(|(v, _)| v.is_finite() && *v >= 0.0)
                    .map(|(v, m)| (v * 90.0 / m * 100.0).round() / 100.0);
                measure["per_90_recorded_minutes"] = json!(rate);
            }
        }
    }
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_production_on_equal_denominators_without_inventing_missing_goals() {
        let source = json!({"recorded_sample":{"minutes_played":900},"measurements":[
            {"measure":"goals","unit":"cumulative_total","value":5},
            {"measure":"assists","unit":"cumulative_total","value":null}
        ]});
        let rendered = with_rates(source.clone(), "FOOTBALL", "player");
        assert_eq!(rendered["measurements"][0]["per_90_recorded_minutes"], 0.5);
        assert!(rendered["measurements"][1]["per_90_recorded_minutes"].is_null());
        assert_eq!(rendered["measurements"][0]["value"], 5);
        assert_eq!(with_rates(source.clone(), "NBA", "player"), source);
    }

    #[test]
    fn zero_or_missing_minutes_never_yield_a_rate() {
        for minutes in [Value::Null, json!(0), json!(-90)] {
            let result = with_rates(
                json!({"recorded_sample":{"minutes_played":minutes},"measurements":[{"measure":"goals","unit":"cumulative_total","value":2}]}),
                "FOOTBALL",
                "player",
            );
            assert!(result["measurements"][0]["per_90_recorded_minutes"].is_null());
        }
    }
}
