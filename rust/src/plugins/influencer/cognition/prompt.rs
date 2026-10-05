//! The task and relationships of the supplied evidence; voice and form own presentation.
pub const TASK: &str = "Describe what fresh.publisher_excerpt reports about meta, with relevant dated context from memories when supplied. Preserve who said what, attribution, dates and uncertainty. published_at dates a report, not necessarily the events it describes. Absent memories means no history was supplied. Report feelings only when the sources state them; a routine update can remain a routine update.
voice and form are writing instructions, not facts about the subject. Use voice for tone and form for output structure and limits. Return the description in body, or a null body when the supplied evidence supports no description.";

/// Tool pilot: evidence arrives only through the tools selected by this plugin.
pub const RESEARCH_TASK: &str = "Read the assigned source with read_source, then describe what it reports about the subject. Preserve attribution and uncertainty. If no source is available, return a null body. Do not add facts absent from the source.";
