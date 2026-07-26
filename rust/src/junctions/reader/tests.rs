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
    let parsed = ArticleEvidenceParser.parse(raw).unwrap().unwrap();
    assert_eq!(parsed.source_language, "de");
    assert_eq!(parsed.evidence_blurb, "Player X returned to training.");
    assert_eq!(parsed.key_facts, vec!["one"]);
    assert_eq!(parsed.story_type, "injury");
}

#[test]
fn parser_accepts_co_mention_verdicts() {
    let raw = r#"{"relevant":true,"source_language":"en","evidence_blurb":"A filed item.","key_facts":[],"relevant_entities":["Club"],"co_mentions":[{"candidate":2,"relevant":true},{"candidate":0,"relevant":true},{"candidate":3,"relevant":false}],"story_type":"general","caveats":""}"#;
    let parsed = ArticleEvidenceParser.parse(raw).unwrap().unwrap();
    assert_eq!(parsed.co_mentions.len(), 2);
    assert_eq!(parsed.co_mentions[0].candidate, 2);
    assert!(parsed.co_mentions[0].relevant);
    assert_eq!(parsed.co_mentions[1].candidate, 3);
    assert!(!parsed.co_mentions[1].relevant);
}

#[test]
fn reject_story_type_overrides_a_model_that_will_not_say_no() {
    // The measured failure, verbatim: gemma3:4b classifies a boxscore `score_stub` — the exact
    // reject class, with the mapping stated as a lookup and the classification emitted BEFORE the
    // verdict — and still answers relevant=true. Three prompt revisions moved this by zero, so
    // the verdict is derived from the classification rather than trusted.
    let raw = r#"{"source_language":"en","story_type":"score_stub","key_facts":["West Ham 3, Southampton 0"],"relevant_entities":["West Ham United"],"co_mentions":[],"caveats":"","evidence_blurb":"West Ham beat Southampton 3-0.","relevant":true}"#;
    let parsed = ArticleEvidenceParser.parse(raw).unwrap().unwrap();
    assert!(!parsed.relevant, "score_stub must force relevant=false");
}

#[test]
fn reject_story_type_match_is_case_insensitive() {
    let raw = r#"{"source_language":"en","story_type":"Broadcast_Listing","key_facts":[],"relevant_entities":[],"co_mentions":[],"caveats":"","evidence_blurb":"A TV listing.","relevant":true}"#;
    let parsed = ArticleEvidenceParser.parse(raw).unwrap().unwrap();
    assert!(!parsed.relevant);
}

#[test]
fn reporting_story_type_never_forces_relevant_true() {
    // The derivation is ONE-WAY on purpose: we are correcting a measured failure to say no, not
    // overriding a no. A model that rejects on its own judgment keeps that judgment.
    let raw = r#"{"source_language":"en","story_type":"transfer","key_facts":[],"relevant_entities":[],"co_mentions":[],"caveats":"","evidence_blurb":"Not about the club after all.","relevant":false}"#;
    let parsed = ArticleEvidenceParser.parse(raw).unwrap().unwrap();
    assert!(!parsed.relevant, "a reporting story_type must not resurrect a rejection");
}

#[test]
fn parser_fails_closed_on_empty_blurb() {
    let parsed = ArticleEvidenceParser
        .parse(r#"{"evidence_blurb":" ","key_facts":[],"relevant_entities":[],"story_type":"general","caveats":""}"#)
        .unwrap();
    assert!(parsed.is_none());
}

#[test]
fn parser_accepts_irrelevant_without_blurb() {
    let parsed = ArticleEvidenceParser
        .parse(r#"{"relevant":false,"evidence_blurb":" ","key_facts":[],"relevant_entities":[],"story_type":"irrelevant","caveats":""}"#)
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
