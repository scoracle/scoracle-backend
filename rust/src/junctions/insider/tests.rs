//! Unit tests for this junction.
//!
//! Split out of `mod.rs` so the stage module reads as the stage and nothing else.
//! `super` still resolves to the junction, so these run exactly as they did inline.

use super::*;

// --- The wire wrap (Phase 4): grammar, parse, debounce pre-image, prompt ---------------------

fn heat_item(cp: &str, heat: i32, direction: &str, stage: &str, summary: &str) -> HeatItem {
    HeatItem {
        counterparty: cp.to_string(),
        heat,
        stage: stage.to_string(),
        direction: direction.to_string(),
        summary: summary.to_string(),
        confidence: None,
    }
}

#[test]
fn insider_score_schema_reads_first_lands_1_to_99() {
    let schema = insider_score_format_schema();
    // is5: read → headline → score. The wire is read first, titled second, and the verdict
    // lands last (sigil doctrine) — the required order IS the generation order.
    assert_eq!(
        schema["required"],
        serde_json::json!(["read", "headline", "score"])
    );
    assert_eq!(
        schema["properties"]["score"]["minimum"],
        serde_json::json!(1)
    );
    assert_eq!(
        schema["properties"]["score"]["maximum"],
        serde_json::json!(99)
    );
}

#[test]
fn insider_score_reply_parses_and_clamps() {
    let ok = parse_insider_score_reply(
        r#"{"read": "The wire holds one advancing call.", "score": 62}"#,
    )
    .unwrap();
    assert_eq!(ok.read, "The wire holds one advancing call.");
    assert_eq!(ok.score, 62);
    // Prose around the object is tolerated (the crown's salvage rule); scores clamp to 1-99.
    let wrapped =
        parse_insider_score_reply(r#"Here: {"read": "Quiet.", "score": 250} done"#).unwrap();
    assert_eq!(wrapped.score, 99);
    assert_eq!(
        parse_insider_score_reply(r#"{"read": "x", "score": -5}"#)
            .unwrap()
            .score,
        1
    );
    // An empty read or missing score is nothing salvageable → retry, never a served row.
    assert!(parse_insider_score_reply(r#"{"read": "  ", "score": 50}"#).is_none());
    assert!(parse_insider_score_reply(r#"{"read": "x"}"#).is_none());
    assert!(InsiderScoreParser.parse("not a reply").is_err());
}

#[test]
fn insider_headline_is_best_effort_and_settled() {
    // is5: the hook rides the wrap and takes the shared title floor at parse.
    let with = parse_insider_score_reply(
        r#"{"read": "The wire is quiet.", "headline": "Nobody is calling about anyone here", "score": 8}"#,
    )
    .unwrap();
    assert_eq!(
        with.headline.as_deref(),
        Some("Nobody is calling about anyone here")
    );
    // A pre-is5 reply shape (no headline key) still parses — the hook is simply absent.
    let without =
        parse_insider_score_reply(r#"{"read": "One advancing call.", "score": 55}"#).unwrap();
    assert_eq!(without.headline, None);
    // A junk title costs the title, never the wrap: >140 chars with no beat to cut drops.
    let overlong = format!(
        r#"{{"read": "Quiet.", "headline": "{}", "score": 5}}"#,
        "x".repeat(200)
    );
    let settled = parse_insider_score_reply(&overlong).unwrap();
    assert_eq!(settled.headline, None);
    assert_eq!(settled.score, 5);
}

#[test]
fn insider_score_read_reaches_the_card_undressed() {
    // is4. The verbatim production shape from `/transfers.wire_read` on 2026-08-22 — the READ
    // was the only served prose in the fleet with no plain-text rule and no `**` ban, because
    // it was audit-only when the other seats got theirs.
    let dressed = parse_insider_score_reply(
        r#"{"read": "West Ham's wire is **one live, advanced call** — the rest are **speculative interests at minimal momentum**.", "score": 38}"#,
    )
    .unwrap();
    assert_eq!(
        dressed.read,
        "West Ham's wire is one live, advanced call — the rest are speculative interests at minimal momentum."
    );
    assert_eq!(dressed.score, 38);

    // Stripped, never REJECTED: a dressed read still ships, because a ban here would fail
    // nearly every wire read until the prompt rule took (the fail-rate lesson of the same-day
    // momentum-s18 / vibe-v21 pass).
    assert!(parse_insider_score_reply(r#"{"read": "**Quiet.**", "score": 4}"#).is_some());

    // Leading list/heading decoration goes too, and apostrophes and in-word underscores are
    // left alone — this is prose hygiene, not a rewrite of what the Insider filed.
    assert_eq!(
        parse_insider_score_reply(r#"{"read": "- The board's snake_case tag holds.", "score": 50}"#)
            .unwrap()
            .read,
        "The board's snake_case tag holds."
    );
}

#[test]
fn insider_score_input_components_content_keyed_and_order_stable() {
    // Same board in a different order ⇒ identical pre-image; summary/confidence never enter
    // it (a re-worded summary alone must not re-score the wire).
    let one = build_insider_score_input_components(&[
        heat_item("Lakers", 80, "incoming", "advanced_talks", "worded one way"),
        heat_item("Heat", 40, "outgoing", "speculation", "x"),
    ]);
    let two = build_insider_score_input_components(&[
        heat_item("Heat", 40, "outgoing", "speculation", "y"),
        heat_item(
            "Lakers",
            80,
            "incoming",
            "advanced_talks",
            "worded another way",
        ),
    ]);
    assert_eq!(one, two);
    assert_eq!(
        one,
        format!(
            r#"{{"prompt_version":"{INSIDER_SCORE_PROMPT_VERSION}","board":["Heat:40:outgoing:speculation","Lakers:80:incoming:advanced_talks"]}}"#
        )
    );
    // A heat/stage move flips the hash pre-image (the re-score trigger).
    let moved = build_insider_score_input_components(&[
        heat_item("Lakers", 85, "incoming", "here_we_go", ""),
        heat_item("Heat", 40, "outgoing", "speculation", ""),
    ]);
    assert_ne!(one, moved);
}

#[test]
fn insider_score_prompt_memory_before_board() {
    let board = vec![heat_item(
        "Lakers",
        80,
        "incoming",
        "advanced_talks",
        "Lakers in advanced talks per ESPN",
    )];
    let p = build_insider_score_prompt(
        "Some Team",
        "NBA",
        "team",
        &board,
        Some("Last read (Jul 18): The wire is warm.\nRecent scores (newest first): 62 (Jul 18) · 55 (Jul 12)"),
    );
    assert_eq!(
        p,
        "Entity: Some Team (NBA team)\n\
\nYOUR PRIOR READS (memory — your own past wire wraps; continuity, not new evidence):\n\
Last read (Jul 18): The wire is warm.\n\
Recent scores (newest first): 62 (Jul 18) · 55 (Jul 12)\n\
\nTHE ACTIVE WIRE (1 live vetted rumor(s), latest per counterparty):\n\
- Lakers — heat 80, incoming, advanced_talks — \"Lakers in advanced talks per ESPN\"\n\
\nReturn the JSON object now."
    );
    // First-ever wrap: no memory section at all.
    let first = build_insider_score_prompt("Some Team", "NBA", "team", &board, None);
    assert!(!first.contains("YOUR PRIOR READS"));
}

fn cand(name: &str, nat: &str, club: &str, pos: &str) -> TransferCandidate {
    TransferCandidate {
        player_id: 1,
        subject_type: "player".to_string(),
        relationship_override: None,
        player_name: name.to_string(),
        nationality: nat.to_string(),
        current_club: club.to_string(),
        position: pos.to_string(),
    }
}

// --- build_transfer_prompt byte-fixtures: the deterministic parity axis. The expected strings
// are computed by hand from the Rust assembly, so prompt drift fails here (offline, no model).
// -----------------------------------------------------------------------------------------------

#[test]
fn prompt_current_with_full_identity_and_news() {
    let c = cand("Bukayo Saka", "English", "Arsenal", "winger");
    let news = vec![NewsItem {
        id: 1,
        title: "Saka linked with move".to_string(),
        description: "Reports suggest interest.".to_string(),
        source: "BBC".to_string(),
    }];
    let evidence = TransferEvidence::from_news(&news, 1, "BBC");
    let p = build_transfer_prompt(
        "Arsenal", &c, "FOOTBALL", "current", &news, &evidence, None, None, None,
    );
    assert_eq!(
        p,
        "Sport: FOOTBALL\nTeam: Arsenal\nPlayer: Bukayo Saka\n\
Identity (the ONE specific player to judge): Bukayo Saka · English · currently at Arsenal · winger\n\
Roster status: Bukayo Saka is CURRENTLY on Arsenal — so any move is a DEPARTURE (outgoing). Frame the summary as other clubs' interest in signing them.\n\
Evidence (computed): 1 article, 1 distinct source; primary source: BBC.\n\
\nNews headlines:\n\
- [BBC] Saka linked with move — Reports suggest interest.\n\
\nReturn the JSON verdict now."
    );
}

#[test]
fn prompt_former_sparse_identity_no_news() {
    // No nationality, unknown club, no position → identity is just name + "current club unknown".
    let c = cand("John Doe", "", "", "");
    let evidence = TransferEvidence::from_news(&[], 0, "");
    let p = build_transfer_prompt(
        "Chelsea",
        &c,
        "FOOTBALL",
        "former",
        &[],
        &evidence,
        None,
        None,
        None,
    );
    assert_eq!(
        p,
        "Sport: FOOTBALL\nTeam: Chelsea\nPlayer: John Doe\n\
Identity (the ONE specific player to judge): John Doe · current club unknown\n\
Roster status: John Doe is a FORMER Chelsea player who has SINCE LEFT. A 'former/ex-Chelsea' mention is just background, NOT a transfer rumor — set is_rumor=false UNLESS the sources genuinely report John Doe RETURNING to Chelsea (then it is incoming).\n\
Evidence (computed): 0 articles, 0 distinct sources; primary source: none attributed.\n\
\nNews headlines:\n\
- (none)\n\
\nReturn the JSON verdict now."
    );
}

#[test]
fn prompt_none_sourceless_headline() {
    // relationship "none" (default arm) + a headline with no source (no "[src] " prefix).
    let c = cand("Victor Wembanyama", "French", "Spurs", "center");
    let news = vec![NewsItem {
        id: 1,
        title: "Trade buzz".to_string(),
        description: String::new(),
        source: String::new(),
    }];
    let evidence = TransferEvidence::from_news(&news, 1, "");
    let p = build_transfer_prompt("Lakers", &c, "NBA", "none", &news, &evidence, None, None, None);
    assert!(
        !p.contains("Relational memory"),
        "no memory ⇒ no section (t7 byte-shape preserved)"
    );
    assert!(
        !p.contains("Source track record"),
        "no reliability card ⇒ no section"
    );
    let with_mem = build_transfer_prompt(
        "Lakers",
        &c,
        "NBA",
        "none",
        &news,
        &evidence,
        None,
        Some("Prior flirtation fizzled: Jun 2026, peak coverage 55/100.\nCurrent story: tracked since Jul 05, peak coverage 80/100, computed likelihood 62/100 (heating up)."),
        None,
    );
    assert!(with_mem.contains("Relational memory (computed history"));
    assert!(with_mem.contains("- Prior flirtation fizzled: Jun 2026, peak coverage 55/100."));
    assert!(with_mem.contains("- Current story: tracked since Jul 05"));
    assert_eq!(
        p,
        "Sport: NBA\nTeam: Lakers\nPlayer: Victor Wembanyama\n\
Identity (the ONE specific player to judge): Victor Wembanyama · French · currently at Spurs · center\n\
Roster status: Victor Wembanyama is NOT on Lakers — so any move is an ARRIVAL (incoming). Frame the summary as Lakers pursuing them.\n\
Evidence (computed): 1 article, 0 distinct sources; primary source: none attributed.\n\
\nNews headlines:\n\
- Trade buzz\n\
\nReturn the JSON verdict now."
    );
}

#[test]
fn prompt_renders_source_reliability_card_before_memory() {
    // t9 (Phase 4): the source-reliability card renders as its own section, header + one
    // "- " line per source, positioned AFTER Evidence and BEFORE the Relational memory card.
    let c = cand("Bukayo Saka", "English", "Arsenal", "winger");
    let news = vec![NewsItem {
        id: 1,
        title: "Saka linked with move".to_string(),
        description: "Reports suggest interest.".to_string(),
        source: "Fabrizio Romano".to_string(),
    }];
    let evidence = TransferEvidence::from_news(&news, 1, "Fabrizio Romano");
    let reliability = "Fabrizio Romano: reliability 83/100 (24 of 30 tracked moves confirmed, 15 reported early).\nTalkSport: reliability 21/100 (3 of 40 tracked moves confirmed).";
    let memory = "Current story: tracked since Jul 05, peak coverage 80/100, computed likelihood 62/100 (heating up).";
    let p = build_transfer_prompt(
        "Arsenal",
        &c,
        "FOOTBALL",
        "current",
        &news,
        &evidence,
        Some(reliability),
        Some(memory),
        None,
    );
    assert!(p.contains("\nSource track record (measured"));
    assert!(p.contains(
        "- Fabrizio Romano: reliability 83/100 (24 of 30 tracked moves confirmed, 15 reported early)."
    ));
    assert!(p.contains("- TalkSport: reliability 21/100 (3 of 40 tracked moves confirmed)."));
    // Ordering: Evidence < Source track record < Relational memory < News headlines.
    let ev = p.find("Evidence (computed)").expect("evidence card");
    let sr = p.find("Source track record").expect("reliability card");
    let mem = p.find("Relational memory").expect("memory card");
    let hdl = p.find("News headlines").expect("headlines");
    assert!(
        ev < sr && sr < mem && mem < hdl,
        "card order: {ev} {sr} {mem} {hdl}"
    );
    // Blank reliability ⇒ no section (mirrors the memory-card guard).
    let none = build_transfer_prompt(
        "Arsenal",
        &c,
        "FOOTBALL",
        "current",
        &news,
        &evidence,
        Some("  "),
        None,
        None,
    );
    assert!(!none.contains("Source track record"));
}

// --- 7.5: the packet rail's material, and the legacy rail's silence -----------------------------

/// Under `RAIL=legacy` the packet arm contributes NOTHING — no framing, no relabelled header —
/// so a binary carrying all of Phase 7 sends the pair prompt Phase 6 sent, byte for byte. This is
/// the property that makes the deploy safe before the flip.
#[test]
fn legacy_pair_prompt_is_byte_identical_to_the_no_packet_prompt() {
    let c = cand("Bukayo Saka", "English", "Arsenal", "winger");
    let news = vec![NewsItem {
        id: 1,
        title: "Saka linked with move".to_string(),
        description: "Reports suggest interest.".to_string(),
        source: "BBC".to_string(),
    }];
    let evidence = TransferEvidence::from_news(&news, 1, "BBC");
    let legacy = build_transfer_prompt(
        "Arsenal", &c, "FOOTBALL", "current", &news, &evidence, None, None, None,
    );
    // An EMPTY framing is indistinguishable from no framing: a packet that contributed no claim
    // to this pair must not leave a fingerprint on the prompt either.
    let empty_framing = build_transfer_prompt(
        "Arsenal",
        &c,
        "FOOTBALL",
        "current",
        &news,
        &evidence,
        None,
        None,
        Some("   \n"),
    );
    assert!(legacy.contains("\nNews headlines:\n"));
    assert!(!legacy.contains("The story these reports belong to"));
    assert_eq!(legacy, empty_framing);
}

/// The packet rail hands the Insider the storyline as FRAMING and the Editor's claims as the
/// evidence lines — and says so in the header, because on this rail a line is a claim, not a
/// headline.
#[test]
fn packet_pair_prompt_carries_the_storyline_framing_above_the_claims() {
    let c = cand("Vinicius Junior", "Brazilian", "Real Madrid", "winger");
    let news = vec![
        NewsItem {
            id: 3,
            title: "⇄ Arsenal have reached an agreement in principle on personal terms".to_string(),
            description: String::new(),
            source: "Football365".to_string(),
        },
        NewsItem {
            id: 2,
            title: "⇄ deal not agreed".to_string(),
            description: String::new(),
            source: "The Athletic".to_string(),
        },
    ];
    let evidence = TransferEvidence::from_news(&news, 2, "Football365");
    let framing = "STORY: Vinicius Junior and Arsenal: where the deal stands\nENTITY: Arsenal (opponent) — in this story 2026-08-02 → 2026-08-05\nTYPE: transfer\n";
    let p = build_transfer_prompt(
        "Arsenal",
        &c,
        "FOOTBALL",
        "none",
        &news,
        &evidence,
        None,
        None,
        Some(framing),
    );
    assert!(p.contains("The story these reports belong to"));
    assert!(p.contains("STORY: Vinicius Junior and Arsenal: where the deal stands"));
    assert!(p.contains("What the reports claim (⇄"));
    assert!(!p.contains("\nNews headlines:\n"), "the header names the material");
    // Both sides of the contradiction stand, attributed (T3/D6) — the Insider is the one voice
    // whose job is to stage a contested claim, so neither may be collapsed on the way in.
    assert!(p.contains("[Football365] ⇄ Arsenal have reached an agreement in principle"));
    assert!(p.contains("[The Athletic] ⇄ deal not agreed"));
    // Framing sits ABOVE the claims: the story, then what is claimed about it.
    let story = p.find("The story these reports belong to").unwrap();
    let claims = p.find("What the reports claim").unwrap();
    assert!(story < claims);
}

/// E1/7.5: the Insider's slice is the transfer-typed claims — the same subset
/// `slice_fingerprints ->> 'transfers'` hashes, so a re-fan and a re-read cannot disagree about
/// what moved.
#[test]
fn insider_slice_is_the_transfer_typed_claims() {
    use crate::junctions::editor::render::{slice_claims, RenderClaim, Voice};
    let claims = vec![
        RenderClaim {
            article_id: 3,
            source: "ESPN".into(),
            fact: "Arsenal agreed personal terms".into(),
            published_at: Some(1_754_000_003),
            story_type: "transfer".into(),
        },
        RenderClaim {
            article_id: 2,
            source: "BBC".into(),
            fact: "He trained fully on Monday after a knock".into(),
            published_at: Some(1_754_000_002),
            story_type: "injury".into(),
        },
    ];
    let slice = slice_claims(&claims, Voice::Insider);
    assert_eq!(slice.len(), 1);
    assert_eq!(slice[0].article_id, 3);
}

// --- TransferParser fail-closed contract (mirrors Go TestParseTransferVerdictFailClosed) ------

#[test]
fn parser_fail_closed_on_non_json() {
    assert!(TransferParser.parse("").unwrap().is_none());
    assert!(TransferParser.parse("Sorry, no idea.").unwrap().is_none());
    assert!(TransferParser.parse("{not json").unwrap().is_none());
}

#[test]
fn parser_missing_is_rumor_is_some_with_none_field() {
    // Parsed object that never committed to is_rumor → Some(verdict) with is_rumor == None; the
    // caller routes THAT to the UNKNOWN marker (Go's verdict.IsRumor == nil branch).
    let v = TransferParser
        .parse(r#"{"subject":"Someone","direction":"incoming"}"#)
        .unwrap()
        .expect("a parseable object is Some");
    assert_eq!(v.is_rumor, None);
    assert_eq!(v.subject, "Someone");
}

#[test]
fn parser_accepts_negative_verdict_with_null_prose_fields() {
    let v = TransferParser
        .parse(
            r#"{"is_rumor":false,"subject":"Coby White","direction":"incoming","stage":null,"summary":null,"confidence":1.0}"#,
        )
        .unwrap()
        .expect("null prose fields should not erase committed false verdict");
    assert_eq!(v.is_rumor, Some(false));
    assert_eq!(v.subject, "Coby White");
    assert_eq!(v.direction, "incoming");
    assert_eq!(v.stage, "");
    assert_eq!(v.summary, "");
    assert_eq!(v.confidence, 1.0);
}

#[test]
fn parser_extracts_committed_verdict_from_wrapped_json() {
    let v = TransferParser
        .parse("noise {\"is_rumor\": true, \"stage\": \"concrete_interest\", \"confidence\": 0.8} trailing")
        .unwrap()
        .expect("salvaged from wrapping");
    assert_eq!(v.is_rumor, Some(true));
    assert_eq!(v.stage, "concrete_interest");
    assert!((v.confidence - 0.8).abs() < 1e-9);
}

// --- norm_stage / clamp_conf (mirror Go TestNormStageDefaultsToSpeculation / TestClampConfBounds)

#[test]
fn norm_stage_normalizes_and_defaults() {
    assert_eq!(norm_stage("Concrete Interest"), "concrete_interest");
    assert_eq!(norm_stage("HERE_WE_GO"), "here_we_go");
    assert_eq!(norm_stage("nonsense"), "speculation");
    assert_eq!(norm_stage(""), "speculation");
}

#[test]
fn clamp_conf_bounds() {
    assert_eq!(clamp_conf(-0.2), 0.0);
    assert_eq!(clamp_conf(1.5), 1.0);
    assert!((clamp_conf(0.73) - 0.73).abs() < 1e-9);
}

// --- the deterministic gates / mappings -------------------------------------------------------

#[test]
fn has_return_signal_detects_return_language() {
    let yes = vec![NewsItem {
        id: 1,
        title: "Star set to rejoin former club".to_string(),
        description: String::new(),
        source: "X".to_string(),
    }];
    let no = vec![NewsItem {
        id: 2,
        title: "Former player scores against old side".to_string(),
        description: "A routine match report.".to_string(),
        source: "X".to_string(),
    }];
    let multilingual = vec![
        NewsItem {
            id: 3,
            title: "Le milieu fait son retour à Paris".to_string(),
            description: String::new(),
            source: "X".to_string(),
        },
        NewsItem {
            id: 4,
            title: "Der Stürmer steht vor der Rückkehr zu United".to_string(),
            description: String::new(),
            source: "Y".to_string(),
        },
    ];
    assert!(has_return_signal(&yes));
    assert!(has_return_signal(&multilingual));
    assert!(!has_return_signal(&no));
}

#[test]
fn identity_score_uses_raw_heat_only() {
    let (heat, confidence) = identity_apply_deterministic_score(83);

    assert_eq!(heat, 83);
    assert_eq!(confidence, 0.83);
}

#[test]
fn direction_is_deterministic_from_relationship() {
    assert_eq!(direction_for("current"), "outgoing");
    assert_eq!(direction_for("former"), "incoming");
    assert_eq!(direction_for("none"), "incoming");
}

#[test]
fn transfer_input_components_are_material_only() {
    // A realistic compute_transfer_heat components blob: the decay fields are PRESENT in the
    // input but must be EXCLUDED from the pre-image (heat, newest_age_hours, recency,
    // recent_3d, recent_frac tick with NOW(); total_14d/volume are id-set redundant).
    let comps = r#"{"distinct_sources": 3, "recent_3d": 2, "total_14d": 5,
        "newest_age_hours": 12.3, "tier_weight": 0.9, "volume": 0.6,
        "recency": 0.842, "recent_frac": 0.4}"#;
    // prompt_version is interpolated (not hard-coded) so the assertion stays green across
    // future prompt bumps — the point is the KEY is present and forces one regen per bump.
    assert_eq!(
        build_transfer_input_components(&[9, 4, 7], comps, "current"),
        format!(
            r#"{{"distinct_sources":3,"news_ids":[4,7,9],"prompt_version":"{TRANSFER_PROMPT_VERSION}","relationship":"current"}}"#
        )
    );
    // Empty/degenerate components (the defensive path) keep a stable pre-image.
    assert_eq!(
        build_transfer_input_components(&[], "{}", "none"),
        format!(
            r#"{{"distinct_sources":0,"news_ids":[],"prompt_version":"{TRANSFER_PROMPT_VERSION}","relationship":"none"}}"#
        )
    );
}

#[test]
fn transfer_input_hash_ignores_id_order_and_decay_drift() {
    // The SAME corpus observed hours apart: ids identical (different order), every
    // NOW()-derived component moved. The fingerprint must NOT move — pure time decay
    // never re-runs the GPU (cooling is the read path's job).
    let fresh = r#"{"distinct_sources":2,"recent_3d":4,"total_14d":4,"newest_age_hours":1.0,
        "tier_weight":0.5,"volume":0.4,"recency":0.986,"recent_frac":1.0}"#;
    let aged = r#"{"distinct_sources":2,"recent_3d":1,"total_14d":4,"newest_age_hours":26.4,
        "tier_weight":0.5,"volume":0.4,"recency":0.693,"recent_frac":0.25}"#;
    let a = hash_components(&build_transfer_input_components(
        &[11, 3, 8, 5],
        fresh,
        "none",
    ));
    let b = hash_components(&build_transfer_input_components(
        &[3, 5, 8, 11],
        aged,
        "none",
    ));
    assert_eq!(a, b);
}

#[test]
fn transfer_input_hash_moves_on_material_change() {
    let comps = r#"{"distinct_sources":2,"tier_weight":0.5}"#;
    let base = hash_components(&build_transfer_input_components(&[3, 5], comps, "none"));
    // New article in the pair corpus.
    let grown = hash_components(&build_transfer_input_components(&[3, 5, 9], comps, "none"));
    // Deterministic relationship flip (identity applied / provider update).
    let former = hash_components(&build_transfer_input_components(&[3, 5], comps, "former"));
    // Source-tier changes are deliberately ignored; source memory is organic, not static tiering.
    let retiered = hash_components(&build_transfer_input_components(
        &[3, 5],
        r#"{"distinct_sources":2,"tier_weight":0.9}"#,
        "none",
    ));
    assert_ne!(base, grown);
    assert_ne!(base, former);
    assert_eq!(base, retiered);
}

#[test]
fn unknown_marker_keeps_direction_drops_model() {
    // verdict == None → is_rumor NULL, direction kept (audit), model NULL. Mirrors Go persist.
    let (row, outcome) = row_from_verdict(None, "current", Some("BBC"), "mistral:7b");
    assert_eq!(outcome, Outcome::Unknown);
    assert_eq!(row.is_rumor, None);
    assert_eq!(row.direction.as_deref(), Some("outgoing"));
    assert_eq!(row.model, None);
    assert_eq!(row.attribution.as_deref(), Some("BBC"));
    assert_eq!(row.trigger_payload, "{}");
}

#[test]
fn cleared_row_drops_direction_keeps_model_and_subject() {
    let v = TransferVerdict {
        is_rumor: Some(false),
        subject: "Florentino Pérez".to_string(),
        ..Default::default()
    };
    let (row, outcome) = row_from_verdict(Some(&v), "none", None, "mistral:7b");
    assert_eq!(outcome, Outcome::Cleared);
    assert_eq!(row.is_rumor, Some(false));
    assert_eq!(row.direction, None); // cleared keeps no direction (mirrors Go)
    assert_eq!(row.model.as_deref(), Some("mistral:7b"));
    assert_eq!(row.stage, None);
    assert!(row.trigger_payload.contains("Florentino"));
}

#[test]
fn rumor_row_sets_all_vetted_fields() {
    let v = TransferVerdict {
        is_rumor: Some(true),
        subject: "Darwin Nunez".to_string(),
        stage: "advanced talks".to_string(), // normalized → advanced_talks
        summary: "  Liverpool eye Nunez  ".to_string(), // trimmed
        confidence: 1.4,                     // clamped → 1.0
        ..Default::default()
    };
    let (row, outcome) = row_from_verdict(Some(&v), "current", Some("Fabrizio"), "mistral:7b");
    assert_eq!(outcome, Outcome::Rumor);
    assert_eq!(row.is_rumor, Some(true));
    assert_eq!(row.direction.as_deref(), Some("outgoing"));
    assert_eq!(row.stage.as_deref(), Some("advanced_talks"));
    assert_eq!(row.summary.as_deref(), Some("Liverpool eye Nunez"));
    assert_eq!(row.confidence, Some(1.0));
    assert_eq!(row.model.as_deref(), Some("mistral:7b"));
}

#[test]
fn t6_system_prompt_carries_the_false_heat_guards() {
    // The false-heat guards must be present and noun-correct.
    let football = transfer_system_prompt("FOOTBALL");
    assert!(football.contains("current transfer involving BOTH"));
    assert!(football.contains("roundup"));
    assert!(football.contains("recently completed"));
    assert!(football.contains("Never estimate, round, or invent money"));
    let nba = transfer_system_prompt("NBA");
    assert!(nba.contains("current trade involving BOTH")); // noun swap for NBA/NFL
}

#[test]
fn identity_adjudication_parser_accepts_strict_apply_json() {
    let raw = r#"{
        "decision":"apply",
        "event_type":"transfer",
        "old_team_id":18,
        "new_team_id":42,
        "reason":"club announced the signing",
        "evidence_spans":["announced the signing"]
    }"#;
    let adj = TransferIdentityAdjudicationParser
        .parse(raw)
        .unwrap()
        .expect("strict JSON parses");
    assert_eq!(adj.decision, "apply");
    assert_eq!(adj.confidence, None);
    assert_eq!(adj.old_team_id, Some(18));
    assert_eq!(adj.new_team_id, 42);
}

#[test]
fn identity_adjudication_parser_fails_closed_on_invalid_json_or_schema() {
    assert!(TransferIdentityAdjudicationParser
        .parse("not json")
        .unwrap()
        .is_none());
    assert!(TransferIdentityAdjudicationParser
        .parse(r#"{"decision":"apply"}"#)
        .unwrap()
        .is_none());
    assert!(TransferIdentityAdjudicationParser
        .parse(
            r#"{"decision":"apply","event_type":"transfer","confidence":1.4,"old_team_id":1,"new_team_id":2,"reason":"","evidence_spans":[]}"#
        )
        .unwrap()
        .is_none());
    assert!(TransferIdentityAdjudicationParser
        .parse(
            r#"{"decision":"maybe","event_type":"transfer","confidence":0.9,"old_team_id":1,"new_team_id":2,"reason":"","evidence_spans":[]}"#
        )
        .unwrap()
        .is_none());
    assert!(TransferIdentityAdjudicationParser
        .parse(
            r#"{"decision":"manual_review","event_type":"transfer","old_team_id":1,"new_team_id":2,"reason":"","evidence_spans":[]}"#
        )
        .unwrap()
        .is_none());
}

#[test]
fn identity_adjudication_prompt_pins_candidate_ids() {
    let prompt = build_transfer_identity_adjudication_prompt(
        "FOOTBALL",
        7,
        "Example Player",
        Some(18),
        "Old FC",
        42,
        "New FC",
        &[NewsItem {
            id: 1,
            title: "New FC announce Example Player".to_string(),
            description: "The club confirmed the transfer.".to_string(),
            source: "BBC".to_string(),
        }],
    );
    assert!(prompt.contains("Current identity: team_id=18 team_name=Old FC"));
    assert!(prompt.contains("Proposed new identity: team_id=42 team_name=New FC"));
    assert!(prompt.contains("Decide only from the evidence articles"));
    assert!(!prompt.contains("heat"));
    assert!(!prompt.contains("Vetted summary"));
}

// --- Self-pacing against the worker's per-item ceiling ---------------------------------------

/// The eval and one-shot binaries hand this junction a zero budget, and zero must mean unbounded
/// rather than "already expired" — a deadline of `start + 0` would abandon a team before its first
/// pair and make every inspection run useless.
#[test]
fn a_zero_budget_is_unbounded_not_already_expired() {
    let start = Instant::now();
    assert!(budget_deadline(start, Duration::ZERO, TRANSFER_PAIR_BUDGET_FRAC).is_none());
    assert!(budget_deadline(start, Duration::ZERO, TRANSFER_WRAP_BUDGET_FRAC).is_none());
    assert!(!past(None), "an absent deadline must never read as past");
}

/// The whole point of the split: the wire wrap runs last, so it only ever gets budget if the pair
/// loop is made to stop short of the ceiling. This pins the ordering that guarantees it —
/// pairs < wrap < the ceiling itself, with headroom left under the axe for the bookkeeping.
#[test]
fn the_wrap_is_guaranteed_a_share_of_the_budget_below_the_ceiling() {
    let budget = Duration::from_secs(1200); // the production COGNITION_HANDLER_TIMEOUT_SECONDS
    let start = Instant::now();
    let pairs = budget_deadline(start, budget, TRANSFER_PAIR_BUDGET_FRAC).unwrap();
    let wrap = budget_deadline(start, budget, TRANSFER_WRAP_BUDGET_FRAC).unwrap();

    assert!(pairs < wrap, "the pair loop must stop before the wrap does");
    assert!(
        wrap < start + budget,
        "the wrap must stop before the worker's ceiling, not at it"
    );
    assert!(
        wrap - pairs >= Duration::from_secs(300),
        "the wrap needs a real share, not a rounding error"
    );
    assert!(
        (start + budget) - wrap >= Duration::from_secs(120),
        "leave headroom under the ceiling for the autofill refresh and bookkeeping"
    );
}

/// A deadline that has arrived reads as past; one still ahead does not. The loops check this
/// *before* each unit of work, so an off-by-one here is the difference between deferring the tail
/// and being cancelled mid-generation.
#[test]
fn past_fires_only_once_the_deadline_has_arrived() {
    let now = Instant::now();
    assert!(past(Some(now - Duration::from_millis(1))));
    assert!(!past(Some(now + Duration::from_secs(30))));
}

#[test]
fn transfer_identity_adjudication_schema_matches_parser_contract() {
    // The raw literal must be valid JSON, and its required list must cover every key the
    // parser demands — a schema-approved reply must never fail the parser's key check.
    let schema: serde_json::Value =
        serde_json::from_str(TRANSFER_IDENTITY_ADJUDICATION_SCHEMA_RAW).expect("valid JSON");
    let required: Vec<&str> = schema["required"]
        .as_array()
        .expect("required array")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for key in [
        "decision",
        "event_type",
        "old_team_id",
        "new_team_id",
        "reason",
        "evidence_spans",
    ] {
        assert!(required.contains(&key), "schema must require {key}");
    }
    // The enums the parser re-checks are the same sets the grammar enforces.
    assert_eq!(
        schema["properties"]["decision"]["enum"],
        serde_json::json!(["apply", "reject"])
    );
    assert_eq!(
        schema["properties"]["event_type"]["enum"],
        serde_json::json!([
            "transfer",
            "trade",
            "loan",
            "signing",
            "extension",
            "rumor",
            "false_positive"
        ])
    );
}

#[test]
fn transfer_identity_adjudication_parser_accepts_schema_shaped_reply() {
    let reply = r#"{"decision":"apply","event_type":"transfer","old_team_id":null,
        "new_team_id":18,"reason":"official club statement","evidence_spans":["signed"]}"#;
    let parsed = TransferIdentityAdjudicationParser
        .parse(reply)
        .expect("no parser error")
        .expect("schema-shaped reply parses");
    assert_eq!(parsed.decision, "apply");
    assert_eq!(parsed.new_team_id, 18);
    assert_eq!(parsed.old_team_id, None);
}
