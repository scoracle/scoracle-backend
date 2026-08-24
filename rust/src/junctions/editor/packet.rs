//! The Desk, part two — packet compilation (PLAN-one-rail Phase 6.3, §1c).
//!
//! A packet is a SNAPSHOT of a storyline at compile time, assembled in code from member
//! `editor_reads`. Zero model tokens (C5: the Editor's output budget is coverage, not
//! summarizing summaries), append-only (a new packet supersedes the prior one; rows are never
//! edited — archive-as-moat).
//!
//! **T3 lives in `claims`.** Facts are attributed to the article and source that carried them
//! and are deduplicated ONLY when byte-identical. "Agreement reached" and "yet to reach
//! agreement" both survive, side by side, attributed — the disagreement is the story, and a
//! similarity threshold that collapsed them would delete it.
//!
//! **The fan-out hazard (read before wiring this into the drain loop).** `INSERT ON packets`
//! fires mig 206's `enqueue_voices_on_packet`. Arm 1 (tag-subscribed voices) is inert until
//! Phase 7.4 seeds `stage_routing_subscriptions`. Arm 2 is NOT: the Journalist's `narratives`
//! fan-out is unconditional by design, so every packet enqueues `narratives` work for each
//! active player/team participant, with `input_version` `pk:<fingerprint>` — while the legacy
//! `article_read` seat is still enqueueing the same (stage, entity) rows with its own `n:` hash.
//! Two writers alternating one row's `input_version` is the mig-197 churn loop. Until that seam
//! is ruled on, the compile sweep stays behind `COGNITION_PACKET_COMPILE` (default off) and this
//! module is a library nothing calls in production.

use super::candidates::slice_quote;
use super::derive::routing_tags;
use super::{EditorRead, NameMention};
use crate::util::hash_components;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use tracing::{debug, info};

/// The packet contract (§1c) — a cache key, not a label (T1).
pub const PACKET_CONTRACT_VERSION: &str = "pk1";
/// A storyline compiles only after it has been quiet this long: a story arriving in a burst of
/// twelve articles should produce one packet, not twelve.
pub const QUIET_DEBOUNCE_MINUTES: i64 = 15;
/// Members loaded per compile, newest first. A storyline with more members than this is a
/// month-long saga; the newest 200 are the snapshot, and `facts.member_articles` reports the
/// true total so the truncation is never silent.
pub const MAX_MEMBERS: i64 = 200;
/// Claims carried per packet, newest first. Phase 7's render truncates again to its own
/// budget; this cap only keeps the archive row bounded.
pub const MAX_CLAIMS: usize = 200;
/// Quotes are evidence, not colour: one per member article, newest members first.
pub const MAX_QUOTES: usize = 6;
/// The unresolved census is a signal about coverage, not a list to read.
pub const MAX_UNRESOLVED: usize = 50;

/// One storyline member, as the compiler sees it.
#[derive(Clone, Debug)]
pub struct Member {
    pub article_id: i64,
    pub title: String,
    pub source: String,
    pub feed_rank: Option<i32>,
    /// Epoch seconds; `None` when the feed carried no publish time (the fetch time is used for
    /// ordering in SQL, but only a real publish time is ever attributed to a claim).
    pub published_at: Option<i64>,
    pub read: EditorRead,
    /// `editor_reads.resolved.unresolved` — B3's census, rolled up onto the packet.
    pub unresolved: Vec<NameMention>,
    /// `news_articles.full_text`, for code-sliced quotes.
    pub body: Option<String>,
}

/// One active storyline participant.
#[derive(Clone, Debug)]
pub struct PacketEntity {
    pub entity_type: String,
    pub entity_id: i32,
    pub sport: String,
    pub role: Option<String>,
    pub name: String,
}

/// One attributed claim: §1c's four fields plus the `story_type` that produced it.
///
/// `story_type` was in-memory-only through 6.3 (the persisted shape was exactly the four). 7.2
/// persists it, because the Insider's packet slice IS the transfer-typed claims (7.5) and its
/// `slice_fingerprints.transfers` hashes exactly that subset: a renderer that cannot see the type
/// would render a different slice than the fingerprint promises, and E2's "re-read only when YOUR
/// slice moved" would be a lie in both directions (silent staleness, or a re-fan that changes
/// nothing). Free to add here: zero packets have ever been compiled, so no row carries the old
/// shape, and readers key on the four fields they already knew.
#[derive(Clone, Debug)]
struct Claim {
    article_id: i64,
    source: String,
    fact: String,
    published_at: Option<i64>,
    story_type: String,
}

impl Claim {
    fn to_json(&self) -> Value {
        json!({
            "article_id": self.article_id,
            "source": self.source,
            "fact": self.fact,
            "published_at": self.published_at,
            "story_type": self.story_type,
        })
    }
}

/// The compiled packet, before it becomes a row.
#[derive(Clone, Debug)]
pub struct PacketDraft {
    pub headline: Option<String>,
    pub story_types: Vec<String>,
    pub register: Option<String>,
    pub register_phrase: Option<String>,
    pub claims: Value,
    pub facts: Value,
    pub quotes: Value,
    pub routing_tags: Vec<String>,
    pub slice_fingerprints: Value,
    pub unresolved_mentions: Value,
}

/// compile is the whole of §1c, pure: members + participants in, a packet draft out. No clock,
/// no database, no model — so the fixture gate (6.5) scores exactly what production writes.
///
/// Members arrive NEWEST FIRST (the loader's order).
pub fn compile(members: &[Member], entities: &[PacketEntity]) -> PacketDraft {
    let headline = best_headline(members);
    let story_types = rollup_story_types(members);
    let (register, register_phrase) = rollup_register(members);

    let all_claims = collect_claims(members);
    let claims: Vec<&Claim> = all_claims.iter().take(MAX_CLAIMS).collect();
    let dropped: Vec<i64> = all_claims
        .iter()
        .skip(MAX_CLAIMS)
        .map(|c| c.article_id)
        .collect();

    let claims_json: Vec<Value> = claims.iter().map(|c| c.to_json()).collect();
    let transfer_claims: Vec<Value> = claims
        .iter()
        .filter(|c| c.story_type.eq_ignore_ascii_case("transfer"))
        .map(|c| c.to_json())
        .collect();
    // The Scout's slice: availability. Same shape as the Insider's transfer slice, and it is
    // what lets the Editor TAG him rather than have the pipeline adjudicate on his behalf.
    //
    // The fingerprint is the CLAIMS HASH, which is why this needs no day-key, no debounce table
    // and no adjudication status: five outlets reporting one knock collapse to the same claim
    // set and enqueue ONCE, while a genuinely new fact (a return, a longer prognosis) moves the
    // hash and wakes him again. Scott's once-per-event-day rule falls out of CONTENT rather than
    // out of a calendar — strictly better, because it also holds across days when nothing new is
    // said, and re-fires within a day when something is.
    let availability_claims: Vec<Value> = claims
        .iter()
        .filter(|c| {
            c.story_type.eq_ignore_ascii_case("injury")
                || c.story_type.eq_ignore_ascii_case("suspension")
        })
        .map(|c| c.to_json())
        .collect();

    let result_line = members
        .iter()
        .find(|m| !m.read.result_line.trim().is_empty())
        .map(|m| m.read.result_line.clone());

    // Thin and structured only — the Scout never reads packet prose (T4).
    let mut facts = json!({
        "story_types": story_types,
        "entities": entities.iter().map(|e| json!({
            "entity_type": e.entity_type,
            "entity_id": e.entity_id,
            "sport": e.sport,
            "role": e.role,
            "name": e.name,
        })).collect::<Vec<_>>(),
        "member_articles": members.len(),
    });
    if let Some(line) = result_line {
        facts["result_line"] = json!(line);
    }
    // The A5 rule: evidence that did not make the packet is NAMED, never silently dropped.
    if !dropped.is_empty() {
        facts["claims_dropped_from"] = json!(dropped);
    }

    let mut tags: Vec<String> = members
        .iter()
        .flat_map(|m| routing_tags(&m.read.story_type, &m.read.register))
        .collect();
    tags.sort();
    tags.dedup();

    // Keys are STAGE strings (mig 202's column comment; mig 206 reads
    // `slice_fingerprints ->> stage`). A voice re-reads only when ITS slice moved.
    let slice_fingerprints = json!({
        "narratives": hash_components(&json!({
            "headline": headline,
            "claims": claims_json,
        }).to_string()),
        "vibe": hash_components(&json!({
            "register": register,
            "register_phrase": register_phrase,
            "claims": claims_json,
        }).to_string()),
        "transfers": hash_components(&json!({ "claims": transfer_claims }).to_string()),
        // `rating` is the STAGE the Scout drains, so this key is what mig 225's
        // `enqueue_voices_on_packet` reads to mint his `pk:` input_version. Before it existed
        // the lookup fell through to the packet id, which made every injury packet its own
        // enqueue — the reason the routing subscription was previously judged unusable. It is
        // usable now.
        "rating": hash_components(&json!({ "claims": availability_claims }).to_string()),
    });

    PacketDraft {
        headline,
        story_types: facts["story_types"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        register,
        register_phrase,
        claims: Value::Array(claims_json),
        facts,
        quotes: Value::Array(collect_quotes(members, entities)),
        routing_tags: tags,
        slice_fingerprints,
        unresolved_mentions: Value::Array(rollup_unresolved(members)),
    }
}

/// The best member title: lowest `feed_rank` (Google's own ordering, mig 194), missing ranks
/// last, ties to the newest.
fn best_headline(members: &[Member]) -> Option<String> {
    members
        .iter()
        .filter(|m| !m.title.trim().is_empty())
        .min_by_key(|m| {
            (
                m.feed_rank.unwrap_or(i32::MAX),
                -m.published_at.unwrap_or(0),
                m.article_id,
            )
        })
        .map(|m| m.title.clone())
}

/// Distinct member story types, most common first — the packet's topic rollup.
fn rollup_story_types(members: &[Member]) -> Vec<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for m in members {
        let st = m.read.story_type.trim().to_lowercase();
        if !st.is_empty() {
            *counts.entry(st).or_default() += 1;
        }
    }
    let mut types: Vec<(String, usize)> = counts.into_iter().collect();
    types.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    types.into_iter().map(|(st, _)| st).collect()
}

/// The strongest non-neutral register among members, with its phrase (§1c).
///
/// "Strongest" is the register the members most AGREE on, newest breaking ties — not an
/// invented intensity ladder. The Editor describes a register; nobody scored its intensity, and
/// the Influencer owns the score (mig 202's column comment). A packet whose members are all
/// neutral carries no register at all.
fn rollup_register(members: &[Member]) -> (Option<String>, Option<String>) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for m in members {
        let r = m.read.register.trim().to_lowercase();
        if !r.is_empty() && r != "neutral" {
            *counts.entry(r).or_default() += 1;
        }
    }
    if counts.is_empty() {
        return (None, None);
    }
    let newest_rank = |reg: &str| {
        members
            .iter()
            .position(|m| m.read.register.eq_ignore_ascii_case(reg))
            .unwrap_or(usize::MAX)
    };
    let (register, _) = counts
        .iter()
        .map(|(r, c)| (r.clone(), *c))
        .max_by(|a, b| {
            a.1.cmp(&b.1)
                .then(newest_rank(&b.0).cmp(&newest_rank(&a.0)))
        })
        .expect("counts is non-empty");
    let phrase = members
        .iter()
        .find(|m| {
            m.read.register.eq_ignore_ascii_case(&register)
                && !m.read.register_phrase.trim().is_empty()
        })
        .map(|m| m.read.register_phrase.clone());
    (Some(register), phrase)
}

/// Every member fact, attributed, newest first.
///
/// Deduplication is BYTE-IDENTICAL ONLY (§1c/T3). Two sources phrasing the same development
/// differently are two claims; two sources filing the identical sentence are one, credited to
/// whoever published it first.
fn collect_claims(members: &[Member]) -> Vec<Claim> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut claims: Vec<Claim> = Vec::new();
    // Oldest first for attribution (first to say it owns it), then reversed for storage order.
    for m in members.iter().rev() {
        for fact in &m.read.key_facts {
            let fact = fact.trim();
            if fact.is_empty() {
                continue;
            }
            if seen.contains_key(fact) {
                continue;
            }
            seen.insert(fact.to_string(), claims.len());
            claims.push(Claim {
                article_id: m.article_id,
                source: m.source.clone(),
                fact: fact.to_string(),
                published_at: m.published_at,
                story_type: m.read.story_type.trim().to_lowercase(),
            });
        }
    }
    claims.reverse();
    claims
}

/// One code-sliced quote per member (newest first), around the first participant named in that
/// member's stored body. The model never emits quotes — mig 202's contract.
fn collect_quotes(members: &[Member], entities: &[PacketEntity]) -> Vec<Value> {
    let mut ordered: Vec<&PacketEntity> = entities.iter().collect();
    ordered.sort_by(|a, b| {
        role_rank(a.role.as_deref())
            .cmp(&role_rank(b.role.as_deref()))
            .then(a.entity_type.cmp(&b.entity_type))
            .then(a.entity_id.cmp(&b.entity_id))
    });

    let mut quotes = Vec::new();
    for m in members {
        if quotes.len() >= MAX_QUOTES {
            break;
        }
        let Some(body) = m.body.as_deref() else {
            continue;
        };
        for e in &ordered {
            if e.name.trim().is_empty() {
                continue;
            }
            if let Some(quote) = slice_quote(body, &e.name) {
                quotes.push(json!({
                    "article_id": m.article_id,
                    "source": m.source,
                    "entity_type": e.entity_type,
                    "entity_id": e.entity_id,
                    "quote": quote,
                }));
                break;
            }
        }
    }
    quotes
}

fn role_rank(role: Option<&str>) -> u8 {
    match role.unwrap_or("").trim().to_lowercase().as_str() {
        "subject" => 0,
        "opponent" => 1,
        "passing_mention" => 2,
        _ => 3,
    }
}

/// B3's census, rolled up: which names this story keeps mentioning that the rail cannot resolve.
/// The Investigator's queue is fed by the nomination sweep, not by this — here it is a coverage
/// signal a human reads.
fn rollup_unresolved(members: &[Member]) -> Vec<Value> {
    let mut by_name: BTreeMap<String, (NameMention, usize)> = BTreeMap::new();
    for m in members {
        for n in &m.unresolved {
            let key = n.name.trim().to_lowercase();
            if key.is_empty() {
                continue;
            }
            let entry = by_name.entry(key).or_insert_with(|| (n.clone(), 0));
            entry.1 += 1;
            if entry.0.descriptor.trim().is_empty() && !n.descriptor.trim().is_empty() {
                entry.0.descriptor = n.descriptor.clone();
            }
        }
    }
    let mut rolled: Vec<(NameMention, usize)> = by_name.into_values().collect();
    rolled.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.name.cmp(&b.0.name)));
    rolled
        .into_iter()
        .take(MAX_UNRESOLVED)
        .map(|(n, count)| {
            json!({
                "name": n.name,
                "kind_hint": n.kind_hint,
                "descriptor": n.descriptor,
                "mentions": count,
            })
        })
        .collect()
}

/// compile_dirty is the drain-loop pass: every storyline whose newest member predates its
/// newest packet, once it has been quiet for [`QUIET_DEBOUNCE_MINUTES`].
pub async fn compile_dirty(pool: &PgPool, limit: i64) -> Result<usize> {
    let dirty: Vec<(i64, String)> = sqlx::query_as(
        r#"
        SELECT s.id, s.sport
          FROM public.storylines s
         WHERE s.status <> 'resolved'
           AND s.last_seen_at <= NOW() - make_interval(mins => $1::int)
           AND s.last_seen_at > COALESCE(
                 (SELECT max(p.compiled_at) FROM public.packets p WHERE p.storyline_id = s.id),
                 '-infinity'::timestamptz)
         ORDER BY s.last_seen_at
         LIMIT $2
        "#,
    )
    .bind(QUIET_DEBOUNCE_MINUTES as i32)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("load dirty storylines")?;

    let mut compiled = 0usize;
    for (storyline_id, sport) in dirty {
        match compile_storyline(pool, storyline_id, &sport).await {
            Ok(Some(packet_id)) => {
                compiled += 1;
                debug!(storyline_id, packet_id, "packet compiled");
            }
            Ok(None) => debug!(storyline_id, "storyline had nothing to compile"),
            Err(e) => {
                tracing::warn!(
                    storyline_id,
                    error = %format!("{e:#}"),
                    "packet compile failed (continuing)"
                );
            }
        }
    }
    if compiled > 0 {
        info!(compiled, "packets compiled");
    }
    Ok(compiled)
}

/// compile_storyline loads one storyline's members + participants, compiles, and appends the
/// packet. Returns the new packet id, or None when the storyline has no compilable members.
pub async fn compile_storyline(
    pool: &PgPool,
    storyline_id: i64,
    sport: &str,
) -> Result<Option<i64>> {
    let members = load_members(pool, storyline_id).await?;
    if members.is_empty() {
        return Ok(None);
    }
    let entities = load_entities(pool, storyline_id).await?;
    let draft = compile(&members, &entities);
    let packet_id = insert_packet(pool, storyline_id, sport, &draft).await?;
    Ok(Some(packet_id))
}

/// A compiled packet as a READER sees it (7.2/7.3) — the row, plus the one continuity line from
/// the packet it supersedes. The writer's shape is [`PacketDraft`]; this is deliberately separate,
/// because a reader must never be able to hand a draft to the renderer and skip the archive.
#[derive(Clone, Debug)]
pub struct PacketView {
    pub packet_id: i64,
    pub storyline_id: i64,
    pub sport: String,
    pub headline: Option<String>,
    pub story_types: Vec<String>,
    pub register: Option<String>,
    pub register_phrase: Option<String>,
    /// `facts.result_line` — verbatim from the text, when the story has a completed result.
    pub result_line: Option<String>,
    /// The headline of the packet this one supersedes; the render's one `PREVIOUSLY:` line.
    pub prior_headline: Option<String>,
    pub claims: Vec<super::render::RenderClaim>,
    pub facts: Value,
}

/// Load the entity's CURRENT packets — the newest packet of each storyline it is an active
/// SUBJECT in, compiled within `hours` — newest first, with its part in each.
///
/// One packet per storyline, always the latest: packets are append-only snapshots, so the older
/// ones are archive (the moat), never context. `left_at IS NULL` honours D5 — an entity written
/// out of a story stops reading it.
///
/// **SUBJECT ONLY (2026-08-24, the Chelsea card).** The Editor classifies every participant's
/// part (`subject | opponent | passing_mention | absent`) and until now nothing consumed the
/// verdict: Chelsea's vibe card was built from a Tottenham transfer saga because Enzo Fernandez
/// was mentioned in passing (vibe_scores 60602 — the "fabricated" Savinho/Marmoush/De Zerbi card
/// whose every claim was in fact sitting in the packet block). Every voice reads through here,
/// so the fence is here: an entity reads only the stories it is the SUBJECT of. `opponent` is
/// deliberately out too — each team has its own query lane, so its real coverage arrives with it
/// as subject, and the opponent-role blocks measured on the live card were cross-team noise. A
/// blank role (the attach's fail-to-empty reconstruction) is out for the same law: fail open to
/// silence, never to a guess. Blank rows heal forward via the strongest-role upsert in
/// `storyline::attach_read`.
pub async fn load_packets_for_entity(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    hours: i64,
    limit: i64,
) -> Result<Vec<(PacketView, super::render::Participation)>> {
    let rows = sqlx::query(
        r#"
        WITH mine AS (
            SELECT se.storyline_id, se.role, se.joined_at, se.last_seen_at
              FROM public.storyline_entities se
             WHERE se.entity_type = $1
               AND se.entity_id = $2
               AND se.sport = $3
               AND se.left_at IS NULL
               AND lower(COALESCE(se.role, '')) = 'subject'
        ),
        latest AS (
            SELECT DISTINCT ON (p.storyline_id)
                   p.id, p.storyline_id, p.sport, p.headline, p.story_types, p.register,
                   p.register_phrase, p.claims, p.facts,
                   EXTRACT(EPOCH FROM p.compiled_at)::bigint AS compiled_epoch,
                   prior.headline AS prior_headline,
                   mine.role,
                   to_char(mine.joined_at, 'YYYY-MM-DD') AS joined_on,
                   to_char(mine.last_seen_at, 'YYYY-MM-DD') AS last_seen_on
              FROM public.packets p
              JOIN mine ON mine.storyline_id = p.storyline_id
              LEFT JOIN public.packets prior ON prior.id = p.supersedes_packet_id
             WHERE p.compiled_at >= NOW() - make_interval(hours => $4::int)
             ORDER BY p.storyline_id, p.compiled_at DESC, p.id DESC
        )
        SELECT * FROM latest
         ORDER BY compiled_epoch DESC, id DESC
         LIMIT $5
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport.to_uppercase())
    .bind(hours as i32)
    .bind(limit.max(1))
    .fetch_all(pool)
    .await
    .with_context(|| format!("load packets for {entity_type} {entity_id}"))?;

    // `latest` already fixed the per-storyline winner and ordered them newest-compiled first.
    let mut out: Vec<(PacketView, super::render::Participation)> = Vec::new();
    for r in rows {
        let claims_json: Value = r.get("claims");
        let facts: Value = r.get("facts");
        let claims = parse_claims(&claims_json);
        let name = String::new(); // filled by the caller, which already knows the entity's name
        out.push((
            PacketView {
                packet_id: r.get("id"),
                storyline_id: r.get("storyline_id"),
                sport: r.get("sport"),
                headline: r.get("headline"),
                story_types: r.get("story_types"),
                register: r.get("register"),
                register_phrase: r.get("register_phrase"),
                result_line: facts
                    .get("result_line")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                prior_headline: r.get("prior_headline"),
                claims,
                facts,
            },
            super::render::Participation {
                name,
                role: r.get("role"),
                joined_on: r.get("joined_on"),
                last_seen_on: r.get("last_seen_on"),
            },
        ));
    }
    Ok(out)
}

/// render_packets_for_entity is [`load_packets_for_entity`] plus the render (7.6/7.8): the
/// entity's live storylines, written down for ONE voice, newest-compiled first.
///
/// Every voice that reads the packet as a BLOCK goes through here, so the voice rule
/// ([`super::render::Voice`]) is applied in one place — the Influencer gets the register and its
/// phrase, nobody else does, and the Scout cannot be passed at all. Voices whose contract needs
/// the claims as data instead (the Journalist's numbered evidence, the Insider's per-article
/// overlay) compose the loader and the renderer themselves.
///
/// A render with nothing in it is dropped rather than returned empty: an empty block is not
/// material, and a caller counting blocks must not be told a story exists when none does.
pub async fn render_packets_for_entity(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport: &str,
    voice: super::render::Voice,
    limit: i64,
) -> Result<Vec<(i64, String)>> {
    let loaded = load_packets_for_entity(
        pool,
        entity_type,
        entity_id,
        sport,
        crate::junctions::journalist::PACKET_LOOKBACK_HOURS,
        limit,
    )
    .await?;

    Ok(loaded
        .into_iter()
        .filter_map(|(view, mut part)| {
            // The loader knows the id; the caller knows the name.
            part.name = entity_name.to_string();
            let packet_id = view.packet_id;
            let rendered = super::render::render(&view, Some(&part), voice);
            (!rendered.text.trim().is_empty()).then_some((packet_id, rendered.text))
        })
        .collect())
}

/// Parse `packets.claims` into the renderer's shape. A claim that does not carry the four
/// required fields is skipped rather than failing the read — the packet is archive, and one
/// malformed element must not cost a voice its whole context.
fn parse_claims(claims: &Value) -> Vec<super::render::RenderClaim> {
    claims
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    Some(super::render::RenderClaim {
                        article_id: c.get("article_id")?.as_i64()?,
                        source: c.get("source")?.as_str()?.to_string(),
                        fact: c.get("fact")?.as_str()?.to_string(),
                        published_at: c.get("published_at").and_then(|v| v.as_i64()),
                        // Packets compiled before 7.2 carry no type; an untyped claim is not in
                        // any voice's narrowed slice, which is the safe side of that fence.
                        story_type: c
                            .get("story_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn load_members(pool: &PgPool, storyline_id: i64) -> Result<Vec<Member>> {
    let rows = sqlx::query(
        r#"
        SELECT a.id, a.title, COALESCE(a.source, '') AS source, a.feed_rank,
               EXTRACT(EPOCH FROM a.published_at)::bigint AS published_at,
               er.read, er.resolved, a.full_text
          FROM public.storyline_articles sa
          JOIN public.news_articles a ON a.id = sa.article_id
          JOIN public.editor_reads er ON er.article_id = sa.article_id
         WHERE sa.storyline_id = $1
           AND er.status = 'success'
         ORDER BY COALESCE(a.published_at, a.fetched_at) DESC, a.id DESC
         LIMIT $2
        "#,
    )
    .bind(storyline_id)
    .bind(MAX_MEMBERS)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load storyline members {storyline_id}"))?;

    let mut members = Vec::with_capacity(rows.len());
    for r in rows {
        let envelope: Value = r.get("read");
        let read: EditorRead = match serde_json::from_value(envelope) {
            Ok(read) => read,
            Err(e) => {
                // A read that will not deserialize is archive, not evidence: skip the member
                // rather than fail the packet (its article stays named in storyline_articles).
                tracing::warn!(
                    storyline_id,
                    article_id = r.get::<i64, _>("id"),
                    error = %e,
                    "packet: member read did not parse (skipping member)"
                );
                continue;
            }
        };
        let resolved: Value = r.get("resolved");
        let unresolved: Vec<NameMention> = resolved
            .get("unresolved")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        members.push(Member {
            article_id: r.get("id"),
            title: r.get("title"),
            source: r.get("source"),
            feed_rank: r.get("feed_rank"),
            published_at: r.get("published_at"),
            read,
            unresolved,
            body: r.get("full_text"),
        });
    }
    Ok(members)
}

async fn load_entities(pool: &PgPool, storyline_id: i64) -> Result<Vec<PacketEntity>> {
    let rows = sqlx::query(
        r#"
        SELECT se.entity_type, se.entity_id, se.sport, se.role,
               COALESCE(p.name, t.name, pe.full_name, '') AS name
          FROM public.storyline_entities se
          LEFT JOIN public.players p
            ON se.entity_type = 'player' AND p.id = se.entity_id AND p.sport = se.sport
          LEFT JOIN public.teams t
            ON se.entity_type = 'team' AND t.id = se.entity_id AND t.sport = se.sport
          LEFT JOIN public.persons pe
            ON se.entity_type = 'person' AND pe.id = se.entity_id
         WHERE se.storyline_id = $1
           AND se.left_at IS NULL
         ORDER BY se.entity_type, se.entity_id
        "#,
    )
    .bind(storyline_id)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load storyline entities {storyline_id}"))?;

    Ok(rows
        .into_iter()
        .map(|r| PacketEntity {
            entity_type: r.get("entity_type"),
            entity_id: r.get("entity_id"),
            sport: r.get("sport"),
            role: r.get("role"),
            name: r.get("name"),
        })
        .collect())
}

/// The append: a new packet, superseding this storyline's previous one. Nothing is ever
/// updated — the prior packet stays exactly as the voices read it.
async fn insert_packet(
    pool: &PgPool,
    storyline_id: i64,
    sport: &str,
    draft: &PacketDraft,
) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO public.packets (
            storyline_id, day, sport, compiled_at, headline, story_types, register,
            register_phrase, claims, facts, quotes, routing_tags, slice_fingerprints,
            unresolved_mentions, supersedes_packet_id, contract_version
        ) VALUES (
            $1, CURRENT_DATE, $2, NOW(), $3, $4::text[], $5,
            $6, $7::jsonb, $8::jsonb, $9::jsonb, $10::text[], $11::jsonb,
            $12::jsonb,
            (SELECT p.id FROM public.packets p
              WHERE p.storyline_id = $1
              ORDER BY p.compiled_at DESC, p.id DESC LIMIT 1),
            $13
        )
        RETURNING id
        "#,
    )
    .bind(storyline_id)
    .bind(sport.to_uppercase())
    .bind(draft.headline.as_deref())
    .bind(&draft.story_types)
    .bind(draft.register.as_deref())
    .bind(draft.register_phrase.as_deref())
    .bind(&draft.claims)
    .bind(&draft.facts)
    .bind(&draft.quotes)
    .bind(&draft.routing_tags)
    .bind(&draft.slice_fingerprints)
    .bind(&draft.unresolved_mentions)
    .bind(PACKET_CONTRACT_VERSION)
    .fetch_one(pool)
    .await
    .with_context(|| format!("insert packet for storyline {storyline_id}"))?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::junctions::editor::EditorRead;

    /// One canned member. `read` is built field by field rather than parsed, so a fixture states
    /// exactly what the Editor described and nothing else.
    fn member(
        article_id: i64,
        source: &str,
        feed_rank: Option<i32>,
        published_at: i64,
        story_type: &str,
        facts: &[&str],
    ) -> Member {
        Member {
            article_id,
            title: format!("{source} headline {article_id}"),
            source: source.to_string(),
            feed_rank,
            published_at: Some(published_at),
            read: EditorRead {
                relevant: true,
                source_language: "en".into(),
                page_kind: "article".into(),
                names: vec![],
                entity_roles: vec![],
                story_type: story_type.into(),
                result_line: String::new(),
                register_phrase: String::new(),
                register: "neutral".into(),
                key_facts: facts.iter().map(|f| f.to_string()).collect(),
                caveats: String::new(),
                evidence_blurb: "blurb".into(),
            },
            unresolved: vec![],
            body: None,
        }
    }

    fn entity(entity_type: &str, entity_id: i32, name: &str) -> PacketEntity {
        PacketEntity {
            entity_type: entity_type.into(),
            entity_id,
            sport: "FOOTBALL".into(),
            role: Some("subject".into()),
            name: name.into(),
        }
    }

    fn claim_facts(draft: &PacketDraft) -> Vec<String> {
        draft
            .claims
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["fact"].as_str().unwrap().to_string())
            .collect()
    }

    /// T3, the law of this phase: two sources contradicting each other are TWO claims on ONE
    /// packet, each attributed — never one merged claim, never two packets.
    #[test]
    fn a_contradiction_survives_compilation_attributed_to_both_sources() {
        let members = vec![
            member(2, "Marca", Some(1), 200, "transfer", &["Vinicius and Real Madrid have yet to reach agreement"]),
            member(1, "AS", Some(2), 100, "transfer", &["Vinicius and Real Madrid have reached agreement"]),
        ];
        let draft = compile(&members, &[entity("player", 1, "Vinicius")]);
        let facts = claim_facts(&draft);
        assert_eq!(facts.len(), 2, "both claims stay: {facts:?}");
        assert!(facts.iter().any(|f| f.contains("have reached")));
        assert!(facts.iter().any(|f| f.contains("yet to reach")));
        let sources: Vec<&str> = draft
            .claims
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["source"].as_str().unwrap())
            .collect();
        assert!(sources.contains(&"Marca") && sources.contains(&"AS"));
    }

    #[test]
    fn only_byte_identical_facts_collapse_and_the_first_filer_keeps_the_credit() {
        let members = vec![
            member(2, "Copycat Wire", Some(1), 200, "transfer", &["Rodri signs a new deal"]),
            member(1, "The Athletic", Some(2), 100, "transfer", &["Rodri signs a new deal"]),
        ];
        let draft = compile(&members, &[]);
        let claims = draft.claims.as_array().unwrap();
        assert_eq!(claims.len(), 1, "byte-identical facts are one claim");
        assert_eq!(
            claims[0]["source"].as_str().unwrap(),
            "The Athletic",
            "the first source to file it owns it"
        );
    }

    #[test]
    fn claims_are_newest_first() {
        let members = vec![
            member(3, "C", Some(1), 300, "transfer", &["newest"]),
            member(2, "B", Some(2), 200, "transfer", &["middle"]),
            member(1, "A", Some(3), 100, "transfer", &["oldest"]),
        ];
        let draft = compile(&members, &[]);
        assert_eq!(claim_facts(&draft), vec!["newest", "middle", "oldest"]);
    }

    #[test]
    fn the_headline_is_the_best_ranked_member_title() {
        let members = vec![
            member(2, "Blog", Some(9), 200, "transfer", &["b"]),
            member(1, "Reuters", Some(1), 100, "transfer", &["a"]),
            member(3, "Unranked", None, 300, "transfer", &["c"]),
        ];
        let draft = compile(&members, &[]);
        assert_eq!(draft.headline.as_deref(), Some("Reuters headline 1"));
    }

    #[test]
    fn the_register_is_the_one_the_members_agree_on_with_its_phrase() {
        let mut members = vec![
            member(3, "C", Some(1), 300, "transfer", &["c"]),
            member(2, "B", Some(2), 200, "transfer", &["b"]),
            member(1, "A", Some(3), 100, "transfer", &["a"]),
        ];
        members[0].read.register = "outrage".into();
        members[0].read.register_phrase = "supporters turned on the board".into();
        members[1].read.register = "celebration".into();
        members[1].read.register_phrase = "a night to remember".into();
        members[2].read.register = "outrage".into();
        members[2].read.register_phrase = "fury at the decision".into();
        let draft = compile(&members, &[]);
        assert_eq!(draft.register.as_deref(), Some("outrage"));
        assert_eq!(
            draft.register_phrase.as_deref(),
            Some("supporters turned on the board"),
            "the newest member carrying the winning register supplies the phrase"
        );
    }

    #[test]
    fn an_all_neutral_packet_carries_no_register() {
        let draft = compile(&[member(1, "A", Some(1), 100, "fixture", &["a"])], &[]);
        assert_eq!(draft.register, None);
        assert_eq!(draft.register_phrase, None);
        assert!(!draft.routing_tags.contains(&"charged".to_string()));
    }

    #[test]
    fn routing_tags_are_the_union_of_member_types_plus_charged() {
        let mut members = vec![
            member(2, "B", Some(1), 200, "injury", &["b"]),
            member(1, "A", Some(2), 100, "transfer", &["a"]),
        ];
        members[0].read.register = "resignation".into();
        members[0].read.register_phrase = "a long season".into();
        let draft = compile(&members, &[]);
        assert_eq!(
            draft.routing_tags,
            vec![
                "charged".to_string(),
                "injury".to_string(),
                "transfer".to_string()
            ]
        );
        assert_eq!(draft.story_types, vec!["injury", "transfer"]);
    }

    /// E2: a packet re-fans only to the voices whose slice moved. The fingerprints are what mig
    /// 206 reads, so a change that touches only the register must leave the Journalist's key
    /// exactly where it was.
    #[test]
    fn a_register_change_moves_only_the_vibe_fingerprint() {
        let members = vec![member(1, "A", Some(1), 100, "transfer", &["a fact"])];
        let before = compile(&members, &[]);
        let mut charged = members.clone();
        charged[0].read.register = "celebration".into();
        charged[0].read.register_phrase = "delirium".into();
        let after = compile(&charged, &[]);

        assert_eq!(
            before.slice_fingerprints["narratives"],
            after.slice_fingerprints["narratives"],
            "the Journalist's slice did not move"
        );
        assert_eq!(
            before.slice_fingerprints["transfers"], after.slice_fingerprints["transfers"],
            "the Insider's slice did not move"
        );
        assert_ne!(
            before.slice_fingerprints["vibe"], after.slice_fingerprints["vibe"],
            "the Influencer's slice DID move"
        );
    }

    #[test]
    fn a_new_claim_moves_the_journalist_and_the_insider() {
        let members = vec![member(1, "A", Some(1), 100, "transfer", &["a fact"])];
        let before = compile(&members, &[]);
        let after = compile(
            &[
                member(2, "B", Some(1), 200, "transfer", &["a second fact"]),
                members[0].clone(),
            ],
            &[],
        );
        assert_ne!(
            before.slice_fingerprints["narratives"],
            after.slice_fingerprints["narratives"]
        );
        assert_ne!(
            before.slice_fingerprints["transfers"],
            after.slice_fingerprints["transfers"]
        );
    }

    #[test]
    fn the_transfers_slice_ignores_non_transfer_claims() {
        let transfer_only = compile(
            &[member(1, "A", Some(1), 100, "transfer", &["a fact"])],
            &[],
        );
        let with_an_injury = compile(
            &[
                member(2, "B", Some(1), 200, "injury", &["an injury fact"]),
                member(1, "A", Some(2), 100, "transfer", &["a fact"]),
            ],
            &[],
        );
        assert_eq!(
            transfer_only.slice_fingerprints["transfers"],
            with_an_injury.slice_fingerprints["transfers"],
            "the Insider reads transfer-typed claims only"
        );
    }

    /// Mig 206 reads `slice_fingerprints ->> stage`. Character names in the keys would fail open
    /// on every packet forever (a silent re-fan, never a starve — but never the E2 debounce
    /// either), so the keys are pinned by a test.
    #[test]
    fn fingerprint_keys_are_stage_strings() {
        let draft = compile(&[member(1, "A", Some(1), 100, "transfer", &["a"])], &[]);
        let keys: Vec<&String> = draft
            .slice_fingerprints
            .as_object()
            .unwrap()
            .keys()
            .collect();
        // `rating` joined 2026-08-23 — the Scout's availability slice. A key here is what mig
        // 225 reads to mint a voice's `pk:` version, so its ABSENCE was why an injury packet
        // fell through to the packet id and enqueued him once per packet instead of once per
        // change of fact.
        assert_eq!(keys, vec!["narratives", "rating", "transfers", "vibe"]);
    }

    /// The Scout's slice moves on availability news and ONLY on availability news — that is what
    /// makes the Editor's tag a tag rather than a firehose.
    #[test]
    fn the_rating_slice_tracks_availability_claims_alone() {
        let base = compile(&[member(1, "A", Some(1), 100, "transfer", &["a"])], &[]);
        let plus_transfer = compile(
            &[
                member(1, "A", Some(1), 100, "transfer", &["a"]),
                member(2, "B", Some(1), 200, "transfer", &["another move"]),
            ],
            &[],
        );
        // A transfer moved the Insider's slice but must not wake the Scout.
        assert_ne!(
            base.slice_fingerprints["transfers"],
            plus_transfer.slice_fingerprints["transfers"]
        );
        assert_eq!(
            base.slice_fingerprints["rating"],
            plus_transfer.slice_fingerprints["rating"]
        );

        // An injury moves his slice.
        let plus_injury = compile(
            &[
                member(1, "A", Some(1), 100, "transfer", &["a"]),
                member(3, "C", Some(1), 300, "injury", &["out six weeks"]),
            ],
            &[],
        );
        assert_ne!(
            base.slice_fingerprints["rating"],
            plus_injury.slice_fingerprints["rating"]
        );

        // So does a suspension — one subject, two causes.
        let plus_suspension = compile(
            &[
                member(1, "A", Some(1), 100, "transfer", &["a"]),
                member(4, "D", Some(1), 400, "suspension", &["banned three games"]),
            ],
            &[],
        );
        assert_ne!(
            base.slice_fingerprints["rating"],
            plus_suspension.slice_fingerprints["rating"]
        );
        assert_ne!(
            plus_injury.slice_fingerprints["rating"],
            plus_suspension.slice_fingerprints["rating"]
        );
    }

    #[test]
    fn facts_stay_thin_and_name_the_participants() {
        let members = vec![member(1, "A", Some(1), 100, "fixture", &["a"])];
        let mut with_result = members.clone();
        with_result[0].read.result_line = "Real Madrid 2-1 Arsenal".into();
        let draft = compile(&with_result, &[entity("team", 3468, "Real Madrid")]);
        assert_eq!(
            draft.facts["result_line"].as_str(),
            Some("Real Madrid 2-1 Arsenal")
        );
        assert_eq!(draft.facts["member_articles"].as_i64(), Some(1));
        assert_eq!(draft.facts["entities"][0]["name"].as_str(), Some("Real Madrid"));
        // No prose reaches the Scout through here (T4).
        assert!(draft.facts.get("claims").is_none());
        assert!(draft.facts.get("quotes").is_none());
    }

    #[test]
    fn dropped_claims_are_named_never_silently_truncated() {
        // One member with more facts than the cap: what did not fit is reported by article.
        let mut big = member(1, "A", Some(1), 100, "transfer", &[]);
        big.read.key_facts = (0..MAX_CLAIMS + 5).map(|i| format!("fact {i}")).collect();
        let draft = compile(&[big], &[]);
        assert_eq!(draft.claims.as_array().unwrap().len(), MAX_CLAIMS);
        assert_eq!(
            draft.facts["claims_dropped_from"].as_array().unwrap().len(),
            5,
            "the A5 rule: evidence dropped from the packet is named"
        );
    }

    #[test]
    fn quotes_are_sliced_from_stored_bodies_around_a_participant() {
        let mut m = member(1, "A", Some(1), 100, "transfer", &["a"]);
        m.body = Some(format!(
            "{} Vinicius Junior trained alone on Tuesday. {}",
            "x".repeat(400),
            "y".repeat(400)
        ));
        let draft = compile(&[m], &[entity("player", 1, "Vinicius Junior")]);
        let quotes = draft.quotes.as_array().unwrap();
        assert_eq!(quotes.len(), 1);
        assert!(quotes[0]["quote"]
            .as_str()
            .unwrap()
            .contains("trained alone on Tuesday"));
        assert_eq!(quotes[0]["article_id"].as_i64(), Some(1));
    }

    #[test]
    fn a_member_with_no_stored_body_simply_contributes_no_quote() {
        let draft = compile(
            &[member(1, "A", Some(1), 100, "transfer", &["a"])],
            &[entity("player", 1, "Vinicius")],
        );
        assert!(draft.quotes.as_array().unwrap().is_empty());
    }

    #[test]
    fn the_unresolved_census_counts_mentions_and_keeps_a_descriptor() {
        let mut a = member(1, "A", Some(1), 100, "transfer", &["a"]);
        let mut b = member(2, "B", Some(2), 200, "transfer", &["b"]);
        a.unresolved = vec![NameMention {
            name: "Kyle Shanahan".into(),
            kind_hint: "person".into(),
            descriptor: String::new(),
        }];
        b.unresolved = vec![
            NameMention {
                name: "kyle shanahan".into(),
                kind_hint: "person".into(),
                descriptor: "49ers head coach".into(),
            },
            NameMention {
                name: "Some Agent".into(),
                kind_hint: "person".into(),
                descriptor: "player's representative".into(),
            },
        ];
        let draft = compile(&[b, a], &[]);
        let census = draft.unresolved_mentions.as_array().unwrap();
        assert_eq!(census.len(), 2);
        assert_eq!(census[0]["mentions"].as_i64(), Some(2));
        assert_eq!(
            census[0]["descriptor"].as_str(),
            Some("49ers head coach"),
            "a descriptor found on any mention survives the rollup"
        );
    }
}
