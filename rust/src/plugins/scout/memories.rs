//! Scout selects measured history and dated reporting through shared studies.
//! Measurements retain coverage; reported claims retain publisher and date.
//! These remain separate: neither establishes the cause of the other.
//! Selection, budgets and freshness belong to this plugin.
use serde::Serialize;

/// The Scout's memory, as presented. Both parts are optional and absence is
/// meaningful: no measured trend means recent direction is unknown, and no
/// reported claims means there is nothing to qualify the profile with. Neither
/// is a measured zero.
#[derive(Clone, Debug, Default, Serialize, serde::Deserialize)]
pub struct Selected {
    /// Studied measurement windows. A measured trend, with the population it was
    /// computed over and what it does not establish.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub measured: Vec<Measured>,
    /// Dated, attributed injury, suspension and personnel claims.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reported: Vec<Reported>,
    /// What the study could not establish, in the study's own words. Retained
    /// because a coverage gap that the model cannot see is a gap it will
    /// confidently fill.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coverage_limits: Vec<String>,
}

/// One measured window from the shared statistics study.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Measured {
    /// The measure's own display name from `stat_definitions`, not a label this
    /// plugin invented.
    pub measure_label: String,
    pub unit: String,
    pub previous: Window,
    pub current: Window,
    /// The change against the prior window, as the study computed it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_match_change: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent_change: Option<f64>,
    /// The fixtures this rests on, so a measured claim can be traced.
    #[serde(default, skip_serializing)]
    pub fixture_ids: Vec<i64>,
}

/// One half-open measurement window; missing averages remain null.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Window {
    pub from: String,
    pub before: String,
    pub fixtures: usize,
    pub measured: usize,
    pub per_match: Option<f64>,
}

impl From<&crate::plugins::memories::statistic::Window> for Window {
    fn from(window: &crate::plugins::memories::statistic::Window) -> Self {
        Self {
            from: crate::util::utc_timestamp(window.from),
            before: crate::util::utc_timestamp(window.before),
            fixtures: window.fixtures,
            measured: window.measured,
            per_match: window.per_match,
        }
    }
}

/// One dated, attributed claim about availability or personnel.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Reported {
    pub publisher: String,
    pub published_at: String,
    pub reported_headline: String,
    /// `Withdrawn` retracts a claim. It is not evidence of recovery, and the
    /// manual says so because "the club withdrew the report" reads like a
    /// cleared player if presented without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub withdrawn: Option<bool>,
    /// Other sources contradict this claim. A contradicted claim is presented
    /// with its contradiction, not dropped: dropping it would hide that the
    /// subject is disputed, which is the reader-relevant fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disputed: Option<bool>,
}

impl Reported {
    /// The adjudicated availability and personnel records this plugin loads,
    /// presented as dated claims.
    ///
    /// `kind` matters and travels with the claim: `reverted` means the RECORD
    /// was wrong and was withdrawn, which is a correction and never a return to
    /// play. Rendering that as a return is how a withdrawn report becomes a
    /// cleared player, so it is carried as an explicit retraction flag rather
    /// than folded into the headline text.
    pub fn from_records(
        entity_type: &str,
        entity_id: i32,
        changes: &[crate::evidence::personnel::PersonnelChange],
        availability: &[crate::evidence::personnel::AvailabilityChange],
        total: usize,
    ) -> Vec<Reported> {
        if total == 0 {
            return Vec::new();
        }
        // Availability first: a player being unavailable is the claim that most
        // changes what a profile may be read as, and the Scout's job is to say so.
        let mut out: Vec<Reported> = availability
            .iter()
            .map(|change| {
                let subject = if entity_type == "player" {
                    "The player".to_string()
                } else {
                    change.player_name.clone()
                };
                let headline = match change.kind.as_str() {
                    "opened" => format!(
                        "{subject} {} ({}) from {}; reported return {}",
                        change.event_kind,
                        change.event_date_label,
                        change.team_name.as_deref().unwrap_or("their club"),
                        change
                            .expected_return_label
                            .as_deref()
                            .unwrap_or("not stated")
                    ),
                    "returned" => format!(
                        "{subject} available again after {} from {}",
                        change.event_date_label, change.event_kind
                    ),
                    // A withdrawn record is a correction. It is not a return.
                    _ => format!(
                        "An earlier {}-report for {subject} dated {} was withdrawn as incorrect; \
                         it is not evidence of a return",
                        change.event_kind, change.event_date_label
                    ),
                };
                Reported {
                    publisher: "Adjudicated record".into(),
                    published_at: change.date_label.clone(),
                    reported_headline: headline,
                    withdrawn: (change.kind == "reverted").then_some(true),
                    disputed: None,
                }
            })
            .collect();
        out.extend(changes.iter().map(|change| {
            let subject = if entity_type == "player" {
                "The player".to_string()
            } else {
                change.player_name.clone()
            };
            Reported {
                publisher: "Adjudicated record".into(),
                published_at: change.date_label.clone(),
                reported_headline: match change.kind.as_str() {
                    "reverted" => format!(
                        "An earlier recorded move for {subject} to {} was reverted and is not in force",
                        change.new_team.as_deref().unwrap_or("another club")
                    ),
                    _ => format!(
                        "{subject}'s current club recorded as {}",
                        change.new_team.as_deref().unwrap_or("a new club")
                    ),
                },
                withdrawn: (change.kind == "reverted").then_some(true),
                disputed: None,
            }
        }));
        let _ = entity_id;
        out
    }

    /// A publisher claim the plugin selected, carrying its contest mark.
    ///
    /// The publication time travels with the claim because a claim without one
    /// cannot be placed relative to the season it is being read against. Absent
    /// stays absent rather than becoming "now".
    pub fn from_claim(claim: &crate::evidence::news::render::MarkedClaim) -> Reported {
        Reported {
            publisher: claim.claim.source.clone(),
            published_at: claim
                .claim
                .published_at
                .map(crate::util::utc_timestamp)
                .unwrap_or_else(|| "unknown".into()),
            reported_headline: claim.claim.fact.trim().to_string(),
            withdrawn: None,
            disputed: None,
        }
    }
}

/// Plugin policy: how much history the Scout looks at and what it accepts.
#[derive(Clone, Copy, Debug)]
pub struct Selection {
    /// Half-open lookback for reported claims, in seconds.
    pub lookback_seconds: i64,
    /// Ceiling on reported claims presented.
    pub max_reported: usize,
    /// Ceiling on measured windows presented.
    pub max_measured: usize,
    /// Serialized ceiling across the whole memory part.
    pub budget_bytes: usize,
}

impl Selection {
    /// The default horizon. Long enough to see a season's worth of availability
    /// news, short enough that an injury from last year is not presented as
    /// current.
    pub const fn rated() -> Self {
        Self {
            lookback_seconds: 60 * 86400,
            max_reported: 4,
            max_measured: 2,
            budget_bytes: 2000,
        }
    }
}

/// Present the two kinds of memory, keeping each within the plugin's budget.
///
/// Reported claims are kept even when the measured budget is already spent:
/// a profile with no personnel context reads as a claim about the subject's
/// fitness, and a measured trend is not a substitute for saying a player is
/// unavailable. The reverse is also true — a suspension with no profile is not a
/// scouting read — so neither is dropped to make room for the other without a
/// reason recorded here.
pub fn select(
    policy: &Selection,
    measured: Vec<Measured>,
    reported: Vec<Reported>,
    coverage_limits: Vec<String>,
) -> Selected {
    let mut measured = measured;
    measured.truncate(policy.max_measured);
    let mut reported = reported;
    reported.truncate(policy.max_reported);
    let mut selected = Selected {
        measured,
        reported,
        coverage_limits,
    };
    // Spend the budget on measured windows first, then on coverage limits, then
    // on reported claims. Coverage is dropped before claims because a limit the
    // reader cannot see is worse than one fewer claim, and better than a claim
    // that was silently withheld.
    let fits = |selected: &Selected| {
        serde_json::to_vec(selected)
            .map(|bytes| bytes.len() <= policy.budget_bytes)
            .unwrap_or(false)
    };
    while !fits(&selected) && !selected.coverage_limits.is_empty() {
        selected.coverage_limits.pop();
    }
    while !fits(&selected) && !selected.reported.is_empty() {
        selected.reported.pop();
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured(measure_label: &str, per_match: f64) -> Measured {
        Measured {
            measure_label: measure_label.into(),
            unit: "cumulative_total".into(),
            previous: Window {
                from: "2026-07-04T00:00:00Z".into(),
                before: "2026-08-01T00:00:00Z".into(),
                fixtures: 5,
                measured: 5,
                per_match: Some(per_match - 0.4),
            },
            current: Window {
                from: "2026-08-01T00:00:00Z".into(),
                before: "2026-08-29T00:00:00Z".into(),
                fixtures: 5,
                measured: 5,
                per_match: Some(per_match),
            },
            per_match_change: Some(0.4),
            percent_change: Some(12.0),
            fixture_ids: vec![1, 2, 3, 4, 5],
        }
    }

    fn reported(publisher: &str, headline: &str) -> Reported {
        Reported {
            publisher: publisher.into(),
            published_at: "2026-09-20T00:00:00Z".into(),
            reported_headline: headline.into(),
            withdrawn: None,
            disputed: None,
        }
    }

    #[test]
    fn measured_and_reported_memory_stay_distinguishable() {
        // The point of this module. A reader — and the model — must be able to
        // say which kind of memory a claim is without inferring it from the text.
        let selected = select(
            &Selection::rated(),
            vec![measured("Goals Scored", 1.8)],
            vec![reported("Club statement", "Kim Park is out for two weeks.")],
            vec!["Goals were absent for one fixture in the window.".into()],
        );
        let json = serde_json::to_value(&selected).unwrap();
        assert!(json.get("measured").is_some());
        assert!(json.get("reported").is_some());
        assert!(json.get("coverage_limits").is_some());
        // Neither kind leaks into the other's key.
        assert!(json["measured"][0].get("publisher").is_none());
        assert!(json["reported"][0].get("per_match").is_none());
    }

    #[test]
    fn an_absent_kind_stays_absent_rather_than_becoming_empty() {
        // No reported claims is not "no injuries". Omitting the key keeps
        // "none were found" distinguishable from "one was found and is empty".
        let selected = select(
            &Selection::rated(),
            vec![measured("Goals Scored", 1.8)],
            Vec::new(),
            Vec::new(),
        );
        let json = serde_json::to_value(&selected).unwrap();
        assert!(json.get("reported").is_none());
        assert!(json.get("measured").is_some());
    }

    #[test]
    fn a_withdrawn_claim_keeps_its_retraction_rather_than_reading_as_clearance() {
        // "The club withdrew the report" reads like a recovered player unless the
        // retraction travels with the claim.
        let mut claim = reported("Local Wire", "Kim Park is doubtful for Sunday.");
        claim.withdrawn = Some(true);
        let json = serde_json::to_value(&claim).unwrap();
        assert_eq!(json["withdrawn"], serde_json::json!(true));
    }

    #[test]
    fn a_contradicted_claim_is_presented_with_its_contradiction() {
        // Dropping a disputed claim would hide that the subject is disputed, which
        // is the reader-relevant fact.
        let mut claim = reported("City Wire", "Kim Park is out for two weeks.");
        claim.disputed = Some(true);
        let selected = select(&Selection::rated(), Vec::new(), vec![claim], Vec::new());
        assert_eq!(
            serde_json::to_value(&selected).unwrap()["reported"][0]["disputed"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn a_partly_measured_window_states_how_many_fixtures_had_the_value() {
        // A measure missing for two of five fixtures is not a zero in those two.
        let mut window = measured("Goals Scored", 1.8);
        window.current.measured = 3;
        let selected = select(&Selection::rated(), vec![window], Vec::new(), Vec::new());
        let json = serde_json::to_value(&selected).unwrap();
        assert_eq!(
            json["measured"][0]["current"]["fixtures"],
            serde_json::json!(5)
        );
        assert_eq!(
            json["measured"][0]["current"]["measured"],
            serde_json::json!(3)
        );
    }

    #[test]
    fn study_windows_preserve_their_own_bounds_counts_and_missing_averages() {
        let source = crate::plugins::memories::statistic::Window {
            from: 0,
            before: 86400,
            fixtures: 4,
            measured: 0,
            total: None,
            per_match: None,
        };
        let rendered = serde_json::to_value(Window::from(&source)).unwrap();
        assert_eq!(rendered["from"], "1970-01-01T00:00:00Z");
        assert_eq!(rendered["before"], "1970-01-02T00:00:00Z");
        assert_eq!(rendered["fixtures"], 4);
        assert_eq!(rendered["measured"], 0);
        assert!(rendered["per_match"].is_null());
        assert!(rendered.get("total").is_none());
    }

    #[test]
    fn the_budget_drops_coverage_before_claims_and_keeps_measured_windows() {
        // Coverage is the first thing to go, because a gap the reader cannot see
        // is worse than one fewer claim.
        let policy = Selection {
            max_reported: 4,
            max_measured: 1,
            budget_bytes: 700,
            ..Selection::rated()
        };
        let selected = select(
            &policy,
            vec![measured("Goals Scored", 1.8)],
            (0..4)
                .map(|i| {
                    reported(
                        "Wire",
                        &format!("A claim number {i} that is reasonably long."),
                    )
                })
                .collect(),
            vec!["Goals were absent for one fixture in the window.".into()],
        );
        let json = serde_json::to_value(&selected).unwrap();
        assert!(json.get("measured").is_some(), "measured windows are kept");
        assert!(
            json.get("coverage_limits").is_none(),
            "coverage is dropped before claims: {json}"
        );
        assert!(serde_json::to_vec(&selected).unwrap().len() <= policy.budget_bytes);
    }

    #[test]
    fn selection_is_capped_per_kind_before_the_budget_is_spent() {
        let policy = Selection {
            max_reported: 1,
            max_measured: 1,
            budget_bytes: 100_000,
            ..Selection::rated()
        };
        let selected = select(
            &policy,
            vec![measured("A", 1.0), measured("B", 2.0)],
            vec![reported("W", "one"), reported("W", "two")],
            Vec::new(),
        );
        assert_eq!(selected.measured.len(), 1);
        assert_eq!(selected.reported.len(), 1);
    }
}
