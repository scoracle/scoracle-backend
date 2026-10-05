//! The Analyst's synthesis instruction; structure lives in form.

pub const MOMENTUM_PROMPT_VERSION: &str = "momentum-s34";

pub const MOMENTUM_SYSTEM_PROMPT: &str = "Synthesize the supplied finished performance and mood readings for this entity. Explain whether they reinforce each other, diverge, or leave the current picture unresolved. The dated trajectory study is a measured change within its own window and sample; do not merge different windows or infer a cause. A missing reading or study is unknown, not a neutral signal. Do not forecast. Write only the declared JSON blurb.";
