//! Current measurements and complete reporting; no historical context.
use super::performance::Profile;
use crate::tools::{fresh::Report, source::SourceContext};
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct Evidence<'a> {
    #[serde(flatten)]
    profile: &'a Profile,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    reporting: Vec<Report<'a>>,
}

pub(super) fn evidence<'a>(profile: &'a Profile, reporting: &'a [SourceContext]) -> Evidence<'a> {
    Evidence {
        profile,
        reporting: crate::tools::fresh::reports(reporting),
    }
}
