//! Article transfer/non-transfer buckets and routing-tag projection.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArticleBucket {
    Transfer,
    NonTransfer,
}

impl ArticleBucket {
    pub fn as_db(self) -> &'static str {
        match self {
            ArticleBucket::Transfer => "transfer",
            ArticleBucket::NonTransfer => "non_transfer",
        }
    }

    pub fn from_model_tag(tag: &str) -> Option<Self> {
        match tag.trim().to_lowercase().as_str() {
            "transfer" | "trade" | "transfers" | "trades" => Some(ArticleBucket::Transfer),
            "other" | "non_transfer" | "non-transfer" | "nontransfer" => {
                Some(ArticleBucket::NonTransfer)
            }
            _ => None,
        }
    }

    /// Projects the Editor's topic taxonomy onto the transfer routing decision.
    ///
    /// **`contract` and `roster` are NonTransfer, and that is the arguable call.** A contract
    /// renewal is a player staying, which is the opposite of the move the Insider hunts; a roster
    /// note is squad administration. Both sit adjacent enough that the old n9 Journalist sometimes
    /// tagged them `transfer` (20 and a handful respectively in the week before the move). Erring
    /// toward NonTransfer keeps the Insider's candidate set honest, and the cost of a miss is
    /// bounded: transfer heat is recomputed from the pair corpus in SQL, so one mislabelled
    /// article delays a rumor rather than hiding it.
    ///
    /// `None` for anything unrecognised — including the off-vocabulary values the model does emit
    /// A NULL bucket is "not yet judged", so unknown topics fail open into candidacy.
    pub fn from_story_type(story_type: &str) -> Option<Self> {
        match story_type.trim().to_lowercase().as_str() {
            "transfer" => Some(ArticleBucket::Transfer),
            "injury" | "performance" | "fixture" | "roster" | "contract" | "general" => {
                Some(ArticleBucket::NonTransfer)
            }
            _ => None,
        }
    }
}

/// Projects the Editor's topic taxonomy onto content facts voices can subscribe to.
///
/// **Tags are content facts, not voice names.** Which stage wants `injury` lives in
/// `stage_routing_subscriptions`, as data. That keeps the routing decision an INSERT rather than a
/// code change, and it means this function never has to know the cast.
///
/// Tags are derived from `story_type`; the model does not choose downstream voices.
///
/// Off-vocabulary `story_type` values yield NO tags — the same fail-open discipline as
/// `from_story_type` returning `None`. An unknown topic routes to nobody rather than being guessed
/// onto a voice, and the article still reaches The Journalist through the corpus, which is not
/// tag-gated.
///
pub fn routing_tags_from_story_type(story_type: &str) -> Vec<&'static str> {
    match story_type.trim().to_lowercase().as_str() {
        "transfer" => vec!["transfer"],
        "injury" => vec!["injury"],
        // Suspension also carries injury so existing availability subscribers receive it.
        "suspension" => vec!["injury", "suspension"],
        "roster" => vec!["roster"],
        "contract" => vec!["contract"],
        "performance" => vec!["performance"],
        "fixture" => vec!["fixture"],
        "general" => vec!["general"],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every value in the Editor's schema enum must produce at least one tag, for the same reason
    /// `every_editor_story_type_projects_to_a_bucket` exists: a topic this mapping does not know
    /// routes to nobody, silently and forever. A new topic in the Editor's taxonomy has to be
    /// answered here too, and this test is what says so.
    #[test]
    fn every_editor_story_type_produces_a_tag() {
        for st in [
            "transfer",
            "injury",
            "performance",
            "fixture",
            "roster",
            "contract",
            "general",
        ] {
            assert!(
                !routing_tags_from_story_type(st).is_empty(),
                "story_type {st} must produce at least one routing tag"
            );
        }
    }

    /// Off-vocabulary values route to nobody rather than being guessed onto a voice. The model does
    /// emit them — `irrelevant` turned up 43 times in a week despite not being in the schema enum.
    #[test]
    fn off_vocabulary_story_types_produce_no_tags() {
        assert!(routing_tags_from_story_type("irrelevant").is_empty());
        assert!(routing_tags_from_story_type("").is_empty());
    }

    /// The tag projection must agree with the bucket projection on the one routing decision that
    /// is live today, or mig 175 and mig 197 would disagree about the same article at cutover.
    #[test]
    fn tags_agree_with_bucket_on_transfer() {
        for st in [
            "transfer",
            "injury",
            "performance",
            "fixture",
            "roster",
            "contract",
            "general",
        ] {
            let is_transfer_bucket =
                ArticleBucket::from_story_type(st) == Some(ArticleBucket::Transfer);
            let has_transfer_tag = routing_tags_from_story_type(st).contains(&"transfer");
            assert_eq!(
                is_transfer_bucket, has_transfer_tag,
                "bucket and tags disagree about transfer for story_type {st}"
            );
        }
    }

    #[test]
    fn tags_normalize_case_and_whitespace() {
        assert_eq!(
            routing_tags_from_story_type("  TRANSFER  "),
            vec!["transfer"]
        );
    }

    #[test]
    fn model_bucket_tags_parse() {
        assert_eq!(
            ArticleBucket::from_model_tag("transfer"),
            Some(ArticleBucket::Transfer)
        );
        assert_eq!(
            ArticleBucket::from_model_tag("other"),
            Some(ArticleBucket::NonTransfer)
        );
        assert_eq!(ArticleBucket::from_model_tag("unknown"), None);
    }

    /// Every value in The Editor's schema enum must project to a decision. A story_type the
    /// mapping does not know would leave the article permanently unbucketed, which reads as
    /// "eligible" forever — so a new topic added to the Editor's taxonomy has to be answered here
    /// too, and this test is what says so.
    #[test]
    fn every_editor_story_type_projects_to_a_bucket() {
        for st in [
            "injury",
            "performance",
            "fixture",
            "roster",
            "contract",
            "general",
        ] {
            assert_eq!(
                ArticleBucket::from_story_type(st),
                Some(ArticleBucket::NonTransfer),
                "story_type {st} must project to a bucket"
            );
        }
        assert_eq!(
            ArticleBucket::from_story_type("transfer"),
            Some(ArticleBucket::Transfer)
        );
    }

    /// The model emits values outside its own schema enum (`irrelevant`, 43 times in a week).
    /// Those must stay NULL rather than being guessed into a bucket: NULL is read as eligible by
    /// the transfers candidate query, so an unknown topic fails open into candidacy.
    #[test]
    fn off_vocabulary_story_types_stay_unjudged() {
        assert_eq!(ArticleBucket::from_story_type("irrelevant"), None);
        assert_eq!(ArticleBucket::from_story_type(""), None);
        assert_eq!(
            ArticleBucket::from_story_type("  TRANSFER  "),
            Some(ArticleBucket::Transfer)
        );
    }
}
