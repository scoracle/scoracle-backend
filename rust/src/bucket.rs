//! The article transfer/non-transfer bucket domain type.
//!
//! Historically the candle classified this (a `BucketClassifier` + topic-heat clustering). Then the
//! judgment moved to the narratives junction, where the Journalist labelled every article in its
//! corpus as the tail of its generation (the n9 `article_buckets` section).
//!
//! It now belongs to **The Editor**, which is where sorting work belongs. Three reasons, in order
//! of how much they cost:
//!
//! 1. **It was the wrong character.** Labelling articles is assignment-desk work. The Journalist's
//!    job is voicing the developing story, and it was spending most of its generation on
//!    bookkeeping instead — measured over real generations, the prose in a full six-storyline
//!    generation never exceeded 887 tokens while the generation itself reached 2,567.
//! 2. **It was the wrong host.** n9 ran on the Mac's saturated 14B, once per corpus article. The
//!    Editor runs on the 1070 Ti, which has headroom, and pays nothing extra: `story_type` is a
//!    field it already emits.
//! 3. **It was the worse-informed judge.** The Editor reads the FULL body; the Journalist saw a
//!    900-byte evidence blurb of it.
//!
//! What made the move safe is a property that is easy to miss: since Phase 2 retired auto-vetting,
//! a player link is vetted ONLY by an Editor read, and `load_candidates` requires a vetted player
//! link. So bucket can only ever influence transfers on an article the Editor has already read —
//! the labels on unread articles (14,059 of 16,395 in the week before the move) were dead weight.
//!
//! Written by the article_read junction, read by the transfers stage and serving. A change to
//! 'transfer' also fires the mig-175 trigger, which enqueues transfers for the article's teams.

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

    /// from_story_type projects The Editor's seven-value topic taxonomy onto this two-value
    /// routing decision. Deliberately NOT a rename of [`from_model_tag`]: that one reads a field
    /// whose whole purpose was this label, where an unrecognised value is off-vocabulary noise.
    /// `story_type` is a topic, and the projection is a judgment about which topics can carry a
    /// transfer.
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
    /// (`irrelevant` turned up 43 times in a week despite not being in the schema enum). A NULL
    /// bucket is "not yet judged", and the transfers candidate query reads it as eligible, so an
    /// unknown topic fails OPEN into candidacy rather than being silently excluded from it.
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

/// routing_tags_from_story_type projects The Editor's topic taxonomy onto the CONTENT FACTS a
/// voice can subscribe to (`news_articles.routing_tags`, mig 197).
///
/// This is the multi-valued successor to [`ArticleBucket::from_story_type`], and the difference is
/// the entire point of mig 197: `bucket` can say "transfer" OR "injury" and never both, so a story
/// could only ever reach one voice. A tag set lets the Diomande story reach the Insider as a
/// transfer AND the Influencer as a story about a player choosing one club over another, with
/// neither waiting on the other.
///
/// **Tags are content facts, not voice names.** Which stage wants `injury` lives in
/// `stage_routing_subscriptions`, as data. That keeps the routing decision an INSERT rather than a
/// code change, and it means this function never has to know the cast.
///
/// **Derived, never asked.** `story_type` is already emitted; the projection happens here. Asking
/// the model which voices should see an article is a judgment call, and ar3/ar5/ar6 are three
/// measured demonstrations that a 4B will not render those reliably.
///
/// Off-vocabulary `story_type` values yield NO tags — the same fail-open discipline as
/// `from_story_type` returning `None`. An unknown topic routes to nobody rather than being guessed
/// onto a voice, and the article still reaches The Journalist through the corpus, which is not
/// tag-gated.
///
/// The emotional register (C2) becomes an ADDITIONAL tag here once the Editor emits it. It is
/// deliberately not modelled yet: the field does not exist, and inventing its vocabulary before
/// the contract that produces it is how you get a tag nothing ever sets.
pub fn routing_tags_from_story_type(story_type: &str) -> Vec<&'static str> {
    match story_type.trim().to_lowercase().as_str() {
        "transfer" => vec!["transfer"],
        "injury" => vec!["injury"],
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
        assert_eq!(routing_tags_from_story_type("  TRANSFER  "), vec!["transfer"]);
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
        assert_eq!(ArticleBucket::from_story_type("  TRANSFER  "), Some(ArticleBucket::Transfer));
    }
}
