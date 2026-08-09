//! Unit tests for the greenfield Editor: the ep1 parser's normalization and the derivations.
//! Everything here is offline — the resolver's SQL half is exercised by `derive::group_hits`,
//! its pure grouping core.

use super::derive::*;
use super::*;

/// The hypothesis list these tests score against, decorated as the prompt renders it.
fn hypothesis() -> Vec<String> {
    [
        "West Ham United (team 11)",
        "Baltimore Ravens (team 92)",
        "Real Madrid (team 3468)",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// One ep1 reply. No `relevant` key — the model is never asked.
fn ep1_raw(page_kind: &str, roles: &[(&str, &str)], names: &[(&str, &str, &str)]) -> String {
    let roles: Vec<_> = roles
        .iter()
        .map(|(e, r)| serde_json::json!({"entity": e, "role": r}))
        .collect();
    let names: Vec<_> = names
        .iter()
        .map(|(n, k, d)| serde_json::json!({"name": n, "kind_hint": k, "descriptor": d}))
        .collect();
    serde_json::json!({
        "source_language": "en",
        "page_kind": page_kind,
        "names": names,
        "entity_roles": roles,
        "story_type": "general",
        "result_line": "",
        "register_phrase": "",
        "register": "neutral",
        "key_facts": ["a fact"],
        "caveats": "",
        "evidence_blurb": "A blurb."
    })
    .to_string()
}

fn parse(raw: &str) -> EditorRead {
    EditorReadParser {
        hypothesis: &hypothesis(),
    }
    .parse(raw)
    .unwrap()
    .unwrap()
}

// ── relevance derivation (ported semantics, ep1 shapes) ─────────────────────────────────────

#[test]
fn a_non_reporting_page_shape_is_rejected() {
    let read = parse(&ep1_raw(
        "score_table",
        &[("West Ham United", "subject")],
        &[],
    ));
    assert!(!read.relevant, "score_table is not reporting");
}

#[test]
fn an_opponent_only_story_is_kept() {
    // KEPT since ar7, deliberately: a match against us is news about us.
    let read = parse(&ep1_raw("article", &[("West Ham United", "opponent")], &[]));
    assert!(read.relevant);
}

#[test]
fn an_absent_only_labelling_is_rejected() {
    let read = parse(&ep1_raw("article", &[("Baltimore Ravens", "absent")], &[]));
    assert!(!read.relevant, "absent is the model's rejection word");
}

#[test]
fn empty_roles_are_unknown_not_rejection() {
    let read = parse(&ep1_raw("article", &[], &[]));
    assert!(read.relevant);
}

#[test]
fn names_rescue_an_underfilled_role_array() {
    // The model placed none of ours but found our entity in the text (the 86% case): the
    // missing role is sloppiness, not a verdict.
    let read = parse(&ep1_raw(
        "article",
        &[("Somebody Else", "subject")],
        &[("West Ham United", "club", "the visitors")],
    ));
    assert!(read.relevant);
}

#[test]
fn volunteered_subjects_do_not_outvote_ours() {
    // Ours labelled absent; the model volunteers two non-hypothesis people as subject. Only
    // OUR entities vote, and names[] cannot rescue a name that is not ours.
    let read = parse(&ep1_raw(
        "article",
        &[
            ("Baltimore Ravens", "absent"),
            ("Some Youth Team", "subject"),
        ],
        &[("A Local Coach", "person", "youth team coach")],
    ));
    assert!(!read.relevant);
}

#[test]
fn a_place_described_hypothesis_does_not_count_as_present() {
    // The Moulin Rouge case as gemma3:4b actually emits it (measured 2026-08-01, stable across
    // seven prompt iterations): role `subject`, kind `club` — and the truth in the descriptor.
    // The descriptor is copied from the text; the labels are guesses. It retracts the vote.
    let roles = [EditorEntityRole {
        entity: "Paris".into(),
        role: "subject".into(),
    }];
    let names = [mention("Paris", "club", "capital city")];
    assert!(!derive_relevance(
        "article",
        &roles,
        &["Paris".to_string()],
        &names
    ));
    // Same shape on a real club story — "city rivals" is club language, the veto holds.
    let roles = [EditorEntityRole {
        entity: "Manchester City".into(),
        role: "subject".into(),
    }];
    let names = [mention("Manchester City", "club", "city rivals")];
    assert!(derive_relevance(
        "article",
        &roles,
        &["Manchester City".to_string()],
        &names
    ));
}

#[test]
fn an_unlisted_passing_mention_does_not_count() {
    // The Fortuna shape: the model string-associates a passing_mention role onto the hypothesis
    // name, but its own names[] holds only a different organization. No referent, no vote.
    let read = parse(&ep1_raw(
        "article",
        &[("Baltimore Ravens", "passing_mention")],
        &[("Ravens Mining Corp", "club", "mining company")],
    ));
    assert!(!read.relevant);
    // A passing_mention the model DID list stays in this entity's world (the ar7 lesson).
    let read = parse(&ep1_raw(
        "article",
        &[("Baltimore Ravens", "passing_mention")],
        &[("Baltimore Ravens", "club", "mentioned in the notebook column")],
    ));
    assert!(read.relevant);
}

// ── parser normalization ────────────────────────────────────────────────────────────────────

#[test]
fn parser_normalizes_names_kinds_and_descriptors() {
    let raw = serde_json::json!({
        "source_language": "EN",
        "page_kind": "article",
        "names": [
            {"name": "  Kyle   Shanahan ", "kind_hint": "Person",
             "descriptor": " San Francisco 49ers head coach of many years "},
            {"name": "", "kind_hint": "club", "descriptor": "dropped"},
            {"name": "Mystery FC", "kind_hint": "franchise", "descriptor": ""},
        ],
        "entity_roles": [{"entity": "Real Madrid", "role": "subject"}],
        "story_type": "roster",
        "result_line": "",
        "register_phrase": "",
        "register": "neutral",
        "key_facts": [],
        "caveats": "",
        "evidence_blurb": "A blurb."
    })
    .to_string();
    let read = parse(&raw);
    assert_eq!(read.names.len(), 2, "empty name dropped");
    assert_eq!(read.names[0].name, "Kyle Shanahan");
    assert_eq!(read.names[0].kind_hint, "person");
    // Descriptor enforced to the contract's ≤6 words.
    assert_eq!(
        read.names[0].descriptor,
        "San Francisco 49ers head coach of"
    );
    assert_eq!(
        read.names[1].kind_hint, "other",
        "out-of-enum kind_hint falls back to other"
    );
    assert_eq!(read.source_language, "en");
}

#[test]
fn an_out_of_enum_register_or_empty_phrase_falls_back_to_neutral() {
    let mut v: serde_json::Value =
        serde_json::from_str(&ep1_raw("article", &[("Real Madrid", "subject")], &[])).unwrap();
    v["register"] = "ecstatic".into();
    v["register_phrase"] = "scenes of wild joy".into();
    let read = parse(&v.to_string());
    assert_eq!(read.register, "neutral");

    let mut v: serde_json::Value =
        serde_json::from_str(&ep1_raw("article", &[("Real Madrid", "subject")], &[])).unwrap();
    v["register"] = "celebration".into();
    v["register_phrase"] = "".into();
    let read = parse(&v.to_string());
    assert_eq!(read.register, "neutral", "empty phrase means neutral");
}

#[test]
fn a_kept_register_needs_phrase_and_enum() {
    let mut v: serde_json::Value =
        serde_json::from_str(&ep1_raw("article", &[("Real Madrid", "subject")], &[])).unwrap();
    v["register"] = "outrage".into();
    v["register_phrase"] = "fans stormed the boardroom".into();
    let read = parse(&v.to_string());
    assert_eq!(read.register, "outrage");
    assert_eq!(read.register_phrase, "fans stormed the boardroom");
}

#[test]
fn an_empty_blurb_on_a_relevant_read_fails_closed() {
    let mut v: serde_json::Value =
        serde_json::from_str(&ep1_raw("article", &[("Real Madrid", "subject")], &[])).unwrap();
    v["evidence_blurb"] = "".into();
    let parsed = EditorReadParser {
        hypothesis: &hypothesis(),
    }
    .parse(&v.to_string())
    .unwrap();
    assert!(parsed.is_none(), "no committed blurb = fail-closed");
}

#[test]
fn an_empty_blurb_on_an_irrelevant_read_is_filled() {
    let mut v: serde_json::Value =
        serde_json::from_str(&ep1_raw("roundup", &[("Real Madrid", "subject")], &[])).unwrap();
    v["evidence_blurb"] = "".into();
    let read = parse(&v.to_string());
    assert!(!read.relevant);
    assert!(!read.evidence_blurb.is_empty());
}

#[test]
fn unparseable_reply_is_none_not_error() {
    let parsed = EditorReadParser {
        hypothesis: &hypothesis(),
    }
    .parse("not json at all")
    .unwrap();
    assert!(parsed.is_none());
}

// ── result_line parse (derived, never asked) ────────────────────────────────────────────────

#[test]
fn result_line_parses_the_verbatim_shape() {
    assert_eq!(
        parse_result_line("Real Madrid 2-1 Arsenal"),
        Some(ParsedResult {
            home: "Real Madrid".into(),
            home_score: 2,
            away_score: 1,
            away: "Arsenal".into(),
        })
    );
    assert_eq!(
        parse_result_line("Borussia Dortmund 0:0 Bayern Munich"),
        Some(ParsedResult {
            home: "Borussia Dortmund".into(),
            home_score: 0,
            away_score: 0,
            away: "Bayern Munich".into(),
        })
    );
}

#[test]
fn result_line_refuses_what_is_not_a_single_final_result() {
    assert_eq!(parse_result_line(""), None);
    assert_eq!(parse_result_line("Real Madrid beat Arsenal"), None);
    assert_eq!(parse_result_line("2-1"), None, "no names");
    assert_eq!(parse_result_line("Real Madrid 2-1"), None, "no away side");
    assert_eq!(
        parse_result_line("the 2024-25 season for Arsenal"),
        None,
        "years are not scores"
    );
    assert_eq!(
        parse_result_line("Madrid 2-1 Arsenal 3-3 Chelsea"),
        None,
        "two score tokens is a roundup, not a result"
    );
}

// ── routing tags (derived; Phase 6 consumes) ────────────────────────────────────────────────

#[test]
fn routing_tags_carry_story_type_and_charged() {
    assert_eq!(routing_tags("transfer", "neutral"), vec!["transfer"]);
    assert_eq!(routing_tags("transfer", "outrage"), vec!["transfer", "charged"]);
    assert_eq!(routing_tags("", "celebration"), vec!["charged"]);
}

// ── nomination classification (5.2; Phase 5 consumes) ───────────────────────────────────────

#[test]
fn a_person_with_a_descriptor_nominates_immediately() {
    assert!(nominates_immediately("person", "Real Madrid manager"));
    assert!(!nominates_immediately("person", ""), "bare names wait for the floor");
    assert!(!nominates_immediately("club", "the visitors"), "clubs are not person-discovery");
}

// ── resolver grouping (the pure core of the T9 gate) ────────────────────────────────────────

fn mention(name: &str, kind: &str, desc: &str) -> NameMention {
    NameMention {
        name: name.into(),
        kind_hint: kind.into(),
        descriptor: desc.into(),
    }
}

fn hit(name: &str, entity_type: &str, id: i32) -> SurfaceHit {
    SurfaceHit {
        name: name.into(),
        entity_type: entity_type.into(),
        entity_id: id,
        sport: "FOOTBALL".into(),
        norm: name.to_lowercase(),
    }
}

#[test]
fn a_single_kind_compatible_hit_links() {
    let names = [mention("West Ham United", "club", "the hosts")];
    let hits = [hit("West Ham United", "team", 11)];
    let r = group_hits(&names, &hits);
    assert_eq!(r.links.len(), 1);
    assert_eq!(r.links[0].entity_type, "team");
    assert_eq!(r.links[0].entity_id, 11);
    assert!(r.unresolved.is_empty() && r.refused_ambiguous.is_empty());
}

#[test]
fn a_namesake_tie_is_refused_never_flipped() {
    // The Vinicius tie: two same-sport players on one norm. Exact match fires twice → refuse.
    let names = [mention("Vinicius", "person", "Real Madrid forward")];
    let hits = [hit("Vinicius", "player", 600687), hit("Vinicius", "player", 999)];
    let r = group_hits(&names, &hits);
    assert!(r.links.is_empty());
    assert_eq!(r.refused_ambiguous.len(), 1);
    assert_eq!(r.refused_ambiguous[0].candidates.len(), 2);
}

#[test]
fn a_place_named_other_never_auto_links() {
    // The Moulin Rouge case: "Paris" the city exact-matches the club's surface, and the
    // descriptor's kind_hint is what refuses the link.
    let names = [mention("Paris", "other", "city hosting the finale")];
    let hits = [hit("Paris", "team", 4508)];
    let r = group_hits(&names, &hits);
    assert!(r.links.is_empty(), "kind `other` never auto-links");
    assert_eq!(r.unresolved.len(), 1);
}

#[test]
fn a_place_described_club_kind_never_takes_the_team_link() {
    // The descriptor arm (§1a): kind_hint says club, the descriptor says city — no link, and
    // the mention is recorded as unresolved, not silently dropped.
    let names = [mention("Paris", "club", "capital city")];
    let hits = [hit("Paris", "team", 4508)];
    let r = group_hits(&names, &hits);
    assert!(r.links.is_empty(), "the descriptor arm refuses the club link");
    assert_eq!(r.unresolved.len(), 1);
}

#[test]
fn descriptor_place_words_and_the_club_veto() {
    assert!(descriptor_names_place("capital city"));
    assert!(descriptor_names_place("city hosting the finale"));
    assert!(!descriptor_names_place("city rivals"), "club sense vetoes");
    assert!(!descriptor_names_place("selling club"));
    assert!(!descriptor_names_place("the hosts"));
    assert!(!descriptor_names_place(""));
}

#[test]
fn a_person_hit_only_by_a_team_surface_is_unresolved() {
    // kyle-shanahan shape: a person must not link to a team (or vice versa); with no
    // kind-compatible hit the name flows to discovery.
    let names = [mention("Lee", "person", "veteran coach")];
    let hits = [hit("Lee", "team", 71)];
    let r = group_hits(&names, &hits);
    assert!(r.links.is_empty());
    assert_eq!(r.unresolved.len(), 1);
}

#[test]
fn multiple_surfaces_of_one_entity_are_one_link() {
    // name + alias normalize to different norms but the same entity: still exactly one link.
    let names = [mention("Vinicius Junior", "person", "Real Madrid forward")];
    let hits = [
        hit("Vinicius Junior", "player", 600687),
        SurfaceHit {
            norm: "vinicius jr".into(),
            ..hit("Vinicius Junior", "player", 600687)
        },
    ];
    let r = group_hits(&names, &hits);
    assert_eq!(r.links.len(), 1);
    assert!(r.refused_ambiguous.is_empty());
}

#[test]
fn a_person_may_resolve_to_a_person_entity() {
    // The living database compounds (mig 207): a verified coach resolves tomorrow.
    let names = [mention("Kyle Shanahan", "person", "49ers head coach")];
    let hits = [hit("Kyle Shanahan", "person", 3)];
    let r = group_hits(&names, &hits);
    assert_eq!(r.links.len(), 1);
    assert_eq!(r.links[0].entity_type, "person");
}

// ── shared-runner agreement ─────────────────────────────────────────────────────────────────

/// The Editor, graph and the Insider's transfer call share one pinned runner; a num_ctx
/// disagreement forces a reload on every alternation (measured — see route.rs's VOICE_NUM_CTX
/// history). Phase 9 removed the legacy reader from this set and re-anchored the shared size on
/// `route::LOCAL_STAGE_NUM_CTX`; the agreement this test defends is unchanged.
#[test]
fn editor_num_ctx_matches_the_shared_runner() {
    assert_eq!(EDITOR_NUM_CTX, crate::route::LOCAL_STAGE_NUM_CTX);
}

/// Property order IS the contract (§1a): extraction first, in the exact ep1 order. The schema's
/// `required` list is what constrained decoding emits, in order — drift here is a contract bump.
#[test]
fn ep1_schema_property_order_is_the_contract() {
    let schema = editor_format_schema();
    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        required,
        vec![
            "source_language",
            "page_kind",
            "names",
            "entity_roles",
            "story_type",
            "result_line",
            "register_phrase",
            "register",
            "key_facts",
            "caveats",
            "evidence_blurb"
        ]
    );
}

/// 7.13 (G1): the Editor hands graph its article ONLY on the packet rail. Under `RAIL=legacy`
/// The cutover blocker measured 2026-08-06: three Editor rows at attempts≥5, one per day, all
/// `persist article full_text <id>: invalid byte sequence for encoding "UTF8": 0x00`. A NUL in a
/// scraped body is not storable in a Postgres text column at all, so the write can only ever
/// fail — and §2 clause 4 needs the dead-letter count at 0 for seven consecutive days, which one
/// arrival per day resets forever. The body is sanitised where it enters the Editor, so the same
/// clean text is what gets hashed, prompted, sliced for candidate evidence, and persisted.
#[test]
fn a_body_carrying_nul_is_sanitised_before_it_can_reach_a_text_column() {
    let fetched = FetchedArticle {
        final_url: "https://example.com/a".to_string(),
        final_domain: Some("example.com".to_string()),
        text: "Vinicius \u{0}Junior is \u{0}staying at Real Madrid.\u{0}".to_string(),
    };
    assert!(fetched.text.contains('\0'), "the fixture must carry the byte under test");

    let clean = sanitize_fetched(fetched);
    assert!(!clean.text.contains('\0'), "no NUL may survive into full_text");
    assert_eq!(clean.text, "Vinicius Junior is staying at Real Madrid.");
    // The derivations that ride the same body must be computable on it.
    assert_eq!(count_words(&clean.text), 7);
    assert!(!content_hash(&clean.text).is_empty());
    // The URL columns are bound from the same struct and must survive untouched.
    assert_eq!(clean.final_url, "https://example.com/a");
    assert_eq!(clean.final_domain.as_deref(), Some("example.com"));
}

/// Sanitisation must be invisible to the 99.99% of bodies that carry no NUL: same bytes in, same
/// bytes out, so no prompt and no `content_hash` moves for an ordinary article.
#[test]
fn a_body_without_nul_passes_through_byte_identical() {
    let body = "Arsenal have agreed a fee.\n\nThe deal is not done.\t— sources";
    let clean = sanitize_fetched(FetchedArticle {
        final_url: "https://example.com/b".to_string(),
        final_domain: None,
        text: body.to_string(),
    });
    assert_eq!(clean.text, body);
    assert_eq!(content_hash(&clean.text), content_hash(body));
}
