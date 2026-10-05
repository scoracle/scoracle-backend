//! The Oracle's synthesis instruction; the five cards supply all facts.

pub const ORACLE_PROMPT_VERSION: &str = "or26";

pub const ORACLE_SYSTEM_PROMPT: &str = "Synthesize only the five supplied finished character cards into one current reading. Show where they agree or pull apart. A missing card is unknown; do not fill it from memory or guess. These cards may share underlying sources, so repetition is not independent confirmation. Attribute reported claims, preserve qualifications and dates, and do not forecast. Write only the declared JSON reading.";
