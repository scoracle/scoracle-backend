//! Assemble the three real context design cases, without a database or model call.
//! cargo run --example memory_packages -- [optional max context bytes]
//! Source capture and official-web verification are separate, dated inputs.

use anyhow::{bail, ensure, Context, Result};
use scoracle_cognition::composition::memories::{
    Entity, EvidenceGroup, Mission, Omission, Package, Record, Section, SourceRef, VERSION,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Clone, Deserialize)]
struct RawRecord {
    case: String,
    kind: String,
    source: String,
    source_key: String,
    observed_at: Option<String>,
    observed_unix: Option<i64>,
    data: Value,
}

impl RawRecord {
    fn record(&self, section: Section) -> Record {
        Record {
            section,
            sources: vec![SourceRef {
                table: self.source.clone(),
                key: self.source_key.clone(),
            }],
            observed_at: self.observed_at.clone(),
            observed_unix: self.observed_unix,
            data: self.data.clone(),
        }
    }
}

#[derive(Deserialize)]
struct Snapshot {
    captured_at: String,
    captured_unix: i64,
    schema_version: String,
    records: Vec<RawRecord>,
}

#[derive(Deserialize)]
struct WebChecks {
    checked_at: String,
    checked_unix: i64,
    records: Vec<RawRecord>,
}

fn group(id: &str, records: Vec<Record>, notes: &[&str]) -> EvidenceGroup {
    EvidenceGroup {
        id: id.into(),
        required: true,
        records,
        qualifications: notes.iter().map(|s| s.to_string()).collect(),
    }
}

fn packages() -> Result<Vec<Package>> {
    let snapshot: Snapshot =
        serde_json::from_str(include_str!("../fixtures/memories/source-2026-09-14.json"))?;
    let web: WebChecks = serde_json::from_str(include_str!(
        "../fixtures/memories/web-checks-2026-09-14.json"
    ))?;
    ensure!(
        web.checked_unix >= snapshot.captured_unix,
        "web verification predates database capture"
    );
    let mut out = Vec::new();
    for (case, sport, kind, id, mission) in [
        ("morgan", "FOOTBALL", "player", 4592198, Mission::Scout),
        ("iraola", "FOOTBALL", "person", 11, Mission::Insider),
        ("lions", "NFL", "team", 25, Mission::Journalist),
    ] {
        let rows: Vec<_> = snapshot
            .records
            .iter()
            .chain(&web.records)
            .filter(|r| r.case == case)
            .collect();
        let identity = rows
            .iter()
            .find(|r| r.kind == "identity")
            .context("essential identity absent from capture")?;
        let mut package = Package {
            version: VERSION.into(),
            entity: Entity {
                sport: sport.into(),
                entity_type: kind.into(),
                id,
                name: identity.data["name"]
                    .as_str()
                    .context("identity name missing")?
                    .into(),
            },
            mission,
            captured_at: web.checked_at.clone(),
            captured_unix: web.checked_unix,
            groups: vec![],
            unknowns: vec![],
            omissions: vec![],
            diagnostics: vec![],
            previous_score: None,
            historical: false,
        };
        let select = |k: &str, section| {
            rows.iter()
                .filter(|r| r.kind == k)
                .map(|r| r.record(section))
                .collect::<Vec<_>>()
        };
        let mut identities = select("identity", Section::Identity);
        identities.extend(select("verified_affiliation", Section::Identity));
        identities.extend(select("affiliation_observation", Section::Identity));
        identities.extend(select("role_relationship", Section::Identity));
        let identity_notes: &[&str] = match case {
            "morgan" => &["Official Chelsea signing establishes the current club. The Villa roster projection is stale. Keep the Villa performance baseline attached to its own season/team; box-score history timestamps do not establish the transfer date."],
            "iraola" => &["Official Liverpool appointment supports head coach at Liverpool. The active Athletic Club coach_of edges conflict with the official account of his playing career. These are database defects, not evidence of concurrent coaching jobs. The latest check returned unknown; its timestamp does not verify employment."],
            _ => &["Identity is a house record; fixture participation supplies the competition scope."],
        };
        package.groups.push(group(
            "Identity and reconciliation",
            identities,
            identity_notes,
        ));
        let mut clocks = select("reporting_clock", Section::ReportingClock);
        clocks.extend(select("verified_calendar", Section::CompetitionClock));
        clocks.extend(select("schedule_coverage", Section::CompetitionClock));
        clocks.extend(select("fixture", Section::CompetitionClock));
        package.groups.push(group("Sporting time", clocks, &[
            "Reporting week, named competition round and entity participation are separate clocks. Schedule coverage is not assumed complete; unverified nominations cannot establish a fixture or stage.",
        ]));
        match case {
            "morgan" => {
                for row in rows.iter().filter(|r| r.kind == "statistics") {
                    let prior = row.data["season"] == 2025;
                    let affiliation = rows
                        .iter()
                        .find(|r| {
                            r.kind == "affiliation_observation"
                                && r.data["team_id"] == row.data["team_id"]
                        })
                        .context("missing captured team name")?;
                    let played_for = &affiliation.data["team"];

                    let mut sample = row.record(Section::ParticipationClock);
                    sample.data = json!({"season":row.data["season"],"league_id":row.data["league_id"],"played_for":played_for,"team_id":row.data["team_id"],"appearances":row.data["appearances"],"minutes":row.data["minutes"]});
                    let mut measurement = row.record(if prior {
                        Section::EstablishedHistory
                    } else {
                        Section::PresentEvidence
                    });
                    // Source keys are measurement identities here. No old pct/z/rating
                    // columns or guessed zero-suppression rules enter this prototype.
                    measurement.data = if prior {
                        json!({"season":row.data["season"],"league_id":row.data["league_id"],"played_for":played_for,"team_id":row.data["team_id"],"basis":row.data["basis"],"coverage":"stored sample; completeness unknown","goals":row.data["goals"],"assists":row.data["assists"],"shots_on_target":row.data["shots_on_target"]})
                    } else {
                        json!({"season":row.data["season"],"league_id":row.data["league_id"],"played_for":played_for,"team_id":row.data["team_id"],"basis":row.data["basis"],"coverage":"stored sample; completeness unknown","goals":row.data["goals"],"assists":row.data["assists"],"expected_goals":row.data["expected_goals"],"expected_assists":row.data["expected_assists"]})
                    };
                    sample
                        .sources
                        .extend(affiliation.record(Section::Identity).sources);
                    measurement
                        .sources
                        .extend(affiliation.record(Section::Identity).sources);
                    package.groups.push(group(if prior { "Historical performance baseline" } else { "Current statistical evidence" }, vec![sample,measurement], &[
                        "Historical production remains useful without a current rank. These unequal samples and different measurement keys do not establish an ability trend. Null is missing, not zero.",
                    ]));
                }
                package.unknowns.extend([
                    "The one-appearance stored sample does not establish current total appearances, fitness or why coverage is thin.",
                    "Exact transfer effective date is not established by the inspected official page.",
                    "Stored ranks lack measurement identity; no rank comparisons are supplied.",
                ].map(str::to_owned));
            }
            "iraola" => {
                let evidence = select("news", Section::DevelopingHistory);
                package.groups.push(group("Recruitment subject and dated reporting", evidence, &[
                    "Article 145830 names Alex Scott as the recruit and Iraola as his prior coach. Co-mention with Chelsea does not establish a Chelsea move for Iraola. The September Liverpool reports support role context, not independent official confirmation.",
                    "Article 585118 has published_at later than fetched_at. Preserve both supplied times; do not invent exact event chronology from that record.",
                ]));
                // P54 is sports-team membership, distinct from a coaching position.
                // Keep the adult spell, source statement ID and year precision;
                // the complete statements remain in the immutable source capture.
                for row in rows.iter().filter(|r| r.kind == "career_statement") {
                    let statement = &row.data["statement"];
                    let start = &statement["qualifiers"]["P580"][0]["datavalue"]["value"];
                    let end = &statement["qualifiers"]["P582"][0]["datavalue"]["value"];
                    if start["time"] != "+2003-01-01T00:00:00Z" {
                        package.omissions.push(Omission {
                            editorial: false,
                            group: "other Athletic Club career spell".into(),
                            reason: "adult playing spell suffices for role disambiguation".into(),
                            sources: row.record(Section::EstablishedHistory).sources,
                        });
                        continue;
                    }
                    let mut r = row.record(Section::EstablishedHistory);
                    r.data = json!({"relationship":"playing membership","team":"Athletic Club","wikidata_team":"Q8687","start":start,"end":end,"statement_id":statement["id"]});
                    let mut g = group("Relevant playing history",vec![r],&["Wikidata precision 9 is year precision, not a January 1 signing date. Playing membership does not establish current coaching employment."]);
                    g.required = false;
                    package.groups.push(g);
                }
                for row in rows.iter().filter(|r| r.kind == "role_fact") {
                    package.omissions.push(Omission { editorial: false, group:"repeated coach role fact".into(), reason:"role already retained in identity and official appointment; repeated imports are not new corroboration".into(), sources:row.record(Section::Identity).sources });
                }
                package.unknowns.push("Exact employment start date is not the June 4 announcement date; the source establishes the 2026/27 appointment.".into());
            }
            "lions" => {
                package.unknowns.push("The selected fixture results are database records. The official schedule verifies dates, opponents and regular-season stage, not the recorded score.".into());
                package.unknowns.push("No availability or player participation record was selected; this does not establish a healthy squad.".into());
            }
            _ => bail!("unhandled case"),
        }
        package.omissions.push(Omission {
            editorial: false,
            group: "prior generated prose".into(),
            reason:
                "not selected for these cases; no model-authored text promoted to factual memory"
                    .into(),
            sources: vec![],
        });
        package.diagnostics.push(format!("Database capture: {}; schema {}. Web checks are a separate review overlay, not persisted metadata. Stored rating bundles predate migrations 252/253; their ranks were not reused.",snapshot.captured_at,snapshot.schema_version));
        package.validate()?;
        out.push(package);
    }
    Ok(out)
}

fn main() -> Result<()> {
    let budget = std::env::args()
        .nth(1)
        .map(|v| v.parse::<usize>())
        .transpose()?;
    let mut outputs = Vec::new();
    for p in packages()? {
        let p = match budget {
            Some(n) => p.within_bytes(n)?,
            None => p,
        };
        let composed = scoracle_cognition::composition::compose_card(&p, "")?;
        outputs.push(json!({"context_bytes":composed.prompt.len(),"context_chars":composed.prompt.chars().count(),"fingerprint":p.fingerprint()?,"package":p,"prompt":composed.prompt,"system":composed.system}));
    }
    println!("{}", serde_json::to_string_pretty(&outputs)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_keeps_the_live_voice_and_form_separate_from_new_evidence() {
        let p = packages().unwrap().remove(0);
        let evidence = "A newly fetched, attributed report goes here.";
        let composed = scoracle_cognition::composition::compose_card(&p, evidence).unwrap();
        assert_eq!(
            composed.system,
            scoracle_cognition::studio::scout::RATING_SYSTEM_PROMPT.as_str()
        );
        assert_eq!(composed.prompt.matches(evidence).count(), 1);
        assert!(composed.prompt.contains("Historical performance baseline:"));
        assert!(composed.prompt.contains("played for: Aston Villa"));
        assert!(!composed.system.contains(evidence));
    }

    #[test]
    fn real_cases_keep_baseline_conflicts_and_clocks() {
        let p = packages().unwrap();
        let rogers = &p[0];
        let old = rogers
            .groups
            .iter()
            .find(|g| g.id == "Historical performance baseline")
            .unwrap();
        assert_eq!(old.records[0].data["appearances"], 37);
        assert_eq!(old.records[1].data["goals"], 10);
        let current = rogers
            .groups
            .iter()
            .find(|g| g.id == "Current statistical evidence")
            .unwrap();
        assert_eq!(current.records[0].data["appearances"], 1);
        assert!(current.records[1].data["goals"].is_null());
        assert!(rogers.groups[0]
            .records
            .iter()
            .any(|r| r.data["team"] == "Aston Villa"));
        assert!(rogers.groups[0]
            .records
            .iter()
            .any(|r| r.data["to_team"] == "Chelsea"));
        let iraola = &p[1];
        assert!(iraola.groups[0]
            .records
            .iter()
            .any(|r| r.data["predicate"] == "coach_of" && r.data["team"] == "Athletic Club"));
        assert!(iraola.groups[0]
            .records
            .iter()
            .any(|r| r.data["to_team"] == "Liverpool"));
        let clocks = &p[2].groups[1].records;
        assert!(clocks
            .iter()
            .any(|r| r.section == Section::ReportingClock && r.data["reporting_week"] == 2));
        assert!(clocks
            .iter()
            .any(|r| r.section == Section::CompetitionClock && r.data["round"] == "Week 1"));
    }

    #[test]
    fn source_correction_changes_fingerprint_but_capture_wall_time_does_not() {
        let mut p = packages().unwrap().remove(0);
        let original = p.fingerprint().unwrap();
        p.captured_unix += 60;
        p.captured_at = "later capture, identical source material".into();
        assert_eq!(original, p.fingerprint().unwrap());
        p.groups[0].records[0].data["team"] = json!("Chelsea");
        assert_ne!(original, p.fingerprint().unwrap());
    }

    #[test]
    fn backdated_snapshot_and_missing_identity_fail_closed() {
        let mut p = packages().unwrap().remove(0);
        p.captured_unix = 0;
        assert!(p.validate().is_err());
        p.captured_unix = i64::MAX;
        p.groups.remove(0);
        assert!(p.validate().is_err());
    }

    #[test]
    fn budget_omits_whole_optional_history_never_half_a_conflict() {
        let p = packages().unwrap().remove(1);
        let mut essential = p.clone();
        essential.groups.retain(|g| g.required);
        let budget = essential.render().unwrap().len();
        let selected = p.within_bytes(budget).unwrap();
        assert!(selected.render().unwrap().len() <= budget);
        assert!(selected.groups.iter().all(|g| g.required));
        assert_eq!(
            selected.groups[0].records.len(),
            essential.groups[0].records.len()
        );
        assert!(selected.clone().within_bytes(10).is_err());
    }
}
