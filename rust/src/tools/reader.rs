//! Read unchanged retained publisher text. Receipt verification precedes this tool.
use crate::plugins::harvester::cognition::{full_article, Excerpt};

pub fn read(body: &str) -> Excerpt {
    full_article(body)
}

pub fn needs_article(plugin_id: &str) -> bool {
    [
        crate::plugins::journalist::manifest::MANIFEST.id,
        crate::plugins::influencer::manifest::MANIFEST.id,
        crate::plugins::insider::manifest::MANIFEST.id,
    ]
    .iter()
    .any(|id| id.as_str() == plugin_id)
}

#[cfg(test)]
mod tests {
    #[test]
    fn reader_keeps_late_quotes_and_qualifications() {
        let body = "  First.\n\nSecond.\n\nThird.\n\n‘Nobody is sensitive,’ said Graham.\n\nHe qualified that account.  ";
        let read = super::read(body);
        assert_eq!(body.get(read.start..read.end), Some(read.text.as_str()));
        assert!(read.text.ends_with("He qualified that account."));
        assert!(super::needs_article(
            crate::plugins::influencer::manifest::MANIFEST.id.as_str()
        ));
        assert!(!super::needs_article(
            crate::plugins::scout::manifest::MANIFEST.id.as_str()
        ));
    }
}
