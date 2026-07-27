//! Unit tests for this junction.
//!
//! Split out of `mod.rs` so the stage module reads as the stage and nothing else.
//! `super` still resolves to the junction, so these run exactly as they did inline.

use super::*;

#[test]
fn clean_html_removes_tags_scripts_and_normalizes_space() {
    let html = "<html><script>bad()</script><body><h1>Title</h1><p>A&nbsp;B &amp; C.</p></body></html>";
    assert_eq!(clean_html(html), "Title A B & C.");
}

#[test]
fn google_news_article_id_extracts_rss_token() {
    let url = "https://news.google.com/rss/articles/CBMiabc123?oc=5&hl=en-US";
    assert_eq!(google_news_article_id(url).as_deref(), Some("CBMiabc123"));
    assert!(google_news_article_id("https://example.com/rss/articles/CBMiabc123").is_none());
}

#[test]
fn html_attr_extracts_google_news_tokens() {
    let html =
        r#"<div data-n-a-id="CBMiabc" data-n-a-ts="1784915408" data-n-a-sg="A&amp;B"></div>"#;
    assert_eq!(html_attr(html, "data-n-a-id").as_deref(), Some("CBMiabc"));
    assert_eq!(
        html_attr(html, "data-n-a-ts").as_deref(),
        Some("1784915408")
    );
    assert_eq!(html_attr(html, "data-n-a-sg").as_deref(), Some("A&B"));
}

#[test]
fn google_news_resolver_response_extracts_publisher_url() {
    let body = r#")]}'

[["wrb.fr","Fbv4je","[\"garturlres\",\"https://www.goal.com/en/news/example\",1]",null,null,null,"generic"],["di",23]]"#;
    assert_eq!(
        parse_google_news_resolver_response(body).as_deref(),
        Some("https://www.goal.com/en/news/example")
    );
}

#[test]
fn parser_accepts_compact_evidence_card() {
    let raw = r#"{"source_language":"DE","evidence_blurb":"  Player X returned to training.  ","key_facts":[" one ",""],"relevant_entities":["Club"],"co_mentions":[],"story_type":" injury ","caveats":""}"#;
    let parsed = ArticleEvidenceParser { vetted: &vetted() }.parse(raw).unwrap().unwrap();
    assert_eq!(parsed.source_language, "de");
    assert_eq!(parsed.evidence_blurb, "Player X returned to training.");
    assert_eq!(parsed.key_facts, vec!["one"]);
    assert_eq!(parsed.story_type, "injury");
}

#[test]
fn parser_accepts_co_mention_verdicts() {
    let raw = r#"{"relevant":true,"source_language":"en","evidence_blurb":"A filed item.","key_facts":[],"relevant_entities":["Club"],"co_mentions":[{"candidate":2,"relevant":true},{"candidate":0,"relevant":true},{"candidate":3,"relevant":false}],"story_type":"general","caveats":""}"#;
    let parsed = ArticleEvidenceParser { vetted: &vetted() }.parse(raw).unwrap().unwrap();
    assert_eq!(parsed.co_mentions.len(), 2);
    assert_eq!(parsed.co_mentions[0].candidate, 2);
    assert!(parsed.co_mentions[0].relevant);
    assert_eq!(parsed.co_mentions[1].candidate, 3);
    assert!(!parsed.co_mentions[1].relevant);
}

/// An out-of-range candidate index costs its own verdict and nothing else.
///
/// The literal below is the reply that broke production on 2026-07-26: gemma3:4b answered
/// `"candidate": 2080781384616956`, which does not fit the `i32` the field used to be, so serde
/// failed the whole `ArticleEvidence` parse and took a valid reading with it. Because the defect
/// lives in the reply's shape, the retry reproduced it every time — `article_read` entity 173300
/// re-failed on a 30-minute backoff indefinitely. What must hold is that the article still parses
/// and the surviving co-mentions are intact.
#[test]
fn parser_survives_an_unrepresentable_candidate_index() {
    let raw = r#"{"source_language":"en","evidence_blurb":"A filed item.","key_facts":["one"],"relevant_entities":["Club"],"co_mentions":[{"candidate":2080781384616956,"relevant":true},{"candidate":3,"relevant":true}],"story_type":"general","caveats":""}"#;
    let parsed = ArticleEvidenceParser { vetted: &vetted() }.parse(raw).unwrap().unwrap();
    // The junk index is zeroed and dropped by the `candidate > 0` filter; 3 is untouched.
    assert_eq!(parsed.co_mentions.len(), 1);
    assert_eq!(parsed.co_mentions[0].candidate, 3);
    assert_eq!(parsed.evidence_blurb, "A filed item.");
}

/// Models write `"3"` for 3. That is the same index, so it must not be discarded as noise.
#[test]
fn parser_accepts_a_numeric_string_candidate_index() {
    let raw = r#"{"source_language":"en","evidence_blurb":"A filed item.","key_facts":["one"],"relevant_entities":["Club"],"co_mentions":[{"candidate":"4","relevant":true}],"story_type":"general","caveats":""}"#;
    let parsed = ArticleEvidenceParser { vetted: &vetted() }.parse(raw).unwrap().unwrap();
    assert_eq!(parsed.co_mentions.len(), 1);
    assert_eq!(parsed.co_mentions[0].candidate, 4);
}

/// The vetted entity list these tests score against. Only these names get a vote in
/// `derive_relevance`, which is the whole point — the model volunteers others.
fn vetted() -> Vec<String> {
    ["West Ham United", "Baltimore Ravens", "Norwich City", "Aston Villa", "X"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// One ar6 reply. Note there is no `relevant` key — the model is never asked.
fn ar6_raw(page_kind: &str, roles: &[(&str, &str)]) -> String {
    let roles: Vec<_> = roles
        .iter()
        .map(|(e, r)| serde_json::json!({"entity": e, "role": r}))
        .collect();
    serde_json::json!({
        "source_language": "en",
        "page_kind": page_kind,
        "entity_roles": roles,
        "story_type": "general",
        "key_facts": ["a fact"],
        "relevant_entities": [],
        "co_mentions": [],
        "caveats": "",
        "evidence_blurb": "A blurb."
    })
    .to_string()
}

#[test]
fn a_non_reporting_page_shape_is_rejected() {
    // The measured failure: gemma will not say no. Under ar6 it is not asked to — a score table
    // is rejected on its SHAPE, even though the entity is genuinely named all over it.
    let parsed = ArticleEvidenceParser { vetted: &vetted() }
        .parse(&ar6_raw("score_table", &[("West Ham United", "subject")]))
        .unwrap()
        .unwrap();
    assert!(!parsed.relevant, "score_table is not reporting");
}

#[test]
fn an_opponent_only_story_is_rejected_by_role() {
    // The gap no page-shape signal could ever reach: real prose reporting, but the vetted entity
    // is only who the subject plays against.
    let parsed = ArticleEvidenceParser { vetted: &vetted() }
        .parse(&ar6_raw("article", &[("West Ham United", "opponent")]))
        .unwrap()
        .unwrap();
    assert!(!parsed.relevant);
}

#[test]
fn a_name_collision_is_rejected_as_absent() {
    let parsed = ArticleEvidenceParser { vetted: &vetted() }
        .parse(&ar6_raw("article", &[("Baltimore Ravens", "absent")]))
        .unwrap()
        .unwrap();
    assert!(!parsed.relevant);
}

#[test]
fn one_subject_among_several_entities_is_enough() {
    let parsed = ArticleEvidenceParser { vetted: &vetted() }
        .parse(&ar6_raw(
            "article",
            &[("Norwich City", "passing_mention"), ("Aston Villa", "subject")],
        ))
        .unwrap()
        .unwrap();
    assert!(parsed.relevant);
}

#[test]
fn empty_entity_roles_is_unknown_not_rejection() {
    // Rejection clears the article's vetted links, so a degenerate reply with NO labels at all
    // must not trigger it. Page shape still applies on its own.
    assert!(derive_relevance("article", &[], &vetted()));
    assert!(!derive_relevance("listing_or_schedule", &[], &vetted()));
    // Nor may an empty vetted list reject everything.
    assert!(derive_relevance("article", &[], &[]));
}

#[test]
fn omitting_the_vetted_entity_from_a_populated_list_rejects() {
    // THE opponent-only failure, measured verbatim: the model labels the two people it found in
    // the body as `subject` and leaves the vetted entity out entirely. It labels the entity
    // whenever the story is about it, so a populated list that skips it is evidence of absence.
    let raw = ar6_raw(
        "article",
        &[
            ("Vanja Dragojevic", "subject"),
            ("Philippe Clement", "subject"),
        ],
    );
    let parsed = ArticleEvidenceParser { vetted: &vetted() }
        .parse(&raw)
        .unwrap()
        .unwrap();
    assert!(
        !parsed.relevant,
        "non-vetted subjects must not carry an article the vetted entity is absent from"
    );
}

#[test]
fn role_and_page_kind_matching_is_case_insensitive() {
    let parsed = ArticleEvidenceParser { vetted: &vetted() }
        .parse(&ar6_raw("Score_Table", &[("X", "Subject")]))
        .unwrap()
        .unwrap();
    assert!(!parsed.relevant);
}

#[test]
fn parser_fails_closed_on_empty_blurb() {
    let parsed = ArticleEvidenceParser { vetted: &vetted() }
        .parse(r#"{"evidence_blurb":" ","key_facts":[],"relevant_entities":[],"story_type":"general","caveats":""}"#)
        .unwrap();
    assert!(parsed.is_none());
}

#[test]
fn parser_accepts_irrelevant_without_blurb() {
    // ar6: the rejection comes from the page SHAPE, not from a `relevant` key the model no longer
    // emits. A rejected read still gets the fallback blurb rather than failing closed to None.
    let parsed = ArticleEvidenceParser { vetted: &vetted() }
        .parse(r#"{"page_kind":"listing_or_schedule","entity_roles":[],"evidence_blurb":" ","key_facts":[],"relevant_entities":[],"story_type":"general","caveats":""}"#)
        .unwrap()
        .unwrap();
    assert!(!parsed.relevant);
    assert_eq!(
        parsed.evidence_blurb,
        "Full text is not materially about the vetted entities."
    );
}

#[test]
fn reading_fingerprint_distinguishes_body_changes() {
    assert_ne!(
        reading_fingerprint(Some("success"), Some("a"), Some(1)),
        reading_fingerprint(Some("success"), Some("b"), Some(1)),
    );
    assert_eq!(reading_fingerprint(None, None, None), "none::0");
}

#[test]
fn article_reading_input_hash_is_order_stable() {
    let a = build_article_reading_input_components(&[
        (2, "success:b:9".to_string()),
        (1, "none::0".to_string()),
    ]);
    let b = build_article_reading_input_components(&[
        (1, "none::0".to_string()),
        (2, "success:b:9".to_string()),
    ]);
    assert_eq!(a, b);
}

#[test]
fn prompt_renders_co_mention_candidates_by_number() {
    let article = ArticleRow {
        url: "https://example.test/a".to_string(),
        source: "Example".to_string(),
        title: "Club tracks midfielder".to_string(),
        description: String::new(),
        duplicate_of: None,
        vetted_count: 1,
    };
    let entities = ArticleReadEntities {
        vetted_names: vec!["Manchester United (team 14)".to_string()],
        co_mentions: vec![CoMentionCandidate {
            number: 1,
            entity_type: "player".to_string(),
            entity_id: 70,
            name: "Example Midfielder".to_string(),
            nationality: "England".to_string(),
            current_club: "Leeds".to_string(),
            position: "Midfielder".to_string(),
        }],
    };

    let prompt = build_article_read_prompt(&article, "The body text.", &entities);
    assert!(prompt.contains("Known vetted entities"));
    assert!(prompt.contains("Co-mention candidates"));
    assert!(prompt.contains("1. Example Midfielder (player 70, Midfielder, Leeds, England)"));
}
