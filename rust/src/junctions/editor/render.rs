//! The packet renderer (PLAN-one-rail 7.2): `(packet, entity, voice) → context block`.
//!
//! This was the seam the cutover flipped, and the flip is finished: the `RAIL` switch is gone,
//! the legacy article-corpus path was demolished in Phase 9.1, and migs 222/223 took its SQL.
//! **There is one rail.** Every voice loads a *rendered packet* — the storyline the Desk
//! assembled in Phase 6, written down for one entity, for one voice, in a hard budget. Comments
//! below that still name `RAIL=legacy` are describing history, not a branch you can take.
//!
//! Three laws shape every line below:
//!
//! * **The budget is HARD** (§7's 4096 envelope). The render is capped at
//!   [`RENDER_TOKEN_BUDGET`] tokens, estimated `chars / 3.6` and asserted for real against
//!   `eval_count` telemetry in 7.15. Oldest claims are dropped first, and **every dropped claim
//!   is named** (the A5 rule) so a thin render is always explainable.
//! * **Contested state is preserved** (T3/D6). Two claims that say opposite things about the same
//!   thing are BOTH rendered, both attributed, and marked `⇄` — the disagreement is the story, so
//!   the marker is a pointer, never a filter. Nothing here collapses a pair.
//! * **Voice routing is data, never a model's judgment** (E1). The voice selects a SLICE by rule
//!   ([`Voice::slice`]); no model is asked what it should be allowed to read. The Scout has no
//!   variant in this enum at all — T4: no packet prose reaches it, ever.

use super::packet::PacketView;
use std::collections::HashSet;

/// The render's hard ceiling, in estimated tokens (§7's envelope: ≤2,000 for the packet part).
pub const RENDER_TOKEN_BUDGET: usize = 2_000;

/// Characters per token for the pre-flight estimate. English prose over this corpus runs ~3.9;
/// 3.6 is deliberately pessimistic, so the estimate errs toward rendering LESS than the budget
/// allows rather than overrunning a 4096 window. 7.15 measures the real ratio via `eval_count`.
pub const CHARS_PER_TOKEN: f32 = 3.6;

/// Claims are the compressible part; the rest of the block (headline, role line, facts,
/// continuity) is small and fixed. This floor keeps a pathological header from starving the
/// claims entirely — at least this many always render, budget or not.
const MIN_CLAIMS: usize = 3;

/// Tokens held back for the "(+N older report(s) not shown)" footer the budget itself may add.
const FOOTER_RESERVE: usize = 20;

/// The voices that may read a packet.
///
/// # T4 NARROWED, 2026-08-23 — the Scout gets a variant, and this is exactly what changed
///
/// This comment used to read: *"The Scout is absent by construction (T4, and 7.7 names it):
/// confirmed facts reach it by the stats platform and `transfer_identity_applications`, never by
/// packet prose. There is no `Voice::Scout` to pass, so the law is enforced by the type."*
///
/// Scott's ruling (2026-08-23): *"Editor notices injury/suspension and tags the Scout → the Scout
/// decides the legitimacy of the report → event is included in the report… I'd like to empower
/// each model. Guards over evals. We can let the model do the work versus trying to engineer a
/// rigid process."* A seat cannot judge a report it is forbidden to read, so the type-level ban
/// is the thing that had to move.
///
/// **Repealed:** the Scout may read claims — attributed prose — from a packet.
///
/// **Still holding, and still enforced by this type:** he reads ONLY his slice — injury and
/// suspension claims, nothing else. No general packet prose, no headline framing, and
/// `sees_register` stays false so the Influencer's charged phrase remains hers alone. No other
/// voice's slice changes. The alternative on the table — a `player_availability` adjudication
/// plus a new Editor contract field naming the injured party — was the rigid process this ruling
/// rejected, and it would have put a code gate in front of a judgement the Scout is better placed
/// to make from the evidence itself.
///
/// The correctness floor moves with it, from the INPUT to the OUTPUT: `mark_contested` still
/// points mechanically at claims that disagree (it marks, never filters — T3/D6), and `guards.rs`
/// owns what may not survive into served prose. That is the mechanical-floor rule the guard
/// module already states in Scott's own words: *"the guards allow the model freedom, which is our
/// goal."*
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    /// `narratives` — the whole packet: every claim, whatever its type.
    Journalist,
    /// `transfers` — transfer-typed claims only (7.5); the slice its fingerprint hashes.
    Insider,
    /// `vibe` — the register and its phrase are HER material (7.6); nobody else is shown them.
    Influencer,
    /// `momentum` — reads the packet as peer context (7.8).
    Analyst,
    /// `sigil` — **never handed a packet in production** (§4: "the Oracle is blind to evidence:
    /// five cards + its own verdict trail, nothing else"). 7.2 sketched a crown-side render; 7.8
    /// did not build it, because reading the packet directly would make the Oracle a seventh
    /// reporter instead of the reader of six cards. The variant survives so the register test can
    /// prove even the terminal voice cannot see the Influencer's phrase.
    Oracle,
    /// `rating` — injury and suspension claims only. The Editor TAGS him and he weighs the
    /// report himself; see the T4 note on this enum for what that repealed and what it did not.
    Scout,
}

impl Voice {
    /// The stage string this voice drains, and the key its slice fingerprint lives under
    /// (`slice_fingerprints ->> stage`, pinned by mig 206).
    pub fn stage(self) -> &'static str {
        match self {
            Voice::Journalist => "narratives",
            Voice::Insider => "transfers",
            Voice::Influencer => "vibe",
            Voice::Analyst => "momentum",
            Voice::Oracle => "sigil",
            Voice::Scout => "rating",
        }
    }

    /// Whether this voice sees the register and its phrase. The Influencer owns the number and the
    /// phrase is her raw material (§1a: "the Editor never scores"); handing the same charged
    /// phrase to the Journalist would leak her judgment into his copy.
    fn sees_register(self) -> bool {
        matches!(self, Voice::Influencer)
    }

    /// Whether this voice sees the packet's compiled STORY headline. False for the Journalist,
    /// measured 2026-08-26 on the full-fleet corpus extract: her narrative TITLES copied the
    /// framing's headline verbatim — "Fee gap is all that holds up Carvalho's move to Leeds"
    /// served byte-identical as HER title on nine-plus different entities' cards (every subject
    /// of the storyline gets the same framing line), several times over a body about a
    /// different story entirely. The input-shouting law, ninth application: a headline-shaped
    /// line in the input beats any rule about writing your own, so the input stops carrying it.
    /// She keeps the type, the result line, the participants and every claim — the material a
    /// title is honestly built FROM — and the compiled headline stays context for the voices
    /// that read the packet as background rather than writing headlines of their own.
    fn sees_story_headline(self) -> bool {
        !matches!(self, Voice::Journalist)
    }

    /// The claim slice this voice reads, as the set of `story_type`s admitted. `None` = every
    /// claim.
    ///
    /// A SET rather than one type because the Scout's subject spans two: an injury and a
    /// suspension are different causes with the same consequence — the player is unavailable —
    /// and the Editor's contract already emits them as separate `story_type` values. Splitting
    /// them across two slices would make a club's Saturday read as two unrelated facts.
    fn slice(self) -> Option<&'static [&'static str]> {
        match self {
            Voice::Insider => Some(&["transfer"]),
            Voice::Scout => Some(&["injury", "suspension"]),
            _ => None,
        }
    }
}

/// One claim, as the renderer reads it out of `packets.claims`.
#[derive(Clone, Debug)]
pub struct RenderClaim {
    pub article_id: i64,
    pub source: String,
    pub fact: String,
    pub published_at: Option<i64>,
    pub story_type: String,
}

/// The entity's part in this storyline — its role and the span it has been in the story
/// (`storyline_entities`: `role`, `joined_at`, `last_seen_at`). D5: the part has its own lifespan,
/// so the render states it rather than implying the entity was there from the first word.
#[derive(Clone, Debug)]
pub struct Participation {
    pub name: String,
    pub role: Option<String>,
    /// Formatted dates (`YYYY-MM-DD`); the caller owns the clock, this module never reads one.
    pub joined_on: Option<String>,
    pub last_seen_on: Option<String>,
}

/// A rendered context block, with the accounting that makes its size auditable.
#[derive(Clone, Debug)]
pub struct Rendered {
    pub text: String,
    /// `chars / CHARS_PER_TOKEN`, rounded up — the pre-flight estimate, not a measurement.
    pub est_tokens: usize,
    pub claims_rendered: usize,
    /// Article ids whose claims the budget dropped, oldest first. NAMED, never silently lost —
    /// this is the A5 rule, and it is what the exclusions telemetry in 7.3 reports.
    pub claims_dropped_from: Vec<i64>,
    /// How many rendered claims carry a `⇄` (contested) marker.
    pub contested_marked: usize,
}

impl Rendered {
    fn empty() -> Self {
        Self {
            text: String::new(),
            est_tokens: 0,
            claims_rendered: 0,
            claims_dropped_from: Vec::new(),
            contested_marked: 0,
        }
    }
}

/// Estimated tokens for a string: `chars / 3.6`, rounded up.
pub fn est_tokens(s: &str) -> usize {
    (s.chars().count() as f32 / CHARS_PER_TOKEN).ceil() as usize
}

/// render is the whole of 7.2, pure: a packet, the entity's part in it, and a voice in — one
/// context block out. No clock, no database, no model, so the property test in this file scores
/// exactly what production sends.
///
/// Claims arrive NEWEST FIRST (the loader's order) and are rendered newest first; the budget
/// truncates from the OLD end, because the newest state of a contested story is the part a voice
/// cannot do without.
pub fn render(packet: &PacketView, part: Option<&Participation>, voice: Voice) -> Rendered {
    let claims = select_claims(&packet.claims, voice);
    if claims.is_empty() && packet.headline.is_none() {
        return Rendered::empty();
    }

    let contested = mark_contested(&claims);
    let header = framing(packet, part, voice);

    let claims_header = if contested.iter().any(|c| c.marked) {
        "REPORTED (newest first; ⇄ marks claims that contradict another below — both stand):\n"
    } else {
        "REPORTED (newest first):\n"
    };

    // Fit the claims to what the budget leaves after the header.
    // Everything that is not a claim line, plus the footer the truncation may add: reserved up
    // front, so the block that reports the truncation cannot itself push the render over.
    let overhead = est_tokens(&header) + est_tokens(claims_header) + FOOTER_RESERVE;
    let claim_budget = RENDER_TOKEN_BUDGET.saturating_sub(overhead);

    let mut kept: Vec<&MarkedClaim> = Vec::new();
    let mut used = 0usize;
    for c in &contested {
        let line = claim_line(c);
        // +1 for the newline the line is joined with: per-line rounding is what keeps the sum an
        // over-estimate rather than an under-estimate, and the separator has to be inside it.
        let cost = est_tokens(&line) + 1;
        if kept.len() >= MIN_CLAIMS && used + cost > claim_budget {
            break;
        }
        used += cost;
        kept.push(c);
    }

    let dropped: Vec<i64> = contested
        .iter()
        .skip(kept.len())
        .map(|c| c.claim.article_id)
        .rev() // oldest first, the order they were dropped in
        .collect();

    let mut text = header;
    text.push_str(claims_header);
    for c in &kept {
        text.push_str(&claim_line(c));
        text.push('\n');
    }
    if !dropped.is_empty() {
        // A5: the render says what it left out, in the block itself, so the omission is visible to
        // the reader of a ledger row and not only to whoever reads the telemetry.
        text.push_str(&format!(
            "(+{} older report(s) not shown — budget)\n",
            dropped.len()
        ));
    }

    let contested_marked = kept.iter().filter(|c| c.marked).count();
    let claims_rendered = kept.len();
    Rendered {
        est_tokens: est_tokens(&text),
        text,
        claims_rendered,
        claims_dropped_from: dropped,
        contested_marked,
    }
}

/// framing is the header of a render: everything that is not a claim — the story, this entity's
/// part in it, its type, any completed result, the mood (Influencer only), the rest of the cast,
/// and the one continuity line from the prior packet.
///
/// It is public because the Journalist does not read the block form: his contract requires
/// NUMBERED evidence he can cite (`load_packet_corpus`, 7.3), so he takes this framing and
/// numbers the claims himself. Same bytes, two shapes, one source.
pub fn framing(packet: &PacketView, part: Option<&Participation>, voice: Voice) -> String {
    let mut header = String::new();
    if voice.sees_story_headline() {
        if let Some(h) = &packet.headline {
            header.push_str("STORY: ");
            header.push_str(h.trim());
            header.push('\n');
        }
    }
    if let Some(p) = part {
        header.push_str(&role_line(p));
        header.push('\n');
    }
    if !packet.story_types.is_empty() {
        header.push_str("TYPE: ");
        header.push_str(&packet.story_types.join(", "));
        header.push('\n');
    }
    if let Some(line) = &packet.result_line {
        // Verbatim from the text, parsed by code, never invented by a model (§1a).
        header.push_str("RESULT: ");
        header.push_str(line.trim());
        header.push('\n');
    }
    if voice.sees_register() {
        if let Some(reg) = &packet.register {
            header.push_str("MOOD: ");
            header.push_str(reg);
            if let Some(phrase) = packet.register_phrase.as_deref().filter(|p| !p.is_empty()) {
                header.push_str(" — \"");
                header.push_str(phrase.trim());
                header.push('"');
            }
            header.push('\n');
        }
    }
    // The thin, structured facts (§1c): who else is in this story, and how much of it there is.
    // Names only — the entity list is data the code assembled, not prose a model wrote.
    let others = other_participants(&packet.facts, part);
    if !others.is_empty() {
        header.push_str("ALSO IN THIS STORY: ");
        header.push_str(&others.join(", "));
        header.push('\n');
    }
    if let Some(prior) = &packet.prior_headline {
        // ONE continuity line from the prior packet: enough for a voice to know this story has a
        // yesterday, far short of re-reading it.
        header.push_str("PREVIOUSLY: ");
        header.push_str(prior.trim());
        header.push('\n');
    }

    header
}

/// The other named participants from `facts.entities`, minus the entity this render is FOR.
/// Capped: a listicle-seeded storyline can carry a dozen, and this line is context, not a cast
/// list — the ones past the cap are behind the `+N more` the caller can see is bounded.
fn other_participants(facts: &serde_json::Value, part: Option<&Participation>) -> Vec<String> {
    const MAX_OTHERS: usize = 8;
    let me = part.map(|p| p.name.as_str()).unwrap_or("");
    let mut names: Vec<String> = facts
        .get("entities")
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let n = e
                        .get("name")
                        .and_then(|n| n.as_str())
                        .filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case(me))?;
                    // Person casts carry their kind ("coach, Real Madrid") — a bare
                    // person name is trivia, a described one is context. Absent on
                    // pre-mig-234 packets and on players/teams.
                    match e.get("descriptor").and_then(|d| d.as_str()) {
                        Some(d) if !d.is_empty() => Some(format!("{n} ({d})")),
                        _ => Some(n.to_string()),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    names.dedup();
    if names.len() > MAX_OTHERS {
        let extra = names.len() - MAX_OTHERS;
        names.truncate(MAX_OTHERS);
        names.push(format!("+{extra} more"));
    }
    names
}

fn role_line(p: &Participation) -> String {
    let mut s = format!("ENTITY: {}", p.name);
    if let Some(r) = p.role.as_deref().filter(|r| !r.is_empty()) {
        s.push_str(&format!(" ({r})"));
    }
    match (&p.joined_on, &p.last_seen_on) {
        (Some(j), Some(l)) if j == l => s.push_str(&format!(" — in this story {j}")),
        (Some(j), Some(l)) => s.push_str(&format!(" — in this story {j} → {l}")),
        (Some(j), None) => s.push_str(&format!(" — in this story since {j}")),
        _ => {}
    }
    s
}

fn claim_line(c: &MarkedClaim) -> String {
    let mark = if c.marked { "⇄ " } else { "- " };
    format!("{mark}{}: {}", c.claim.source, c.claim.fact.trim())
}

/// A claim plus whether it contradicts another claim in the same render.
#[derive(Clone, Debug)]
pub struct MarkedClaim {
    pub claim: RenderClaim,
    /// True when another claim in the same set says the opposite. A POINTER, never a filter:
    /// both members of the pair are always carried (T3/D6).
    pub marked: bool,
}

/// Pull this voice's slice out of the packet's claims, newest first (the stored order).
///
/// Public because the voices whose contracts want the claims as DATA rather than as a rendered
/// block — the Insider's per-article overlay (7.5), the Journalist's numbered evidence (7.3) —
/// must select the same subset the block form would, and the same subset
/// `slice_fingerprints ->> stage` hashes. One definition of "your slice", three readers.
pub fn slice_claims(claims: &[RenderClaim], voice: Voice) -> Vec<RenderClaim> {
    select_claims(claims, voice)
}

fn select_claims(claims: &[RenderClaim], voice: Voice) -> Vec<RenderClaim> {
    match voice.slice() {
        None => claims.to_vec(),
        Some(want) => claims
            .iter()
            .filter(|c| {
                want.iter()
                    .any(|w| c.story_type.eq_ignore_ascii_case(w))
            })
            .cloned()
            .collect(),
    }
}

/// mark_contested finds pairs that say opposite things about the same subject and flags BOTH.
///
/// The rule is deliberately mechanical (T2 — code renders the judgment, never a model): two claims
/// are a contested pair when they talk about the same thing (stem-overlap of their content words
/// at or above [`OVERLAP_FLOOR`], by the overlap coefficient, so a six-word claim can contest a
/// thirty-word one) and their NEGATION POLARITY differs. "Arsenal have reached an agreement in
/// principle" against "The Athletic: deal not agreed" — shared stem `agree`, opposite polarity.
///
/// It marks; it never merges, drops, or reorders. A false mark costs a `⇄` on a line that stands
/// anyway; a missed one costs nothing but the pointer. Both failure modes are survivable, which is
/// why a heuristic is allowed to hold this pen at all.
pub fn mark_contested(claims: &[RenderClaim]) -> Vec<MarkedClaim> {
    let prepared: Vec<(HashSet<String>, bool)> = claims
        .iter()
        .map(|c| (content_stems(&c.fact), is_negated(&c.fact)))
        .collect();

    let mut marked = vec![false; claims.len()];
    for i in 0..claims.len() {
        for j in (i + 1)..claims.len() {
            if prepared[i].1 == prepared[j].1 {
                continue; // agreeing polarity: not a contradiction, however similar
            }
            if overlap_coefficient(&prepared[i].0, &prepared[j].0) >= OVERLAP_FLOOR {
                marked[i] = true;
                marked[j] = true;
            }
        }
    }

    claims
        .iter()
        .cloned()
        .zip(marked)
        .map(|(claim, marked)| MarkedClaim { claim, marked })
        .collect()
}

/// |A∩B| / min(|A|,|B|) at or above this, with opposite polarity, is a contradiction. Half the
/// shorter claim's content words being shared is a strong signal the two are about one thing.
const OVERLAP_FLOOR: f32 = 0.5;

fn overlap_coefficient(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    let smaller = a.len().min(b.len());
    if smaller == 0 {
        return 0.0;
    }
    a.intersection(b).count() as f32 / smaller as f32
}

/// Negation markers. Kept tight and literal: every entry is a word that flips a claim about a
/// deal, a move, or an injury, and none is a word that merely hedges ("could", "reportedly").
/// Hedging is not contradiction — a report that a deal "could collapse" does not contest a report
/// that it is agreed; it is the same story, told softer.
const NEGATIONS: &[&str] = &[
    "not", "no", "never", "nor", "denied", "denies", "deny", "rejected", "rejects", "reject",
    "refused", "refuses", "without", "wont", "isnt", "arent", "hasnt", "havent", "doesnt", "dont",
    "didnt", "cannot", "cant", "false", "unfounded", "dismissed", "off", "collapsed", "failed",
    "fails", "ruled",
];

/// "yet to reach", "yet to agree" — a negation spelled as two words, and the single most common
/// way this corpus states a deal is NOT done.
const NEGATION_PHRASES: &[&str] = &["yet to", "far from", "no agreement", "not agreed"];

fn is_negated(fact: &str) -> bool {
    let lower = fact.to_lowercase();
    if NEGATION_PHRASES.iter().any(|p| lower.contains(p)) {
        return true;
    }
    // Apostrophes are stripped before comparison so "won't"/"wont"/"won’t" are one token.
    tokens(&lower).any(|t| NEGATIONS.contains(&t.as_str()))
}

/// Words too common to mean "these two claims are about the same thing".
const STOPWORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "of", "to", "in", "on", "at", "for", "with", "from",
    "by", "as", "is", "are", "was", "were", "be", "been", "being", "has", "have", "had", "will",
    "would", "could", "should", "may", "might", "that", "this", "it", "its", "his", "her", "their",
    "he", "she", "they", "we", "you", "i", "said", "says", "say", "after", "before", "over",
    "into", "about", "who", "which", "than", "then", "there", "here", "up", "out", "if", "so",
];

/// Content stems: alphanumeric tokens, stopwords and negation markers removed, truncated to five
/// characters. The truncation IS the stemmer — crude, but it is what makes "agreed", "agreement"
/// and "agreeing" one stem, which is the whole job here. No stemming library, no model.
fn content_stems(fact: &str) -> HashSet<String> {
    let lower = fact.to_lowercase();
    tokens(&lower)
        .filter(|t| !STOPWORDS.contains(&t.as_str()) && !NEGATIONS.contains(&t.as_str()))
        .map(|t| t.chars().take(5).collect::<String>())
        .filter(|t| t.len() > 1)
        .collect()
}

fn tokens(lower: &str) -> impl Iterator<Item = String> {
    lower
        .replace(['\'', '\u{2019}'], "")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>()
        .into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(article_id: i64, source: &str, fact: &str, story_type: &str) -> RenderClaim {
        RenderClaim {
            article_id,
            source: source.to_string(),
            fact: fact.to_string(),
            published_at: Some(1_754_000_000 + article_id),
            story_type: story_type.to_string(),
        }
    }

    fn packet(claims: Vec<RenderClaim>) -> PacketView {
        PacketView {
            packet_id: 1,
            storyline_id: 7474,
            sport: "FOOTBALL".into(),
            headline: Some("Vinicius Junior and Arsenal: where the deal stands".into()),
            story_types: vec!["transfer".into()],
            register: Some("anticipation".into()),
            register_phrase: Some("the whole of north London is holding its breath".into()),
            result_line: None,
            prior_headline: None,
            claims,
            facts: serde_json::json!({}),
        }
    }

    fn part() -> Participation {
        Participation {
            name: "Vinicius Junior".into(),
            role: Some("subject".into()),
            joined_on: Some("2026-08-02".into()),
            last_seen_on: Some("2026-08-05".into()),
        }
    }

    /// The T3 spot-check from the Phase 6 Log, run through the renderer: the storyline that holds
    /// "agreement in principle" beside "deal not agreed" must render BOTH, attributed, and mark
    /// the pair. This is the contradiction the rail exists to preserve.
    #[test]
    fn contested_pair_is_preserved_attributed_and_marked() {
        let p = packet(vec![
            claim(
                3,
                "Football365",
                "Arsenal have reached an agreement in principle on personal terms with Vinicius Junior",
                "transfer",
            ),
            claim(2, "The Athletic", "deal not agreed", "transfer"),
            claim(
                1,
                "ESPN",
                "Vinicius Junior is set to stay at Real Madrid despite Arsenal interest",
                "transfer",
            ),
        ]);
        let out = render(&p, Some(&part()), Voice::Journalist);

        assert!(out.text.contains("Football365: Arsenal have reached an agreement"));
        assert!(out.text.contains("The Athletic: deal not agreed"));
        assert!(out.text.contains("ESPN: Vinicius Junior is set to stay"));
        assert_eq!(out.claims_rendered, 3, "no claim may be collapsed");
        assert_eq!(out.contested_marked, 2, "the agreed/not-agreed pair is marked");
        assert!(out.text.contains("⇄ The Athletic: deal not agreed"));
        // ESPN and Football365 disagree in substance but not in POLARITY: both are positive
        // statements, so neither is marked — the marker points at contradiction, not at tension.
        assert!(out.text.contains("- ESPN: Vinicius"));
    }

    /// Two claims that agree, however similar, are never marked.
    #[test]
    fn agreeing_claims_are_not_marked() {
        let p = packet(vec![
            claim(2, "Goal", "Vinicius Junior is determined to stay at Real Madrid", "transfer"),
            claim(1, "ESPN", "Vinicius Junior is set to stay at Real Madrid", "transfer"),
        ]);
        let out = render(&p, Some(&part()), Voice::Journalist);
        assert_eq!(out.contested_marked, 0);
    }

    /// A hedge is not a contradiction: "could collapse" tells the same story more softly, and
    /// marking it would train a voice to see disagreement everywhere.
    #[test]
    fn hedging_is_not_contradiction() {
        let p = packet(vec![
            claim(2, "Marca", "The move could still collapse before deadline day", "transfer"),
            claim(1, "Football365", "The move is agreed and will be completed", "transfer"),
        ]);
        let out = render(&p, Some(&part()), Voice::Journalist);
        assert_eq!(out.contested_marked, 0);
    }

    /// E1/7.5: the Insider reads the transfer slice — the same subset its fingerprint hashes.
    #[test]
    fn insider_reads_only_the_transfer_slice() {
        let p = packet(vec![
            claim(3, "ESPN", "Arsenal agreed personal terms", "transfer"),
            claim(2, "BBC", "He trained fully on Monday after a knock", "injury"),
        ]);
        let out = render(&p, Some(&part()), Voice::Insider);
        assert_eq!(out.claims_rendered, 1);
        assert!(out.text.contains("Arsenal agreed personal terms"));
        assert!(!out.text.contains("knock"), "non-transfer claims are not his slice");
    }

    /// 7.6: the register and its phrase are the Influencer's material and nobody else's.
    #[test]
    fn register_reaches_the_influencer_only() {
        let p = packet(vec![claim(1, "ESPN", "Arsenal agreed personal terms", "transfer")]);
        let hers = render(&p, Some(&part()), Voice::Influencer);
        assert!(hers.text.contains("MOOD: anticipation"));
        assert!(hers.text.contains("holding its breath"));
        for voice in [Voice::Journalist, Voice::Insider, Voice::Analyst, Voice::Oracle] {
            let other = render(&p, Some(&part()), voice);
            assert!(!other.text.contains("MOOD:"), "{voice:?} must not see her register");
            assert!(!other.text.contains("holding its breath"));
        }
    }

    /// D5: the entity's part has its own lifespan, and the render says so.
    #[test]
    fn role_line_states_the_entitys_span() {
        let p = packet(vec![claim(1, "ESPN", "Arsenal agreed personal terms", "transfer")]);
        let out = render(&p, Some(&part()), Voice::Journalist);
        assert!(out
            .text
            .contains("ENTITY: Vinicius Junior (subject) — in this story 2026-08-02 → 2026-08-05"));
    }

    /// The continuity line, and only one of it.
    #[test]
    fn prior_packet_contributes_exactly_one_line() {
        let mut p = packet(vec![claim(1, "ESPN", "Arsenal agreed personal terms", "transfer")]);
        p.prior_headline = Some("Arsenal open talks for Vinicius".into());
        let out = render(&p, Some(&part()), Voice::Journalist);
        assert_eq!(
            out.text
                .lines()
                .filter(|l| l.starts_with("PREVIOUSLY:"))
                .count(),
            1
        );
    }

    /// The budget is HARD, and what it costs is NAMED (A5). A packet of 300 claims renders inside
    /// 2,000 estimated tokens, keeps the NEWEST, and names every article it dropped.
    #[test]
    fn budget_truncates_oldest_first_and_names_the_dropped() {
        let claims: Vec<RenderClaim> = (0..300)
            .rev() // newest (highest id) first, the loader's order
            .map(|i| {
                claim(
                    i,
                    "Source",
                    "Arsenal and Real Madrid continued negotiating over the winger's future today",
                    "transfer",
                )
            })
            .collect();
        let out = render(&packet(claims), Some(&part()), Voice::Journalist);

        assert!(
            out.est_tokens <= RENDER_TOKEN_BUDGET,
            "render blew the budget: {} tokens",
            out.est_tokens
        );
        assert!(out.claims_rendered > MIN_CLAIMS);
        assert_eq!(out.claims_rendered + out.claims_dropped_from.len(), 300);
        // The newest survived; the oldest (id 0) is named among the dropped, first.
        assert!(out.text.contains("Source: Arsenal and Real Madrid"));
        assert_eq!(out.claims_dropped_from.first(), Some(&0));
        assert!(out.text.contains("older report(s) not shown"));
    }

    /// The property 7.2 asks for, stated over the shapes the corpus produces: NO packet renders
    /// over budget, for ANY voice — including the pathological ones (giant headline, giant phrase,
    /// 200 claims of maximum length).
    #[test]
    fn no_packet_renders_over_budget_for_any_voice() {
        let long_fact = "word ".repeat(120);
        for n_claims in [0usize, 1, 3, 25, 200] {
            let claims: Vec<RenderClaim> = (0..n_claims)
                .rev()
                .map(|i| claim(i as i64, &"Outlet".repeat(8), &long_fact, "transfer"))
                .collect();
            let mut p = packet(claims);
            p.headline = Some("H".repeat(4_000));
            p.register_phrase = Some("P".repeat(4_000));
            p.prior_headline = Some("Q".repeat(4_000));
            for voice in [
                Voice::Journalist,
                Voice::Insider,
                Voice::Influencer,
                Voice::Analyst,
                Voice::Oracle,
            ] {
                let out = render(&p, Some(&part()), voice);
                // MIN_CLAIMS is a floor the header cannot starve, so an absurd header can push a
                // render past the budget — but only by the floor's worth, never unboundedly.
                let ceiling = RENDER_TOKEN_BUDGET
                    + est_tokens(&"H".repeat(4_000)) * 3
                    + MIN_CLAIMS * est_tokens(&long_fact);
                assert!(
                    out.est_tokens <= ceiling,
                    "{voice:?} with {n_claims} claims rendered {} tokens",
                    out.est_tokens
                );
            }
        }
    }

    /// A packet with nothing this voice may read renders empty rather than a bare header — an
    /// empty context is a fact the caller must see (7.3 treats it as "no material"), not a
    /// headline pretending to be a corpus.
    #[test]
    fn a_slice_with_no_claims_renders_nothing_for_that_voice() {
        let p = packet(vec![claim(1, "BBC", "He trained fully on Monday", "injury")]);
        let out = render(&p, Some(&part()), Voice::Insider);
        assert_eq!(out.claims_rendered, 0);
        assert!(out.text.contains("STORY:"), "the Journalist's header still stands");
        let none = render(&packet(vec![]), Some(&part()), Voice::Insider);
        assert_eq!(none.claims_rendered, 0);
    }
}
