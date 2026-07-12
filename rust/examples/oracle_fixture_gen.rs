//! oracle_fixture_gen — generate the oracle eval fixtures through the REAL production prompt
//! builder, so the frozen fixture prompts are byte-true to the current `ORACLE_PROMPT_VERSION`.
//!
//! Follows `sigil_fixture_gen`: each scenario holds the consumed Sigil + pillar STRUCTS and
//! calls `compute_omen` + `build_oracle_prompt` + `ORACLE_SYSTEM_PROMPT`, so a future prompt
//! bump regenerates the set mechanically:
//!
//!     cargo run --example oracle_fixture_gen > /tmp/oracle_fixtures.json
//!
//! Output: a JSON array of fixture objects; split into `fixtures/oracle/<name>.json` by hand
//! or script. Offline — no DB, no model, no queue.

use scoracle_cognition::corpus::HeatItem;
use scoracle_cognition::oracle::{
    build_oracle_prompt, compute_omen, ConsumedSigil, ORACLE_PROMPT_VERSION, ORACLE_SYSTEM_PROMPT,
};
use scoracle_cognition::sigil::{SynthMomentum, SynthNarrative, SynthRating, SynthVibe};
use serde_json::json;

struct Scenario {
    name: &'static str,
    note: &'static str,
    entity: &'static str,
    sport: &'static str,
    sigil: ConsumedSigil,
    narratives: Vec<SynthNarrative>,
    rating: Option<SynthRating>,
    vibe: Option<SynthVibe>,
    momentum: SynthMomentum,
    transfers: Vec<HeatItem>,
    expect: serde_json::Value,
}

fn sigil(
    score: i32,
    convergence: Option<i32>,
    blurb: &str,
    disagreement: Option<&str>,
    why_now: Option<&str>,
) -> ConsumedSigil {
    ConsumedSigil {
        score,
        blurb: blurb.to_string(),
        convergence,
        disagreement: disagreement.map(str::to_string),
        why_now: why_now.map(str::to_string),
        input_hash: Some("fixture-sigil-hash".to_string()),
    }
}

fn rating(divined_peak: &str, notability: i32, trajectory: &str, label: &str) -> SynthRating {
    SynthRating {
        divined_peak: divined_peak.to_string(),
        body: String::new(), // the oracle card renders peak/notability/label only
        notability,
        peak_trajectory: trajectory.to_string(),
        peak_trajectory_label: label.to_string(),
    }
}

fn vibe(sentiment: i32, prompt: &str) -> SynthVibe {
    SynthVibe {
        sentiment,
        prompt: prompt.to_string(),
    }
}

fn momentum(direction: &str, score: f64) -> SynthMomentum {
    SynthMomentum {
        direction: Some(direction.to_string()),
        momentum_score: Some(score),
        ..SynthMomentum::default()
    }
}

fn narrative(title: &str, trajectory: &str) -> SynthNarrative {
    SynthNarrative {
        title: title.to_string(),
        body: String::new(), // the oracle card renders titles + trajectory tags only
        impact: 5.0,
        trajectory: trajectory.to_string(),
        source_count: 2,
        source_age_days: Some(1),
    }
}

/// The jargon words the system prompt bans from every reading — internal bookkeeping the
/// Oracle must interpret, never recite. Asserted on every fixture.
fn jargon_excludes() -> serde_json::Value {
    json!(["notability", "convergence", "sentiment"])
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "ascendant-aligned",
            note: "every card points up and the panel agrees — the omen is ascendant. The reading must ride the rise without hype and stay inside the 2-4 sentence budget.",
            entity: "Aria Cole",
            sport: "NBA",
            sigil: sigil(
                82, Some(85),
                "A rising lead guard putting together an All-Star-calibre season, with the numbers and the narrative pulling the same direction.",
                None, None,
            ),
            narratives: vec![narrative("Breakout stretch drawing All-Star buzz", "heating_up")],
            rating: Some(rating("Playmaking", 72, "rising", "Composite and PEAK z-scores trending up over recent games")),
            vibe: Some(vibe(78, "Clearly on the rise.")),
            momentum: momentum("rising", 2.0),
            transfers: vec![],
            expect: json!({
                "reading_min_sentences": 2,
                "reading_max_sentences": 4,
                "reading_excludes": jargon_excludes(),
            }),
        },
        Scenario {
            name: "crossroads-conflict",
            note: "the classic split: elite production against a souring room and sliding momentum, panel agreement 40 — omen crossroads. The reading must name THIS tension, not generic fate talk.",
            entity: "Marcus Vale",
            sport: "NBA",
            sigil: sigil(
                71, Some(40),
                "An elite two-way wing whose production has not blinked, but the mood around him has turned and the trade-demand story is live.",
                Some("elite PEAK strength against negative vibe and sliding momentum"),
                Some("trade-demand reports broke this week"),
            ),
            narratives: vec![narrative("Locker-room friction and trade-demand reports", "heating_up")],
            rating: Some(rating("Two-way wing play", 90, "steady", "")),
            vibe: Some(vibe(34, "Sour mood around the player despite the production.")),
            momentum: momentum("falling", -2.0),
            transfers: vec![],
            expect: json!({
                "reading_min_sentences": 2,
                "reading_max_sentences": 4,
                "reading_excludes": jargon_excludes(),
            }),
        },
        Scenario {
            name: "waning-freefall",
            note: "every card points down and agrees — omen waning, sigil 30. A grim reading, told level: no melodrama, no fabricated turnaround.",
            entity: "Ty Marsh",
            sport: "NBA",
            sigil: sigil(
                30, Some(85),
                "A fringe rotation forward in a shrinking role, with the coverage, the numbers, and the coach's minutes all telling the same story.",
                None, None,
            ),
            narratives: vec![narrative("Benched after another quiet outing", "heating_up")],
            rating: Some(rating("No standout skill", 20, "falling", "Composite and PEAK z-scores trending down over recent games")),
            vibe: Some(vibe(25, "Gloomy — fans and beat writers see a shrinking role.")),
            momentum: momentum("falling", -3.0),
            transfers: vec![],
            expect: json!({
                "reading_min_sentences": 2,
                "reading_max_sentences": 4,
                "reading_excludes": jargon_excludes(),
            }),
        },
        Scenario {
            name: "steady-quiet",
            note: "a genuinely quiet spread: mid sigil, steady momentum, mild vibe — omen steady. The calm-reading rule under test: no manufactured drama.",
            entity: "Diego Ferran",
            sport: "NBA",
            sigil: sigil(
                55, Some(75),
                "A solid rotation forward doing rotation-forward things; nothing in the coverage or the numbers is moving.",
                None, None,
            ),
            narratives: vec![],
            rating: Some(rating("Spot-up shooting", 45, "steady", "")),
            vibe: Some(vibe(55, "Mixed, unremarkable chatter.")),
            momentum: momentum("steady", 0.0),
            transfers: vec![],
            expect: json!({
                "reading_min_sentences": 2,
                "reading_max_sentences": 4,
                "reading_excludes": ["notability", "convergence", "sentiment", "freefall", "collapse"],
            }),
        },
        Scenario {
            name: "transfer-crossroads",
            note: "steady elite form against an advanced exit rumor, panel agreement 45 — omen crossroads. Grounding under test: a real reading names the counterparty from the transfer card.",
            entity: "Luca Moretti",
            sport: "Football",
            sigil: sigil(
                76, Some(45),
                "A first-choice creator in unshaken form, while an advanced move to Madrid hangs over every match he plays.",
                Some("steady elite form against an advanced exit rumor"),
                Some("advanced talks with Real Madrid reported"),
            ),
            narratives: vec![narrative("Real Madrid talks reach advanced stage", "heating_up")],
            rating: Some(rating("Chance creation", 84, "steady", "")),
            vibe: Some(vibe(61, "Fans bracing for a departure while enjoying the form.")),
            momentum: momentum("steady", 0.0),
            transfers: vec![HeatItem {
                counterparty: "Real Madrid".to_string(),
                heat: 78,
                stage: "advanced_talks".to_string(),
                direction: "outgoing".to_string(),
                summary: "Multiple insiders report advanced talks over a summer move to Real Madrid".to_string(),
                confidence: Some(0.8),
            }],
            expect: json!({
                "reading_min_sentences": 2,
                "reading_max_sentences": 4,
                "reading_includes": ["Real Madrid"],
                "reading_excludes": jargon_excludes(),
            }),
        },
        Scenario {
            name: "ascendant-thin-hype",
            note: "rising momentum and warm vibe over a thin production profile, panel agreement 55 — omen ascendant (momentum leads), but the panel's read names the thin base. The reading must ride the rise while staying honest about what would confirm it.",
            entity: "Rio Vance",
            sport: "NBA",
            sigil: sigil(
                48, Some(55),
                "A deep-bench wing riding a viral highlight run; the buzz is real, the underlying role has not changed yet.",
                None,
                Some("a viral highlight run drove a wave of breakout talk"),
            ),
            narratives: vec![narrative("Viral highlight run has the fanbase dreaming", "heating_up")],
            rating: Some(rating("No standout skill", 25, "steady", "")),
            vibe: Some(vibe(74, "Buzzing — highlight-reel plays have fans excited.")),
            momentum: momentum("rising", 2.0),
            transfers: vec![],
            expect: json!({
                "reading_min_sentences": 2,
                "reading_max_sentences": 4,
                "reading_excludes": jargon_excludes(),
            }),
        },
    ]
}

fn main() {
    let out: Vec<serde_json::Value> = scenarios()
        .into_iter()
        .map(|s| {
            let (omen, omen_reason) = compute_omen(&s.sigil, s.rating.as_ref(), &s.momentum);
            let prompt = build_oracle_prompt(
                "player",
                s.entity,
                s.sport,
                &s.sigil,
                &s.narratives,
                s.rating.as_ref(),
                s.vibe.as_ref(),
                &s.momentum,
                &s.transfers,
                omen,
                &omen_reason,
            );
            json!({
                "name": s.name,
                "task": "oracle",
                "prompt_version": ORACLE_PROMPT_VERSION,
                "note": s.note,
                "omen": omen,
                "system": ORACLE_SYSTEM_PROMPT,
                "user_prompt": prompt,
                "temperature": 0.0,
                "expect": s.expect,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
