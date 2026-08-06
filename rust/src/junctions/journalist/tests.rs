//! Unit tests for this junction.
//!
//! Split out of `mod.rs` so the stage module reads as the stage and nothing else.
//! `super` still resolves to the junction, so these run exactly as they did inline.

use super::*;

fn item(id: i64, source: &str, title: &str, desc: &str, epoch: Option<i64>) -> CorpusItem {
    CorpusItem {
        id,
        title: title.to_string(),
        description: desc.to_string(),
        source: source.to_string(),
        url: String::new(),
        published_at_epoch: epoch,
        fetched_at_epoch: epoch,
        full_text: None,
        article_read_blurb: None,
        article_read_status: None,
        article_read_content_hash: None,
        article_read_updated_epoch: None,
    }
}

fn req(name: &str, sport: &str, etype: &str) -> NarrativesReq {
    NarrativesReq {
        entity_type: etype.to_string(),
        entity_id: 1,
        entity_name: name.to_string(),
        sport: sport.to_string(),
        trigger_type: "periodic".to_string(),
    }
}

// --- build_narratives_prompt byte-fixtures: deterministic prompt assembly. ----------------------

#[test]
fn prompt_numbered_news_no_heat() {
    let news = vec![
        item(
            10,
            "BBC",
            "Saka shines again",
            "A strong display in the win.",
            None,
        ),
        item(11, "", "Arsenal eye a new winger", "", None),
    ];
    let p = build_narratives_prompt(
        &req("Bukayo Saka", "FOOTBALL", "player"),
        &news,
        &[],
        None,
        None,
        None,
    );
    assert!(
        !p.contains("Relational memory"),
        "no memory ⇒ no section (n7 byte-shape preserved)"
    );
    let with_mem = build_narratives_prompt(
        &req("Bukayo Saka", "FOOTBALL", "player"),
        &news,
        &[],
        Some("Prior story: Real Madrid — fizzled (Jun 2026, peak coverage 82/100).\nGround truth: Bukayo Saka completed a confirmed move to Arsenal on Jul 01 2026."),
        None,
        None,
    );
    assert!(with_mem.contains("Relational memory (computed history"));
    assert!(with_mem.contains("- Prior story: Real Madrid — fizzled"));
    assert!(with_mem.contains("- Ground truth: Bukayo Saka completed"));
    assert_eq!(
        p,
        "Entity: Bukayo Saka (FOOTBALL player)\n\
\nRecent news (numbered):\n\
1. [BBC] Saka shines again — A strong display in the win.\n\
2. Arsenal eye a new winger\n\
\nReturn the JSON object now."
    );
}

#[test]
fn prompt_with_heat_section() {
    let news = vec![item(20, "ESPN", "Trade buzz grows", "", None)];
    let heat = vec![
        HeatItem {
            counterparty: "Lakers".to_string(),
            heat: 80,
            stage: "advanced talks".to_string(),
            direction: "incoming".to_string(),
            summary: "Lakers in advanced talks per ESPN".to_string(),
            confidence: Some(0.8),
        },
        HeatItem {
            counterparty: "Heat".to_string(),
            heat: 40,
            stage: String::new(),
            direction: String::new(),
            summary: String::new(),
            confidence: None,
        },
    ];
    let p = build_narratives_prompt(&req("Some Team", "NBA", "team"), &news, &heat, None, None, None);
    assert_eq!(
        p,
        "Entity: Some Team (NBA team)\n\
\nRecent news (numbered):\n\
1. [ESPN] Trade buzz grows\n\
\nKnown transfer/trade activity (vetted facts — ground any transfer storyline in these, do not contradict them):\n\
- Lakers — heat 80, incoming, advanced talks (confidence 0.8) — \"Lakers in advanced talks per ESPN\"\n\
- Heat — heat 40\n\
\nReturn the JSON object now."
    );
}

// --- headline passthrough ----------------------------------------------------------------

#[test]
fn unread_article_travels_on_its_headline_alone() {
    // The real shape of a Google News RSS row: description is the headline restated with the
    // outlet glued on. The caller has already written "[Sky Sports] Arsenal sign ...", so
    // repeating it as the body would render the same sentence twice.
    let c = item(
        1,
        "Sky Sports",
        "Arsenal sign Greek winger Christos Tzolis",
        "Arsenal sign Greek winger Christos Tzolis Sky Sports",
        None,
    );
    assert_eq!(article_context(&c).0, "");
}

#[test]
fn description_with_real_content_survives() {
    // The 0.3% that carry an actual blurb must not be thrown away with the duplicates — one
    // word of new content is enough to keep it.
    let c = item(
        2,
        "BBC Sport",
        "Arsenal sign Tzolis",
        "The 23-year-old joins from Club Brugge for a fee of 35m euros.",
        None,
    );
    assert!(article_context(&c).0.contains("Club Brugge"));
}

#[test]
fn description_adds_nothing_is_token_based() {
    // Punctuation and case drift between title and description; containment, not equality.
    assert!(description_adds_nothing(
        "Arsenal sign Tzolis! - Sky Sports",
        "Arsenal sign Tzolis",
        "Sky Sports"
    ));
    assert!(description_adds_nothing("", "Any title", "Any source"));
    // A subset of the title is still nothing new.
    assert!(description_adds_nothing("Arsenal sign", "Arsenal sign Tzolis", ""));
    // One genuinely new token is enough to keep it.
    assert!(!description_adds_nothing(
        "Arsenal sign Tzolis for 35m",
        "Arsenal sign Tzolis",
        "Sky Sports"
    ));
}

// --- article_read/full_text corpus seam --------------------------------------------------------

#[test]
fn article_context_prefers_article_read_then_body_then_description() {
    let none = item(
        1,
        "BBC",
        "Saka shines again",
        "A strong display in the win.",
        None,
    );
    assert_eq!(article_context(&none).0, "A strong display in the win.");

    let mut read = item(4, "BBC", "Title", "short blurb", None);
    read.full_text = Some("The full article body with much more detail.".to_string());
    read.article_read_status = Some("success".to_string());
    read.article_read_blurb = Some(
        "Evidence card: Saka scored, Arsenal won, and Arteta discussed his workload."
            .to_string(),
    );
    assert_eq!(
        article_context(&read).0,
        "Evidence card: Saka scored, Arsenal won, and Arteta discussed his workload."
    );

    // Some(non-empty): the fetched body wins over the provider blurb when no reader card exists.
    let mut full = item(2, "BBC", "Title", "short blurb", None);
    full.full_text = Some("The full article body with much more detail.".to_string());
    assert_eq!(
        article_context(&full).0,
        "The full article body with much more detail."
    );

    // Some(blank): whitespace-only body is not a body — fall back to description.
    let mut blank = item(3, "BBC", "Title", "blurb here", None);
    blank.full_text = Some("   \n ".to_string());
    assert_eq!(article_context(&blank).0, "blurb here");

    let p = build_narratives_prompt(
        &req("Bukayo Saka", "FOOTBALL", "player"),
        &[read],
        &[],
        None,
        None,
        None,
    );
    assert!(p.contains("Evidence card: Saka scored"));
}

// --- input components: the debounce pre-image ---------------------------------------------------

#[test]
fn input_components_are_stable_across_input_order() {
    // Same articles + heat in a different order ⇒ identical pre-image (sorted), and the
    // heat SUMMARY/CONFIDENCE never enter it (derived commentary — a re-worded transfer
    // summary alone must not regenerate narratives).
    let a = |id: i64| item(id, "ESPN", "t", "", None);
    let h = |cp: &str, heat: i32, summary: &str| HeatItem {
        counterparty: cp.to_string(),
        heat,
        stage: "speculation".to_string(),
        direction: "incoming".to_string(),
        summary: summary.to_string(),
        confidence: Some(0.5),
    };
    let one = build_narratives_input_components(
        &[a(3), a(1)],
        &[h("B", 40, "worded one way"), h("A", 70, "x")],
    );
    let two = build_narratives_input_components(
        &[a(1), a(3)],
        &[h("A", 70, "y"), h("B", 40, "worded another way")],
    );
    assert_eq!(one, two);
    let article_readings_hash = crate::junctions::article_reader::build_article_reading_input_components(&[
        (1, "none::0".to_string()),
        (3, "none::0".to_string()),
    ]);
    // prompt_version leads the pre-image (single-sourced from the const, so a bump can't silently
    // rot this pin) — an n-bump changes every entity's hash once, forcing the cutover regen.
    assert_eq!(
        one,
        format!(
            r#"{{"prompt_version":"{NARRATIVES_PROMPT_VERSION}","article_ids":[1,3],"article_readings_hash":"{article_readings_hash}","transfer_heat":["A:70:incoming:speculation","B:40:incoming:speculation"]}}"#
        )
    );
    // No heat ⇒ no transfer_heat key (mirrors sigil's conditional-key convention).
    let single_article_readings_hash =
        crate::junctions::article_reader::build_article_reading_input_components(&[(
            1,
            "none::0".to_string(),
        )]);
    assert_eq!(
        build_narratives_input_components(&[a(1)], &[]),
        format!(
            r#"{{"prompt_version":"{NARRATIVES_PROMPT_VERSION}","article_ids":[1],"article_readings_hash":"{single_article_readings_hash}"}}"#
        )
    );

    let mut read = a(1);
    read.article_read_status = Some("success".to_string());
    read.article_read_content_hash = Some("body-a".to_string());
    read.article_read_updated_epoch = Some(10);
    assert_ne!(
        build_narratives_input_components(&[read], &[]),
        build_narratives_input_components(&[a(1)], &[])
    );
}

// --- parse_narratives: the tolerant salvager ---------------------------------------------------

#[test]
fn parse_clean_array() {
    let raw = r#"{"narratives": [{"title":"A","body":"b","articles":[1,2]},{"title":"C","body":"d","articles":[3]}]}"#;
    let (ns, ok) = parse_narratives(raw);
    assert!(ok);
    assert_eq!(ns.len(), 2);
    assert_eq!(ns[0].title, "A");
    assert_eq!(ns[0].articles, vec![1, 2]);
    assert_eq!(ns[1].articles, vec![3]);
}

#[test]
fn parse_empty_array_is_ok_no_narratives() {
    // A cleanly-closed empty array is a SUCCESSFUL parse with zero narratives → marker, not failure.
    let (ns, ok) = parse_narratives(r#"{"narratives": []}"#);
    assert!(ok);
    assert!(ns.is_empty());
}

#[test]
fn parse_truncated_tail_salvages_complete_objects() {
    // EOF before the array closes: keep the complete leading object, drop the half-written tail.
    let raw = r#"{"narratives": [{"title":"A","body":"b","articles":[1]},{"title":"C","body":"#;
    let (ns, ok) = parse_narratives(raw);
    assert!(ok); // salvaged ≥1
    assert_eq!(ns.len(), 1);
    assert_eq!(ns[0].title, "A");
}

#[test]
fn parse_missing_key_is_failure() {
    let (ns, ok) = parse_narratives(r#"{"something_else": 1}"#);
    assert!(!ok);
    assert!(ns.is_empty());
}

#[test]
fn parse_malformed_nothing_salvaged_is_failure() {
    // Has the key + '[' but the lone object never closes and nothing parses → failure (retry).
    let (ns, ok) = parse_narratives(r#"{"narratives": [{"title":"A"#);
    assert!(!ok);
    assert!(ns.is_empty());
}

#[test]
fn parse_respects_braces_inside_strings() {
    // A '}' inside a string value must not close the object early.
    let raw = r#"{"narratives": [{"title":"A } B","body":"x","articles":[1]}]}"#;
    let (ns, ok) = parse_narratives(raw);
    assert!(ok);
    assert_eq!(ns.len(), 1);
    assert_eq!(ns[0].title, "A } B");
}

// --- n12 card_score: schema, prompt section, tolerant parse -----------------------------------

#[test]
fn schema_requires_card_score_after_the_storylines() {
    let schema = narratives_format_schema();
    assert_eq!(
        schema["required"],
        json!(["narratives", "card_score"]),
        "verdict lands last (sigil doctrine: signs first, verdict second)"
    );
    assert_eq!(schema["properties"]["card_score"]["minimum"], json!(1));
    assert_eq!(schema["properties"]["card_score"]["maximum"], json!(99));
}

#[test]
fn prompt_score_context_renders_last_before_reply_instruction() {
    let news = vec![item(1, "BBC", "Saka shines again", "", None)];
    let p = build_narratives_prompt(
        &req("Bukayo Saka", "FOOTBALL", "player"),
        &news,
        &[],
        None,
        Some("SIGNALS (deterministic tally for your card score): 1 article(s) after dedup · 1 distinct source(s)\nYOUR PRIOR CARD READS (memory — your own previous card scores; continuity, not new evidence):\nCard scores (newest first): 58 (Jul 18) · 55 (Jul 12)"),
        None,
    );
    let signals = p.find("SIGNALS (deterministic").unwrap();
    let reply = p.find("\nReturn the JSON object now.").unwrap();
    assert!(
        signals < reply,
        "score context precedes the reply instruction"
    );
    assert!(p.contains("Card scores (newest first): 58 (Jul 18) · 55 (Jul 12)"));
    // None ⇒ byte-identical to the pre-n12 shape (the fixtures above pin it).
    let bare = build_narratives_prompt(
        &req("Bukayo Saka", "FOOTBALL", "player"),
        &news,
        &[],
        None,
        None,
        None,
    );
    assert!(!bare.contains("SIGNALS"));
}

#[test]
fn signals_line_counts_and_ages() {
    let news = vec![
        item(1, "BBC", "a", "", Some(10_000)),
        item(2, "BBC", "b", "", Some(6_000)),
        item(3, "ESPN", "c", "", None),
    ];
    // now = 10_000 + 6h → freshest 6h ago; BBC deduped to one source name.
    let line = render_signals_line(&news, 10_000 + 6 * 3600);
    assert_eq!(
        line,
        "SIGNALS (deterministic tally for your card score): 3 article(s) after dedup · 2 distinct source(s) · freshest 6h ago"
    );
    // ≥48h flips to days; no timestamps at all drops the age clause.
    let old = render_signals_line(&news, 10_000 + 72 * 3600);
    assert!(old.ends_with("freshest 3d ago"));
    let untimed = vec![item(4, "BBC", "d", "", None)];
    assert!(!render_signals_line(&untimed, 10_000).contains("freshest"));
}

#[test]
fn parse_card_score_tolerant_and_clamped() {
    assert_eq!(parse_card_score(r#"{"card_score": 58}"#), Some(58));
    assert_eq!(parse_card_score(r#"{"card_score": 250}"#), Some(99)); // clamp high
    assert_eq!(parse_card_score(r#"{"card_score": -3}"#), Some(1)); // clamp low
    assert_eq!(parse_card_score(r#"{"card_score": "72"}"#), Some(72)); // quoted
    assert_eq!(parse_card_score(r#"{"card_score": 63.7}"#), Some(64)); // fractional
    assert_eq!(parse_card_score(r#"{"narratives": []}"#), None); // pre-n12 reply
    assert_eq!(parse_card_score(r#"{"card_score": "high"}"#), None); // non-numeric
    assert_eq!(parse_card_score(r#"{"card_score":"#), None); // truncated at the value
}

#[test]
fn parser_carries_card_score_and_tolerates_absence() {
    // A pre-n16 reply still carrying the retired buckets section: the extra key is ignored, the
    // document parses. Offline bins replay real captured output, so tolerance here is not
    // hypothetical.
    let raw = r#"{"narratives": [{"title":"A","body":"b","articles":[1]}], "article_buckets": [{"article":1,"transfer":false}], "card_score": 41}"#;
    let doc = NarrativesParser.parse(raw).unwrap().unwrap();
    assert_eq!(doc.card_score, Some(41));
    // The model-called EMPTY document still lands its verdict — the quiet week gets the
    // Journalist's own low number (persisted on the marker row downstream).
    let quiet = NarrativesParser
        .parse(r#"{"narratives": [], "card_score": 4}"#)
        .unwrap()
        .unwrap();
    assert!(quiet.narratives.is_empty());
    assert_eq!(quiet.card_score, Some(4));
    // Pre-n12 shape: still a successful parse, score None (NULL row → Veil).
    let pre = NarrativesParser
        .parse(r#"{"narratives": []}"#)
        .unwrap()
        .unwrap();
    assert_eq!(pre.card_score, None);
}

// --- n16: the retired n9 buckets section -----------------------------------------------------

/// The Journalist no longer labels articles — The Editor writes `news_articles.bucket` from the
/// `story_type` it already emits. This is the regression guard for the reason it moved: the
/// section was one object per CORPUS article, so the output grew with the corpus rather than with
/// the story, on the saturated host, from a 900-byte blurb of a body the Editor had read in full.
#[test]
fn the_schema_no_longer_asks_the_journalist_to_label_articles() {
    let schema = narratives_format_schema();
    assert!(
        schema["properties"]["article_buckets"].is_null(),
        "article_buckets must not be back in the output contract"
    );
    assert!(
        !NARRATIVES_SYSTEM_PROMPT.contains("article_buckets"),
        "the prompt must not ask for a section the schema does not accept"
    );
    assert!(
        !NARRATIVES_SYSTEM_PROMPT.contains("label every numbered article"),
        "labelling is the Editor's job now; the Journalist voices the story"
    );
}

// --- ground_narratives: numbering, dedupe, bounds, drop-rules ---------------------------------

#[test]
fn ground_maps_numbers_dedupes_and_bounds() {
    let news = vec![
        item(100, "BBC", "one", "", Some(1_000)),
        item(101, "ESPN", "two", "", Some(2_000)),
    ];
    let parsed = vec![
        ModelNarrative {
            title: " Title ".to_string(), // trimmed
            body: "Body".to_string(),
            articles: vec![1, 1, 2, 9, 0, -3], // dup 1, out-of-range 9/0/-3 dropped
        },
        ModelNarrative {
            title: "".to_string(), // empty title → dropped
            body: "x".to_string(),
            articles: vec![1],
        },
        ModelNarrative {
            title: "no articles".to_string(),
            body: "y".to_string(),
            articles: vec![9, 0], // all out of range → ungrounded → dropped
        },
    ];
    let out = ground_narratives(&parsed, &news, 10_000);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].title, "Title");
    assert_eq!(out[0].input_news_ids, vec![100, 101]); // 1→id100, 2→id101, dup/oob removed
    assert_eq!(out[0].source_count, 2);
    assert_eq!(out[0].source_names, vec!["BBC", "ESPN"]);
    assert_eq!(out[0].source_latest_epoch, Some(2_000));
    assert_eq!(out[0].source_oldest_epoch, Some(1_000));
}

// --- compute_news_impact: the deterministic score ---------------------------------------------

#[test]
fn impact_volume_corroboration_recency() {
    // 2 articles, 2 distinct sources, newest 1h old (now=10000, newest=6400 → age 3600s ≤ 12h).
    let news = vec![
        item(1, "BBC", "a", "", Some(6_400)),
        item(2, "ESPN", "b", "", Some(5_000)),
    ];
    let (score, comp) = compute_news_impact(&news, 10_000);
    // volume = 60*(1-e^(-2/5)) ≈ 19.78 → round1 19.8; corroboration = min(25, 2*6)=12; recency 15.
    // score = round(19.78 + 12 + 15) = round(46.78) = 47.
    assert_eq!(score, 47);
    assert_eq!(comp["article_count"], json!(2));
    assert_eq!(comp["distinct_sources"], json!(2));
    assert_eq!(comp["corroboration"], json!(12.0));
    assert_eq!(comp["recency"], json!(15.0));
}

#[test]
fn impact_clamps_and_buckets_recency() {
    // No publish times → recency 0; one source → corroboration capped low.
    let news = vec![item(1, "src", "a", "", None)];
    let (score, comp) = compute_news_impact(&news, 10_000);
    // volume = 60*(1-e^-0.2) ≈ 10.88; corroboration 6; recency 0 → round(16.88)=17.
    assert_eq!(score, 17);
    assert_eq!(comp["recency"], json!(0.0));
}

#[test]
fn impact_recency_buckets() {
    let day = 24 * 3600;
    // newest 30h old → falls in the ≤48h bucket → recency 5.
    let news = vec![item(1, "s", "a", "", Some(0))];
    let (_, comp) = compute_news_impact(&news, 30 * 3600);
    assert_eq!(comp["recency"], json!(5.0));
    // newest 20h old → ≤24h → recency 10.
    let (_, comp2) = compute_news_impact(&news, 20 * 3600);
    assert_eq!(comp2["recency"], json!(10.0));
    // 3 days old → no bucket → 0.
    let (_, comp3) = compute_news_impact(&news, 3 * day);
    assert_eq!(comp3["recency"], json!(0.0));
}

// --- 7.3: the packet rail's prompt seam ---------------------------------------------------------

/// The law the whole phase rests on: under `RAIL=legacy` nothing about the prompt changes. The
/// packet framing is the ONLY new byte, and `None` means it contributes none of them — so a
/// legacy-rail deploy carrying all of Phase 7 sends exactly what the Phase 6 binary sent.
#[test]
fn legacy_rail_prompt_is_byte_identical_to_the_no_framing_prompt() {
    let news = vec![item(10, "BBC", "Saka shines again", "A strong display.", None)];
    let entity = req("Bukayo Saka", "FOOTBALL", "player");
    let legacy = build_narratives_prompt(&entity, &news, &[], None, None, None);
    // An empty or whitespace framing must be indistinguishable from no framing: a packet with
    // nothing to frame must not leave a dangling header in the prompt.
    for empty in ["", "   ", "\n"] {
        assert_eq!(
            legacy,
            build_narratives_prompt(&entity, &news, &[], None, None, Some(empty)),
            "an empty framing block changed the prompt"
        );
    }
    assert!(!legacy.contains("The story so far"));
}

/// On the packet rail the framing lands ABOVE the numbered evidence — the story first, then what
/// each source said about it — and the numbered list is untouched, because grounding still maps
/// those numbers to real article ids.
#[test]
fn packet_framing_precedes_the_numbered_evidence() {
    let news = vec![item(10, "Football365", "Arsenal agreed personal terms", "", None)];
    let p = build_narratives_prompt(
        &req("Vinicius Junior", "FOOTBALL", "player"),
        &news,
        &[],
        None,
        None,
        Some("STORY: Vinicius Junior and Arsenal: where the deal stands\nENTITY: Vinicius Junior (subject) — in this story 2026-08-02 → 2026-08-05\nPREVIOUSLY: Arsenal open talks for Vinicius"),
    );
    let framing = p.find("The story so far").expect("framing block present");
    let news_block = p.find("Recent news (numbered):").expect("evidence present");
    assert!(framing < news_block, "the story frames the evidence, not the reverse");
    assert!(p.contains("PREVIOUSLY: Arsenal open talks"));
    assert!(p.contains("1. [Football365] Arsenal agreed personal terms"));
}

/// The rail decides the window AND the reservation together, and the legacy pair is the one that
/// ships until Phase 8 (§7's envelope: 4096 with a 700 reservation on the packet rail).
#[test]
fn decode_budget_is_rail_scoped() {
    use crate::config::Rail;
    assert_eq!(
        narratives_decode_budget(Rail::Legacy),
        (NARRATIVES_NUM_CTX, NARRATIVES_NUM_PREDICT)
    );
    assert_eq!(narratives_decode_budget(Rail::Packet), (4096, 700));
    // The packet reservation must fit §7's ≤800 share, and the prompt budget must leave room for
    // it: a window that cannot hold its own reservation is the silent-eviction bug.
    let (ctx, predict) = narratives_decode_budget(Rail::Packet);
    assert!(predict <= 800);
    assert!(ctx - predict >= 3_300, "no room for the p99 prompt envelope");
}
