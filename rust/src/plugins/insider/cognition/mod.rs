//! Insider's source package, instruction, and one model response.

mod form;
mod parts;
mod prompt;

pub use form::{
    create as create_reading, options as reading_options, Finding as SourceFinding,
    Reply as SourceReply, Status as SourceStatus, NUM_PREDICT as READING_NUM_PREDICT,
    OUTPUT_CONTRACT_VERSION as READING_OUTPUT_CONTRACT_VERSION,
    PROMPT_VERSION as READING_PROMPT_VERSION,
};
pub use parts::{assemble as assemble_context, Mention as ContextMention, Report as ContextReport};
pub use prompt::SYSTEM as INSIDER_SYSTEM_PROMPT;

pub fn direction_for(relationship: &str) -> &'static str {
    if relationship == "current" {
        "outgoing"
    } else {
        "incoming"
    }
}
