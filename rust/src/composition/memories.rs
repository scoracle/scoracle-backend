//! Shared, sourced context for character voices and form.
//!
//! Callers select relevant records from existing sources; this module preserves
//! their authority, clocks and provenance through budgeting and rendering. It
//! performs no inference, database writes or identity reconciliation. The offline
//! `memory_packages` example preserves inspectable offline cases.

use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

mod identity;
mod performance;
mod sources;
pub use identity::{load_identity_card, load_identity_record, IDENTITY_CARD_FRAMING};
pub use sources::{load, MemoryRequest};

pub const VERSION: &str = "memories-v2";

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mission {
    Scout,
    Analyst,
    Journalist,
    Influencer,
    Insider,
    Oracle,
    Editor,
    Investigator,
    Graph,
}

impl Mission {
    pub fn purpose(self) -> &'static str {
        match self {
            Self::Scout => "Interpret current measurements against supported historical baselines, participation and personnel context.",
            Self::Analyst => "Explain measured trajectories in their evidence windows and historical setting.",
            Self::Journalist => "Identify consequential developments using attributed chronology and unresolved threads.",
            Self::Influencer => "Interpret supported emotional context and its development; distinguish reporting tone from audience sentiment.",
            Self::Insider => "Assess the exact proposed relationship using roles, affiliations and this story's progression.",
            Self::Oracle => "Synthesize the selected findings while retaining their dates and shared evidence origins.",
            Self::Editor => "Locate the article's claims in the correct entity, role and time context.",
            Self::Investigator => "Resolve identity and verify claims against dated, sourced records.",
            Self::Graph => "Attach reported claims to the correct entities, relationships and time windows.",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Entity {
    pub sport: String,
    pub entity_type: String,
    pub id: i32,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Identity,
    ReportingClock,
    CompetitionClock,
    ParticipationClock,
    PresentEvidence,
    EstablishedHistory,
    DevelopingHistory,
    EditorialMemory,
}

/// A row reference, not a claim of independent corroboration. Several rows may
/// derive from one underlying source; retain those origins in `data` as well.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SourceRef {
    pub table: String,
    pub key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Record {
    pub section: Section,
    pub sources: Vec<SourceRef>,
    /// Observation time only. Effective dates/precision belong in the source data.
    pub observed_at: Option<String>,
    pub observed_unix: Option<i64>,
    pub data: Value,
}

/// An indivisible selection unit. Keep contradictions and their qualifications
/// together. The caller orders optional groups by relevance to the mission.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EvidenceGroup {
    pub id: String,
    pub required: bool,
    pub records: Vec<Record>,
    pub qualifications: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Omission {
    #[serde(default)]
    pub editorial: bool,
    pub group: String,
    pub reason: String,
    pub sources: Vec<SourceRef>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Package {
    pub version: String,
    pub entity: Entity,
    pub mission: Mission,
    /// This is a current-record snapshot, not a reconstruction of what was known
    /// during an old statistical season. Never accept an arbitrary backtest date.
    pub captured_at: String,
    pub captured_unix: i64,
    pub groups: Vec<EvidenceGroup>,
    pub unknowns: Vec<String>,
    pub omissions: Vec<Omission>,
    /// Review notes stay out of model context and material invalidation.
    #[serde(default)]
    pub diagnostics: Vec<String>,
    /// Numeric persistence anchor; never new evidence or a regeneration trigger.
    #[serde(default)]
    pub previous_score: Option<i16>,
    /// An old-season request must not inherit present employment or availability.
    #[serde(default)]
    pub historical: bool,
}

impl Package {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == VERSION || self.version == "memories-v1-preview",
            "unsupported context version"
        );
        ensure!(
            !self.entity.name.trim().is_empty(),
            "missing entity identity"
        );
        let mut ids = std::collections::HashSet::new();
        for group in &self.groups {
            let editorial_count = group
                .records
                .iter()
                .filter(|r| r.section == Section::EditorialMemory)
                .count();
            ensure!(
                editorial_count == 0 || editorial_count == group.records.len(),
                "editorial interpretation must be a separate group"
            );
            ensure!(
                !group.required
                    || group
                        .records
                        .iter()
                        .all(|r| r.section != Section::EditorialMemory),
                "editorial memory cannot be required"
            );
            ensure!(
                ids.insert(&group.id),
                "duplicate context group {}",
                group.id
            );
            ensure!(
                !group.records.is_empty(),
                "empty context group {}",
                group.id
            );
            for record in &group.records {
                ensure!(
                    !record.sources.is_empty(),
                    "context record without provenance"
                );
                ensure!(
                    record
                        .sources
                        .iter()
                        .all(|s| !s.table.is_empty() && !s.key.is_empty()),
                    "empty source reference"
                );
                ensure!(
                    record.observed_unix.is_none_or(|t| t <= self.captured_unix),
                    "record observed after context snapshot"
                );
                ensure!(
                    record.observed_at.is_some() == record.observed_unix.is_some(),
                    "incomplete observation timestamp"
                );
            }
        }
        ensure!(
            self.groups
                .iter()
                .any(|g| g.required && g.records.iter().any(|r| r.section == Section::Identity)),
            "required identity group missing"
        );
        Ok(())
    }

    /// Model-independent byte measurement, not a tokenizer estimate. Output
    /// reservations and actual provider token counts remain separate concerns.
    pub fn render(&self) -> Result<String> {
        self.validate()?;
        let mut out = format!(
            "{} ({}, {}). As of {}.\n{}\n",
            self.entity.name,
            self.entity.entity_type,
            self.entity.sport,
            self.captured_at.get(..10).unwrap_or(&self.captured_at),
            self.mission.purpose()
        );
        for group in &self.groups {
            for record in &group.records {
                let label = match record.section {
                    Section::Identity => "Identity",
                    Section::ReportingClock => "Reporting period (not competition progress)",
                    Section::CompetitionClock => "Schedule",
                    Section::ParticipationClock => "Recorded sample",
                    Section::PresentEvidence => "Current observations",
                    Section::EstablishedHistory => "Earlier observations",
                    Section::DevelopingHistory => "Reported history",
                    Section::EditorialMemory => "Our previous interpretation (not new evidence)",
                };
                out.push_str(&format!("{label}: {}", render_data(&record.data)));
                if let Some(date) = &record.observed_at {
                    out.push_str(&format!("; observed {}", date.get(..10).unwrap_or(date)));
                }
                // Keep source references resolvable in the inspectable package.
                let refs = record
                    .sources
                    .iter()
                    .map(|s| format!("{}:{}", s.table, &s.key))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!(" [{refs}]\n"));
            }
            for note in &group.qualifications {
                out.push_str(note);
                out.push('\n');
            }
        }
        for unknown in &self.unknowns {
            out.push_str(&format!("Unknown: {unknown}\n"));
        }
        if self.omissions.iter().any(|o| !o.editorial) {
            out.push_str(
                "Some candidate context was omitted; absence is not evidence of no history.\n",
            );
        }
        Ok(out)
    }

    /// Retain required foundations first, then whole optional groups in caller
    /// relevance order. Never trim prose or remove one side of a contradiction.
    pub fn within_bytes(mut self, max_bytes: usize) -> Result<Self> {
        self.validate()?;
        // Select facts independently of the presence or length of prior prose.
        let mut editorial = Vec::new();
        self.groups.retain(|g| {
            if g.records
                .iter()
                .all(|r| r.section == Section::EditorialMemory)
            {
                editorial.push(g.clone());
                false
            } else {
                true
            }
        });
        if self.render()?.len() > max_bytes {
            let mut optional = Vec::new();
            self.groups.retain(|g| {
                if !g.required {
                    optional.push(g.clone());
                }
                g.required
            });
            let notice =
                "Some candidate context was omitted; absence is not evidence of no history.\n"
                    .len();
            let reserve = if self.omissions.iter().any(|o| !o.editorial) || optional.is_empty() {
                0
            } else {
                notice
            };
            ensure!(
                self.render()?.len() + reserve <= max_bytes,
                "required context exceeds byte budget"
            );
            for group in optional {
                self.groups.push(group);
                let reserve = if self.omissions.iter().any(|o| !o.editorial) {
                    0
                } else {
                    reserve
                };
                if self.render()?.len() + reserve > max_bytes {
                    let group = self.groups.pop().expect("just pushed");
                    self.omissions.push(Omission {
                        editorial: false,
                        group: group.id,
                        reason: "whole group omitted for context byte budget".into(),
                        sources: group.records.into_iter().flat_map(|r| r.sources).collect(),
                    });
                }
            }
        }
        for group in editorial {
            self.groups.push(group);
            if self.render()?.len() > max_bytes {
                let group = self.groups.pop().expect("just pushed");
                self.omissions.push(Omission {
                    editorial: true,
                    group: group.id,
                    reason: "prior interpretation omitted for context byte budget".into(),
                    sources: group.records.into_iter().flat_map(|r| r.sources).collect(),
                });
            }
        }
        Ok(self)
    }

    /// Add the exact selected memory material to the junction's existing debounce key.
    pub fn with_input_components(&self, components: &str) -> Result<String> {
        let mut value: Value = serde_json::from_str(components)?;
        ensure!(value.is_object(), "input components must be an object");
        value["memories"] = Value::String(self.fingerprint()?);
        Ok(value.to_string())
    }

    /// Selected facts invalidate a reading. Capture and observation wall time, prior generated
    /// prose, its score and its omissions never create a refresh loop.
    pub fn fingerprint(&self) -> Result<String> {
        self.validate()?;
        let groups: Vec<_> = self.groups.iter().filter(|g| !g.records.iter().all(|r| r.section == Section::EditorialMemory)).map(|g| {
            let records: Vec<_> = g.records.iter().filter(|r| r.section != Section::EditorialMemory).map(|r| serde_json::json!({"section":r.section,"sources":r.sources,"data":r.data})).collect();
            serde_json::json!({"id":g.id,"required":g.required,"records":records,"qualifications":g.qualifications})
        }).collect();
        let material = serde_json::json!({
            "version": VERSION, "entity": self.entity, "mission": self.mission, "historical": self.historical,
            "groups": groups, "unknowns": self.unknowns,
            "omissions": self.omissions.iter().filter(|o| !o.editorial).collect::<Vec<_>>(),
        });
        Ok(hex::encode(Sha256::digest(serde_json::to_vec(&material)?)))
    }
}

/// Render semantic source fields, without the package's audit envelope.
fn render_data(data: &Value) -> String {
    if let Some(text) = data.get("text").and_then(Value::as_str) {
        return match data
            .get("evidence_article_ids")
            .filter(|v| v.as_array().is_some_and(|a| !a.is_empty()))
        {
            Some(ids) => format!("{text}; based on articles {ids}"),
            None => text.to_string(),
        };
    }
    let Some(fields) = data.as_object() else {
        return data.to_string();
    };
    fields
        .iter()
        .map(|(key, value)| {
            let value = match value {
                Value::Null => "unknown".into(),
                Value::String(s) => s.clone(),
                _ => value.to_string(),
            };
            format!("{}: {value}", key.replace('_', " "))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
pub(crate) fn test_package() -> Package {
    Package {
        version: VERSION.into(),
        entity: Entity {
            sport: "NBA".into(),
            entity_type: "team".into(),
            id: 1,
            name: "Test Team".into(),
        },
        mission: Mission::Analyst,
        captured_at: "2026-09-14".into(),
        captured_unix: 1,
        groups: vec![EvidenceGroup {
            id: "identity".into(),
            required: true,
            records: vec![Record {
                section: Section::Identity,
                sources: vec![SourceRef {
                    table: "teams".into(),
                    key: "1".into(),
                }],
                observed_at: None,
                observed_unix: None,
                data: serde_json::json!({"name":"Test Team"}),
            }],
            qualifications: vec![],
        }],
        unknowns: vec![],
        omissions: vec![],
        diagnostics: vec![],
        previous_score: None,
        historical: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn extra(id: &str, section: Section, text: &str) -> EvidenceGroup {
        EvidenceGroup {
            id: id.into(),
            required: false,
            records: vec![Record {
                section,
                sources: vec![SourceRef {
                    table: "news_articles".into(),
                    key: "42".into(),
                }],
                observed_at: None,
                observed_unix: None,
                data: json!({"text":text}),
            }],
            qualifications: vec![],
        }
    }

    #[test]
    fn editorial_changes_cannot_evict_evidence_or_trigger_another_generation() {
        let mut package = test_package();
        package.groups.push(extra(
            "history",
            Section::DevelopingHistory,
            "Supported earlier report.",
        ));
        let limit = package.render().unwrap().len() + 160;
        let initial = package.clone().within_bytes(limit).unwrap();
        // Even if a caller supplies continuity first, new prose gets remaining space.
        package.groups.insert(
            1,
            extra(
                "any editorial group name",
                Section::EditorialMemory,
                &"Long prior read. ".repeat(80),
            ),
        );
        package.previous_score = Some(70);
        let longer = package.within_bytes(limit).unwrap();
        assert_eq!(
            initial.fingerprint().unwrap(),
            longer.fingerprint().unwrap()
        );
        assert!(longer
            .render()
            .unwrap()
            .contains("Supported earlier report."));
        assert!(longer.omissions.iter().any(|o| o.editorial));
        assert!(longer.render().unwrap().len() <= limit);
    }

    #[test]
    fn an_exact_fit_keeps_all_factual_context_without_reserved_padding() {
        let mut package = test_package();
        package.groups.push(extra(
            "history",
            Section::DevelopingHistory,
            "Earlier sourced production.",
        ));
        let size = package.render().unwrap().len();
        let original = package.clone().within_bytes(size).unwrap();
        assert!(original.omissions.is_empty());
        package.groups.push(extra(
            "prior",
            Section::EditorialMemory,
            "Optional earlier prose.",
        ));
        let selected = package.within_bytes(size).unwrap();
        assert_eq!(original.render().unwrap(), selected.render().unwrap());
        assert_eq!(
            original.fingerprint().unwrap(),
            selected.fingerprint().unwrap()
        );
    }

    #[test]
    fn reobserving_unchanged_rows_does_not_refresh_the_card() {
        let package = test_package();
        let mut observed = package.clone();
        observed.groups[0].records[0].observed_at = Some("1970-01-01T00:00:01Z".into());
        observed.groups[0].records[0].observed_unix = Some(1);
        assert_eq!(
            package.fingerprint().unwrap(),
            observed.fingerprint().unwrap()
        );
    }

    #[test]
    fn corrected_facts_invalidate_the_existing_material_key() {
        let original = test_package();
        let mut corrected = original.clone();
        corrected.groups[0].records[0].data = json!({"name":"Test Team","coach":"New Coach"});
        assert_ne!(
            original
                .with_input_components("{\"packet_ids\":[1]}")
                .unwrap(),
            corrected
                .with_input_components("{\"packet_ids\":[1]}")
                .unwrap()
        );
        assert!(corrected.with_input_components("[]").is_err());
    }

    #[test]
    fn an_essential_conflict_cannot_be_sliced_to_make_room() {
        let mut package = test_package();
        package.groups[0].records.push(Record {
            section: Section::Identity,
            sources: vec![SourceRef {
                table: "entity_relationships".into(),
                key: "7".into(),
            }],
            observed_at: None,
            observed_unix: None,
            data: json!({"text":"Conflicting current affiliation"}),
        });
        let size = package.render().unwrap().len();
        assert!(package.clone().within_bytes(size - 1).is_err());
        let selected = package.within_bytes(size + 80).unwrap();
        assert_eq!(selected.groups[0].records.len(), 2);
    }

    #[test]
    fn editorial_prose_retains_its_original_article_references() {
        let mut package = test_package();
        let mut prior = extra("prior", Section::EditorialMemory, "Earlier interpretation.");
        prior.records[0].data["evidence_article_ids"] = json!([12, 42]);
        package.groups.push(prior);
        let rendered = package.render().unwrap();
        assert!(rendered.contains("Our previous interpretation (not new evidence)"));
        assert!(rendered.contains("based on articles [12,42]"));
    }
}
