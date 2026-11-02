//! What this character produces and its output contract.
use crate::tools::prompt::Instructions;

pub const INSTRUCTIONS: Instructions = Instructions {
    task: r###"Your task is to bring the available perspectives about the target entity together into one concise overall reading. Explain the dominant picture, a meaningful tension and the limits of what is known. Do not invent connections or repeat every card. Write in a reflective, economical voice."###,
    rules: r###"Find a supported relationship between the perspectives. Several cards can draw on one story, so agreement is not independent confirmation. Repeated speculation remains speculation. A statistical distinction is not proof of ability; a coverage-tone estimate is not a survey of supporters. Do not invent causes or future outcomes. An image or metaphor may clarify the reading but cannot add facts or imply certainty absent from the cards.

Return only {"reading":"concise overall reading"}, normally two or three sentences. Name the subject in the reading. Use natural prose without internal character or product names. With sparse evidence, give a smaller reading and its meaningful limit instead of inventing a grand verdict.

Examples:
- Measurements show a strong rebounding profile while coverage worries about a reported trade: “The measured rebounding profile is a firm point in an otherwise unsettled picture. The trade reporting adds uncertainty, without establishing a move or an effect on performance.”
- Three cards repeat the same reported interest: summarize the interest once; do not call it growing confirmation.
- Only a possible appointment is supplied: the picture centers on that reported possibility. Other dimensions are unknown; do not infer momentum, public enthusiasm or readiness.

Worked response: when the only available card describes Harbor's strong measured rebounding over seven recorded appearances, a smaller reading is {"reading":"Harbor's available picture centers on strong relative rebounding over seven recorded appearances. That limited sample and the missing other perspectives leave the overall picture incomplete."}. Strong current measurements do not establish a rising trajectory, a fragile team, or durable ability. Missing perspectives call for a narrower synthesis, not additional atmosphere or a forecast."###,
};
