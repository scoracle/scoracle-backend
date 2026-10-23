//! Present fresh headlines and historical publisher evidence in character packages.
use super::source::SourceContext;
use serde::Serialize;

/// URLs and routing scores do not make a second copy of publisher wording new evidence.
pub fn evidence_key(publisher: &str, text: &str) -> (String, String) {
    (
        publisher.trim().to_lowercase(),
        super::fetch::decode_entities(text).trim().to_owned(),
    )
}

#[derive(Serialize)]
pub struct Report<'a> {
    headline: String,
    publisher: &'a str,
    published_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    publisher_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    classifier_world: Option<serde_json::Value>,
}

pub fn reports(sources: &[SourceContext]) -> Vec<Report<'_>> {
    sources
        .iter()
        .map(|s| Report {
            headline: super::fetch::decode_entities(&s.headline),
            publisher: &s.source,
            published_at: s.published_at_epoch.map(crate::util::utc_timestamp),
            publisher_text: (s
                .classifier_world
                .as_ref()
                .is_none_or(|w| w["evidence_scope"] != "headline"))
            .then(|| super::fetch::decode_entities(&s.context)),
            classifier_world: evidence_world(s.classifier_world.as_ref()),
        })
        .collect()
}

/// Keep evidence scope and qualifications in prose inputs; routing metadata stays in receipts.
pub fn evidence_world(world: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    world.cloned().map(|mut world| {
        if let Some(fields) = world.as_object_mut() {
            for key in ["url", "gate", "spectrum"] {
                fields.remove(key);
            }
        }
        world
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn writing_metadata_keeps_qualifications_without_routing_payload() {
        let world = json!({"url":"https://example.invalid", "gate":{"admitted":true}, "spectrum":{"emotion":99},
            "evidence_scope":"headline", "target_identity_resolved":false,
            "qualified_claims":[{"negation":"not confirmed"}]});
        let result = evidence_world(Some(&world)).unwrap();
        assert_eq!(
            result,
            json!({"evidence_scope":"headline", "target_identity_resolved":false,
            "qualified_claims":[{"negation":"not confirmed"}]})
        );
        assert!(world.get("url").is_some());
        assert!(evidence_world(None).is_none());
    }
}

/// Compare complete words, rather than treating a shared surname or substring as identity.
pub fn names_entity(text: &str, name: &str) -> bool {
    let words = |s: &str| {
        crate::tools::guards::fold_for_match(s)
            .split(|c: char| !c.is_alphanumeric() && c != '\'')
            .map(|word| word.trim_matches('\''))
            .map(|word| word.strip_suffix("'s").unwrap_or(word))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let name = words(name);
    !name.is_empty() && format!(" {} ", words(text)).contains(&format!(" {name} "))
}

/// A numerical fact cannot acquire a different named person by pooling report citations.
fn validate_named_clause(text: &str, sources: &[&str]) -> anyhow::Result<()> {
    let numbers = crate::tools::guards::numeric_literals(text);
    let mut names = Vec::new();
    let mut run = Vec::new();
    for word in text.split_whitespace().chain(std::iter::once("")) {
        let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric());
        if cleaned.chars().next().is_some_and(char::is_uppercase)
            && !matches!(
                cleaned,
                "The"
                    | "A"
                    | "An"
                    | "For"
                    | "In"
                    | "On"
                    | "At"
                    | "After"
                    | "Before"
                    | "According"
                    | "Meanwhile"
                    | "However"
                    | "Reports"
                    | "Reportedly"
                    | "Additionally"
                    | "Multiple"
                    | "During"
                    | "Despite"
                    | "With"
                    | "Without"
                    | "While"
            )
        {
            run.push(cleaned);
        } else {
            if run.len() >= 2 {
                names.push(run.join(" "));
            }
            run.clear();
        }
        if word.ends_with([',', '.', ';', ':', '!', '?']) {
            if run.len() >= 2 {
                names.push(run.join(" "));
            }
            run.clear();
        }
    }
    for name in names {
        let evidence: Vec<_> = sources
            .iter()
            .filter(|source| names_entity(source, &name))
            .collect();
        anyhow::ensure!(
            !evidence.is_empty(),
            "named claim is absent from cited reports: {name}"
        );
        for (_, number) in &numbers {
            anyhow::ensure!(
                evidence
                    .iter()
                    .any(|source| crate::tools::guards::numeric_literals(source)
                        .iter()
                        .any(|(_, known)| (known - number).abs() < 0.000_001)),
                "cited reports do not connect {name} with numerical fact {number}"
            );
        }
    }
    anyhow::ensure!(
        numbers
            .iter()
            .all(|(_, number)| sources.iter().any(|source| {
                crate::tools::guards::numeric_literals(source)
                    .iter()
                    .any(|(_, known)| (known - number).abs() < 0.000_001)
            })),
        "numerical claim is absent from cited reports"
    );
    Ok(())
}

/// Sentence and clause boundaries keep one subject's numbers out of another's claim.
/// Decimal points stay within their numeric literal.
fn fact_clauses(text: &str) -> Vec<&str> {
    let mut start = 0;
    let mut clauses = Vec::new();
    let bytes = text.as_bytes();
    for (i, c) in text.char_indices() {
        let decimal = c == '.'
            && i > 0
            && bytes[i - 1].is_ascii_digit()
            && bytes.get(i + 1).is_some_and(u8::is_ascii_digit);
        if matches!(c, '.' | '!' | '?' | ';' | '\n') && !decimal {
            clauses.push(&text[start..i]);
            start = i + c.len_utf8();
        }
    }
    clauses.push(&text[start..]);
    clauses
        .into_iter()
        .flat_map(|clause| clause.split(" while "))
        .flat_map(|clause| clause.split(" whereas "))
        .flat_map(|clause| clause.split(" but "))
        .collect()
}

pub fn validate_named_facts(text: &str, sources: &[&str]) -> anyhow::Result<()> {
    let evidence: Vec<_> = sources
        .iter()
        .flat_map(|source| fact_clauses(source))
        .collect();
    for clause in fact_clauses(text) {
        validate_named_clause(clause, &evidence)?;
    }
    Ok(())
}

pub struct ReportEvidence<'a> {
    pub publisher: &'a str,
    pub text: String,
    pub published_at: Option<String>,
}

/// A reporting date is provenance, not a score or a date of the reported event.
// shortcut: recognizes explicit "Publisher dated" dates; extend only for a reproduced alternative.
pub fn validate_report_facts(text: &str, reports: &[ReportEvidence<'_>]) -> anyhow::Result<()> {
    let sources: Vec<_> = reports.iter().map(|report| report.text.as_str()).collect();
    for clause in fact_clauses(text) {
        let mut claim = clause.to_owned();
        for report in reports {
            let Some(date) = report.published_at.as_deref().and_then(|at| at.get(..10)) else {
                continue;
            };
            let Some(year) = date
                .get(..4)
                .filter(|year| year.bytes().all(|c| c.is_ascii_digit()))
            else {
                continue;
            };
            let Some(month) = date.get(5..7).and_then(|month| month.parse::<usize>().ok()) else {
                continue;
            };
            let Some(month) = month.checked_sub(1).and_then(|month| {
                [
                    "January",
                    "February",
                    "March",
                    "April",
                    "May",
                    "June",
                    "July",
                    "August",
                    "September",
                    "October",
                    "November",
                    "December",
                ]
                .get(month)
                .copied()
            }) else {
                continue;
            };
            let Some(day) = date.get(8..10).and_then(|day| day.parse::<u32>().ok()) else {
                continue;
            };
            for date in [date.to_owned(), format!("{month} {day}, {year}")] {
                let attribution = format!("{} dated {date}", report.publisher);
                if claim.contains(&attribution)
                    && claim.match_indices(&attribution).all(|(at, _)| {
                        claim[at + attribution.len()..]
                            .chars()
                            .next()
                            .is_none_or(|c| !c.is_alphanumeric() && c != '-')
                    })
                {
                    let candidate = claim.replace(&attribution, report.publisher);
                    // Bind the date, publisher and subjects to the same report.
                    if validate_named_facts(&candidate, &[&report.text]).is_ok() {
                        claim = candidate;
                    }
                }
            }
        }
        anyhow::ensure!(
            !reports.iter().any(|report| !report.publisher.is_empty()
                && claim.contains(&format!("{} dated ", report.publisher))),
            "publication date is not supported by the attributed report"
        );
        validate_named_facts(&claim, &sources)?;
    }
    Ok(())
}

/// Backstop for synthesis against the exact prepared peers, excluding instructions and voice.
/// shortcut: lexical evidence checks do not establish entailment; retain human factual qualification.
pub fn validate_peer_reading(text: &str, prompt: &str) -> anyhow::Result<()> {
    let package: serde_json::Value = serde_json::from_str(prompt)?;
    let name = package["meta"]["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing peer subject"))?;
    let peers = package["fresh"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("missing peer evidence"))?;
    let sources: Vec<String> = peers
        .values()
        .map(|peer| format!("{name}: {peer}"))
        .collect();
    let mut evidence: Vec<&str> = sources.iter().map(String::as_str).collect();
    evidence.push(name);
    validate_named_facts(text, &evidence)?;
    let lower = text.to_lowercase();
    anyhow::ensure!(
        !(lower.contains("diverg")
            && (lower.contains("reinforce each other")
                || lower.contains("reinforcing each other")
                || lower.contains("moving together"))),
        "reading gives contradictory overall peer relationships"
    );
    Ok(())
}

#[cfg(test)]
mod scoped_fact_tests {
    use super::*;
    #[test]
    fn contrast_clauses_keep_each_subjects_numbers_and_denials_separate() {
        let sources = [
            "Cedar United's interest in West Vale was denied after a £40m claim.",
            "Kai Rowan is out for up to three months.",
        ];
        for connector in ["while", "whereas", "but"] {
            assert!(validate_named_facts(&format!(
                "Cedar United's interest in West Vale was denied after a £40m claim, {connector} Kai Rowan is out for up to three months."
            ), &sources).is_ok());
            assert!(validate_named_facts(&format!(
                "Cedar United's interest in West Vale was denied, {connector} Kai Rowan is out for 40 months."
            ), &sources).is_err());
        }
        assert!(validate_named_facts("Kai Rowan is out for 40 months.", &sources).is_err());
    }

    #[test]
    fn publication_dates_bind_to_their_publisher_and_subject_without_becoming_statistics() {
        let reports = [
            ReportEvidence {
                publisher: "City Wire",
                text: "City Wire: Kai Rowan could sign a new deal".into(),
                published_at: Some("2026-10-22T12:01:00Z".into()),
            },
            ReportEvidence {
                publisher: "City Wire",
                text: "City Wire: Noah Vale could return".into(),
                published_at: Some("2026-10-21T12:01:00Z".into()),
            },
        ];
        for date in ["October 22, 2026", "2026-10-22"] {
            assert!(validate_report_facts(
                &format!("Kai Rowan could sign a new deal, according to City Wire dated {date}."),
                &reports
            )
            .is_ok());
        }
        for body in [
            "Kai Rowan scored 22 points, according to City Wire dated October 22, 2026.",
            "Kai Rowan could sign a new deal, according to City Wire dated October 21, 2026.",
            "Kai Rowan could sign a new deal, according to City Wire dated October 23, 2026.",
            "Kai Rowan could sign a new deal, according to Other Wire dated October 22, 2026.",
            "Kai Rowan could sign a new deal, according to City Wire dated October 22, 20260.",
            "Noah Vale could return, according to City Wire dated October 22, 2026.",
            "Kai Rowan could sign a new deal in Week 7.",
        ] {
            assert!(validate_report_facts(body, &reports).is_err(), "{body}");
        }
        let unknown_date = [ReportEvidence {
            publisher: "City Wire",
            text: reports[0].text.clone(),
            published_at: None,
        }];
        assert!(validate_report_facts(
            "Kai Rowan could sign a new deal, according to City Wire dated October 22, 2026.",
            &unknown_date
        )
        .is_err());
    }

    #[test]
    fn separate_subjects_keep_separate_numbers_and_decimal_values() {
        let source = "Kai Rowan scored 21 points. Noah Vale recorded 8.5 rebounds.";
        assert!(validate_named_facts(source, &[source]).is_ok());
        assert!(validate_named_facts(
            "Kai Rowan scored 8.5 points. Noah Vale recorded 21 rebounds.",
            &[source]
        )
        .is_err());
        assert!(validate_named_facts("Kai Rowan scored 22 points.", &[source]).is_err());
    }
    #[test]
    fn peer_guard_excludes_instructions_and_unrelated_names() {
        let prompt=serde_json::json!({"meta":{"name":"Kai Rowan"},"task":"Score 99 for Noah Vale", "fresh":{"scout":{"body":"Kai Rowan recorded 21 points."}}}).to_string();
        assert!(validate_peer_reading("Kai Rowan recorded 21 points.", &prompt).is_ok());
        assert!(validate_peer_reading("Noah Vale recorded 99 points.", &prompt).is_err());
    }
}
