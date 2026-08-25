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
        published_at_epoch: epoch,
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
fn prompt_numbered_news() {
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

// --- description corpus seam -------------------------------------------------------------------

#[test]
fn article_context_renders_a_description_that_adds_content() {
    let none = item(
        1,
        "BBC",
        "Saka shines again",
        "A strong display in the win.",
        None,
    );
    assert_eq!(article_context(&none).0, "A strong display in the win.");

    let p = build_narratives_prompt(
        &req("Bukayo Saka", "FOOTBALL", "player"),
        &[none],
        None,
        None,
        None,
    );
    assert!(p.contains("A strong display in the win."));
}

// --- input components: the debounce pre-image ---------------------------------------------------

#[test]
fn input_components_are_stable_across_input_order() {
    // Same articles in a different order ⇒ identical pre-image (sorted). n17: transfer heat is
    // no longer an input, so no heat term exists to enter it — heat movement alone must NOT
    // regenerate narratives (the separation pass).
    let a = |id: i64| item(id, "ESPN", "t", "", None);
    let one = build_narratives_input_components(&[a(3), a(1)]);
    let two = build_narratives_input_components(&[a(1), a(3)]);
    assert_eq!(one, two);
    // The readings term is a constant per article now (see READING_FINGERPRINT_NONE) — carried
    // byte-for-byte so the field strip cost zero regens. This pin is what "byte-for-byte" means.
    let article_readings_hash = build_article_reading_input_components(&[
        (1, READING_FINGERPRINT_NONE.to_string()),
        (3, READING_FINGERPRINT_NONE.to_string()),
    ]);
    // prompt_version leads the pre-image (single-sourced from the const, so a bump can't silently
    // rot this pin) — an n-bump changes every entity's hash once, forcing the cutover regen.
    assert_eq!(
        one,
        format!(
            r#"{{"prompt_version":"{NARRATIVES_PROMPT_VERSION}","article_ids":[1,3],"article_readings_hash":"{article_readings_hash}"}}"#
        )
    );
    assert_eq!(READING_FINGERPRINT_NONE, "none::0");
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
        json!(["narratives", "headline", "card_score"]),
        "verdict lands last (sigil doctrine: signs first, the hook second, verdict third)"
    );
    assert_eq!(schema["properties"]["card_score"]["minimum"], json!(1));
    assert_eq!(schema["properties"]["card_score"]["maximum"], json!(99));
}

#[test]
fn headline_parses_best_effort_and_takes_the_title_floor() {
    // The entity-level hook (mig 232): present → settled through guards::settle_title.
    let parsed = NarrativesParser
        .parse(r#"{"narratives": [], "headline": "A quiet week around the Bridge", "card_score": 12}"#)
        .unwrap()
        .unwrap();
    assert_eq!(parsed.headline(), Some("A quiet week around the Bridge"));
    // A pre-headline reply still parses — the hook is simply absent (the card_score pattern).
    let old = NarrativesParser
        .parse(r#"{"narratives": [], "card_score": 12}"#)
        .unwrap()
        .unwrap();
    assert_eq!(old.headline(), None);
    // A junk title costs the title, never the edition: >140 chars with no beat drops to None.
    let overlong = format!(
        r#"{{"narratives": [], "headline": "{}", "card_score": 12}}"#,
        "x".repeat(200)
    );
    let dropped = NarrativesParser.parse(&overlong).unwrap().unwrap();
    assert_eq!(dropped.headline(), None);
    assert_eq!(dropped.card_score(), Some(12));
    // The raw-scan fallback holds when prose wraps the object (the salvager's territory).
    let wrapped = NarrativesParser
        .parse(r#"Here you go: {"narratives": [], "headline": "Deadline day finds the back door", "card_score": 70} done"#)
        .unwrap()
        .unwrap();
    assert_eq!(wrapped.headline(), Some("Deadline day finds the back door"));
}

#[test]
fn prompt_score_context_renders_last_before_reply_instruction() {
    let news = vec![item(1, "BBC", "Saka shines again", "", None)];
    let p = build_narratives_prompt(
        &req("Bukayo Saka", "FOOTBALL", "player"),
        &news,
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
    let legacy = build_narratives_prompt(&entity, &news, None, None, None);
    // An empty or whitespace framing must be indistinguishable from no framing: a packet with
    // nothing to frame must not leave a dangling header in the prompt.
    for empty in ["", "   ", "\n"] {
        assert_eq!(
            legacy,
            build_narratives_prompt(&entity, &news, None, None, Some(empty)),
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

/// The WINDOW decides the reservation: a 4,000-token reservation inside a 4,096 window leaves
/// nothing for the prompt. §7's envelope was 4096 with a 700 reservation; the MLX cutover
/// (n21, 2026-08-19) raised the packet share to 900 — the openai path has no grammar, so the
/// edition pays JSON structural overhead the constrained path never did. A pinned-large window
/// (`VOICE_NUM_CTX=16384`) gets the large reservation back.
#[test]
fn decode_budget_follows_the_window() {
    assert_eq!(
        narratives_decode_budget(16384),
        (16384, NARRATIVES_NUM_PREDICT)
    );
    assert_eq!(narratives_decode_budget(crate::route::VOICE_NUM_CTX_PACKET), (4096, 900));
    // The prompt budget must still clear the p99 prompt envelope — and on MLX the binding
    // ceiling is the ~4k PROMPT boundary (the ministral3 mask crash), which ctx−predict
    // keeps prompts safely under. 4096−900 = 3196 ≥ the measured ~3.1k p99.
    let (ctx, predict) = narratives_decode_budget(crate::route::VOICE_NUM_CTX_PACKET);
    assert!(predict <= 1_000);
    assert!(ctx - predict >= 3_100, "no room for the p99 prompt envelope");
}

/// 7.9: the packet render replaces the CORPUS, never the memory. A packet-rail prompt still
/// carries the relational memory card and the prior-card-reads block, with their provenance
/// labels intact — and the memory still contributes nothing to the debounce hash, because
/// `build_narratives_input_components` takes only the corpus and the heat.
#[test]
fn the_packet_rail_keeps_the_memory_block() {
    let news = vec![item(10, "ESPN", "Arsenal agreed personal terms", "", None)];
    let entity = req("Vinicius Junior", "FOOTBALL", "player");
    let memory = "Prior story: Arsenal — fizzled (Jun 2026, peak coverage 82/100).";
    let framing = "STORY: Vinicius Junior and Arsenal\nENTITY: Vinicius Junior (subject)";
    let p = build_narratives_prompt(
        &entity,
        &news,
        Some(memory),
        Some("SIGNALS (deterministic tally for your card score): 1 article(s) after dedup"),
        Some(framing),
    );
    assert!(p.contains("Relational memory (computed history"), "memory label intact");
    assert!(p.contains("- Prior story: Arsenal — fizzled"));
    assert!(p.contains("SIGNALS (deterministic tally"));
    assert!(p.contains("The story so far"));

    // The debounce hash is blind to memory and to the framing by construction — it is computed
    // from the material fact (what evidence exists) alone, on both rails.
    let with = build_narratives_input_components(&news);
    assert!(!with.contains("Prior story"));
    assert!(!with.contains("STORY:"));
}

// --- n20: the news-block char budget ----------------------------------------------------------

#[test]
fn news_budget_keeps_the_newest_and_names_the_cut() {
    // Mega-storyline shape (measured 2026-08-15: ~160 items, 63 KB): items are
    // newest-first, so the budget must keep the head and NAME the tail.
    let corpus: Vec<CorpusItem> = (0..40)
        .map(|i| {
            item(
                i,
                "Fixture Wire",
                &format!("Claim number {i} carrying a headline of realistic length for a claim"),
                "Second and third claims joined here as the body of the numbered evidence item.",
                Some(1_700_000_000),
            )
        })
        .collect();

    let (kept, dropped) = apply_news_budget(corpus, 2_000);

    assert!(!kept.is_empty(), "the budget must never empty the corpus");
    assert!(!dropped.is_empty(), "40 long items cannot fit 2,000 chars");
    assert_eq!(kept.len() + dropped.len(), 40);
    // Prefix kept, in order; the drop is exactly the tail ids.
    assert_eq!(kept[0].id, 0);
    assert_eq!(dropped[0], kept.len() as i64);
}

#[test]
fn news_budget_always_keeps_at_least_one_item() {
    // One oversized item still renders — an edition with evidence beats an empty prompt,
    // and build_narratives_prompt caps the rendered body anyway.
    let corpus = vec![item(7, "Wire", &"t".repeat(500), &"d".repeat(5_000), None)];
    let (kept, dropped) = apply_news_budget(corpus, 100);
    assert_eq!(kept.len(), 1);
    assert!(dropped.is_empty());
}
