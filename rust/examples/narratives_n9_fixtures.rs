//! narratives_n9_fixtures — regenerate the hand-authored n9 (Journalist) eval fixtures.
//!
//! Cognition refactor Phase 3 M4. The n9 contract makes narratives the primary junction: it voices
//! the relational memory card's episode state — NEW vs CONTINUING, and heating / cooling / steady
//! (heat is VOICED from the measurement card, never authored here; plan decisions 7 + 8). The live
//! `NarrativeTask::build_prompt` pins the memory-FREE prompt shape (`memory: None`), so a captured
//! fixture could never exercise that voicing. These fixtures are therefore HAND-AUTHORED: each freezes
//! a faithful `build_narratives_prompt` output WITH a real `narrative_context_for_entity` memory card
//! baked in, so `eval --task narratives --fixtures` runs the exact memory-informed prompt through the
//! model and scores the voicing.
//!
//! Emitting them through `build_narratives_prompt` + `NARRATIVES_SYSTEM_PROMPT` (rather than hand-typing
//! the JSON) guarantees the frozen `system` is byte-exact and every `user_prompt` matches the live
//! render — a prompt bump means "re-run this example", not "hand-patch the JSON files".
//!
//! VOICE IS A DRAFT: the `body_includes_any` / `body_excludes` voicing checks are voice-tuning TARGETS
//! (a red on a target is the documented honesty gap, not a harness failure — `bin/eval`). The grounding
//! floor (count discipline, every storyline cited, no invented refs) holds regardless of voice and is
//! the real n8→n9 regression net. Re-annotate the voicing axes when the voice-tuning session lands.
//!
//!   cargo run --example narratives_n9_fixtures

use std::path::Path;

use scoracle_cognition::corpus::HeatItem;
use scoracle_cognition::eval_tasks::{Expect, Fixture};
use scoracle_cognition::narratives::{
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

fn player(name: &str, id: i32) -> NarrativesReq {
    NarrativesReq {
        entity_type: "player".to_string(),
        entity_id: id,
        entity_name: name.to_string(),
        sport: "SOCCER".to_string(),
        trigger_type: "periodic".to_string(),
    }
}

/// Build one hand-authored fixture: render the exact live prompt (with memory), pin temp 0 for
/// reproducibility, and attach the property rubric.
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
        user_prompt: build_narratives_prompt(req, corpus, heat, Some(memory)),
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
                    out.push(("note".to_string(), serde_json::Value::String(note.to_string())));
                }
                out
            })
            .collect();
        v = serde_json::Value::Object(reordered);
    }
    let path = dir.join(format!("{}.json", fx.name));
    std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&v)?))?;
    println!("wrote {} ({} chars prompt)", path.display(), fx.user_prompt.len());
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/narratives");
    std::fs::create_dir_all(&dir)?;

    // ── Fixture 1 — new-vs-ongoing: the CONTINUING side ────────────────────────────────────────────
    // A live, multi-month saga the memory card already tracks (Current story) + a fresh corpus that
    // advances the SAME story. The Journalist should voice it as ongoing/continuing, not brand-new.
    let f1 = fixture(
        "ongoing-saga-continuation",
        &player("Diego Ferreira", 8801),
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
    write_fixture(&dir, &f1, "n9 new-vs-ongoing (CONTINUING): a tracked multi-month saga (Current story on the memory card) with a corpus that advances it. FLOOR: grouping+grounding (count, all cite, in-range). TARGET (voice-tuning): body voices it as continuing/ongoing, never brand-new. article_buckets: all four are transfer-related.")?;

    // ── Fixture 2 — heat monotonicity: the COOLING / fizzled side ──────────────────────────────────
    // The memory card holds a FIZZLED prior story (high past peak, dead), and the fresh corpus is thin
    // and de-escalating (interest played down). Heat should be voiced DOWN — cooled/unlikely — never up.
    // No transfer heat (the section is omitted), matching a dead rumor.
    let f2 = fixture(
        "fizzled-prior-now-cooling",
        &player("Kai Sorensen", 8802),
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
    write_fixture(&dir, &f2, "n9 heat monotonicity (DOWN): a fizzled prior story on the memory card + a thin, de-escalating corpus. FLOOR: restraint (don't over-split a dying story) + grounding. TARGET (voice-tuning): heat voiced down (cooled/unlikely/settled), never up. article_buckets: both transfer-related.")?;

    // ── Fixture 3 — heat monotonicity: the HEATING side ────────────────────────────────────────────
    // A modest prior peak (45/100) on the memory card, then a fresh MULTI-SOURCE surge that escalates
    // across the last day. Heat should be voiced UP — accelerating/gathering pace — above the prior peak.
    let f3 = fixture(
        "heating-multi-source-surge",
        &player("Mateus Andrade", 8803),
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
    write_fixture(&dir, &f3, "n9 heat monotonicity (UP): a modest prior peak (45/100) on the memory card + a fresh multi-source surge escalating in a day. FLOOR: grouping+grounding. TARGET (voice-tuning): heat voiced up (accelerating/gathering pace), never cooling. article_buckets: all five transfer-related.")?;

    println!("\nrun: cargo run --bin eval -- --task narratives --fixtures   (needs Ollama)");
    Ok(())
}
