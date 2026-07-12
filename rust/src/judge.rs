//! Judge — the LLM-as-judge quality dimension (lens quality plan Phase 4).
//!
//! The fixture harness measures STRUCTURE (required lines, ranges, substrings); it cannot tell
//! a specific, well-grounded sentence from a correct-shaped generic one. This module adds that
//! axis: a small, independent critic model scores a (evidence, reply) pair on three 1-5 scales
//! — specificity, grounding, non-genericness — and names the single worst claim.
//!
//! STRICTLY OFF the production path: only `bin/eval --judge` constructs a judge backend
//! (`COGNITION_JUDGE_MODEL`, default `gemma3:4b` — deliberately a model that serves NO
//! production role, so neither production model grades its own homework). The judge never
//! competes for the GPU during live drains and its verdicts are advisory eval output, never
//! persisted product truth.

use crate::ollama::GenerateOptions;
use crate::route::Inference;
use anyhow::Result;
use serde::Deserialize;

pub const JUDGE_PROMPT_VERSION: &str = "judge-v1";

pub const JUDGE_NUM_PREDICT: i32 = 300;

pub const JUDGE_SYSTEM_PROMPT: &str = r#"You are an exacting quality judge for sports-analysis text generated from supplied evidence.

Score the REPLY strictly against the EVIDENCE on three axes, each an integer 1-5:
- specificity: 5 = concrete names, numbers, and events; 1 = vague filler that asserts nothing checkable.
- grounding: 5 = every factual claim is traceable to the EVIDENCE; 1 = invented facts. You know NOTHING beyond the EVIDENCE — treat any name, number, fee, or event not present in it as invented.
- non_generic: 5 = the text could describe ONLY this entity in this moment; 1 = template prose that would fit any player or team.

Be strict: professional-sounding filler is exactly what these scores exist to catch.

Reply with ONLY this JSON object, nothing else:
{"specificity": <1-5>, "grounding": <1-5>, "non_generic": <1-5>, "worst_claim": "<the single worst ungrounded or generic claim, or an empty string>"}"#;

/// One judged reply. All three scores are validated 1-5 at parse time.
#[derive(Clone, Debug, Deserialize)]
pub struct JudgeVerdict {
    pub specificity: i32,
    pub grounding: i32,
    pub non_generic: i32,
    #[serde(default)]
    pub worst_claim: String,
}

pub fn build_judge_prompt(task_name: &str, evidence: &str, reply: &str) -> String {
    format!(
        "Task being judged: {task_name}\n\n=== EVIDENCE (the exact input the generating model saw) ===\n{evidence}\n\n=== REPLY (the text to judge) ===\n{reply}\n\nJudge now."
    )
}

/// parse_judge_verdict is fail-closed: anything but a complete, in-range JSON verdict is `None`
/// (the case is reported unjudged, never defaulted).
pub fn parse_judge_verdict(raw: &str) -> Option<JudgeVerdict> {
    let v: JudgeVerdict = serde_json::from_str(raw.trim()).ok()?;
    let ok = |n: i32| (1..=5).contains(&n);
    (ok(v.specificity) && ok(v.grounding) && ok(v.non_generic)).then_some(v)
}

/// judge_reply scores one (evidence, reply) pair. Temp 0 — the judge should be a ruler, not a
/// sampler. An empty reply is auto-scored floor (nothing to judge is the worst outcome on every
/// axis) without a model call.
pub async fn judge_reply(
    backend: &dyn Inference,
    task_name: &str,
    evidence: &str,
    reply: &str,
) -> Result<Option<JudgeVerdict>> {
    if reply.trim().is_empty() {
        return Ok(Some(JudgeVerdict {
            specificity: 1,
            grounding: 1,
            non_generic: 1,
            worst_claim: "(empty reply)".to_string(),
        }));
    }
    let opts = GenerateOptions {
        system: Some(JUDGE_SYSTEM_PROMPT.to_string()),
        temperature: Some(0.0),
        num_predict: JUDGE_NUM_PREDICT,
        num_ctx: 8192, // evidence + reply can exceed the 4096 server default (narratives)
        json_mode: true,
    };
    let prompt = build_judge_prompt(task_name, evidence, reply);
    let (gen, _) = backend.generate(&prompt, &opts).await?;
    Ok(parse_judge_verdict(&gen.response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_verdict_and_rejects_out_of_range() {
        let v = parse_judge_verdict(
            r#"{"specificity": 4, "grounding": 5, "non_generic": 3, "worst_claim": ""}"#,
        )
        .unwrap();
        assert_eq!((v.specificity, v.grounding, v.non_generic), (4, 5, 3));
        // Out-of-range or incomplete → None, never clamped or defaulted.
        assert!(parse_judge_verdict(r#"{"specificity": 9, "grounding": 5, "non_generic": 3}"#)
            .is_none());
        assert!(parse_judge_verdict(r#"{"grounding": 5}"#).is_none());
        assert!(parse_judge_verdict("not json").is_none());
    }
}
