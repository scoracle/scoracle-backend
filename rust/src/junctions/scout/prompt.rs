//! # THE SCOUT — the opposing scout
//!
//! The stats rail's voice. It briefs its own coaching staff on the game plan AGAINST this entity,
//! working only from the Rating Engine profile. The framing is deliberate: a scouting brief has to
//! be honest about strength, because the staff it is written for has to play against it.
//!
//! | | |
//! |---|---|
//! | **Seat** | `Role::StatsLogic` — the first consumer of that route |
//! | **Contract** | `s14` |
//! | **Reads** | the Postgres-computed rating profile (composite, T-score, `rating_breakdown` percentiles), plus a cross-season memory card |
//! | **Feeds** | The Analyst and The Oracle, via `rating_summaries` |
//!
//! ## Authority — it verbalizes a tier it is not allowed to assign
//!
//! This is the L8 breakthrough, and it is the reason this junction is reliable. The
//! percentile→tier mapping (`pct_band`) happens DETERMINISTICALLY in code, and the tier is fed to
//! the model as a labeled FACT. The Scout may only put that label into words. It never maps
//! percentile→quality itself — some local models invert the relation outright, calling a 37th
//! percentile skill "above average" — so the mapping is simply taken away from it.
//!
//! The same discipline draws the line around the rest of the seat: composite scores, T-scores and
//! percentiles are stored derived stats, READ here and never recomputed. What Rust owns is the
//! transient prompt shaping — notability, the band labels, float trimming, fact ordering — mirrored
//! byte-for-byte because it is not a stored stat and belongs beside the model call.
//!
//! ## The contract's shape
//!
//! s13 fixed three sections: *Strengths to respect*, *Exploitation opportunities*, *Summary*. s14
//! made the persona lead — a coaching-staff brief in clipped game-plan imperatives — and folded
//! `prompt_version` into the debounce pre-image, so a voice change re-runs the stage instead of
//! silently serving prose written to the old contract.
//!
//! ## Fail closed — with one asymmetry worth knowing
//!
//! The Scout's ONLY marker is the PRE-model no-stats path: no usable rating row writes a NULL-body
//! marker. There is deliberately no POST-model marker. An empty model body is a hard error that
//! fails and retries the work item — never a served row. `RatingParser` therefore never returns
//! `Ok(None)`.

use super::{
    RatingProfile,
    RatingReq,
    build_scouting_decision,
    collect_rate_standouts,
    format_datapoint_evidence,
    ordered_facts,
    render_scouting_decision,
};

/// System prompt for the PEAK scouting-report contract. s14 (Characters Phase B): the voice IS
/// The Scout — a persona-first coaching-staff brief in clipped game-plan imperatives (wiki
/// Characters.md craft appendix: names the skill and the number; speaks to a coaching staff,
/// never fans; tier is the truth). The hard invariants survive verbatim from s10-s13: the tier
/// is the truth, the PEAK line is copied not chosen, weaknesses need a materially negative z,
/// nothing below the 50th percentile gets praised, nothing is invented, and trend talk stays
/// banned (The Analyst's turn — Scott, Session D: "leave the trend of a metric to momentum").
pub const RATING_SYSTEM_PROMPT: &str = r#"Task: you are The Scout — the opposing scout. Brief your own coaching staff on the game plan AGAINST this entity, from the supplied Rating Engine profile.

Voice: clipped, tactical, game-plan imperatives. You speak to a coaching staff, never to fans — no hype, no fan framing, no essay prose. Stop this, attack that: name the skill and the number in every call. Respect the subject's real weapons — underrating them gets your side burned — and name exactly where to attack.

Definitions:
- COMPOSITE = how well the entity performs overall.
- Each skill gives value, percentile, tier, and z-score.
- TIER IS THE TRUTH. Do not reinterpret percentile quality yourself.
- Per-x marks can support an efficient lower-minutes or per-90 edge.
- SCOUTING DECISION is deterministic. Use it as the decision card, not a suggestion.

Output — the PEAK line, then THREE labeled sections, each on its own line, in this exact order:
1. First line: the Required PEAK line from SCOUTING DECISION, verbatim, with no extra words.
2. Strengths to respect: the weapons your staff must take away — the PEAK skill and EVERY strong/elite secondary skill on the decision card, each named with a cited number (value, percentile, or z), each stated as a threat to respect; a weapon you omit is a weapon your staff does not prepare for. If nothing is strong or elite, say so plainly and still name the best available impact with its percentile (the Why-no-standout line supplies it) — never leave this section a bare None.
3. Exploitation opportunities: where you attack — the exploit from SCOUTING DECISION, written as a game-plan instruction that names the skill and its cited number. When the card names a weakness, attack exactly that skill. Only when the card's Exploitation line itself says no clean exploit do you say so too — keep the words "no clean exploit" — and never claim no clean exploit when the card names a weakness.
4. Summary: a one-sentence scouting verdict on how to play THIS profile, tied to a named strength or weakness and its number — not boilerplate. Open the verdict with the specific action on the named skill, never with generic effort language.

Write each section's label exactly ("Strengths to respect:", "Exploitation opportunities:", "Summary:") followed by its content on the same line.

PEAK rules:
- Copy the Required PEAK line exactly.
- Put nothing before the PEAK marker and no explanatory text on the PEAK line.
- If the first output line does not begin with PEAK:, the answer is invalid.
- Do not start with the skill label by itself; the PEAK: marker must be present.
- Never choose the entity name, role, team name, or a different skill as the peak.
- Never choose an average, below average, poor, or merely above-average skill as the peak.

Scouting-report rules:
- You brief the staff preparing to FACE this entity: what to take away, and where to attack.
- Imperatives over observations: call the action — take it away, force it, attack it — and every call names the specific skill and its cited number (value, percentile, or z). Generic advice — play physical, disrupt rhythm, bring energy, stay disciplined — is invalid scouting even when a named skill follows it: the action itself must be specific to that skill.
- Never copy an evidence line from the card verbatim into a section: translate it into a call — verb first, then the skill and its cited number in plain words (write "96th percentile", never the card's " · " notation).
- Keep sentences short and tactical: lines a coach can read aloud in the film room, not analyst prose.
- Cite values and percentiles as given. When the card's strength line carries a per-x corroboration (per-36, per-90), your Strengths section repeats that per-x number — it is the proof the edge is real and not a minutes artifact, and dropping it undersells the threat.
- TIER IS THE TRUTH: never inflate an average mark into a strength, and nothing below the 50th percentile gets praise.
- Name a weakness only when a skill is below average or poor AND its z is meaningfully negative. A poor percentile with a near-zero z is a usage artifact, not a liability — do not present it as an exploit.
- A profile with no standout is not a game-plan priority: keep each section to a single line.
- This is a static profile: no trajectory, trend, or momentum talk — the trend read is another character's turn, never yours.
- Keep it tight — one line per section for a modest profile; only a truly rich profile earns more.
- The supplied tiers and datapoints are everything you know: never invent a number, rate, role, or skill not in the data."#;

/// Prompt version for the PEAK scouting-report contract.
pub const RATING_PROMPT_VERSION: &str = "s14"; // s12: cross-season memory card (mig 164); s13: three-section contract (Strengths to respect / Exploitation opportunities / Summary); s14: The Scout voice pass (Characters Phase B) — persona-first coaching-staff brief, clipped game-plan imperatives, prompt_version folded into the debounce pre-image

/// build_stat_prompt assembles the user prompt. `memory` is the cross-season memory card
/// (s12, mig 164) — `None` when the graph holds none, and for the parity/eval paths
/// (which pin the memory-free shape).
pub fn build_stat_prompt(
    req: &RatingReq,
    p: &RatingProfile,
    notability: i32,
    memory: Option<&str>,
) -> String {
    let mut b = String::new();

    let mut header = format!("{} {}", req.sport, req.entity_type);
    if !p.position.is_empty() {
        header.push_str(", ");
        header.push_str(&p.position);
    }
    b.push_str(&format!("Entity: {} ({header})\n", req.entity_name));

    b.push_str(&format!(
        "\nProfile distinctiveness: {notability}/100 (higher = more standout skills — let a richer profile earn a fuller read).\n"
    ));

    if let Some(comp) = p.composite_score {
        b.push_str(&format!(
            "\nComposite (how WELL overall — T-score, 50 = average): {comp:.0}\n"
        ));
    }

    let decision = build_scouting_decision(p);
    b.push_str(&render_scouting_decision(&decision));

    b.push_str("\nDatapoints — value · percentile + TIER (the percentile mapped to elite/strong/above average/average/below average/poor; THIS TIER IS THE TRUTH) · z (standard deviations above the mean: the scarcity/scale of the edge; a high z is a rarer, more premium skill); [position] percentile shown when present:\n");
    for d in ordered_facts(&p.breakdown) {
        b.push_str("- ");
        b.push_str(&format_datapoint_evidence(&d));
        b.push('\n');
    }

    let rs = collect_rate_standouts(p);
    if !rs.is_empty() {
        b.push_str("\nRate-adjusted (per-x) corroboration — these also rate elite on a per-minute / per-90 basis (so the edge is not just a counting-stat artifact of heavy minutes):\n");
        for r in &rs {
            b.push_str(&format!(
                "- [{}] {}: {:.0}th pct\n",
                r.mode.replace('_', "-"),
                r.label,
                r.pct
            ));
        }
    }

    // Cross-season memory card (s12, mig 164): prior-season PEAK read (banked junction
    // output — the echo-chamber rule applies), confirmed moves (regime-change context),
    // matchup edges with reliability framing (presented, not gatekept). The L8 invariant
    // survives untouched: the TIERS above are this season's truth; memory explains arc,
    // never overrides a tier. Deliberately NOT part of input_components / input_hash.
    if let Some(m) = memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\nCross-season memory (computed history — arc context only: the datapoints and TIERS above are this season's truth and are never overridden by memory; use these lines for trajectory, new-club context, and matchup quirks; weigh each matchup line by its reliability — a low-reliability edge deserves an explicit grain of salt; a prior read is continuity, never evidence for the new one):\n");
        for line in m.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }

    b.push_str(&format!(
        "\nWrite the scouting report now. Start with this exact first line and no text before it: {}\nThen write the three labeled sections (Strengths to respect / Exploitation opportunities / Summary), each on its own line. The first output characters must be PEAK:.",
        decision.required_peak_line
    ));
    b
}
