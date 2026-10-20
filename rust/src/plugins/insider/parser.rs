//! Exact source spans, named counterparties and served-prose validation.
use super::prompt::Report;
use super::{Reply, Status};
use crate::harness::Parser;
use crate::tools::meta::EntityMeta;
use anyhow::{ensure, Result};

pub(super) struct ReplyParser<'a> {
    pub(super) subject: &'a EntityMeta,
    pub(super) reports: &'a [Report],
}

impl Parser<Reply> for ReplyParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<Reply>> {
        let mut reply: Reply = serde_json::from_str(raw)?;
        reply.body = crate::tools::guards::clean_served_prose(&crate::tools::form::normalize_body(
            &reply.body,
        ));
        crate::tools::form::validate_body(&reply.body)?;
        ensure!(
            reply.body.chars().count() <= super::prompt::BODY_MAX_CHARS,
            "Insider body exceeds the reading budget"
        );
        ensure!(
            crate::tools::guards::title_names_entity(&reply.body, &self.subject.name),
            "Insider reading does not name the subject"
        );
        ensure!(
            !crate::tools::guards::has_bookkeeping_citation(&reply.body)
                && crate::tools::guards::first_product_name(&reply.body).is_none()
                && !crate::tools::guards::has_foreign_script(&reply.body),
            "Insider reading violates served prose guard"
        );
        ensure!(
            reply.findings.len() <= super::prompt::MAX_FINDINGS.min(self.reports.len() * 8),
            "too many findings"
        );
        let mut seen = std::collections::HashSet::new();
        for finding in &reply.findings {
            let report = self
                .reports
                .get(finding.report_index)
                .ok_or_else(|| anyhow::anyhow!("finding cites a missing report"))?;
            ensure!(
                report
                    .co_mentions
                    .iter()
                    .any(|mention| mention.name == finding.counterparty),
                "finding counterparty was not co-mentioned in its report"
            );
            ensure!(
                match finding.status {
                    Status::Reported => matches!(
                        finding.stage.as_deref(),
                        Some("speculation" | "concrete_interest" | "advanced_talks" | "here_we_go")
                    ),
                    Status::Denied => finding.stage.is_none(),
                },
                "finding stage does not match its status"
            );
            ensure!(
                !finding.evidence_quote.trim().is_empty()
                    && finding.evidence_quote.chars().count()
                        <= super::prompt::EVIDENCE_QUOTE_MAX_CHARS
                    && (report.headline.contains(&finding.evidence_quote)
                        || report.publisher_excerpt.contains(&finding.evidence_quote)),
                "finding quote is not an exact publisher span"
            );
            ensure!(
                crate::tools::fresh::names_entity(&finding.evidence_quote, &self.subject.name)
                    && crate::tools::fresh::names_entity(
                        &finding.evidence_quote,
                        &finding.counterparty
                    ),
                "finding quote must name both the target and counterparty in full"
            );
            let quote = finding.evidence_quote.to_lowercase();
            ensure!(
                [
                    "transfer", "trade", "move", "moving", "leave", "leaving", "join", "sign",
                    "talks", "deal", "interest", "pursu"
                ]
                .iter()
                .any(|term| quote.contains(term)),
                "finding quote does not state a move"
            );
            if finding.status == Status::Denied {
                ensure!(
                    [
                        "deny talks",
                        "denies talks",
                        "denied talks",
                        "no talks",
                        "not in talks",
                        "no move",
                        "no transfer",
                        "not joining",
                        "not moving",
                        "not signing",
                        "ruled out a move",
                        "rules out a move",
                        "denied a move",
                        "denies a move",
                        "denied transfer",
                        "denies transfer",
                        "rejects a move",
                        "rejected a move"
                    ]
                    .iter()
                    .any(|term| quote.contains(term)),
                    "denied finding requires an explicit denial of that move"
                );
            }
            ensure!(
                seen.insert((finding.report_index, finding.counterparty.as_str())),
                "duplicate source/counterparty finding"
            );
        }
        Ok(Some(reply))
    }
}

#[cfg(test)]
mod source_fact_tests {
    use super::*;
    #[test]
    fn a_snub_is_not_a_denial_and_partial_names_are_not_counterparties() {
        let subject = EntityMeta {
            name: "Chelsea".into(),
            entity_type: "team".into(),
            entity_id: 18,
            sport: "FOOTBALL".into(),
        };
        let reports = vec![Report {
            classifier_world: None,
            publisher: "Wire".into(),
            published_at: None,
            headline: "Chelsea star urged to leave after Xabi Alonso snub".into(),
            publisher_excerpt: String::new(),
            co_mentions: vec![super::super::prompt::Mention {
                name: "Xabi Alonso".into(),
                entity_type: "person".into(),
            }],
        }];
        let parser = ReplyParser {
            subject: &subject,
            reports: &reports,
        };
        let raw=serde_json::json!({"body":"Chelsea has no supported move involving Xabi Alonso.","findings":[{"report_index":0,"counterparty":"Xabi Alonso","status":"denied","stage":null,"evidence_quote":reports[0].headline}]}).to_string();
        assert!(parser
            .parse(&raw)
            .unwrap_err()
            .to_string()
            .contains("explicit denial"));
        assert!(!crate::tools::fresh::names_entity(
            "Cleveland denied talks for Jordan Sample",
            "Cleveland Browns"
        ));
    }
}
