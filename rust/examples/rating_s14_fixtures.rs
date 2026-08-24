//! rating_s14_fixtures — regenerate the hand-authored Scout eval fixtures.
//!
//! s18 retires the verbatim PEAK marker line: the divined peak is code-owned
//! (`RatingReady.divined_peak`), the model emits only the three labeled sections, and the
//! brief is product-name-free (Scott's 2026-08-10 brief; gated by the harness's per-reply
//! `no_product_names` invariant). The old `peak_includes`/`peak_excludes` axes asserted the
//! copy step and are retired with it — the specificity they guarded moves into
//! `prose_includes`: the Strengths section must name the decision card's primary skill.
//!
//! s14 was the Characters Phase B voice pass: the system prompt speaks The Scout's telling
//! (persona-first — clipped, tactical, game-plan imperatives; speaks to a coaching staff,
//! never fans; names the skill and the number; tier is the truth). The four s11-era
//! scenarios (specificity, no-standout restraint, per-x corroboration, fixed-budget
//! richness) carry over as the regression floor; s14 added four Scout-refusal guards: the
//! no-clean-exploit contract phrase, the usage-artifact z-rule (the Drake London
//! precedent), a team profile with trend-talk exclusions, and secondary-strength number
//! coverage.
//!
//! Each scenario holds a RatingProfile and renders through the REAL production builder
//! (`build_stat_prompt` + `RATING_SYSTEM_PROMPT` + the deterministic scouting decision),
//! so the frozen `system`/`user_prompt` are byte-exact — a prompt bump means "re-run this
//! example", not "hand-patch the JSON". Writes the fixture files directly (the
//! transfers/vibe/momentum generator pattern):
//!
//!     cargo run --example rating_s14_fixtures
//!     cargo run --bin eval -- --task rating --fixtures   (needs Ollama)

use std::collections::HashMap;
use std::path::Path;

use scoracle_cognition::junctions::scout::{
    build_stat_prompt, compute_notability, RatingDatapoint, RatingProfile, RatingReq,
    RATING_PROMPT_VERSION, RATING_SYSTEM_PROMPT,
};
use serde_json::json;

struct Scenario {
    name: &'static str,
    note: &'static str,
    entity: &'static str,
    entity_type: &'static str, // "player" | "team"
    sport: &'static str,       // UPPER, as the caller supplies it
    position: &'static str,    // "" for teams
    composite: f64,
    breakdown: Vec<RatingDatapoint>,
    rate_modes: HashMap<String, Vec<RatingDatapoint>>,
    expect: serde_json::Value,
}

/// dp builds a clean in-composite datapoint. `z` is authored as the SIGN-ADJUSTED value
/// (sign stays 1), so what you write here is exactly what `format_datapoint_evidence`
/// renders — negative z = bad, matching the prompt the model sees.
fn dp(label: &str, value: f64, z: f64, pct: f64) -> RatingDatapoint {
    RatingDatapoint {
        label: label.to_string(),
        value,
        z,
        pct,
        in_comp: true,
        sign: 1,
        facet: "all".to_string(),
        ..RatingDatapoint::default()
    }
}

/// dp_pos adds a position-scoped percentile (renders as the "[position: …]" suffix).
fn dp_pos(label: &str, value: f64, z: f64, pct: f64, pos_pct: f64) -> RatingDatapoint {
    let mut d = dp(label, value, z, pct);
    d.scoped_pct.insert("position".to_string(), pos_pct);
    d
}

/// The D-T51 (s17) gate, shared by every rating fixture: the three exact section labels, the
/// " · " card-notation ban, the plain-text guard, and the word floor. Adopted into the
/// generator at s18 — until then this gate lived ONLY in the on-disk JSON, and a regen would
/// have silently dropped it (the momentum-generator lesson, caught the same evening).
fn rating_gate(includes: &[&str], excludes: &[&str], min_words: i64) -> serde_json::Value {
    let mut inc: Vec<String> = includes.iter().map(|s| s.to_string()).collect();
    // s21 renamed the sections (the FRONT-OFFICE pass: "Strengths to respect"/"Exploitation
    // opportunities" → "Strengths"/"Limitations"), and its refreeze updated the on-disk JSON
    // by hand while THIS helper kept the old labels — the exact drift this helper's own doc
    // warns about, sprung 2026-08-23 when a regen clobbered the hand-updates and the gate
    // read 68/87 against an 82/87 baseline. The generator is the home of every expect.
    inc.extend(["Strengths:", "Limitations:", "Summary:"].map(String::from));
    // (` · ` and `**` left the per-fixture excludes 08-19: they are the global
    // `no_banned_phrases` invariant now — `guards::RATING_BODY_BANS`, enforced in
    // production by `RatingParser`.)
    let exc: Vec<String> = excludes.iter().map(|s| s.to_string()).collect();
    json!({
        "prose_includes": inc,
        "prose_excludes": exc,
        "prose_min_words": min_words,
        "prose_max_words": 420,
    })
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "rim-protector-specificity",
            note: "stats identity specificity (s11 floor). TARGET: the PEAK label names the actual elite skill, not a collapsed report; the exploit call cites the turnovers number. s14: three-section budget, clipped register.",
            entity: "Nia Torres", entity_type: "player", sport: "NBA", position: "C",
            composite: 64.0,
            breakdown: vec![
                dp_pos("Rim protection", 3.2, 2.4, 96.0, 94.0),
                dp_pos("Defensive rebounds", 10.8, 1.7, 88.0, 82.0),
                dp("Screen assists", 5.9, 1.1, 79.0),
                dp("Turnovers", 2.8, -1.2, 28.0),
            ],
            rate_modes: HashMap::new(),
            expect: rating_gate(&["rim protection", "96th", "turnover"], &[], 30),
        },
        Scenario {
            name: "no-standout-restraint",
            note: "PEAK restraint (s11 floor). Highest datapoint is only above average, so the PEAK line must not invent a standout, and the modest profile earns one clipped line per section.",
            entity: "Eli Stone", entity_type: "player", sport: "NBA", position: "SG",
            composite: 49.0,
            breakdown: vec![
                dp_pos("Spot-up shooting", 38.1, 0.5, 64.0, 58.0),
                dp("Catch-and-shoot volume", 4.4, 0.1, 55.0),
                dp("Turnovers", 3.7, -1.4, 23.0),
            ],
            rate_modes: HashMap::new(),
            expect: rating_gate(&["64th", "turnover"], &["play physical"], 30),
        },
        Scenario {
            name: "rate-adjusted-limited-minutes",
            note: "PEAK specificity plus per-x corroboration (s11 floor). The strong first datapoint is the peak; the per-36 section is supporting evidence that the edge is real, not an invented larger role.",
            entity: "Dario Fen", entity_type: "player", sport: "NBA", position: "PF",
            composite: 55.0,
            breakdown: vec![
                dp("Paint finishing", 4.1, 1.3, 82.0),
                dp("Offensive rebounds", 2.2, 0.6, 66.0),
                dp("Fouls", 3.9, -1.1, 26.0),
            ],
            rate_modes: HashMap::from([(
                "per_36".to_string(),
                vec![dp("Paint finishing", 7.8, 1.9, 91.0)],
            )]),
            expect: rating_gate(&["finishing", "82", "per-36", "foul"], &["play physical"], 30),
        },
        Scenario {
            name: "fixed-budget-rich-profile",
            note: "prose richness under a fixed budget (s11 floor). Rich profile: the brief must cover the elite peak and the strong secondaries with their numbers, name the foul exploit, and still stay bounded.",
            entity: "Rui Almeida", entity_type: "player", sport: "FOOTBALL", position: "MF",
            composite: 61.0,
            breakdown: vec![
                dp("High press regains", 9.3, 2.2, 94.0),
                dp("Shot creation", 3.6, 1.6, 88.0),
                dp("Progressive passes", 6.9, 1.2, 84.0),
                dp("Final-third entries", 8.1, 0.9, 73.0),
                dp("Fouls committed", 2.4, -1.3, 22.0),
            ],
            rate_modes: HashMap::new(),
            expect: rating_gate(&["high press", "94th", "shot creation", "foul"], &[], 50),
        },
        Scenario {
            name: "no-clean-exploit",
            note: "NEW (s14): the decision card supplies no weakness — every non-peak mark is average or better. The Scout must say the profile offers no clean exploit, not manufacture one from an average mark.",
            entity: "Marcus Vale", entity_type: "player", sport: "NBA", position: "PG",
            composite: 60.0,
            breakdown: vec![
                dp("Pick-and-roll passing", 8.7, 2.1, 93.0),
                dp("Free-throw drawing", 6.2, 1.0, 77.0),
                dp("Perimeter defense", 1.1, 0.2, 58.0),
                dp("Mid-range volume", 3.3, -0.1, 52.0),
            ],
            rate_modes: HashMap::new(),
            expect: rating_gate(&["pick-and-roll", "93", "no clean exploit"], &[], 30),
        },
        Scenario {
            name: "usage-artifact-not-exploit",
            note: "NEW (s14): the z-rule guard (the Drake London precedent). Giveaways sit at the 5th percentile with z -0.2 — a usage artifact the decision card correctly skips; the real exploit is the 31st-pct drop rate. The brief must attack drops and never present giveaways as a target.",
            entity: "Trey Marsh", entity_type: "player", sport: "NFL", position: "WR",
            composite: 62.0,
            breakdown: vec![
                dp("Yards after catch", 612.0, 2.0, 93.0),
                dp("Contested catch rate", 58.3, 1.2, 80.0),
                dp("Giveaways", 1.0, -0.2, 5.0),
                dp("Drop rate", 9.8, -1.4, 31.0),
            ],
            rate_modes: HashMap::new(),
            expect: rating_gate(&["yards after catch", "drop", "31"], &["giveaway"], 30),
        },
        Scenario {
            name: "team-profile-clipped",
            note: "NEW (s14): team entity (no position) in the coaching-staff register, with the no-trend-talk refusal pinned — a static profile never earns momentum vocabulary; that read is another character's turn.",
            entity: "Harbor City FC", entity_type: "team", sport: "FOOTBALL", position: "",
            composite: 59.0,
            breakdown: vec![
                dp("Chance suppression", 0.9, 1.9, 91.0),
                dp("Set-piece threat", 14.2, 1.1, 78.0),
                dp("Build-up progression", 42.7, -1.0, 33.0),
            ],
            rate_modes: HashMap::new(),
            expect: rating_gate(&["suppression", "91", "progression"], &["momentum", "trending"], 30),
        },
        Scenario {
            name: "secondary-strengths-coverage",
            note: "NEW (s14): the signature check — names the skill AND the number. Strengths to respect must cover the elite peak and both strong secondaries with cited numbers, plus the rebounding exploit.",
            entity: "Jaylen Okafor", entity_type: "player", sport: "NBA", position: "PG",
            composite: 66.0,
            breakdown: vec![
                dp_pos("Pull-up shooting", 5.6, 2.3, 95.0, 92.0),
                dp("Playmaking", 7.4, 1.5, 87.0),
                dp("Steals", 1.9, 1.0, 76.0),
                dp("Defensive rebounds", 3.1, -0.9, 34.0),
            ],
            rate_modes: HashMap::new(),
            expect: rating_gate(&["pull-up", "95th", "playmaking", "steal"], &[], 50),
        },
    ]
}

fn main() -> anyhow::Result<()> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/rating");
    std::fs::create_dir_all(&dir)?;
    let scenarios = scenarios();
    let n = scenarios.len();
    for s in scenarios {
        let profile = RatingProfile {
            entity_type: s.entity_type.to_string(),
            season: 2025,
            position: s.position.to_string(),
            composite_score: Some(s.composite),
            breakdown: s.breakdown,
            scoped_ranks: HashMap::new(),
            rate_modes: s.rate_modes,
        };
        let req = RatingReq {
            entity_type: s.entity_type.to_string(),
            entity_id: 0,
            entity_name: s.entity.to_string(),
            sport: s.sport.to_string(),
            season: Some(2025),
            trigger_type: "eval".to_string(),
        };
        // Notability comes from the real computation, so the distinctiveness line the model
        // reads matches what production would say about this exact profile.
        let (notability, _) = compute_notability(&profile);
        // Fixtures pin the memory-free shape (the s12/n8 eval discipline) — and that now includes
        // the tagged availability reports: the frozen shape is the one with NO enrichment.
        let prompt = build_stat_prompt(&req, &profile, notability, None, None, None, None, None);
        let v = json!({
            "name": s.name,
            "task": "rating",
            "prompt_version": RATING_PROMPT_VERSION,
            "note": s.note,
            "system": RATING_SYSTEM_PROMPT,
            "user_prompt": prompt,
            "temperature": 0.0,
            "expect": s.expect,
        });
        let path = dir.join(format!("{}.json", s.name));
        std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&v)?))?;
        println!("wrote {} ({} chars prompt)", path.display(), prompt.len());
    }
    println!("done — {n} fixtures at {RATING_PROMPT_VERSION}");
    Ok(())
}
