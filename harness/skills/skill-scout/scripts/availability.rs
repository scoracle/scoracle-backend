//! Availability-only headline evidence and transactional delivery outcomes.
use crate::tools::{meta::EntityMeta, source::SourceContext};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Availability {
    pub sources: Vec<SourceContext>,
    pub held: Vec<SourceContext>,
    pub redundant: Vec<SourceContext>,
}
impl Availability {
    pub async fn load(connection: &mut PgConnection, subject: &EntityMeta) -> Result<Self> {
        let sources = crate::harness::skills::classifier::delivery::load_for_character_on(
            connection,
            super::manifest::MANIFEST.id.as_str(),
            &subject.entity_type,
            subject.entity_id,
            &subject.sport,
        )
        .await?;
        Ok(Self::pack(
            sources,
            crate::harness::skills::influencer::now(),
        ))
    }
    pub fn pack(sources: Vec<SourceContext>, cutoff: i64) -> Self {
        let mut result = Self::default();
        let mut seen = std::collections::HashSet::new();
        for source in sources {
            if !source.published_at_epoch.is_some_and(|at| at <= cutoff)
                || !source.classifier_world.as_ref().is_some_and(|w| {
                    w["target_identity_resolved"] == true
                        && w["gate"]["characters"]["scout"] == true
                })
                || crate::harness::skills::influencer::fresh::source_disposition(&source.context)
                    .is_some()
            {
                result.held.push(source);
            } else if !seen.insert(crate::tools::fresh::evidence_key(
                &source.source,
                &source.context,
            )) {
                result.redundant.push(source);
            } else {
                result.sources.push(source);
            }
        }
        result
    }
    pub async fn settle(
        &self,
        connection: &mut PgConnection,
        item: &crate::harness::queue::work::Item,
        used: bool,
    ) -> Result<()> {
        let all: Vec<_> = self
            .sources
            .iter()
            .chain(&self.held)
            .chain(&self.redundant)
            .cloned()
            .collect();
        if all.is_empty() {
            return Ok(());
        }
        // Source locks and identity checks are the same ones used by the other headline consumers.
        // The caller supplies its claim-fenced transaction through this connection.
        let current = crate::harness::skills::classifier::delivery::load_for_character_on(
            connection,
            super::manifest::MANIFEST.id.as_str(),
            &item.entity_type,
            item.entity_id_i32()?,
            &item.sport,
        )
        .await?;
        anyhow::ensure!(
            all.iter().all(|s| current.contains(s)),
            "Scout availability source changed"
        );
        for (sources, status, reason) in [
            (&self.sources, if used { "used" } else { "abstained" }, None),
            (
                &self.held,
                "held",
                Some("unusable_or_oversized_availability"),
            ),
            (&self.redundant, "redundant", Some("known_copy")),
        ] {
            let (spam, accepted): (Vec<_>, Vec<_>) = sources
                .iter()
                .partition(|s| crate::tools::source::is_headline_spam(&s.context));
            let spam_ids: Vec<_> = spam.iter().map(|s| s.classification_id).collect();
            crate::harness::skills::classifier::delivery::settle(
                connection,
                super::manifest::MANIFEST.id.as_str(),
                &spam_ids,
                "abstained",
                Some("livestream_spam"),
                serde_json::json!({}),
            )
            .await?;
            let ids: Vec<_> = accepted.iter().map(|s| s.classification_id).collect();
            crate::harness::skills::classifier::delivery::settle(
                connection,
                super::manifest::MANIFEST.id.as_str(),
                &ids,
                status,
                reason,
                serde_json::json!({"contract":crate::harness::skills::classifier::AVAILABILITY_CONTRACT}),
            )
            .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delivery_headlines_do_not_change_the_stats_card_or_its_fingerprint() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/quality/rating/synthetic-strong.json"
        )))
        .unwrap();
        let mut parts: super::super::request::Parts =
            serde_json::from_value(fixture["parts"].clone()).unwrap();
        let prompt = parts.render();
        let components = parts.input_components(serde_json::json!({}));
        parts.availability.sources.push(SourceContext {
            classification_id: 1,
            article_id: 1,
            headline: "Kai Rowan may return".into(),
            context: "Kai Rowan may return".into(),
            source: "Wire".into(),
            published_at_epoch: Some(100),
            classifier_world: None,
        });
        assert_eq!(parts.render(), prompt);
        assert_eq!(parts.input_components(serde_json::json!({})), components);
        assert!(!prompt.contains("availability"));
    }

    #[test]
    fn only_current_resolved_availability_enters_a_bounded_profile() {
        let source = |id| SourceContext {
            classification_id: id,
            article_id: id,
            headline: "Kai Rowan may return".into(),
            context: "Kai Rowan may return".into(),
            source: "Wire".into(),
            published_at_epoch: Some(100),
            classifier_world: Some(
                serde_json::json!({"target_identity_resolved":true,"gate":{"characters":{"scout":true}}}),
            ),
        };
        let mut future = source(3);
        future.published_at_epoch = Some(201);
        let mut irrelevant = source(4);
        irrelevant.classifier_world.as_mut().unwrap()["gate"]["characters"]["scout"] =
            serde_json::json!(false);
        let mut spam = source(5);
        spam.context = "[+[LIVE@STREAMs]+] Cedar FC vs Maple United".into();
        let a = Availability::pack(vec![source(1), source(2), future, irrelevant, spam], 200);
        assert_eq!(a.sources.len(), 1);
        assert_eq!(a.redundant.len(), 1);
        assert_eq!(a.held.len(), 3);
        assert_eq!(a.sources[0].headline, "Kai Rowan may return");
    }
}

include!("../../../tests/internal/scout/availability.rs");
