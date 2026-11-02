//! Assemble Journalist identity, voice and fresh reporting into one JSON payload.
use crate::harness::route::RouteKey;
pub const MODEL: RouteKey = RouteKey::new("narrative-logic", "NARRATIVE_LOGIC");
pub use super::fresh::CorpusItem;
use crate::harness::model::GenerateOptions;
use crate::tools::meta::EntityMeta;
use serde::{Deserialize, Serialize};

/// The template trims surrounding whitespace; use the same text for hashing and replay.
pub fn instructions() -> String {
    include_str!("instructions.txt").trim().to_owned()
}

pub const NARRATIVES_PROMPT_VERSION: &str = "n119-detailed-summary";
pub const NUM_PREDICT: i32 = 900;
pub const NARRATIVES_OUTPUT_CONTRACT_VERSION: &str = "narratives-v16-visible-report-facts";
pub const LOOKBACK_SECONDS: i64 = 72 * 3600;

#[derive(Clone, Debug)]
pub struct Disposition {
    pub article_id: i64,
    pub reason: &'static str,
}
#[derive(Clone, Debug)]
pub struct Assignment {
    pub database_record: Option<String>,
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
    pub database_record: Option<String>,
}

impl Parts {
    pub fn parse(&self, raw: &str) -> anyhow::Result<crate::tools::form::JournalistReply> {
        super::parser::parse_reports(raw, &self.reports)
    }

    pub fn assemble(&self) -> String {
        assemble(
            &self.subject,
            &self.reports,
            self.database_record.as_deref(),
        )
    }
}

/// Budget checks, hashing, fixtures and production all use this payload.
pub fn assemble(
    subject: &EntityMeta,
    reports: &[CorpusItem],
    database_record: Option<&str>,
) -> String {
    #[derive(Serialize)]
    struct Meta<'a> {
        #[serde(flatten)]
        identity: crate::tools::meta::WritingIdentity<'a>,
        database_record: Option<&'a str>,
    }
    #[derive(Serialize)]
    struct Input<'a> {
        meta: Meta<'a>,
        fresh: Vec<super::fresh::Report<'a>>,
        tone: &'static str,
    }
    serde_json::to_string(&Input {
        meta: Meta {
            identity: subject.for_writing(),
            database_record,
        },
        fresh: super::fresh::reports(reports),
        tone: super::voice::VOICE,
    })
    .expect("journalist payload serializes")
}

pub fn prompt(assignment: &Assignment) -> String {
    assemble(
        &assignment.subject,
        &assignment.selected,
        assignment.database_record.as_deref(),
    )
}

pub fn generation_options(assignment: &Assignment, num_ctx: i32) -> GenerateOptions {
    GenerateOptions {
        system: Some(instructions()),
        temperature: Some(0.0),
        num_predict: (assignment.selected.len().min(3) as i32 * 700).max(NUM_PREDICT),
        num_ctx: if num_ctx > 0 {
            num_ctx
        } else {
            crate::harness::model::VOICE_NUM_CTX_PACKET
        },
        json_mode: false,
        // Native shape validation stays in place without form instructions in the payload.
        format_schema: Some(crate::tools::form::journalist_schema(
            assignment.selected.len(),
        )),
        format_schema_raw: None,
    }
}
