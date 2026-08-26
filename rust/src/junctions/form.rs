//! # THE FORM — the shared structure every character composes, so prompts describe VOICE.
//!
//! Scott, 2026-08-25, the architecture in two sentences: *"one dedicated format/structure file
//! and each of the character files are just character descriptions/tuning."* And the insight
//! it rests on, from teaching English: *"kids can write perfect papers when they're given a
//! clear structure … Narratives, emotional threads, scouting reports, they all have
//! arguments/claims to make. Those claims need evidence, and then a closing sentence. With
//! this format, we can tell as many stories as there are gracefully … and remove a lot of the
//! restrictive prompting and really let the model express its story."*
//!
//! Three shared pieces, and the split matters:
//!
//! - [`STORY_FORM`] — the STRUCTURE: lead, then one paragraph per claim (claim sentence,
//!   evidence sentences, closing sentence). The seat says what its claims ARE (the
//!   Influencer's are feelings, the Analyst's is the decided direction); the form says how a
//!   claim is told.
//! - [`WIRE_COPY`] — the SENTENCE REGISTER: short and declarative, AP-desk discipline. Scott:
//!   *"concise sentences and short paragraphs for all seats, like AP journalism would teach
//!   us, because that's READABLE and ENGAGING. A giant blurb, no matter how elegant the
//!   prose, is not the content."*
//! - [`card_face`] — the tarot-card fit block, parameterized on the two nouns each seat uses
//!   for its front and back (previously hand-copied per seat with drifting wording).
//!
//! These live in the PROMPT layer because they are a pathway, not a check (Scott's framing:
//! *"we give them the pathway which gives them the platform to express themselves"*). The
//! mechanical floor stays where it is: `guards.rs` polices integrity and leaks in production;
//! whether a paragraph carries its claim is the fixture gate's and the human deck's to judge.
//!
//! Worked examples stay SEAT-SIDE deliberately — an example teaches a voice, and the voice is
//! exactly what a seat's file is for (the Scout's Harborview report, the Influencer's v17
//! example). The migration is incremental: a seat adopts the shared pieces on its next
//! contract pass, replacing its hand-copied variants byte-consciously — never a silent fleet
//! rewrite.

/// What deserves a statement — the claim-SELECTION doctrine, separate from [`STORY_FORM`]'s
/// claim CONSTRUCTION so a seat whose reply contract is not yet paragraphs (the Scout's
/// labelled lines) still trains on it.
///
/// Scott, 2026-08-25: *"Sometimes 'boring, middle of the pack' is the claim to make, just as
/// much as 'elite', or 'abysmal' are. That should be the training all the characters are
/// looking for … you're looking for what aspects deserve a statement. Sometimes that's
/// ambiguity, sometimes it's exceptionalism."* The final line is his compression direction
/// wearing its selection hat: a budget is a ceiling, not a target, and a claim made to fill
/// space is the one kind of claim the card cannot carry.
pub const CLAIM_SELECTION: &str = r#"WHAT DESERVES A STATEMENT. That is your selection question, and the answer is not always the loudest thing. Exceptionalism deserves one: elite earns a claim, and abysmal earns a claim. Ordinariness deserves one when it is the story — "boring, middle of the pack" is as honest a claim as "elite". Ambiguity deserves one when the signals genuinely point both ways: naming the tension is a claim, not a failure to choose. Filler never deserves one — a claim made because space existed is the one claim the card cannot carry."#;

/// The story structure — lead, then claim-paragraphs. See the module doc for provenance.
///
/// The HOOK/HEADLINE line is the form's LEAD and keeps its seat-side contract (the tweet
/// rule); this const governs the BODY between the hook and the card's edge.
pub const STORY_FORM: &str = r#"THE FORM. Your body is built from claims, one paragraph per claim. The material decides how many claims you have — one story or several, told as many as the evidence supports and no more.

Each paragraph is built the same way. The first sentence states the claim, plain and committed. The next one to three sentences are the evidence: named facts, cited numbers, attributed reports. The last sentence closes the claim, reinforced by what the evidence just showed.

A claim without evidence is not a paragraph — cut it. Evidence without a claim is a list — find its claim or let it go. Separate paragraphs with a blank line.

The structure is invisible on the card. Never write the words "Claim", "Evidence" or "Close" — not as labels, and not as subjects: you never write "the claim is tension", you write the tension itself. Never describe or grade your own structure. The paragraph simply reads that way, and the reader never sees the frame."#;

/// The sentence register — wire copy, every seat. Generalized from the Scout's 2026-08-25
/// wire-copy pass (the "big blurb of AI-speak" correction), where it was measured against
/// 40-word clause-chained live output.
pub const WIRE_COPY: &str = r#"WRITE LIKE WIRE COPY. Short, declarative sentences: subject, verb, fact. One idea per sentence — if a sentence needs a second breath, it is two sentences. Never chain ideas with "while", "where" or "as"; a chained sentence buries both. Plain words over grand ones: a side concedes, wins, holds, slips — nothing "amplifies the stakes" or "underscores the narrative". Read every line aloud: if it would not survive being spoken, cut it."#;

/// The tarot-card fit block. `front` and `back` are the seat's own nouns for what the fan
/// sees first and what they turn the card over for — e.g. `("HOOK", "a VIBE")` or
/// `("HEADLINE", "the report")`.
pub fn card_face(front: &str, back: &str) -> String {
    format!(
        "THE CARD IS A TAROT CARD. Everything you write has to fit on its face: a {front} the \
         reader sees first, and {back} they turn it over for. That shape is the format — \
         nothing here runs to a page."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_face_carries_the_seat_nouns() {
        let s = card_face("HOOK", "a VIBE");
        assert!(s.contains("a HOOK the reader sees first"));
        assert!(s.contains("a VIBE they turn it over for"));
    }

    #[test]
    fn the_form_states_the_paragraph_shape() {
        assert!(STORY_FORM.contains("one paragraph per claim"));
        assert!(STORY_FORM.contains("first sentence states the claim"));
        assert!(STORY_FORM.contains("closes the claim"));
        assert!(WIRE_COPY.contains("subject, verb, fact"));
    }

    #[test]
    fn selection_names_all_four_kinds_of_claim() {
        // Exceptional both ways, honest ordinariness, real ambiguity — and never filler.
        assert!(CLAIM_SELECTION.contains("elite earns a claim"));
        assert!(CLAIM_SELECTION.contains("abysmal earns a claim"));
        assert!(CLAIM_SELECTION.contains("middle of the pack"));
        assert!(CLAIM_SELECTION.contains("naming the tension is a claim"));
        assert!(CLAIM_SELECTION.contains("Filler never"));
    }
}
