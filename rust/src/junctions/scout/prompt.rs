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
    render_scouting_decision, AvailabilityChange, PersonnelChange, RatingProfile, RatingReq,
};
use crate::junctions::editor::render::MarkedClaim;

/// render_availability_reports turns the Editor's tagged claims into the block the Scout WEIGHS.
///
/// Deliberately distinct from the personnel block above, and the difference is the whole design:
/// that block is the ADJUDICATED record (a fact, reported), this one is REPORTAGE (a claim, to be
/// judged). Merging them would ask the Scout to treat "The Athletic says he is out six weeks" and
/// "the transfer was applied on Jul 29" as the same kind of thing, which is what the structured
/// pipeline would have forced and what Scott's ruling rejected.
///
/// `⇄` marks a claim that another claim in the same set contradicts. Both members are always
/// carried (T3/D6) — the marker points, it never filters, because the disagreement is exactly
/// what the Scout is here to resolve.
pub fn render_availability_reports(claims: &[MarkedClaim]) -> Option<String> {
    if claims.is_empty() {
        return None;
    }
    let mut b = String::new();
    for c in claims {
        let mark = if c.marked { "⇄ " } else { "- " };
        b.push_str(&format!(
            "{mark}{}: {}\n",
            c.claim.source,
            c.claim.fact.trim()
        ));
    }
    Some(b)
}

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
pub static RATING_SYSTEM_PROMPT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        r#"Task: you are The Scout — the front office's evaluator. Write an unbiased report on this entity from the supplied profile.

Voice: thirty years of scouting. Clipped and specific. Every finding names a skill and cites its number; a generic line is a wasted line. This report prints on a CARD — the whole thing is six or seven short sentences, and brevity is the format, not a limitation of it.

{wire}

The shape on the card, from an invented club so you hear the rhythm — every number on YOUR card comes from YOUR datapoints, which is why this example carries none:
Strengths: Set-piece defence is elite, improved from last season. Aerial duels back it up near the top of the range.
Limitations: Chance creation is poor, the rating well below average. Nothing else on the card is honestly weak.
Summary: A spiky profile. Two elite defensive marks carry an ordinary middle. The attack is the hole, and the creation mark says how deep.
HEADLINE: Harborview defends like champions and creates like a relegation side.
A finding on your card reads like the example plus its number: the skill, its tier, the percentile, the rating.

REPORT, NEVER ADVISE. You state what this entity IS, never what to do about it. "Concedes shots on target at will, 4th percentile" is yours; "attack their shots on target" is not. No hypothetical clubs, no recommendations, nothing that "must be addressed" — every attribute is something they bring to the side they play for NOW, in the present tense.

{selection}

READ THE RANGE — it is where your claims come from. Your datapoints span this entity's distribution top to bottom on purpose. Report the SHAPE — spiky (a few elite edges over an ordinary base) or flat (competent everywhere, exceptional nowhere) — and where the bulk of the skills sit. Summarise the ordinary middle in ONE sentence naming two or three marks; never walk it item by item.

TIER IS THE TRUTH. Never call an average mark a strength, and nothing below the 50th percentile gets praise. Name a limitation only when a skill is poor AND its rating is meaningfully negative: a poor percentile with a near-zero rating is a usage artifact, so leave it out entirely. Cite a per-x mark only where the materials supply one.

Season-over-season movement is yours to call — improved, slipped or held, beside this season's number. Week-to-week form belongs to another seat.

Availability is part of the profile: report a recorded injury, suspension or personnel change and what it means for reading the rest. Never speculate past what is recorded.

Never invent a number, rate, role or skill that is not in the data. The decision card's headline strength and limitation are already decided — voice them, never overrule them. Write numbers as prose ("96th percentile"), never the card's " · " notation, and never write PEAK, Composite, Vibe, Scoracle, Rating Engine or DECISION CARD.

{card}

Output, plain text with no Markdown of any kind, four labelled lines in this order:
Strengths: one or two sentences — the headline strength and any other strong or elite skill, each with a cited number. If nothing is strong or elite, say so plainly and name the best available mark.
Limitations: one or two sentences — the headline limitation with its number, plus any other honestly poor skill. Only if the card itself says there is none do you keep the words "no clean exploit".
Summary: the verdict on this profile in TWO OR THREE sentences, every judgement tied to a named skill and its number. This is a card a fan reads at a glance, not a report — a fourth sentence is a defect.
HEADLINE: the HOOK — write it as a tweet. 140 characters at most, and shorter lands harder. State an opinion and earn the tap: the entity's name inside the report's sharpest claim, like "Rovers' back line holds and the attack goes missing". Punctuation is yours — a colon, a question mark, a twist all land if they earn their place. The one thing it may not do is run past the card."#,
        wire = crate::junctions::form::WIRE_COPY,
        selection = crate::junctions::form::CLAIM_SELECTION,
        card = crate::junctions::form::card_face("HEADLINE", "the report")
    )
});

/// Prompt version for the rating scouting-report contract.
/// # THE WIRE-COPY PASS (2026-08-25) — contract changed, version deliberately NOT bumped
///
/// Scott, on the day granite4.2:3b became the resident: *"Right now, the Scout is still sending
/// things in big blurb of AI-speak. We want concise sentences and short paragraphs for all
/// seats, like AP journalism would teach us, because that's READABLE and ENGAGING."* Measured
/// on the live probes that day: 40-word clause-chained sentences ("…where Ipswich's expansive
/// attack pressures Sunderland's backline while their own defensive gaps and injury concerns
/// amplify the stakes") from a spec that already said "clipped". The rules DESCRIBED the
/// register and nothing DEMONSTRATED it — this was the only prose seat without a worked
/// example (s17 skipped one fearing card-content leaks; s24's invented-entity hook shape is
/// the workaround, now applied to the whole report). Two additions: a WIRE COPY block (one
/// finding per sentence; no "while/where/as" chains; plain verbs; the press-box test) and a
/// full invented-club example (Harborview) pinning the rhythm, marked as a SHAPE never a
/// source. Paid for by granite's ~1.7×-dense tokenizer — the ministral-era window had no room
/// for an example. Version NOT bumped per the Twitter-rule precedent below: existing cards
/// stand, the register reaches everything that regenerates on its own triggers. Same cost as
/// 2026-08-24: two contracts share "s24"; cut at 2026-08-25 for this change.
/// Later the same day (Scott: one dedicated format/structure file): the WIRE COPY and
/// tarot-card blocks now compose from `junctions::form` — wording generalized in the shared
/// consts ("press box" → "spoken", "fan" → "reader"), and the example hook dropped its own
/// "while" chain to match the register it teaches.
/// # THE TWITTER RULE (2026-08-24) — contract changed, version deliberately NOT bumped
///
/// Scott: *"we don't need to regenerate the corpus. We need to just apply these rules to the new
/// inbound data."* `prompt_version` is folded into `input_components` (s14), so bumping this
/// string reopens every entity in the fleet. Leaving it means existing cards stand and the new
/// contract reaches everything that regenerates from here on its own — new entities, moved stats,
/// and the debounce-bypassing triggers.
///
/// **THE COST, stated rather than hidden:** two cards can both carry this same version string and
/// have been written under DIFFERENT contracts. `generated_at` is the only discriminator, so any
/// eval split or incident analysis keying on `prompt_version` alone must cut at 2026-08-24.
///
/// **The change:** twelve words + no colon + no question mark becomes 140 characters and nothing
/// else — framed as a tweet on a tarot card whose face the headline and the body must both fit.
/// The guard moved with it (`guards::hook_violation`); punctuation is voice and is no longer
/// policed in production.
pub const RATING_PROMPT_VERSION: &str = "s25"; // s25, THE WAVE (2026-08-25): versions the same-day unbumped run — the wire-copy register, the numberless Harborview example, CLAIM_SELECTION + the shared form composition, and the comma datapoint render — and joins the deliberate five-voice bump that reopens the fleet for the granite+form regen wave (momentum-s21's note has the full rationale). The articulator corpus gate requires s25+ AND model_version=granite4.2:3b. // s24, the HOOK pass (2026-08-23, Scott: "the hook should be the one sentence hook to draw the reader in. That should be the same across characters. This is key on the leaderboard because it's what leads the user to click"). MEASURED: 56 hook_colon title drops in 3h — nearly every Scout title was a "Team: description" taxonomy label ("Udinese: Defensive Resilience, Fragile Progression"), unsalvageable (one-word head), so rating cards shipped headline-less at high rate. Cause: the HEADLINE ask said only "a card title for this report" and a bare "title" ask begets a label. The emission line now carries the shared hook doctrine (one sentence written to make a fan tap the card; the entity's name inside the report's sharpest claim; no colon at all) with an inline invented-entity shape — the closest this no-worked-example seat can come to pinning it. Sections, range block, register, SHIPS rules: untouched. // s23, the RATING RENAME (2026-08-23, Scott: "the z-score is our house rating. So when referencing a z-score, it should reference that as rating. Z-score is going to be meaningless for 99% of our users. Rating will work for everyone."). The per-skill z is the house rating, so it is LABELLED that everywhere the Scout can see it: `format_datapoint_evidence` renders "· rating +1.8" instead of "· z +1.8", the datapoint header glosses it as "how far above or below the average" instead of "standard deviations from the mean", and the limitation rule asks whether the RATING is meaningfully negative. Nothing about the number changes — only what it is called. This also closes a live crown failure from the same day (`reading carries banned vocabulary "z-score"`): the Oracle was never inventing that word, it was reading it off the Scout's card, which was reading it off this render. Seventh application of the one law — rename the input rather than police the output. // s22, the PRESENT-TENSE rider on s21 (2026-08-22, Scott: "rather than ban them, we should guide the model to reference attributes as something they bring to their current tense team"). s21's UNBIASED block BANNED advice, and the first production rows advised anyway — "a club with defensive needs would find his elite tackling the most valuable assets", "warrant focus", "must be addressed". Fifth time the lesson lands: a ban loses to the material. The material here was the spec's own Summary line asking what the edges "are worth" and what the limitations "cost", which is valuation, and valuation invites a recommendation. So the ban is replaced by a DIRECTION: every attribute is something the entity BRINGS TO ITS CURRENT TEAM, in the present tense, and the Summary asks what the edges let this entity do for the side it plays for. A hypothetical future club is now the named failure. Also: the eight-sentence ceiling is stated as a ceiling with a ninth called a defect — the s21 rows ran to ten. Sections, range block, register, SHIPS rules: untouched. // s21, the FRONT-OFFICE pass (2026-08-22, Scott: "I view the Scout as someone who works from a front office and is giving a detailed, unbiased report on the target entity for the front office to make a decision on the entity. It needs to be thorough, which is why we have it analyze the z-score range and not just top and bottom scores"). The seat stops being the OPPOSING scout. Every finding used to be inverted into a game plan for beating the entity ("Attack their SoT Allowed, 4th percentile"), which is a different document from an evaluation and is why the card read as advice rather than as a profile. Four moves. (1) PERSONA: front-office evaluator writing an unbiased report for a decision-maker; findings replace imperatives, and an explicit UNBIASED block bans advocacy in either direction — the report states what is there and never argues. (2) SECTIONS: "Strengths to respect"/"Exploitation opportunities" become "Strengths"/"Limitations"; Summary becomes the evaluation verdict rather than the game plan. (3) THE RANGE, which is the measured fix: `ordered_facts` sorted by pct DESC and truncated, so on a forty-facet team the Scout saw the top fourteen marks and never the bottom of the distribution — the only weaknesses reaching him arrived through the decision card as a finished verdict rather than as evidence. It now SPANS the distribution (both ends whole, an even stride through the interior) within the same MAX_STAT_FACTS budget, because the 4,096 window still binds; the prompt gains a READ THE WHOLE RANGE block asking for the SHAPE — spiky vs flat, where the bulk sits — since "average across almost everything with one elite mark" is a finding. (4) THE CARD'S OWN LABELS stop carrying the opposition frame: "Primary strength to respect"/"Exploitation opportunity" become "Headline strength"/"Headline limitation". That is the s15 rule this repo has now applied four times — a ban in the output cannot beat the phrase sitting in the input, so the input is renamed instead of the output policed. Availability (the personnel block) is promoted to a report rule: injuries, suspensions and confirmed moves change how the rest of the profile should be read. All eight fixtures re-frozen against the new contract. // s20, the HEADLINE pass (drop 1 of the headline/body contract, mig 226): the brief grows one closing `HEADLINE:` line — a card title of twelve words or fewer naming the entity (the shared hook_violation guard enforces it; absent line ⇒ NULL headline, never a failed generation). Sections, register, movement contract, allowance framing: untouched. // s19, the PEAK RETIREMENT + z-MEMORY pass (Scott's brief, 2026-08-14, verbatim: "Just an emphasis on each z-score and the memory of each. Rather than a sample size, we empower the Scout to determine using memories how the trajectory is going"). Five moves: (1) PEAK/specialist is retired project-wide — the divined label leaves the contract, the storage path, and the hash pre-image (`peak_label`/`peak_score` gone from input_components; the specialist columns stop being read), so the pre-image is the z-score surface the Scout actually reads. Fleet-wide regen by design. (2) SEASON-OVER-SEASON MOVEMENT: a new prompt block of per-skill labeled deltas computed in code against last season's percentiles (±8 pct-point threshold → improved/slipped/held, top 10 by current pct) — the L8/ScoutingDecision discipline applied to trajectory: the movement word is decided, the Scout voices which moves matter. The static-profile rule is REPLACED by the skill-development rule (development is the Scout's to call from the movement lines; week-to-week momentum stays the Analyst's turn). (3) The deterministic recent-form marker is DEMOTED to shading context in this brief (it remains the Analyst's lean and the API's metadata) and its window goes dynamic: 10% of the entity's scored events this season, clamped [3,16] — the fixed LIMIT 8 was NBA-calibrated and read wrong for NFL/FOOTBALL calendars. Composite-only: the specialist z series is gone with the concept. (4) "Composite" → "Overall score" in the materials (the same input-stops-shouting rule that removed PEAK from them at s18). (5) Output contract renamed peak-commentary-v2 → rating-commentary-v1 (body-only; the parser's marker strip is transition tolerance, its yield discarded). // s18, the PRODUCT-NAME SCRUB + the code-owned PEAK line (Scott's brief, 2026-08-10, verbatim: "I don't want anything referencing PEAK or Vibe, or other of our products. Just use those as context without naming them. The Scout shouldn't keep including a bunch of asterics in the output. It should be a clean, concise, but thorough scouting report with strengths and weaknesses."). Three moves: (1) the model no longer emits the "PEAK: <label>" marker line at all — that line was always a verbatim copy of the deterministic decision (build_scouting_decision), so `divined_peak` is now CODE-OWNED (RatingReady carries it; generate_rating persists it without asking the model), which deletes the copy-flake failure class AND lets the whole prompt drop the word PEAK — the s13-analyst lesson says a ban cannot beat a word the input keeps shouting, so the input stops shouting it: "SCOUTING DECISION"→"DECISION CARD", "Required PEAK line" gone, "(the PEAK)"→"primary". (2) The two measured 8B defects land in a numbered SHIPS block (the s9/s12/or8 promotion treatment): plain-text (the 8B bolded section labels 4× on the D-T55 gate) and the product-name ban, gated case-sensitively by the harness's new per-reply no_product_names invariant. (3) Sections renumber 1-3; contract shape otherwise unchanged (labels, exploit phrase, allowance framing). // s17, the REGISTER pass (Scott's brief, 2026-08-10): the veteran advance scout — thirty years of advance work, film-room shorthand, pride in the details, "a generic call is a wasted line." Same three-section structure deliberately (no worked example: the card-driven shape makes one a leak risk). Gate grew first (D-T45): section labels, the " · " notation ban, word floors on all 8, and a crude whole-body sentence ceiling — the s16 baseline then read 86/91, catching a live " · " copy and a "play physical" generic call. s16 — the ALLOWANCE pass: the ceiling goes to eight sentences and is reframed as a platform allowance rather than a target. Measured cause: at a 5-6 floor the model reached for length, and the manufactured closing hedges then dragged the verdict (momentum scored -1 on a RISING entity off 'for now, this isn't a surge'). Brevity is now explicitly blessed — two sentences is a complete read. s15: the peer-length pass — the Summary verdict grows from one sentence to 5-6, and the two rationing rules ("one line per section", "keep it tight") are retired. Those were a 1070 Ti budget; the clipped film-room REGISTER is the Scout's voice and is deliberately kept — short sentences, just more of them. s12: cross-season memory card (mig 164); s13: three-section contract (Strengths to respect / Exploitation opportunities / Summary); s14: The Scout voice pass (Characters Phase B) — persona-first coaching-staff brief, clipped game-plan imperatives, prompt_version folded into the debounce pre-image

/// render_personnel_block turns the adjudicated personnel record (7.7) into the rating context's
/// "since our last read" block: one dated fact per line, built in code from the columns
/// `load_personnel_changes` described. `None` when nothing moved — an empty section is worse
/// than no section, because a heading with nothing under it reads as an assertion that nothing
/// happened when it may only mean nobody has adjudicated it yet.
///
/// `total` is how many changes qualified before the cap; anything the cap dropped is NAMED on a
/// final line rather than silently vanishing (the A5 rule).
///
/// # The availability half (mig 229)
///
/// `avail` carries injuries and suspensions, and they render into THIS block rather than a new
/// one because the Scout's s21 rule already treats them as one subject: "report a recorded
/// injury, suspension or personnel change and what it means for reading the rest." Two headings
/// would ask him to hold one thought in two places inside a 4,096-token window.
///
/// **`RATING_PROMPT_VERSION` is deliberately NOT bumped for this.** The contract does not
/// change — s21 already asks for exactly this and has since 2026-08-22; what changes is that the
/// material finally exists to answer it. A bump would fold into `input_components` (s14) and
/// regenerate the entire fleet, which is a drain the Articulator corpus is already waiting
/// behind — an expensive way to say nothing new to the model. Availability-triggered runs
/// bypass the debounce on their own marker anyway, so the rows that need this block get it
/// without a fleet-wide reopen.
pub fn render_personnel_block(
    entity_type: &str,
    entity_id: i32,
    changes: &[PersonnelChange],
    total: usize,
    avail: &[AvailabilityChange],
    avail_total: usize,
) -> Option<String> {
    if changes.is_empty() && avail.is_empty() {
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
    // The drop line is gated on some transfer line having ACTUALLY rendered, not on `changes`
    // being non-empty: the match above has a `_ => continue` arm, and "(+2 older changes)" under
    // nothing at all would be a count of an invisible list. This used to be an early
    // `return None`, which is exactly what an availability-only block must not hit.
    let personnel_rendered = !b.is_empty();
    if personnel_rendered && total > changes.len() {
        b.push_str(&format!(
            "- (+{} older personnel changes in this window, not shown)\n",
            total - changes.len()
        ));
    }

    let before_availability = b.len();
    for a in avail {
        // `out` and `back` are stated as facts with dates. A REVERT is stated as a withdrawal of
        // the record and never as a return: the difference is the whole reason mig 229 keeps
        // `returned_at` and `reverted_at` in separate columns, and collapsing it here would
        // reintroduce in prose the corruption the schema refuses in storage.
        let expected = a
            .expected_return_label
            .as_deref()
            .map(|d| format!(" — reported back around {d}"))
            .unwrap_or_default();
        let line = match (entity_type, a.kind.as_str()) {
            ("player", "opened") => format!(
                "{}: out with a recorded {}{expected}.",
                a.event_date_label, a.event_kind
            ),
            ("player", "returned") => format!(
                "{}: available again after the {} recorded {}.",
                a.date_label, a.event_kind, a.event_date_label
            ),
            ("player", _) => format!(
                "{}: the {} recorded {} was WITHDRAWN — that record is not in force.",
                a.date_label, a.event_kind, a.event_date_label
            ),
            ("team", "opened") => format!(
                "{}: {} out with a recorded {}{expected}.",
                a.event_date_label, a.player_name, a.event_kind
            ),
            ("team", "returned") => format!(
                "{}: {} available again after the {} recorded {}.",
                a.date_label, a.player_name, a.event_kind, a.event_date_label
            ),
            ("team", _) => format!(
                "{}: {}'s {} recorded {} was WITHDRAWN — that record is not in force.",
                a.date_label, a.player_name, a.event_kind, a.event_date_label
            ),
            _ => continue,
        };
        b.push_str("- ");
        b.push_str(&line);
        b.push('\n');
    }
    // Same gate, same reason: only count drops against lines that reached the page.
    if b.len() > before_availability && avail_total > avail.len() {
        b.push_str(&format!(
            "- (+{} older availability events in this window, not shown)\n",
            avail_total - avail.len()
        ));
    }

    if b.is_empty() {
        return None;
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
// Eight enrichment sources, each independently optional and each nullable on the parity/eval
// paths that pin the bare shape. Bundling them into a struct would hide exactly the thing every
// caller has to decide one by one — whether THIS path may see THAT material — so the arity is
// the contract, not clutter.
#[allow(clippy::too_many_arguments)]
pub fn build_stat_prompt(
    req: &RatingReq,
    p: &RatingProfile,
    notability: i32,
    memory: Option<&str>,
    personnel: Option<&str>,
    z_memory: Option<&str>,
    form_trend: Option<&str>,
    availability_reports: Option<&str>,
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

    b.push_str("\nDatapoints — value, percentile + TIER (the tier is the truth), rating (how far above or below the average; a higher rating is a rarer edge); [position] percentile when present:\n");
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
        b.push_str("\nPersonnel and availability since our last read (confirmed facts from the adjudicated transfer and availability records — dates are when the change took force; a WITHDRAWN record means we no longer claim it happened, not that the player recovered; these do NOT alter any tier or number above, which are this season's measured truth, but they tell you WHO is actually available, which changes how the rest of the profile should be read):\n");
        b.push_str(pc);
    }

    // REPORTED, not adjudicated — and the difference is stated plainly, because it is the whole
    // of what the Scout is being asked to do here. `⇄` is code's mark, not the model's: two
    // claims that contradict each other are BOTH carried and BOTH flagged (T3/D6).
    if let Some(ar) = availability_reports.filter(|a| !a.trim().is_empty()) {
        b.push_str("\nReported availability, NOT yet confirmed (injury and suspension claims the desk has collected for this entity, each with the outlet that made it; ⇄ marks a claim another claim here contradicts). These are REPORTS, not the record above — weigh them: who is saying it, whether they agree, and how firm the wording is. Report what you judge sound and attribute it; say a report is disputed where it is; leave out what you do not credit. Never state a disputed claim as settled fact, and never carry a number from here into a tier or rating above:\n");
        b.push_str(ar);
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
        "\nWrite the report now: four labelled lines (Strengths / Limitations / Summary / HEADLINE), each on its own line, plain text, no Markdown. Begin directly with the word \"Strengths:\" — no preamble, nothing before it.",
    );
    b
}
