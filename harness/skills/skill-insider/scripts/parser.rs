use super::{request::Report, Reply, Status};
use crate::harness::Parser;
use crate::tools::meta::EntityMeta;
use anyhow::Result;

pub(super) struct ReplyParser<'a> {
    pub(super) subject: &'a EntityMeta,
    pub(super) reports: &'a [Report],
}

impl Parser<Reply> for ReplyParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<Reply>> {
        let mut reply: Reply = serde_json::from_str(raw)?;
        reply.headline = crate::tools::form::card_hook(&reply.headline);
        reply.body = crate::tools::form::normalize_body(&reply.body);
        crate::tools::form::validate_body(&reply.body)?;
        reply.card_score = crate::tools::source::activity_score(self.reports.len());
        // A bad link cannot erase a finished briefing.
        reply.findings.retain(|f| {
            self.reports.get(f.report_index).is_some_and(|report| {
                !f.evidence_quote.is_empty()
                    && (report.headline.contains(&f.evidence_quote)
                        || report.publisher_excerpt.contains(&f.evidence_quote))
                    && report
                        .co_mentions
                        .iter()
                        .any(|m| m.name == f.counterparty && m.name != self.subject.name)
                    && match f.status {
                        Status::Reported => matches!(
                            f.stage.as_deref(),
                            Some(
                                "speculation"
                                    | "concrete_interest"
                                    | "advanced_talks"
                                    | "here_we_go"
                            )
                        ),
                        Status::Denied => f.stage.is_none(),
                    }
            })
        });
        Ok(Some(reply))
    }
}
