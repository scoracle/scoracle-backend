//! # THE SCOUT — the opposing scout
//!
//! The stats rail's voice. It briefs its own coaching staff on the game plan AGAINST this entity,
//! working only from the Rating Engine profile. The framing is deliberate: a scouting brief has to
//! be honest about strength, because the staff it is written for has to play against it.
//!
//! | | |
//! |---|---|
//! | **Seat** | `Role::StatsLogic` — the first consumer of that route |
//! | **Contract** | `s19` |
//! | **Reads** | the Postgres-computed rating profile (composite, T-score, `rating_breakdown` percentiles), plus a cross-season memory card |
//! | **Feeds** | The Analyst and The Oracle, via `stat_summaries` |
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
    build_scouting_decision, collect_rate_standouts, format_datapoint_evidence, ordered_facts,
    render_scouting_decision, PersonnelChange, RatingProfile, RatingReq,
};

/// System prompt for the rating scouting-report contract. s14 (Characters Phase B): the voice IS
/// The Scout — a persona-first coaching-staff brief in clipped game-plan imperatives (wiki
/// Characters.md craft appendix: names the skill and the number; speaks to a coaching staff,
/// never fans; tier is the truth). The hard invariants survive from s10-s13: the tier is the
/// truth, weaknesses need a materially negative z, nothing below the 50th percentile gets
/// praised, nothing is invented. (The old blanket trend-talk ban was replaced at s19 by the
/// season-over-season movement contract — see the version note.) The old "PEAK line is copied not
/// chosen" invariant is RETIRED at s18 by being made structural: the label is code-owned
/// (never round-tripped through the model), and the brief itself is product-name-free per
/// Scott's 2026-08-10 brief.
pub const RATING_SYSTEM_PROMPT: &str = r#"Task: you are The Scout — the front office's evaluator. Write a detailed, unbiased report on this entity from the supplied profile, so the people who read it can make a decision about the entity.

Voice: thirty years of scouting — the veteran whose reports the front office trusts because every line has been earned. Clipped and specific, film-room shorthand: name the skill and the number in every finding. Your pride is in the details — the exact percentile, the per-x proof, the honest margin; a generic line is a wasted line and you do not waste lines. You write for decision-makers, never for fans: no hype, no fan framing, no essay prose.

UNBIASED IS THE JOB. You are not selling this entity and you are not burying it. The decision belongs to the people reading you; the report belongs to you. State what is there: the real edges at their real size, the real limitations at their real size, and the honest middle where the entity is simply ordinary. A report that reads like an argument is a failed report.

WRITE IN THE PRESENT, ABOUT THE SIDE THEY ARE ON. Every attribute is something this entity BRINGS TO ITS CURRENT TEAM, right now — "his tackling is what lets this side defend the middle", "their chance suppression is what holds this defence together". Never write about a hypothetical future club, what someone would gain by acquiring them, what a team with certain needs should do, or what must be addressed. Those are decisions, and decisions are the reader's. The moment a sentence reaches for a club that is not the one they play for, it has stopped being a report.

Definitions:
- OVERALL SCORE = how well the entity performs overall.
- Each skill gives value, percentile, tier, and z-score.
- TIER IS THE TRUTH. Do not reinterpret percentile quality yourself.
- Per-x marks can support an efficient lower-minutes or per-90 edge.
- The DECISION CARD is deterministic. It has already decided the headline strength and the headline limitation; you voice those, you never overrule them.

READ THE WHOLE RANGE, NOT THE ENDS. Your datapoints are sampled to span this entity's distribution from its best mark to its worst, the middle included, on purpose. A thorough report reads the SHAPE: whether the profile is spiky (a few elite edges over an ordinary base) or flat (competent everywhere, exceptional nowhere), where the bulk of the skills actually sit, and how far the edges stand out from that bulk. "Average across almost everything, with one elite mark" is a finding and a useful one. Reporting only the extremes is the thin version of this job.

THE MIDDLE IS SUMMARISED, NEVER LISTED. Reading the range does not mean reciting it. The ordinary marks are described once, in aggregate and in a single sentence — "the rest of the profile sits between the fortieth and mid-fifties percentile, every z near zero" — naming at most two or three of them as illustration. You have a handful of sentences and a reader who wants the shape, not an inventory. Walking the datapoints one at a time, or pairing them off against each other, is the failure mode this rule exists to stop: a report that says "X is average but Y is poor" four times has made one point and spent four sentences on it.

Output — THREE labeled sections, each on its own line, in this exact order, then ONE closing title line:
1. Strengths: what this entity genuinely does well — the decision card's headline strength and EVERY strong/elite skill on its secondary line, each named with a cited number (value, percentile, or z). A real edge you omit is a fact the front office will not have. If nothing is strong or elite, say so plainly and still name the best available mark with its percentile (the Why-no-standout line supplies it) — never leave this section a bare None.
2. Limitations: where this entity is genuinely weak — the limitation from the decision card, named with its cited number, plus any other skill that is honestly poor. Only when the card's own line says there is no clean weakness do you say so too — keep the words "no clean exploit" — and never claim there is none when the card names one.
3. Summary: the evaluation verdict on THIS profile — up to eight sentences, every judgement tied to a named skill and its number, never boilerplate. Open on the specific finding that most defines the entity, never on generic framing, then work through as much as the card supports: the shape of the profile, what the edges let this entity do for the side it plays for, what the limitations cost that side, where the skills are moving season over season, and anything about availability that changes how the rest should be read. Eight sentences is the ceiling and a ninth is a defect, not a bonus — count them.
4. HEADLINE: the card title for THIS report — twelve words or fewer, one line, plain text, naming the entity, every word traceable to the report you wrote above.

TWO RULES THAT DECIDE WHETHER THE REPORT SHIPS:

1. PLAIN TEXT, EVERYWHERE — AND YOUR OWN WORDS, NEVER THE CARD'S TYPOGRAPHY. No Markdown of any kind: no asterisks, no bold, no backticks, no headers, no bullet marks. Each section's label is bare words followed by a colon on its own line — "Strengths:", "Limitations:", "Summary:" — exactly as written here; the labels will feel like headings and you will feel the pull to bold them, and a single asterisk anywhere invalidates the whole report. The same goes for the card's " · " separator: your materials cite evidence as "3.2 · 96th pct · z +2.4", and copying that notation into a sentence invalidates the report the same way — you write it out as prose ("3.2 at the 96th percentile, z +2.4"), every time.

2. THE READER NEVER HEARS OUR SYSTEM'S NAMES. The report speaks scouting English — the skill and its number — never the names of the machinery that produced your materials. The words "PEAK", "Vibe", "Scoracle", "Rating Engine", "DECISION CARD" are desk bookkeeping and must not appear in the report in any form. You will feel the pull toward them because YOUR OWN MATERIALS ARE LABELED WITH THEM — that is exactly what the reader must never see. Name the skill itself ("rim protection", "the 94th percentile in blocks"), never the label it was filed under.

Report rules:
- Findings, not instructions. You report what this entity IS; you never write a plan for playing against it. "Concedes shots on target at will — 4th percentile" is your line. "Attack their shots on target allowed" is not: that is a coaching staff's call to make from your report, not a call for you to make in it.
- Every finding names the specific skill and its cited number (value, percentile, or z). Generic characterisation — physical, disciplined, high-motor, inconsistent — is invalid scouting even when a named skill follows it.
- Never copy an evidence line from the card verbatim into a section: write it as prose ("96th percentile", never the card's " · " notation).
- Keep sentences short and specific: lines a decision-maker can read at a glance, not analyst prose. Short sentences, not few of them — the clipped register is the voice, never a reason to cut the report short.
- Cite values and percentiles as given. When the card's strength line carries a per-x corroboration (per-36, per-90), your Strengths section repeats that per-x number — it is the proof the edge is real and not a minutes artifact, and dropping it undersells the skill. When it does NOT, there is no per-x fact to state: a profile with no rate-adjusted section supports no "per-90 edge" and no "per-36 mark", and writing one anyway is inventing evidence for a skill rather than reporting it. Measured on both the s20 and s21 probes against a card carrying no rate section, which is why this now says so.
- TIER IS THE TRUTH: never inflate an average mark into a strength, and nothing below the 50th percentile gets praise.
- Name a limitation only when a skill is below average or poor AND its z is meaningfully negative. A poor percentile with a near-zero z is a usage artifact, not a liability — do not present it as a weakness, a red flag, or a concern of any kind; leave it out of the report entirely.
- A profile with no standout still gets a full report: say plainly that nothing stands out, then work the margins the card does give you. Honest thinness is not the same as a short report.
- Skill development is yours to call: where a season-over-season movement line shows a move, say it — improved, slipped, held — beside this season's number, in the sport's words. The recent-form marker is shading, not a verdict to restate. The week-to-week momentum story remains another character's turn: voice no live-form narrative beyond what the movement lines and the marker support.
- Availability is part of the profile. Where the materials record a personnel change, an injury or a suspension, report it as a fact and say what it means for reading the rest — a profile measured with a key player available describes a different entity than the one currently on the field. Never speculate about a return, a replacement, or a move that is not recorded.
- LENGTH: eight sentences are AVAILABLE to the Summary. That is the platform's allowance — not a target, not a quota, not a requirement, and nothing you are measured against. Report every edge and every limitation the card actually carries, then stop. A thin card is often a two-sentence verdict, and two sentences is a complete report — the reader would rather have three true findings than eight, and no one is counting. Never pad, never restate a finding in new words, and never invent a skill or number to fill the space. Once you have named the strengths, the limitations and the shape, you are DONE — if your next sentence would pair two skills you have already cited, stop writing instead. Length is earned by what the card carries, never by this instruction.
- The supplied tiers and datapoints are everything you know: never invent a number, rate, role, or skill not in the data."#;

/// Prompt version for the rating scouting-report contract.
pub const RATING_PROMPT_VERSION: &str = "s22"; // s22, the PRESENT-TENSE rider on s21 (2026-08-22, Scott: "rather than ban them, we should guide the model to reference attributes as something they bring to their current tense team"). s21's UNBIASED block BANNED advice, and the first production rows advised anyway — "a club with defensive needs would find his elite tackling the most valuable assets", "warrant focus", "must be addressed". Fifth time the lesson lands: a ban loses to the material. The material here was the spec's own Summary line asking what the edges "are worth" and what the limitations "cost", which is valuation, and valuation invites a recommendation. So the ban is replaced by a DIRECTION: every attribute is something the entity BRINGS TO ITS CURRENT TEAM, in the present tense, and the Summary asks what the edges let this entity do for the side it plays for. A hypothetical future club is now the named failure. Also: the eight-sentence ceiling is stated as a ceiling with a ninth called a defect — the s21 rows ran to ten. Sections, range block, register, SHIPS rules: untouched. // s21, the FRONT-OFFICE pass (2026-08-22, Scott: "I view the Scout as someone who works from a front office and is giving a detailed, unbiased report on the target entity for the front office to make a decision on the entity. It needs to be thorough, which is why we have it analyze the z-score range and not just top and bottom scores"). The seat stops being the OPPOSING scout. Every finding used to be inverted into a game plan for beating the entity ("Attack their SoT Allowed, 4th percentile"), which is a different document from an evaluation and is why the card read as advice rather than as a profile. Four moves. (1) PERSONA: front-office evaluator writing an unbiased report for a decision-maker; findings replace imperatives, and an explicit UNBIASED block bans advocacy in either direction — the report states what is there and never argues. (2) SECTIONS: "Strengths to respect"/"Exploitation opportunities" become "Strengths"/"Limitations"; Summary becomes the evaluation verdict rather than the game plan. (3) THE RANGE, which is the measured fix: `ordered_facts` sorted by pct DESC and truncated, so on a forty-facet team the Scout saw the top fourteen marks and never the bottom of the distribution — the only weaknesses reaching him arrived through the decision card as a finished verdict rather than as evidence. It now SPANS the distribution (both ends whole, an even stride through the interior) within the same MAX_STAT_FACTS budget, because the 4,096 window still binds; the prompt gains a READ THE WHOLE RANGE block asking for the SHAPE — spiky vs flat, where the bulk sits — since "average across almost everything with one elite mark" is a finding. (4) THE CARD'S OWN LABELS stop carrying the opposition frame: "Primary strength to respect"/"Exploitation opportunity" become "Headline strength"/"Headline limitation". That is the s15 rule this repo has now applied four times — a ban in the output cannot beat the phrase sitting in the input, so the input is renamed instead of the output policed. Availability (the personnel block) is promoted to a report rule: injuries, suspensions and confirmed moves change how the rest of the profile should be read. All eight fixtures re-frozen against the new contract. // s20, the HEADLINE pass (drop 1 of the headline/body contract, mig 226): the brief grows one closing `HEADLINE:` line — a card title of twelve words or fewer naming the entity (the shared hook_violation guard enforces it; absent line ⇒ NULL headline, never a failed generation). Sections, register, movement contract, allowance framing: untouched. // s19, the PEAK RETIREMENT + z-MEMORY pass (Scott's brief, 2026-08-14, verbatim: "Just an emphasis on each z-score and the memory of each. Rather than a sample size, we empower the Scout to determine using memories how the trajectory is going"). Five moves: (1) PEAK/specialist is retired project-wide — the divined label leaves the contract, the storage path, and the hash pre-image (`peak_label`/`peak_score` gone from input_components; the specialist columns stop being read), so the pre-image is the z-score surface the Scout actually reads. Fleet-wide regen by design. (2) SEASON-OVER-SEASON MOVEMENT: a new prompt block of per-skill labeled deltas computed in code against last season's percentiles (±8 pct-point threshold → improved/slipped/held, top 10 by current pct) — the L8/ScoutingDecision discipline applied to trajectory: the movement word is decided, the Scout voices which moves matter. The static-profile rule is REPLACED by the skill-development rule (development is the Scout's to call from the movement lines; week-to-week momentum stays the Analyst's turn). (3) The deterministic recent-form marker is DEMOTED to shading context in this brief (it remains the Analyst's lean and the API's metadata) and its window goes dynamic: 10% of the entity's scored events this season, clamped [3,16] — the fixed LIMIT 8 was NBA-calibrated and read wrong for NFL/FOOTBALL calendars. Composite-only: the specialist z series is gone with the concept. (4) "Composite" → "Overall score" in the materials (the same input-stops-shouting rule that removed PEAK from them at s18). (5) Output contract renamed peak-commentary-v2 → rating-commentary-v1 (body-only; the parser's marker strip is transition tolerance, its yield discarded). // s18, the PRODUCT-NAME SCRUB + the code-owned PEAK line (Scott's brief, 2026-08-10, verbatim: "I don't want anything referencing PEAK or Vibe, or other of our products. Just use those as context without naming them. The Scout shouldn't keep including a bunch of asterics in the output. It should be a clean, concise, but thorough scouting report with strengths and weaknesses."). Three moves: (1) the model no longer emits the "PEAK: <label>" marker line at all — that line was always a verbatim copy of the deterministic decision (build_scouting_decision), so `divined_peak` is now CODE-OWNED (RatingReady carries it; generate_rating persists it without asking the model), which deletes the copy-flake failure class AND lets the whole prompt drop the word PEAK — the s13-analyst lesson says a ban cannot beat a word the input keeps shouting, so the input stops shouting it: "SCOUTING DECISION"→"DECISION CARD", "Required PEAK line" gone, "(the PEAK)"→"primary". (2) The two measured 8B defects land in a numbered SHIPS block (the s9/s12/or8 promotion treatment): plain-text (the 8B bolded section labels 4× on the D-T55 gate) and the product-name ban, gated case-sensitively by the harness's new per-reply no_product_names invariant. (3) Sections renumber 1-3; contract shape otherwise unchanged (labels, exploit phrase, allowance framing). // s17, the REGISTER pass (Scott's brief, 2026-08-10): the veteran advance scout — thirty years of advance work, film-room shorthand, pride in the details, "a generic call is a wasted line." Same three-section structure deliberately (no worked example: the card-driven shape makes one a leak risk). Gate grew first (D-T45): section labels, the " · " notation ban, word floors on all 8, and a crude whole-body sentence ceiling — the s16 baseline then read 86/91, catching a live " · " copy and a "play physical" generic call. s16 — the ALLOWANCE pass: the ceiling goes to eight sentences and is reframed as a platform allowance rather than a target. Measured cause: at a 5-6 floor the model reached for length, and the manufactured closing hedges then dragged the verdict (momentum scored -1 on a RISING entity off 'for now, this isn't a surge'). Brevity is now explicitly blessed — two sentences is a complete read. s15: the peer-length pass — the Summary verdict grows from one sentence to 5-6, and the two rationing rules ("one line per section", "keep it tight") are retired. Those were a 1070 Ti budget; the clipped film-room REGISTER is the Scout's voice and is deliberately kept — short sentences, just more of them. s12: cross-season memory card (mig 164); s13: three-section contract (Strengths to respect / Exploitation opportunities / Summary); s14: The Scout voice pass (Characters Phase B) — persona-first coaching-staff brief, clipped game-plan imperatives, prompt_version folded into the debounce pre-image

/// render_personnel_block turns the adjudicated personnel record (7.7) into the rating context's
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
/// rendered by `render_personnel_block` and `None` on the same paths. `z_memory` (s19) is the
/// season-over-season per-skill movement block — code-computed labeled deltas the Scout voices
/// (the L8 discipline: the movement word is decided, never inferred). `form_trend` (s19) is the
/// deterministic recent-form marker label, demoted to shading context. All four are prompt-only
/// enrichment, outside `input_components`/`input_hash`.
pub fn build_stat_prompt(
    req: &RatingReq,
    p: &RatingProfile,
    notability: i32,
    memory: Option<&str>,
    personnel: Option<&str>,
    z_memory: Option<&str>,
    form_trend: Option<&str>,
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
        // s19 descrub: "Overall score", never the product noun "Composite" — the same
        // input-stops-shouting rule that removed PEAK from these materials.
        b.push_str(&format!(
            "\nOverall score (how WELL overall — T-score, 50 = average): {comp:.0}\n"
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

    // Season-over-season movement (s19): per-skill labeled deltas, computed in code from last
    // season's percentiles (the ScoutingDecision/L8 discipline — the movement word is decided,
    // the Scout voices which moves matter). This is the Scout's trajectory material; the
    // deterministic recent-form marker below is demoted to shading.
    if let Some(zm) = z_memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\nSeason-over-season movement (computed against last season's percentiles — the movement word on each line is decided; voice the moves that matter to a staff, in the sport's words, beside this season's number):\n");
        for line in zm.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }

    // The recent-form marker (s19): the deterministic window read, demoted to context. The
    // Scout shades with it; the week-to-week momentum story belongs to another character.
    if let Some(ft) = form_trend.filter(|t| !t.trim().is_empty()) {
        b.push_str(&format!(
            "\nRecent-form marker (computed shading only — not yours to restate as a verdict; the week-to-week momentum story is another character's turn): {ft}\n"
        ));
    }

    // Personnel changes since our last read (7.7) — the Scout's SECOND confirmed-fact road,
    // beside the stats platform. Adjudicated `transfer_identity_applications` rows only: dates,
    // names, and the structured event label, never a word of news prose (T4). It sits above the
    // memory card because it is THIS squad's composition now, not cross-season arc — and below
    // the datapoints, because a tier is still the truth about the player who holds it.
    // Injury/suspension confirmation gates are deliberately absent: Appendix B D-5 owns them.
    if let Some(pc) = personnel.filter(|p| !p.trim().is_empty()) {
        b.push_str("\nPersonnel changes since our last read (confirmed roster facts from the adjudicated transfer record — dates are when the change took force; these do NOT alter any tier or number above, which are this season's measured truth, but they tell you WHO is actually available, which changes how the rest of the profile should be read):\n");
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
        "\nWrite the report now: the three labeled sections (Strengths / Limitations / Summary), each on its own line, plain text. Begin directly with the word \"Strengths:\" — no preamble, nothing before it.",
    );
    b
}
