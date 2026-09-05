//! vibe_v13_fixtures — regenerate the hand-authored Influencer eval fixtures.
//!
//! v13 is the Characters Phase B voice pass: the system prompt now speaks The Influencer's
//! telling (persona-first, from wiki/Characters.md's craft appendix — vivid, present tense,
//! engagement-first, farmed sincerely; the harmony-not-disdain tone test applies hardest
//! here). The v12 SCORE contract — bands, impact weighting, heat-as-energy, stay-near-50,
//! previous-read prior — is unchanged; VIBE upgrades to card-quality prose and the reply
//! gains the HOOK line (the Influencer's card title, `vibe_scores.hook`, mig 180).
//!
//! The live vibe build reads narratives + transfer heat from the DB and rides two prompt-only
//! enrichments (the previous-read prior and the relational memory card); a frozen fixture
//! cannot, so these are HAND-AUTHORED: each freezes a faithful `build_sentiment_prompt`
//! render — two of them WITH a prior/memory card baked in — so `eval --task vibe --fixtures`
//! runs the exact enriched prompt through the model and scores it.
//!
//! Emitting them through `build_sentiment_prompt` + `VIBE_SYSTEM_PROMPT` (not hand-typed
//! JSON) guarantees the frozen `system`/`user_prompt` are byte-exact — a prompt bump means
//! "re-run this example", not "hand-patch the JSON".
//!
//! FLOOR vs TARGET (the narratives/transfers discipline): the score bands are the regression
//! floor — direction-correct sentiment holds regardless of voice. `hook_nonempty` asserts the
//! v13 card contract materializes; the continuity/memory fixtures assert the prior is a real
//! anchor (deliberate movement, no whiplash) and that a warm memory never props up a score the
//! cold current coverage does not support (memory is continuity, never evidence).
//!
//!   cargo run --example vibe_v13_fixtures
//!   cargo run --bin eval -- --task vibe --fixtures   (needs Ollama)

use std::path::Path;

use scoracle_cognition::corpus::HeatItem;
use scoracle_cognition::eval_tasks::{Expect, Fixture};
use scoracle_cognition::junctions::influencer::{
    build_sentiment_prompt, Narrative, PrevVibe, VIBE_PROMPT_VERSION, VIBE_SYSTEM_PROMPT,
};

/// One storyline as the vibe loader would surface it. `topic_heat`/`source_count`/
/// `source_age_days` are prompt tags only (never hashed), so the fixtures pin realistic values.
fn nar(
    title: &str,
    body: &str,
    impact: i32,
    trajectory: &str,
    topic_heat: i32,
    source_count: i32,
    source_age_days: Option<i32>,
) -> Narrative {
    Narrative {
        title: title.to_string(),
        body: body.to_string(),
        impact,
        trajectory: trajectory.to_string(),
        topic_heat,
        source_count,
        source_age_days,
    }
}

fn heat(counterparty: &str, heat: i32, direction: &str, stage: &str, summary: &str) -> HeatItem {
    HeatItem {
        counterparty: counterparty.to_string(),
        heat,
        stage: stage.to_string(),
        direction: direction.to_string(),
        summary: summary.to_string(),
        confidence: Some(0.7),
    }
}

/// Build one hand-authored fixture: render the exact live prompt (optionally with the
/// previous-read prior and the relational memory card), pin temp 0 for reproducibility, and
/// attach the property rubric.
#[allow(clippy::too_many_arguments)]
fn fixture(
    name: &str,
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    narratives: &[Narrative],
    heat_items: &[HeatItem],
    previous: Option<&PrevVibe>,
    memory: Option<&str>,
    expect: Expect,
) -> Fixture {
    Fixture {
        name: name.to_string(),
        task: "vibe".to_string(),
        prompt_version: VIBE_PROMPT_VERSION.to_string(),
        system: VIBE_SYSTEM_PROMPT.to_string(),
        user_prompt: build_sentiment_prompt(
            entity_type,
            entity_name,
            sport,
            narratives,
            heat_items,
            // Fixtures pin the LEGACY prompt shape: no packet block (7.6).
            &[],
            previous,
            memory,
        ),
        temperature: 0.0,
        expect,
    }
}

/// Serialize a fixture and inject a documentary `"note"` (an unknown key the loader ignores,
/// like the existing hand-authored fixtures carry), so the on-disk file self-describes.
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

/// The D-T50 (v17) gate, shared by every vibe fixture: the hook caps (12 words, no colon
/// constructions, no question-mark bait), the plain-text guard, the prose length floors, and
/// the sentence ceiling. Adopted into the generator at v18 — until then this gate lived ONLY in
/// the on-disk JSON, and a regen would have silently dropped it (the momentum-generator lesson,
/// caught the same evening).
fn vibe_gate(prose_includes: &[&str], score_min: Option<i32>, score_max: Option<i32>) -> Expect {
    // (The hook contract and the `**` body ban left the per-fixture expects 08-19: they are
    // GLOBAL invariants now — `hook_contract`/`no_banned_phrases` checks in `VibeTask`, the
    // same rules `VibeParser` enforces in production via `guards`.)
    Expect {
        score_min,
        score_max,
        prose_includes: Some(prose_includes.iter().map(|s| s.to_string()).collect()),
        prose_min_words: Some(40),
        prose_max_words: Some(320),
        total_sentences_max: Some(10),
        ..Default::default()
    }
}

fn main() -> anyhow::Result<()> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/vibe");
    std::fs::create_dir_all(&dir)?;

    // 1. clearly-negative — the v10-era floor scenario re-frozen at v13: uniformly negative
    //    coverage must land low, and the card contract must still materialize.
    let fx = fixture(
        "clearly-negative",
        "player",
        "Marcus Vale",
        "NBA",
        &[
            nar(
                "Benched amid slump",
                "Coaches pulled him from the rotation after a string of poor outings; local radio openly questioning his future.",
                8,
                "cooling_off",
                5,
                3,
                Some(1),
            ),
            nar(
                "Trade-block chatter",
                "Front office reportedly gauging interest; fans frustrated.",
                6,
                "cooling_off",
                4,
                2,
                Some(2),
            ),
        ],
        &[],
        None,
        None,
        vibe_gate(&["Vale"], None, Some(40)),
    );
    write_fixture(
        &dir,
        &fx,
        "FLOOR: uniformly negative narratives score ≤40 (the v10-era band, re-frozen at v13) and the HOOK line materializes.",
    )?;

    // 2. clearly-positive — surging coverage plus live incoming interest (heat is energy).
    let fx = fixture(
        "clearly-positive",
        "player",
        "Dario Fenn",
        "FOOTBALL",
        &[
            nar(
                "Hat-trick hero seals statement win",
                "Three goals against the league leaders; the away end sang his name for twenty minutes and pundits are calling it the performance of the season.",
                9,
                "heating_up",
                8,
                5,
                Some(0),
            ),
            nar(
                "Golden Boot race tightens",
                "His scoring streak has him two behind the leader with momentum firmly on his side.",
                6,
                "heating_up",
                6,
                3,
                Some(1),
            ),
        ],
        &[heat(
            "Real Madrid",
            55,
            "outgoing",
            "concrete_interest",
            "Reported scouting presence at his last three matches.",
        )],
        None,
        None,
        vibe_gate(&["Fenn", "goals"], Some(60), None),
    );
    write_fixture(
        &dir,
        &fx,
        "FLOOR: surging coverage + live interest scores ≥60; the hook must ride the real emotion (the hat-trick, the race), not invent one.",
    )?;

    // 3. quiet-mixed — a flat cycle must stay near 50: a true quiet read beats a loud false
    //    one (the sincerity guard, harmony-not-disdain's sharpest edge).
    let fx = fixture(
        "quiet-mixed",
        "team",
        "Harbor City Sharks",
        "NFL",
        &[nar(
            "Routine win closes quiet week",
            "A workmanlike divisional win with no standout storylines; beat coverage focused on practice-squad moves.",
            3,
            "developing_story",
            2,
            1,
            Some(2),
        )],
        &[],
        None,
        None,
        vibe_gate(&["Sharks"], Some(35), Some(65)),
    );
    write_fixture(
        &dir,
        &fx,
        "FLOOR: a flat cycle stays in the 35-65 band — The Influencer does not manufacture drama when the room is quiet.",
    )?;

    // 4. continuity-deliberate-move — the previous-read prior is a real anchor: one cooling
    //    story moves the score down from 72 deliberately, not into freefall (no whiplash) and
    //    not pinned (no sticky euphoria).
    let prev = PrevVibe {
        sentiment: 72,
        vibe_prompt: "The building is buzzing after back-to-back statement wins — Kade Morrow has the crowd believing again.".to_string(),
    };
    let fx = fixture(
        "continuity-deliberate-move",
        "player",
        "Kade Morrow",
        "NBA",
        &[nar(
            "Road loss cools the streak",
            "A flat second half ended the five-game run; his 12 points on 4-of-15 shooting drew groans but no panic from the locker room.",
            5,
            "cooling_off",
            4,
            2,
            Some(0),
        )],
        &[],
        Some(&prev),
        None,
        vibe_gate(&["Morrow"], Some(40), Some(70)),
    );
    write_fixture(
        &dir,
        &fx,
        "CONTINUITY: prior 72 + one cooling story lands 40-70 — the prior is memory, so the read moves deliberately (no whiplash floor, no sticky euphoria).",
    )?;

    // 5. warm-memory-cold-coverage — mood has history: the warm remembered arc under cold
    //    current coverage is the story The Influencer reads, but memory is continuity, never
    //    evidence — it must not prop the score above what today's coverage supports.
    let memory = "Prior story: MVP-caliber spring surge — peaked Apr 2026 (coverage 88/100).\nOur prior read: euphoric through April; the fanbase built its summer expectations on that stretch.";
    let fx = fixture(
        "warm-memory-cold-coverage",
        "player",
        "Elias Trent",
        "NBA",
        &[nar(
            "Slump deepens as minutes shrink",
            "A fourth straight quiet outing; the coach declined to confirm his place in the closing lineup and the home crowd has gone flat on him.",
            7,
            "cooling_off",
            5,
            3,
            Some(0),
        )],
        &[],
        None,
        Some(memory),
        vibe_gate(&["Trent"], None, Some(45)),
    );
    write_fixture(
        &dir,
        &fx,
        "MEMORY GUARD: cold current coverage over a warm remembered spring scores ≤45 — the swing is the story to tell, but memory is continuity, never evidence for a warmer score.",
    )?;

    println!("done — 5 fixtures at {VIBE_PROMPT_VERSION}");
    Ok(())
}
