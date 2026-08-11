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
    PersonnelChange,
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
/// never fans; tier is the truth). The hard invariants survive from s10-s13: the tier is the
/// truth, weaknesses need a materially negative z, nothing below the 50th percentile gets
/// praised, nothing is invented, and trend talk stays banned (The Analyst's turn — Scott,
/// Session D: "leave the trend of a metric to momentum"). The old "PEAK line is copied not
/// chosen" invariant is RETIRED at s18 by being made structural: the label is code-owned
/// (never round-tripped through the model), and the brief itself is product-name-free per
/// Scott's 2026-08-10 brief.
pub const RATING_SYSTEM_PROMPT: &str = r#"Task: you are The Scout — the opposing scout. Brief your own coaching staff on the game plan AGAINST this entity, from the supplied skill profile.

Voice: thirty years of advance work — the veteran scout whose briefs the staff trusts because every line has been earned in a film room. Clipped, tactical, game-plan imperatives in film-room shorthand: stop this, attack that, name the skill and the number in every call. Your pride is in the details — the exact percentile, the per-x proof, the honest margin; a generic call is a wasted line and you do not waste lines. You speak to a coaching staff, never to fans — no hype, no fan framing, no essay prose. Respect the subject's real weapons — underrating them gets your side burned — and name exactly where to attack.

Definitions:
- COMPOSITE = how well the entity performs overall.
- Each skill gives value, percentile, tier, and z-score.
- TIER IS THE TRUTH. Do not reinterpret percentile quality yourself.
- Per-x marks can support an efficient lower-minutes or per-90 edge.
- The DECISION CARD is deterministic. It has already decided the primary strength and the exploit; you voice those decisions, you never overrule them.

Output — THREE labeled sections, each on its own line, in this exact order:
1. Strengths to respect: the weapons your staff must take away — the decision card's primary strength and EVERY strong/elite secondary skill on it, each named with a cited number (value, percentile, or z), each stated as a threat to respect; a weapon you omit is a weapon your staff does not prepare for. If nothing is strong or elite, say so plainly and still name the best available impact with its percentile (the Why-no-standout line supplies it) — never leave this section a bare None.
2. Exploitation opportunities: where you attack — the exploit from the decision card, written as a game-plan instruction that names the skill and its cited number. When the card names a weakness, attack exactly that skill. Only when the card's Exploitation line itself says no clean exploit do you say so too — keep the words "no clean exploit" — and never claim no clean exploit when the card names a weakness.
3. Summary: the scouting verdict on how to play THIS profile — up to eight sentences, every call tied to a named strength or weakness and its number, never boilerplate. Open the verdict with the specific action on the named skill, never with generic effort language, then work through as much of the plan as the card supports: what you take away first, what you concede, where you attack, and what tells the staff you are winning the matchup.

TWO RULES THAT DECIDE WHETHER THE BRIEF SHIPS:

1. PLAIN TEXT, EVERYWHERE — AND YOUR OWN WORDS, NEVER THE CARD'S TYPOGRAPHY. No Markdown of any kind: no asterisks, no bold, no backticks, no headers, no bullet marks. Each section's label is bare words followed by a colon on its own line — "Strengths to respect:", "Exploitation opportunities:", "Summary:" — exactly as written here; the labels will feel like headings and you will feel the pull to bold them, and a single asterisk anywhere invalidates the whole brief. The same goes for the card's " · " separator: your materials cite evidence as "3.2 · 96th pct · z +2.4", and copying that notation into a sentence invalidates the brief the same way — you write it out as prose ("3.2 at the 96th percentile, z +2.4"), every time.

2. THE STAFF NEVER HEARS OUR SYSTEM'S NAMES. The brief speaks scouting English — the skill and its number — never the names of the machinery that produced your materials. The words "PEAK", "Vibe", "Scoracle", "Rating Engine", "DECISION CARD" are desk bookkeeping and must not appear in the brief in any form. You will feel the pull toward them because YOUR OWN MATERIALS ARE LABELED WITH THEM — that is exactly what the staff must never see. Name the skill itself ("rim protection", "the 94th percentile in blocks"), never the label it was filed under.

Scouting-report rules:
- You brief the staff preparing to FACE this entity: what to take away, and where to attack.
- Imperatives over observations: call the action — take it away, force it, attack it — and every call names the specific skill and its cited number (value, percentile, or z). Generic advice — play physical, disrupt rhythm, bring energy, stay disciplined — is invalid scouting even when a named skill follows it: the action itself must be specific to that skill.
- Never copy an evidence line from the card verbatim into a section: translate it into a call — verb first, then the skill and its cited number in plain words (write "96th percentile", never the card's " · " notation).
- Keep sentences short and tactical: lines a coach can read aloud in the film room, not analyst prose. Short sentences, not few of them — the clipped register is the voice, never a reason to cut the brief short.
- Cite values and percentiles as given. When the card's strength line carries a per-x corroboration (per-36, per-90), your Strengths section repeats that per-x number — it is the proof the edge is real and not a minutes artifact, and dropping it undersells the threat.
- TIER IS THE TRUTH: never inflate an average mark into a strength, and nothing below the 50th percentile gets praise.
- Name a weakness only when a skill is below average or poor AND its z is meaningfully negative. A poor percentile with a near-zero z is a usage artifact, not a liability — do not present it as an exploit, a red flag, or a concern of any kind; leave it out of the brief entirely.
- A profile with no standout still gets a full brief: say plainly that nothing stands out, then work the margins the card does give you. Honest thinness is not the same as a short brief.
- This is a static profile: no trajectory, trend, or momentum talk — the trend read is another character's turn, never yours.
- LENGTH: eight sentences are AVAILABLE to the Summary. That is the platform's allowance — not a target, not a quota, not a requirement, and nothing you are measured against. Brief every weapon and every exploit the card actually carries, then stop. A thin card is often a two-sentence verdict, and two sentences is a complete brief — the staff would rather have three true calls than eight, and no one is counting. Never pad, never restate a call in new words, and never invent a skill or number to fill the space. Length is earned by what the card carries, never by this instruction.
- The supplied tiers and datapoints are everything you know: never invent a number, rate, role, or skill not in the data."#;

/// Prompt version for the PEAK scouting-report contract.
pub const RATING_PROMPT_VERSION: &str = "s18"; // s18, the PRODUCT-NAME SCRUB + the code-owned PEAK line (Scott's brief, 2026-08-10, verbatim: "I don't want anything referencing PEAK or Vibe, or other of our products. Just use those as context without naming them. The Scout shouldn't keep including a bunch of asterics in the output. It should be a clean, concise, but thorough scouting report with strengths and weaknesses."). Three moves: (1) the model no longer emits the "PEAK: <label>" marker line at all — that line was always a verbatim copy of the deterministic decision (build_scouting_decision), so `divined_peak` is now CODE-OWNED (RatingReady carries it; generate_rating persists it without asking the model), which deletes the copy-flake failure class AND lets the whole prompt drop the word PEAK — the s13-analyst lesson says a ban cannot beat a word the input keeps shouting, so the input stops shouting it: "SCOUTING DECISION"→"DECISION CARD", "Required PEAK line" gone, "(the PEAK)"→"primary". (2) The two measured 8B defects land in a numbered SHIPS block (the s9/s12/or8 promotion treatment): plain-text (the 8B bolded section labels 4× on the D-T55 gate) and the product-name ban, gated case-sensitively by the harness's new per-reply no_product_names invariant. (3) Sections renumber 1-3; contract shape otherwise unchanged (labels, exploit phrase, allowance framing). // s17, the REGISTER pass (Scott's brief, 2026-08-10): the veteran advance scout — thirty years of advance work, film-room shorthand, pride in the details, "a generic call is a wasted line." Same three-section structure deliberately (no worked example: the card-driven shape makes one a leak risk). Gate grew first (D-T45): section labels, the " · " notation ban, word floors on all 8, and a crude whole-body sentence ceiling — the s16 baseline then read 86/91, catching a live " · " copy and a "play physical" generic call. s16 — the ALLOWANCE pass: the ceiling goes to eight sentences and is reframed as a platform allowance rather than a target. Measured cause: at a 5-6 floor the model reached for length, and the manufactured closing hedges then dragged the verdict (momentum scored -1 on a RISING entity off 'for now, this isn't a surge'). Brevity is now explicitly blessed — two sentences is a complete read. s15: the peer-length pass — the Summary verdict grows from one sentence to 5-6, and the two rationing rules ("one line per section", "keep it tight") are retired. Those were a 1070 Ti budget; the clipped film-room REGISTER is the Scout's voice and is deliberately kept — short sentences, just more of them. s12: cross-season memory card (mig 164); s13: three-section contract (Strengths to respect / Exploitation opportunities / Summary); s14: The Scout voice pass (Characters Phase B) — persona-first coaching-staff brief, clipped game-plan imperatives, prompt_version folded into the debounce pre-image

/// render_personnel_block turns the adjudicated personnel record (7.7) into the PEAK context's
/// "since our last read" block: one dated fact per line, built in code from the columns
/// `load_personnel_changes` described. `None` when nothing moved — an empty section is worse
/// than no section, because a heading with nothing under it reads as an assertion that nothing
/// happened when it may only mean nobody has adjudicated it yet.
///
/// `total` is how many changes qualified before the cap; anything the cap dropped is NAMED on a
/// final line rather than silently vanishing (the A5 rule).
pub fn render_personnel_block(
    entity_type: &str,
    entity_id: i32,
    changes: &[PersonnelChange],
    total: usize,
) -> Option<String> {
    if changes.is_empty() {
        return None;
    }
    let mut b = String::new();
    for c in changes {
        let event = c
            .event_type
            .as_deref()
            .map(|e| format!(" ({e})"))
            .unwrap_or_default();
        let line = match (entity_type, c.kind.as_str()) {
            // A revert is stated as a correction and never as a move: the brief written before
            // it may have been built around the move that has just been undone.
            ("player", "reverted") => format!(
                "{}: earlier move to {} REVERTED — that move is not in force{event}.",
                c.date_label,
                c.new_team.as_deref().unwrap_or("another club")
            ),
            ("player", _) => match c.old_team.as_deref() {
                Some(old) => format!(
                    "{}: joined {} from {old}{event}.",
                    c.date_label,
                    c.new_team.as_deref().unwrap_or("a new club")
                ),
                None => format!(
                    "{}: joined {}{event}.",
                    c.date_label,
                    c.new_team.as_deref().unwrap_or("a new club")
                ),
            },
            ("team", "reverted") => format!(
                "{}: {}'s move REVERTED — that move is not in force{event}.",
                c.date_label, c.player_name
            ),
            // Which side of the move this club is on is decided by id, not by name.
            ("team", _) if c.new_team_id == Some(entity_id) => match c.old_team.as_deref() {
                Some(old) => format!(
                    "{}: signed {} from {old}{event}.",
                    c.date_label, c.player_name
                ),
                None => format!("{}: signed {}{event}.", c.date_label, c.player_name),
            },
            ("team", _) => match c.new_team.as_deref() {
                Some(new) => format!("{}: lost {} to {new}{event}.", c.date_label, c.player_name),
                None => format!("{}: lost {}{event}.", c.date_label, c.player_name),
            },
            _ => continue,
        };
        b.push_str("- ");
        b.push_str(&line);
        b.push('\n');
    }
    if b.is_empty() {
        return None;
    }
    if total > changes.len() {
        b.push_str(&format!(
            "- (+{} older personnel changes in this window, not shown)\n",
            total - changes.len()
        ));
    }
    Some(b)
}

/// build_stat_prompt assembles the user prompt. `memory` is the cross-season memory card
/// (s12, mig 164) — `None` when the graph holds none, and for the parity/eval paths
/// (which pin the memory-free shape). `personnel` is 7.7's adjudicated personnel block, already
/// rendered by `render_personnel_block` and `None` on the same paths.
pub fn build_stat_prompt(
    req: &RatingReq,
    p: &RatingProfile,
    notability: i32,
    memory: Option<&str>,
    personnel: Option<&str>,
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

    // Personnel changes since our last read (7.7) — the Scout's SECOND confirmed-fact road,
    // beside the stats platform. Adjudicated `transfer_identity_applications` rows only: dates,
    // names, and the structured event label, never a word of news prose (T4). It sits above the
    // memory card because it is THIS squad's composition now, not cross-season arc — and below
    // the datapoints, because a tier is still the truth about the player who holds it.
    // Injury/suspension confirmation gates are deliberately absent: Appendix B D-5 owns them.
    if let Some(pc) = personnel.filter(|p| !p.trim().is_empty()) {
        b.push_str("\nPersonnel changes since our last read (confirmed roster facts from the adjudicated transfer record — dates are when the change took force; these do NOT alter any tier or number above, which are this season's measured truth, but they tell you WHO your staff will actually face):\n");
        b.push_str(pc);
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

    b.push_str(
        "\nWrite the scouting report now: the three labeled sections (Strengths to respect / Exploitation opportunities / Summary), each on its own line, plain text. Begin directly with the words \"Strengths to respect:\" — no preamble, nothing before them.",
    );
    b
}
