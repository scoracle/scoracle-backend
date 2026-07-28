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

#[cfg(test)]
mod tests {
    use super::*;

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
