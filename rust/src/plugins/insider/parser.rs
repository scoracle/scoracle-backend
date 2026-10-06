//! Exact source spans, named counterparties and served-prose validation.
use super::prompt::Report;
use super::{Reply, Status};
use crate::plugins::meta::EntityMeta;
use crate::studio::Parser;
use anyhow::{ensure, Result};

pub(super) struct ReplyParser<'a> {
    pub(super) subject: &'a EntityMeta,
    pub(super) reports: &'a [Report],
}

impl Parser<Reply> for ReplyParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<Reply>> {
        let mut reply: Reply = serde_json::from_str(raw)?;
        reply.body = crate::plugins::support::guards::clean_served_prose(
            &crate::plugins::support::form::normalize_body(&reply.body),
        );
        crate::plugins::support::form::validate_body(&reply.body)?;
        ensure!(
            crate::plugins::support::guards::title_names_entity(&reply.body, &self.subject.name),
            "Insider reading does not name the subject"
        );
        ensure!(
            !crate::plugins::support::guards::has_bookkeeping_citation(&reply.body)
                && crate::plugins::support::guards::first_product_name(&reply.body).is_none()
                && !crate::plugins::support::guards::has_foreign_script(&reply.body),
            "Insider reading violates served prose guard"
        );
        ensure!(
            reply.findings.len() <= self.reports.len() * 8,
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
                    && finding.evidence_quote.chars().count() <= 500
                    && (report.headline.contains(&finding.evidence_quote)
                        || report.publisher_excerpt.contains(&finding.evidence_quote)),
                "finding quote is not an exact publisher span"
            );
            ensure!(
                seen.insert((finding.report_index, finding.counterparty.as_str())),
                "duplicate source/counterparty finding"
            );
        }
        Ok(Some(reply))
    }
}
