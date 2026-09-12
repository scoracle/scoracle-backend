//! transfer_t10_fixtures — regenerate the hand-authored Insider eval fixtures. The live pair
//! build reads the reliability + memory cards from the DB; a frozen fixture cannot, so each
//! freezes a faithful `build_transfer_prompt` render with both cards baked in (byte-exact —
//! a prompt bump means "re-run this example", not "hand-patch the JSON"). FLOOR checks
//! (is_rumor, direction, subject, no invented fee) hold regardless of voice; TARGET checks
//! (`transfer_stage`, `confidence_min`/`max`) assert the steam/fizzle weighting.

use std::path::Path;

use scoracle_cognition::eval_tasks::{Expect, Fixture};
use scoracle_cognition::junctions::insider::{
    build_transfer_prompt, transfer_system_prompt, NewsItem, TransferCandidate, TransferEvidence,
    TRANSFER_PROMPT_VERSION,
};

/// One corpus headline. The prompt render reads source/title/description only; the id is inert here.
fn ni(id: i64, source: &str, title: &str, description: &str) -> NewsItem {
    NewsItem {
        id,
        title: title.to_string(),
        description: description.to_string(),
        source: source.to_string(),
    }
}

fn cand(name: &str, nationality: &str, current_club: &str, position: &str) -> TransferCandidate {
    TransferCandidate {
        player_id: 0,
        subject_type: "player".to_string(),
        relationship_override: None,
        player_name: name.to_string(),
        nationality: nationality.to_string(),
        current_club: current_club.to_string(),
        position: position.to_string(),
    }
}

/// Build one hand-authored fixture: render the exact live prompt (with the reliability + memory
/// cards), pin temp 0 for reproducibility, and attach the property rubric. `best` feeds the
/// Evidence card so it agrees with the corpus the model reads.
#[allow(clippy::too_many_arguments)]
fn fixture(
    name: &str,
    team: &str,
    sport: &str,
    relationship: &str,
    c: &TransferCandidate,
    news: &[NewsItem],
    best: &str,
    reliability: &str,
    memory: &str,
    expect: Expect,
) -> Fixture {
    let evidence = TransferEvidence::from_news(news, news.len(), best);
    Fixture {
        name: name.to_string(),
        task: "transfer".to_string(),
        prompt_version: TRANSFER_PROMPT_VERSION.to_string(),
        system: transfer_system_prompt(sport),
        user_prompt: build_transfer_prompt(
            team,
            c,
            sport,
            relationship,
            news,
            &evidence,
            Some(reliability),
            Some(memory),
            // Fixtures pin the prompt shape without packet framing.
            None,
        ),
        temperature: 0.0,
        expect,
    }
}

/// Serialize a fixture and inject a documentary `"note"` (an unknown key the loader ignores, like the
/// existing hand-authored fixtures carry), so the on-disk file self-describes its intent.
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
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/transfer");
    std::fs::create_dir_all(&dir)?;

    // ── Fixture 1 — STEAM: reliable, early-calling sources + a heating story ────────────────────────
    // Reliability + heat should let the read carry steam — advance the stage and commit.
    let f1 = fixture(
        "reliable-source-heating-steam",
        "Bayern Munich",
        "FOOTBALL",
        "none", // player not on the team ⇒ incoming pursuit
        &cand("Mateus Andrade", "Brazilian", "Flamengo", "winger"),
        &[
            ni(1, "Fabrizio Romano", "Bayern in active talks with Flamengo for Andrade", "Bayern and Flamengo are negotiating the fee, with personal terms already agreed on a five-year deal for the winger."),
            ni(2, "Kicker", "Bayern make Andrade their priority, open formal talks", "Bayern's board have opened formal negotiations with Flamengo and see the Brazilian as their top target."),
        ],
        "Fabrizio Romano",
        "Fabrizio Romano: reliability 86/100 (28 of 33 tracked moves confirmed, 19 reported early).\nKicker: reliability 71/100 (18 of 26 tracked moves confirmed, 9 reported early).",
        "Current story: tracked since Jun 20, peak coverage 62/100, computed likelihood 68/100 (heating up).",
        Expect {
            // FLOOR — grounding, holds regardless of voice.
            transfer_is_rumor: Some(true),
            transfer_direction: Some("incoming".into()),
            subject_includes: Some(vec!["Andrade".into()]),
            summary_includes: Some(vec!["Bayern".into(), "Flamengo".into()]),
            summary_excludes: Some(vec!["$".into(), "£".into(), "€".into(), "million".into()]),
            // TARGET — steam should advance the stage and carry real confidence.
            transfer_stage: Some("advanced_talks".into()),
            confidence_min: Some(0.6),
            ..Default::default()
        },
    );
    write_fixture(&dir, &f1, "t10 STEAM: a top-tier, early-calling source (86/100) explicitly reports active negotiation + a corroborating source, and the memory card is heating (likelihood 68/100). FLOOR: is_rumor=true, incoming, names Bayern+Flamengo, no invented fee. VOICE+WEIGHTING: advances to advanced_talks with confidence ≥ 0.6.")?;

    // ── Fixture 2 — FIZZLE: a fizzled prior + a thin, low-reliability report ─────────────────────────
    // Skepticism should hold the read at speculation with low confidence — not re-hyped by the dead saga.
    let f2 = fixture(
        "fizzled-prior-lowrel-skeptic",
        "Chelsea",
        "FOOTBALL",
        "none",
        &cand("Kai Sorensen", "Danish", "Brøndby", "forward"),
        &[ni(
            1,
            "TransferTavern",
            "Chelsea 'keeping tabs' on Sorensen — report",
            "A report claims Chelsea are among clubs monitoring the forward, though no bid or talks are mentioned.",
        )],
        "TransferTavern",
        "TransferTavern: reliability 14/100 (2 of 40 tracked moves confirmed).",
        "Prior flirtation fizzled: Feb 2026, peak coverage 79/100.\nCurrent story: tracked since Jul 10, peak coverage 30/100, computed likelihood 22/100 (cooling off).",
        Expect {
            // FLOOR — grounding.
            subject_includes: Some(vec!["Sorensen".into()]),
            summary_excludes: Some(vec!["$".into(), "£".into(), "€".into(), "million".into()]),
            // TARGET — fizzle should be held at speculation with low confidence.
            transfer_stage: Some("speculation".into()),
            confidence_max: Some(0.5),
            ..Default::default()
        },
    );
    write_fixture(&dir, &f2, "t10 FIZZLE: a lone low-reliability source (14/100) with a thin 'keeping tabs' report, over a fizzled prior + cooling memory (likelihood 22/100). FLOOR: subject=Sorensen, no invented fee. VOICE+WEIGHTING: held at speculation, confidence ≤ 0.5 — not re-hyped by the dead saga.")?;

    println!("\nrun: cargo run --bin eval -- --task transfer --fixtures   (needs Ollama)");
    Ok(())
}
