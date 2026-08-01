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

/// Regression for the 2026-07-26 harness panic: a page whose text lowercases to a *longer*
/// byte string than the original. `İ` (U+0130, 2 bytes) becomes `i̇` (3 bytes), so the old
/// search-the-lowercase-copy-then-index-the-original approach drifted one byte per occurrence
/// and eventually sliced past the end of `html`. A Galatasaray report with 11 of them took the
/// whole cognition service down with `start byte index 1040186 is out of bounds for string of
/// length 1040175`.
///
/// The tag being stripped is deliberately placed *after* the drifting characters, since that is
/// the only arrangement in which the offsets have diverged by the time they are used.
#[test]
fn clean_html_survives_text_whose_lowercase_is_longer() {
    let turkish = "İstanbul İzmir İnönü İlkay İsmail İbrahim İdris İlhan İnan İpek İrem";
    assert!(
        turkish.to_lowercase().len() > turkish.len(),
        "precondition: this text must expand when lowercased"
    );

    let html = format!("<p>{turkish}</p><script>bad()</script><p>Tail.</p>");
    let cleaned = clean_html(&html);

    assert!(!cleaned.contains("bad()"), "script block must still be stripped");
    assert!(cleaned.contains("Tail."), "content after the script must survive");
    assert!(cleaned.contains("İstanbul"), "original casing must be preserved");
}

/// Tag matching is case-insensitive, and must stay so now that it no longer goes through
/// `to_lowercase`.
#[test]
fn clean_html_strips_uppercase_tags() {
    assert_eq!(clean_html("<P>A</P><SCRIPT>bad()</SCRIPT><P>B</P>"), "A B");
}

/// An unclosed block swallows the remainder — preserved from the previous implementation.
#[test]
fn clean_html_drops_tail_of_unclosed_script() {
    assert_eq!(clean_html("<p>Kept</p><script>oops"), "Kept");
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

/// REVERSED in ar7, deliberately. An opponent-only story used to be rejected on the reasoning
/// that our entity is merely who the subject plays against. A match against us is news about us,
/// and the narrower rule is what collapsed the Editor's yield from 73% to 2% once the vetted list
/// held teams only. `absent` is the rejection signal now; `opponent` is not.
#[test]
fn an_opponent_only_story_is_now_kept() {
    let parsed = ArticleEvidenceParser { vetted: &vetted() }
        .parse(&ar6_raw("article", &[("West Ham United", "opponent")]))
        .unwrap()
        .unwrap();
    assert!(parsed.relevant);
}

/// The case the ar6 rule threw away 6,296 times in two days: a person is the subject and our team
/// is placed as a passing mention. Since Phase 2 stopped players auto-vetting, the player is not
/// in the list at all and cannot be the subject — so demanding a subject rejected every
/// player-led story in the corpus, including LeBron James signing with the 76ers.
#[test]
fn a_player_led_story_naming_our_team_is_kept() {
    let parsed = ArticleEvidenceParser { vetted: &vetted() }
        .parse(&ar6_raw("article", &[("Aston Villa", "passing_mention")]))
        .unwrap()
        .unwrap();
    assert!(parsed.relevant, "a passing mention is still this entity's world");
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
    assert!(derive_relevance("article", &[], &vetted(), &[]));
    assert!(!derive_relevance("listing_or_schedule", &[], &vetted(), &[]));
    // Nor may an empty vetted list reject everything.
    assert!(derive_relevance("article", &[], &[], &[]));
}

/// Everything the model placed is `absent` — its own word for a name collision or an entity that
/// is simply not in the text. That is the one thing that still rejects on roles.
#[test]
fn all_absent_is_still_a_rejection() {
    let roles = [
        role("Baltimore Ravens", "absent"),
        role("Norwich City", "absent"),
    ];
    assert!(!derive_relevance("article", &roles, &vetted(), &[]));
    // ...but one non-absent placement among them is enough to keep the article.
    let mixed = [
        role("Baltimore Ravens", "absent"),
        role("Norwich City", "passing_mention"),
    ];
    assert!(derive_relevance("article", &mixed, &vetted(), &[]));
}

/// The model placed none of OUR entities — it only labelled people it volunteered — but it listed
/// our team among the entities it found. That is an under-filled array, not a verdict, and it
/// described 86% of the ar6 rejections.
#[test]
fn an_unplaced_entity_is_rescued_by_the_found_list() {
    let roles = [role("LeBron James", "subject")];
    let found = vec!["Aston Villa".to_string()];
    assert!(
        derive_relevance("article", &roles, &vetted(), &found),
        "our team appearing in relevant_entities means it IS in the text"
    );
    // With nothing of ours in either list, the rejection stands.
    assert!(!derive_relevance("article", &roles, &vetted(), &["Chelsea".to_string()]));
}

/// A volunteered name must never carry the vote on its own — the sound half of the original rule.
#[test]
fn volunteered_entities_still_get_no_vote() {
    let roles = [role("Dragojevic", "subject"), role("Clement", "subject")];
    assert!(!derive_relevance("article", &roles, &vetted(), &[]));
}

fn role(entity: &str, role: &str) -> ArticleEntityRole {
    ArticleEntityRole {
        entity: entity.to_string(),
        role: role.to_string(),
    }
}

#[test]
fn omitting_the_vetted_entity_from_a_populated_list_rejects() {
    // Measured verbatim: the model labels the two people it found in the body as `subject` and
    // leaves our entity out entirely. Under ar7 an omission alone no longer decides — the
    // `relevant_entities` list gets the last word — but here that list is empty too, so nothing
    // in either place puts our entity in the text and the rejection stands.
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
