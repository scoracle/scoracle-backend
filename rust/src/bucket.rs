//! The article transfer/non-transfer bucket domain type.
//!
//! Historically the candle classified this (a `BucketClassifier` + topic-heat clustering).
//! Post-refactor the candle is a pure relevance + novelty gate; the transfer/non-transfer judgment
//! moves to the narratives junction (the Journalist labels each article it reads, as the tail of the
//! read). This enum is the shared representation of that label — written by narratives, read by the
//! transfers stage and serving.

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
}
