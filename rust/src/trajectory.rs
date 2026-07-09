//! Shared trajectory vocabulary for Rust cognition stages.
//!
//! This is the Rust home for the narrative/transfer/vibe/sigil trajectory codes
//! and display labels. The Go API SQL has a second home for the same served
//! vocabulary in `go/internal/db/db.go` (CASE labels plus
//! `COALESCE(trajectory, 'developing_story')`); update both homes if these codes
//! ever change.

pub const DEFAULT_TRAJECTORY: &str = "developing_story";

pub fn trajectory_label(raw: &str) -> &'static str {
    match raw {
        "heating_up" => "Heating up",
        "cooling_off" => "Cooling off",
        _ => "Developing story...",
    }
}

pub fn classify_delta(
    previous: Option<i32>,
    current: Option<i32>,
) -> (&'static str, &'static str, Option<i32>) {
    match (previous, current) {
        (Some(prev), Some(cur)) => {
            let delta = cur - prev;
            if delta >= 10 {
                ("heating_up", "up", Some(delta))
            } else if delta <= -10 {
                ("cooling_off", "down", Some(delta))
            } else {
                (DEFAULT_TRAJECTORY, "stable", Some(delta))
            }
        }
        _ => (DEFAULT_TRAJECTORY, "new_or_unmatched", None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_match_served_vocabulary() {
        assert_eq!(trajectory_label("heating_up"), "Heating up");
        assert_eq!(trajectory_label("cooling_off"), "Cooling off");
        assert_eq!(trajectory_label(DEFAULT_TRAJECTORY), "Developing story...");
    }

    #[test]
    fn classify_delta_uses_ten_point_buckets() {
        assert_eq!(
            classify_delta(Some(40), Some(52)),
            ("heating_up", "up", Some(12))
        );
        assert_eq!(
            classify_delta(Some(52), Some(40)),
            ("cooling_off", "down", Some(-12))
        );
        assert_eq!(
            classify_delta(Some(40), Some(45)),
            (DEFAULT_TRAJECTORY, "stable", Some(5))
        );
        assert_eq!(
            classify_delta(None, Some(45)),
            (DEFAULT_TRAJECTORY, "new_or_unmatched", None)
        );
    }
}
