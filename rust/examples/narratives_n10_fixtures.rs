//! narratives_n10_fixtures — regenerate the hand-authored Journalist eval fixtures.
//!
//! n10 is the Characters Phase B voice pass: the system prompt now speaks The Journalist's
//! telling (persona-first, from wiki/Characters.md's craft appendix); the n9 CONTRACT —
//! primary junction, article_buckets, voiced episode heat + new/ongoing — is unchanged.
//!
//! Two fixture families, all frozen through the LIVE prompt constants so a prompt bump means
//! "re-run this example", never "hand-patch the JSON files":
//!
//! - VOICING fixtures (memory-informed): the live `NarrativeTask::build_prompt` pins the
//!   memory-FREE prompt shape (`memory: None`), so a captured fixture could never exercise the
//!   memory voicing. These are HAND-AUTHORED: each freezes a faithful `build_narratives_prompt`
//!   output WITH a real `narrative_context_for_entity` memory card baked in, so
//!   `eval --task narratives --fixtures` runs the exact memory-informed prompt through the
//!   model and scores the voicing (NEW vs CONTINUING; heating / cooling / steady — heat is
//!   VOICED from the measurement card, never authored here).
//! - FLOOR fixtures (corpus-only): the grounding + restraint regression net (count discipline,
//!   every storyline cited, no invented refs, off-entity/hype refusal, no fabricated moves) —
//!   ported from the n5-era hand-authored set and re-frozen at the current version.
//!
//! With the voice pass landed, the `body_includes_any`/`body_excludes` voicing checks are no
//! longer drafts-with-targets: they assert The Journalist's native trajectory vocabulary. A red
//! is a voice regression (or a documented model gap called out in the fixture's note).
//!
//!   cargo run --example narratives_n10_fixtures

use std::path::Path;

use scoracle_cognition::corpus::HeatItem;
use scoracle_cognition::eval_tasks::{Expect, Fixture};
use scoracle_cognition::junctions::journalist::{
    build_narratives_prompt, CorpusItem, NarrativesReq, NARRATIVES_PROMPT_VERSION,
    NARRATIVES_SYSTEM_PROMPT,
};

/// One corpus article. The prompt render only reads source/title/description, so ids/urls/epochs are
/// inert here — set to placeholders that never reach the frozen prompt.
fn ci(id: i64, source: &str, title: &str, description: &str) -> CorpusItem {
    CorpusItem {
        id,
        title: title.to_string(),
        description: description.to_string(),
        source: source.to_string(),
        url: String::new(),
        published_at_epoch: None,
        fetched_at_epoch: None,
        full_text: None,
        // Fixtures pin the reader-free corpus shape (no article_read evidence card), the same
        // rule as the memory-free/score-context-free prompt pins above.
        article_read_blurb: None,
        article_read_status: None,
        article_read_content_hash: None,
        article_read_updated_epoch: None,
    }
}

fn heat(
    counterparty: &str,
    heat: i32,
    direction: &str,
    stage: &str,
    conf: f64,
    summary: &str,
) -> HeatItem {
    HeatItem {
        counterparty: counterparty.to_string(),
        heat,
        stage: stage.to_string(),
        direction: direction.to_string(),
        summary: summary.to_string(),
        confidence: Some(conf),
    }
}

fn player(name: &str, id: i32, sport: &str) -> NarrativesReq {
    NarrativesReq {
        entity_type: "player".to_string(),
        entity_id: id,
        entity_name: name.to_string(),
        sport: sport.to_string(),
        trigger_type: "periodic".to_string(),
    }
}

/// Build one hand-authored fixture: render the exact live prompt (with memory when the scenario
/// carries one), pin temp 0 for reproducibility, and attach the property rubric. An empty
/// `memory` renders no memory section (the builder filters blank cards), so floor fixtures pass "".
fn fixture(
    name: &str,
    req: &NarrativesReq,
    corpus: &[CorpusItem],
    heat: &[HeatItem],
    memory: &str,
    expect: Expect,
) -> Fixture {
    Fixture {
        name: name.to_string(),
        task: "narratives".to_string(),
        prompt_version: NARRATIVES_PROMPT_VERSION.to_string(),
        system: NARRATIVES_SYSTEM_PROMPT.to_string(),
        // Fixtures pin the score-context-free prompt shape (the n12 SIGNALS/prior-reads block
        // is live-path enrichment, same rule as the eval task).
        user_prompt: build_narratives_prompt(req, corpus, heat, Some(memory), None),
        temperature: 0.0,
        expect,
    }
}

/// Serialize a fixture and inject a documentary `"note"` (an unknown key the loader ignores, exactly
/// like the existing hand-authored fixtures carry), so the on-disk file self-describes its intent.
fn write_fixture(dir: &Path, fx: &Fixture, note: &str) -> anyhow::Result<()> {
    let mut v = serde_json::to_value(fx)?;
    if let Some(obj) = v.as_object_mut() {
        // Place the note right after `prompt_version` for readability (serde_json preserves order).
        let reordered: serde_json::Map<String, serde_json::Value> = obj
            .iter()
            .flat_map(|(k, val)| {
                let mut out = vec![(k.clone(), val.clone())];
                if k == "prompt_version" {
                    out.push((
                        "note".to_string(),
                        serde_json::Value::String(note.to_string()),
                    ));
                }
                out
            })
            .collect();
        v = serde_json::Value::Object(reordered);
    }
    let path = dir.join(format!("{}.json", fx.name));
    std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&v)?))?;
    println!(
        "wrote {} ({} chars prompt)",
        path.display(),
        fx.user_prompt.len()
    );
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/narratives");
    std::fs::create_dir_all(&dir)?;

    // ═══ Voicing fixtures — memory-informed arc + trajectory ═══════════════════════════════════════

    // ── new-vs-ongoing: the CONTINUING side ────────────────────────────────────────────────────────
    // A live, multi-month saga the memory card already tracks (Current story) + a fresh corpus that
    // advances the SAME story. The Journalist should voice it as ongoing/continuing, not brand-new.
    let f1 = fixture(
        "ongoing-saga-continuation",
        &player("Diego Ferreira", 8801, "SOCCER"),
        &[
            ci(1, "The Athletic", "Juventus return with improved offer for Ferreira", "Juventus have gone back to Porto with a raised bid for the midfielder as talks that began in the spring continue into a third month."),
            ci(2, "Sky Sport Italia", "Ferreira-Juventus talks progress on personal terms", "The player's camp and Juventus have moved closer on wages, though the clubs remain apart on the fee."),
            ci(3, "Record", "Porto hold firm on Ferreira valuation", "Porto have again rejected Juventus's latest proposal, insisting on their full asking price for the 24-year-old."),
            ci(4, "O Jogo", "Ferreira starts as Porto win again", "Ferreira played 90 minutes in Porto's league win, his form undimmed by the transfer noise around him."),
        ],
        &[heat("Juventus", 68, "outgoing", "advanced_talks", 0.6, "Juventus are in advanced talks with Porto to sign Ferreira, though a fee gap remains.")],
        "Current story: Juventus — tracked since May 12, peak coverage 68/100, computed likelihood 61/100.\nOur prior read: our transfer lens staged Juventus as advanced_talks on Jul 18 (confidence 0.6).",
        Expect {
            narratives_min: Some(1),
            narratives_max: Some(6),
            all_cite_articles: Some(true),
            max_article_num: Some(4),
            title_excludes: Some(vec!["Transfer news".into()]),
            body_includes_any: Some(vec![
                "continu".into(), "ongoing".into(), "still".into(), "remain".into(),
                "months".into(), "third month".into(), "long-running".into(), "again".into(),
                "return".into(), "since".into(),
            ]),
            body_excludes: Some(vec!["out of nowhere".into(), "first reported".into(), "first reports".into()]),
            ..Default::default()
        },
    );
    write_fixture(&dir, &f1, "n10 new-vs-ongoing (CONTINUING): a tracked multi-month saga (Current story on the memory card) with a corpus that advances it. FLOOR: grouping+grounding (count, all cite, in-range). VOICE: The Journalist places the story in its arc — continuing/ongoing, never brand-new. article_buckets: all four are transfer-related.")?;

    // ── heat monotonicity: the COOLING / fizzled side ──────────────────────────────────────────────
    // The memory card holds a FIZZLED prior story (high past peak, dead), and the fresh corpus is thin
    // and de-escalating (interest played down). Heat should be voiced DOWN — cooled/unlikely — never up.
    // No transfer heat (the section is omitted), matching a dead rumor.
    let f2 = fixture(
        "fizzled-prior-now-cooling",
        &player("Kai Sorensen", 8802, "SOCCER"),
        &[
            ci(1, "Goal", "Chelsea move for Sorensen unlikely to reignite", "Sources say Chelsea have not revisited their winter interest in Sorensen and are focused elsewhere this window."),
            ci(2, "Ekstra Bladet", "Sorensen happy to stay, says agent", "The forward's representative played down transfer talk, saying his client is settled and not pushing to leave."),
        ],
        &[],
        "Prior story: Chelsea — fizzled (Mar 2026, peak coverage 84/100).",
        Expect {
            narratives_min: Some(1),
            narratives_max: Some(2),
            all_cite_articles: Some(true),
            max_article_num: Some(2),
            title_excludes: Some(vec!["Transfer news".into()]),
            body_includes_any: Some(vec![
                "cool".into(), "fad".into(), "fizzl".into(), "quiet".into(), "unlikely".into(),
                "played down".into(), "settled".into(), "not revisit".into(), "moved on".into(),
                "stall".into(), "no longer".into(),
            ]),
            body_excludes: Some(vec![
                "advanced talks".into(), "gathering pace".into(), "here we go".into(),
                "heating up".into(), "close to a deal".into(),
            ]),
            ..Default::default()
        },
    );
    write_fixture(&dir, &f2, "n10 heat monotonicity (DOWN): a fizzled prior story on the memory card + a thin, de-escalating corpus. FLOOR: restraint (don't over-split a dying story) + grounding. VOICE: heat voiced down (cooled/unlikely/settled), never up. article_buckets: both transfer-related.")?;

    // ── heat monotonicity: the HEATING side ────────────────────────────────────────────────────────
    // A modest prior peak (45/100) on the memory card, then a fresh MULTI-SOURCE surge that escalates
    // across the last day. Heat should be voiced UP — accelerating/gathering pace — above the prior peak.
    let f3 = fixture(
        "heating-multi-source-surge",
        &player("Mateus Andrade", 8803, "SOCCER"),
        &[
            ci(1, "Fabrizio Romano", "Bayern accelerate for Andrade", "Bayern have opened formal talks with Flamengo for Andrade in the last 24 hours after weeks of quiet groundwork."),
            ci(2, "Kicker", "Bayern make Andrade a priority signing", "Bayern's board have greenlit a move and see the winger as their top summer target."),
            ci(3, "ESPN", "Flamengo brace for Andrade bids", "Multiple European clubs have contacted Flamengo, with Bayern now leading the race."),
            ci(4, "Bild", "Andrade to Bayern: personal terms close", "The player is said to favour the move and terms are near agreement, with the clubs still negotiating the fee."),
            ci(5, "Globo", "Andrade transfer gathers pace", "What was a slow-burning link has escalated sharply this week amid concrete Bayern interest."),
        ],
        &[heat("Bayern Munich", 82, "outgoing", "advanced_talks", 0.7, "Bayern have opened formal talks with Flamengo for Andrade and personal terms are close.")],
        "Current story: Bayern Munich — tracked since Jun 02, peak coverage 45/100, computed likelihood 42/100.",
        Expect {
            narratives_min: Some(1),
            narratives_max: Some(6),
            all_cite_articles: Some(true),
            max_article_num: Some(5),
            title_excludes: Some(vec!["Transfer news".into()]),
            body_includes_any: Some(vec![
                "accelerat".into(), "gathering pace".into(), "gather".into(), "escalat".into(),
                "picking up".into(), "picked up".into(), "momentum".into(), "intensif".into(),
                "surg".into(), "heating".into(), "leading the race".into(), "priority".into(),
                "close".into(),
            ]),
            body_excludes: Some(vec![
                "cooling".into(), "fizzled".into(), "gone quiet".into(), "faded".into(),
                "unlikely".into(), "stalled".into(),
            ]),
            ..Default::default()
        },
    );
    write_fixture(&dir, &f3, "n10 heat monotonicity (UP): a modest prior peak (45/100) on the memory card + a fresh multi-source surge escalating in a day. FLOOR: grouping+grounding. VOICE: heat voiced up (accelerating/gathering pace), never cooling. article_buckets: all five transfer-related.")?;

    // ═══ Floor fixtures — grounding + restraint (ported from the n5-era hand-authored set) ═════════

    // ── clean two-storyline grounding ──────────────────────────────────────────────────────────────
    let f4 = fixture(
        "clean-two-storyline-grounding",
        &player("Marcus Vale", 8804, "NBA"),
        &[
            ci(1, "The Athletic", "Marcus Vale, Kings agree to four-year extension", "Vale and the Kings finalized a four-year, $180M extension, sources say, keeping the wing in Sacramento through 2030."),
            ci(2, "ESPN", "Kings lock up Marcus Vale on new deal", "Sacramento reached agreement with Vale on a long-term extension Thursday, ending months of negotiations."),
            ci(3, "Sacramento Bee", "Vale returns from ankle sprain, scores 27", "Vale played his first game back from a two-week ankle injury, leading the Kings with 27 points in a win."),
        ],
        &[],
        "",
        Expect {
            narratives_min: Some(1),
            narratives_max: Some(6),
            all_cite_articles: Some(true),
            max_article_num: Some(3),
            title_includes: Some(vec!["Vale".into()]),
            title_excludes: Some(vec!["Transfer news".into()]),
            ..Default::default()
        },
    );
    write_fixture(&dir, &f4, "two distinct clear storylines (extension x2 sources, injury return) over a 3-article corpus. ALL FLOOR: grouping+grounding should just work — each storyline cites its real articles, titles name the player, count stays sane, no invented refs. If any floor reds, grounding is broken.")?;

    // ── don't over-split one story ─────────────────────────────────────────────────────────────────
    let f5 = fixture(
        "dont-over-split-one-story",
        &player("Leo Marsh", 8805, "NBA"),
        &[
            ci(1, "ESPN", "Leo Marsh to miss rest of season with torn ACL", "Marsh suffered a torn ACL in Tuesday's game and will miss the remainder of the season, the team announced."),
            ci(2, "The Athletic", "Marsh's ACL tear ends his season", "Sources confirm Leo Marsh tore his ACL and is out for the year."),
            ci(3, "Sactown Sports", "Marsh out for season after knee injury", "The team confirmed Marsh's season-ending knee injury Wednesday."),
        ],
        &[],
        "",
        Expect {
            narratives_min: Some(1),
            narratives_max: Some(1),
            all_cite_articles: Some(true),
            max_article_num: Some(3),
            title_includes: Some(vec!["Marsh".into()]),
            title_excludes: Some(vec!["Transfer news".into()]),
            ..Default::default()
        },
    );
    write_fixture(&dir, &f5, "three outlets report the SAME single event (Marsh's season-ending ACL). System prompt: one story is one storyline. TARGET: narratives_max 1 (one event = one storyline). FLOOR: title_includes Marsh + grounded refs + not-generic. Red on narratives_max = the model over-split one story into duplicate storylines (a grouping failure).")?;

    // ── draft-around, not a move ───────────────────────────────────────────────────────────────────
    let f6 = fixture(
        "draft-around-not-a-move",
        &player("Darius Kane", 8806, "NBA"),
        &[
            ci(1, "Bleacher Report", "Heat reportedly eyeing a wing to pair with Darius Kane", "Miami's front office wants shooting alongside Kane and views the draft as the path, per league sources."),
            ci(2, "The Ringer", "Mock draft: Heat target sharpshooter to complement Kane", "The latest mock has Miami using its lottery pick on a guard who fits next to Kane's slashing game."),
            ci(3, "Miami Herald", "Kane's usage climbs as Heat build around him", "Kane is seeing career-high touches as the Heat orient their young core around his playmaking."),
        ],
        &[],
        "",
        Expect {
            narratives_max: Some(6),
            all_cite_articles: Some(true),
            max_article_num: Some(3),
            title_excludes: Some(vec!["Transfer news".into(), "traded".into(), "trade".into()]),
            body_excludes: Some(vec!["moving to".into(), "traded to".into(), "joins the".into()]),
            ..Default::default()
        },
    );
    write_fixture(&dir, &f6, "all 3 articles are the HEAT building around Kane (draft/pairing) — NONE say Kane is moving. FLOOR: all_cite_articles + max_article_num + not-generic title. TARGET: body/title excludes any fabricated movement ('moving to'/'traded'/'joins') — the system prompt's hardest rule. Red here = the model invented an entity-move storyline the corpus never supports (a grounding false positive).")?;

    // ── off-entity + hype contamination ────────────────────────────────────────────────────────────
    let f7 = fixture(
        "off-entity-and-hype-contamination",
        &player("Andre Foss", 8807, "NBA"),
        &[
            ci(1, "The Athletic", "Andre Foss traded to the Nuggets in three-team deal", "Foss is headed to Denver as part of a three-team trade finalized Wednesday, sources confirm."),
            ci(2, "HoopsHype", "Andre Foss could be a sleeper MVP candidate, one exec says", "Vague praise with no specifics about Foss's actual season."),
            ci(3, "ESPN", "Rival guard Marcus Vale signs extension with Kings", "Vale agreed to a four-year deal to stay in Sacramento, unrelated to Denver's moves."),
        ],
        &[],
        "",
        Expect {
            narratives_min: Some(1),
            narratives_max: Some(2),
            max_article_num: Some(3),
            title_includes: Some(vec!["Foss".into()]),
            title_excludes: Some(vec!["Vale".into(), "Transfer news".into()]),
            body_excludes: Some(vec!["Vale".into()]),
            ..Default::default()
        },
    );
    write_fixture(&dir, &f7, "corpus mixes a REAL grounded Foss trade (art 1), vague hype (art 2), and an OFF-ENTITY article about Marcus Vale (art 3). n5-era PROBE (mistral:7b, temp 0): PRECISION HELD (no Vale leak, hype refused) but RECALL FAILED — zero storylines, dropping the real trade too. FLOOR (green): title/body exclude 'Vale', max_article_num, narratives_max 2. TARGET: narratives_min 1 + title_includes 'Foss' — the model SHOULD surface the grounded trade under contamination. Re-probe under each voice pass.")?;

    // ── vague-hype restraint ───────────────────────────────────────────────────────────────────────
    let f8 = fixture(
        "vague-hype-restraint",
        &player("Tobias Renn", 8808, "NBA"),
        &[
            ci(1, "FadeawayWorld", "Tobias Renn is ready to take the leap, sources say", "Insiders believe big things are coming for Renn this season, though specifics remain unclear."),
            ci(2, "Hoops Rumors", "Could this be Tobias Renn's breakout year?", "Speculation is building around Renn, with one source calling him a name to watch this winter."),
        ],
        &[],
        "",
        Expect {
            narratives_max: Some(1),
            max_article_num: Some(2),
            title_excludes: Some(vec!["Transfer news".into()]),
            ..Default::default()
        },
    );
    write_fixture(&dir, &f8, "two pure-hype articles that never name who/what/where. System prompt: pass over vague hype + a quiet cycle is an honest answer. TARGET: narratives_max 1 (0 is ideal). FLOOR: not-generic title + no invented refs (vacuously green if it correctly returns nothing). Deliberately NO all_cite_articles/title_includes here — those would falsely red the correct empty answer.")?;

    println!("\nrun: cargo run --bin eval -- --task narratives --fixtures   (needs Ollama)");
    Ok(())
}
