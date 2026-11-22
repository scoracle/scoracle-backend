//! Assemble Journalist identity, voice and fresh reporting into one JSON payload.
use super::TASK;
use crate::harness::route::RouteKey;
pub const MODEL: RouteKey = RouteKey::new("narrative-logic", "NARRATIVE_LOGIC");
pub use super::fresh::CorpusItem;
use crate::harness::model::GenerateOptions;
use crate::tools::meta::EntityMeta;
use serde::{Deserialize, Serialize};

pub const NARRATIVES_PROMPT_VERSION: &str = "n149-full-profile";
pub const NUM_PREDICT: i32 = 900;
pub const NARRATIVES_OUTPUT_CONTRACT_VERSION: &str = "narratives-v18-recap-hook-last";
pub const LOOKBACK_SECONDS: i64 = 72 * 3600;

#[derive(Clone, Debug)]
pub struct Disposition {
    pub article_id: i64,
    pub reason: &'static str,
}
#[derive(Clone, Debug)]
pub struct Assignment {
    pub database_record: Option<crate::tools::meta::DatabaseRecord>,
    pub subject: EntityMeta,
    pub selected: Vec<CorpusItem>,
    pub dispositions: Vec<Disposition>,
    pub input_hash: String,
}

/// Stored parts rebuild through the same assembler as production.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Parts {
    pub subject: EntityMeta,
    pub reports: Vec<CorpusItem>,
    #[serde(default)]
    pub database_record: Option<crate::tools::meta::DatabaseRecord>,
}

impl Parts {
    pub fn parse(&self, raw: &str) -> anyhow::Result<crate::tools::form::JournalistReply> {
        crate::tools::form::decode_journalist(raw, super::fresh::select(&self.reports).len())
    }

    pub fn assemble(&self) -> String {
        assemble(&self.subject, &self.reports, self.database_record.as_ref())
    }
}

/// Budget checks, hashing, fixtures and production all use this payload.
pub fn assemble(
    subject: &EntityMeta,
    reports: &[CorpusItem],
    database_record: Option<&crate::tools::meta::DatabaseRecord>,
) -> String {
    let reports = super::fresh::select(reports);
    crate::tools::prompt::headline_card(TASK, subject, database_record, super::fresh::reports(&reports))
}

pub fn prompt(assignment: &Assignment) -> String {
    assemble(
        &assignment.subject,
        &assignment.selected,
        assignment.database_record.as_ref(),
    )
}

pub fn generation_options(assignment: &Assignment, num_ctx: i32) -> GenerateOptions {
    let count = super::fresh::select(&assignment.selected).len();
    GenerateOptions {
        system: None,
        temperature: Some(0.0),
        num_predict: (count as i32 * 700).clamp(NUM_PREDICT, 3500),
        num_ctx: if num_ctx > 0 {
            num_ctx
        } else {
            crate::harness::model::VOICE_NUM_CTX_PACKET
        },
        json_mode: false,
        // Native shape validation stays in place without form instructions in the payload.
        format_schema: Some(crate::tools::form::journalist_schema(count)),
        format_schema_raw: Some(crate::tools::form::journalist_schema_raw(count)),
    }
}
