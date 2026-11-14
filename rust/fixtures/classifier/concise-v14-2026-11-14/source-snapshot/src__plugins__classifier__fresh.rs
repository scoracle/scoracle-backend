//! Source-only decision context and the rules for interpreting it.
use crate::tools::prompt::{Context, Guide};
use serde::Serialize;

pub(super) fn context<'a>(headline: &'a str, publisher: &'a str) -> Context<impl Serialize + 'a> {
    let publisher = publisher.trim();
    // URL-valued feed labels are provenance, not a publisher identity.
    let publisher = if reqwest::Url::parse(publisher)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
    {
        ""
    } else {
        publisher
    };
    #[derive(Serialize)]
    struct Headline<'a> {
        headline: &'a str,
        #[serde(skip_serializing_if = "str::is_empty")]
        publisher: &'a str,
    }
    Context {
        guide: Guide {
            relationship: "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
            fields: &[],
        },
        data: Headline {
            headline,
            publisher,
        },
    }
}
