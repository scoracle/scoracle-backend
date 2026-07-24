//! Judge — the LLM-as-judge quality dimension (lens quality plan Phase 4).
//!
//! The fixture harness measures STRUCTURE (required lines, ranges, substrings); it cannot tell
//! a specific, well-grounded sentence from a correct-shaped generic one. This module adds that
//! axis: a small, independent critic model scores a (evidence, reply) pair on 1-5 scales
//! — specificity, grounding, non-genericness — and names the single worst claim.
//!
//! judge-v2 (Characters Phase B) adds the VOICE axis: when the caller supplies a [`VoiceSpec`]
//! (the cast identity from `lens_parameters`), the judge also scores `voice_fidelity` — "strip
//! the card label: which character wrote this?". Utility calls (graph) pass no spec and keep
//! the three-axis rubric; the axis is judged on the TELLING, never the facts (grounding is its
//! own scale), so a character can't buy voice points with invented color.
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

pub const JUDGE_PROMPT_VERSION: &str = "judge-v2";

pub const JUDGE_NUM_PREDICT: i32 = 300;

/// VoiceSpec names the character a reply is supposed to be told by — the judge's voice-axis
/// input. Callers build it from the lens registry (`lens_parameters().operator`/`mandate`),
/// so the judge and the registry can never disagree about who a stage is.
#[derive(Clone, Copy, Debug)]
pub struct VoiceSpec<'a> {
    pub character: &'a str,
    pub mandate: &'a str,
}

/// judge_system_prompt builds the rubric: three base axes always, plus the `voice_fidelity`
/// axis when a [`VoiceSpec`] is supplied. The JSON template mirrors the axis set exactly, so
/// the grammar schema in `judge_reply` and this prose can't drift apart in shape.
pub fn judge_system_prompt(voice: Option<&VoiceSpec<'_>>) -> String {
    let (voice_axis, voice_key) = match voice {
        Some(v) => (
            format!(
                "\n- voice_fidelity: strip the card label and ask: which character wrote this? 5 = the telling is unmistakably {character} — whose mandate is: {mandate} — in register, cadence, and what it chooses to care about; 1 = anonymous assistant prose that any stage could have emitted. Judge the TELLING only, never the facts — grounding is scored separately, so invented color must not raise this score.",
                character = v.character,
                mandate = v.mandate,
            ),
            r#", "voice_fidelity": <1-5>"#,
        ),
        None => (String::new(), ""),
    };
    format!(
        r#"You are an exacting quality judge for sports-analysis text generated from supplied evidence.

Score the REPLY strictly against the EVIDENCE, each axis an integer 1-5:
- specificity: 5 = concrete names, numbers, and events; 1 = vague filler that asserts nothing checkable.
- grounding: 5 = every factual claim is traceable to the EVIDENCE; 1 = invented facts. You know NOTHING beyond the EVIDENCE — treat any name, number, fee, or event not present in it as invented.
- non_generic: 5 = the text could describe ONLY this entity in this moment; 1 = template prose that would fit any player or team.{voice_axis}

Be strict: professional-sounding filler is exactly what these scores exist to catch.

Reply with ONLY this JSON object, nothing else:
{{"specificity": <1-5>, "grounding": <1-5>, "non_generic": <1-5>{voice_key}, "worst_claim": "<the single worst ungrounded or generic claim, or an empty string>"}}"#
    )
}

/// One judged reply. All present scores are validated 1-5 at parse time; `voice_fidelity` is
/// `Some` only for voice-judged (character) replies — utility replies have no voice to grade.
#[derive(Clone, Debug, Deserialize)]
pub struct JudgeVerdict {
    pub specificity: i32,
    pub grounding: i32,
    pub non_generic: i32,
    #[serde(default)]
    pub voice_fidelity: Option<i32>,
    #[serde(default)]
    pub worst_claim: String,
}

pub fn build_judge_prompt(task_name: &str, evidence: &str, reply: &str) -> String {
    format!(
        "Task being judged: {task_name}\n\n=== EVIDENCE (the exact input the generating model saw) ===\n{evidence}\n\n=== REPLY (the text to judge) ===\n{reply}\n\nJudge now."
    )
}

/// parse_judge_verdict is fail-closed: anything but a complete, in-range JSON verdict is `None`
/// (the case is reported unjudged, never defaulted). A present `voice_fidelity` must be in
/// range; whether it must be PRESENT is the caller's call (`judge_reply` requires it exactly
/// when a [`VoiceSpec`] was supplied).
pub fn parse_judge_verdict(raw: &str) -> Option<JudgeVerdict> {
    let v: JudgeVerdict = serde_json::from_str(raw.trim()).ok()?;
    let ok = |n: i32| (1..=5).contains(&n);
    (ok(v.specificity) && ok(v.grounding) && ok(v.non_generic) && v.voice_fidelity.is_none_or(ok))
        .then_some(v)
}

/// judge_reply scores one (evidence, reply) pair. Temp 0 — the judge should be a ruler, not a
/// sampler. An empty reply is auto-scored floor (nothing to judge is the worst outcome on every
/// axis) without a model call. `voice` turns on the fourth axis: pass the character's spec for
/// cast stages, `None` for utility replies.
pub async fn judge_reply(
    backend: &dyn Inference,
    task_name: &str,
    evidence: &str,
    reply: &str,
    voice: Option<&VoiceSpec<'_>>,
) -> Result<Option<JudgeVerdict>> {
    if reply.trim().is_empty() {
        return Ok(Some(JudgeVerdict {
            specificity: 1,
            grounding: 1,
            non_generic: 1,
            voice_fidelity: voice.map(|_| 1),
            worst_claim: "(empty reply)".to_string(),
        }));
    }
    // Grammar-constrained verdict: the required keys cannot be omitted and the scores
    // cannot leave 1-5, so an "unjudged" outcome means a genuinely broken reply. The
    // voice_fidelity key exists in the schema only when the axis is being judged.
    let score = serde_json::json!({ "type": "integer", "minimum": 1, "maximum": 5 });
    let mut properties = serde_json::json!({
        "specificity":  score.clone(),
        "grounding":    score.clone(),
        "non_generic":  score.clone(),
        "worst_claim":  { "type": "string" }
    });
    let mut required = vec!["specificity", "grounding", "non_generic", "worst_claim"];
    if voice.is_some() {
        properties["voice_fidelity"] = score;
        required.push("voice_fidelity");
    }
    let opts = GenerateOptions {
        system: Some(judge_system_prompt(voice)),
        temperature: Some(0.0),
        num_predict: JUDGE_NUM_PREDICT,
        num_ctx: 8192, // evidence + reply can exceed the 4096 server default (narratives)
        json_mode: false,
        format_schema: Some(serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required
        })),
    };
    let prompt = build_judge_prompt(task_name, evidence, reply);
    let (gen, _) = backend.generate(&prompt, &opts).await?;
    // Fail closed on a missing axis: a voice-judged reply whose verdict lacks the axis is
    // unjudged, never silently three-axis.
    Ok(
        parse_judge_verdict(&gen.response)
            .filter(|v| voice.is_none() || v.voice_fidelity.is_some()),
    )
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
        assert!(v.voice_fidelity.is_none()); // three-axis verdict stays three-axis
                                             // Out-of-range or incomplete → None, never clamped or defaulted.
        assert!(
            parse_judge_verdict(r#"{"specificity": 9, "grounding": 5, "non_generic": 3}"#)
                .is_none()
        );
        assert!(parse_judge_verdict(r#"{"grounding": 5}"#).is_none());
        assert!(parse_judge_verdict("not json").is_none());
    }

    #[test]
    fn voice_fidelity_parses_and_is_range_checked() {
        let v = parse_judge_verdict(
            r#"{"specificity": 4, "grounding": 5, "non_generic": 3, "voice_fidelity": 2, "worst_claim": ""}"#,
        )
        .unwrap();
        assert_eq!(v.voice_fidelity, Some(2));
        // A present-but-out-of-range voice score fails the whole verdict (fail-closed).
        assert!(parse_judge_verdict(
            r#"{"specificity": 4, "grounding": 5, "non_generic": 3, "voice_fidelity": 0, "worst_claim": ""}"#,
        )
        .is_none());
    }

    #[test]
    fn system_prompt_carries_the_voice_axis_only_when_specified() {
        let base = judge_system_prompt(None);
        assert!(!base.contains("voice_fidelity"));
        let spec = VoiceSpec {
            character: "The Insider",
            mandate: "Get movement predictions out quickly while preserving long-term credibility.",
        };
        let voiced = judge_system_prompt(Some(&spec));
        assert!(voiced.contains("voice_fidelity"));
        assert!(voiced.contains("The Insider"));
        assert!(voiced.contains("which character wrote this?"));
        // The JSON reply template must mirror the axis set exactly.
        assert!(voiced.contains(r#""voice_fidelity": <1-5>"#));
    }
}
