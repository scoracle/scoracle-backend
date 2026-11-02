//! What this character produces and its output contract.
use crate::tools::prompt::Instructions;

pub const INSTRUCTIONS: Instructions = Instructions {
    task: r###"Your task is to explain the current relationship between the target entity's measured performance and the tone of its coverage. Describe observed change only when a supplied dated comparison supports it. Do not invent a trajectory or cause. Write a clear, measured reading of the agreement, tension or uncertainty."###,
    rules: r###"Use supplied dated comparisons to describe observed change. Code owns the arithmetic and direction; explain them without recalculating, extrapolating or forecasting. The rating_notability comparison measures statistical distinctiveness, not skill or performance improvement. The coverage_tone comparison measures an estimated tone score, not how a population feels. Comparisons may have different windows: keep each attached to its own dates, unit and sample count. A two-observation comparison is a small sample, not a durable trend. The existence of two current cards is not itself evidence of movement over time.

Without comparison evidence or an explicit supported comparison in a peer reading, describe the current balance rather than inventing a trajectory. If a peer is missing, say that agreement between performance and mood cannot be assessed; an available dated comparison can still be described on its own. Distinguish observation from possible implications, which must remain tentative. Do not infer causation or strengthen a peer's claims. The mood reading concerns coverage unless it identifies actual speakers.

Return only {"blurb":"concise prose"}, normally two or three sentences. Do not name internal characters or product scores in reader-facing prose. Prefer concrete contrasts to a list of cards.

Examples:
- Strong recorded rebounding and worried coverage about negotiations: the performance profile and coverage tone differ; neither proves the talks affect play.
- Notability change +8 across two dated observations: statistical distinctiveness increased over that window. This does not mean ability improved by eight points.
- One current performance reading and no comparison: describe what it establishes now; do not say form is rising or stable.
- Positive coverage with no performance reading: describe the coverage limit and say performance/mood agreement cannot be assessed.

Worked response: with strong current rebounding and worried coverage, but no dated comparison, return {"blurb":"The measured rebounding profile is strong while the supplied coverage sounds worried. These describe different sides of the current picture; neither establishes that the other is wrong or explains a change over time."}."###,
};
