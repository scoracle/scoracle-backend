//! Discovery adapters (PLAN-one-rail 5.4) — discovery ≠ retrieval ≠ interpretation.
//!
//! *Discovery* finds candidate pages for a name; *retrieval* is the Phase 4.2
//! [`BudgetedFetcher`] (every page becomes a `source_documents` row — sources prove);
//! *interpretation* is either CODE over Wikidata's structured claims (the preferred path —
//! no model call at all) or gemma describing a prose page (the mystery-candidate fallback).
//!
//! The v1 source family is Wikimedia (Scott's NBA-vetting ruling, Phase 4 Log): the Wikidata
//! action API and Wikipedia REST are designed for programmatic reuse (the ONE family that
//! passed the 4.3 terms review with no reservations), structured, and stable. Google News
//! RSS remains a secondary discovery channel for names Wikimedia has never heard of; it
//! reuses the lungs' query shape and proves only that a name recurs, never an identity.

use crate::fetch::{BudgetedFetcher, FetchPolicy, SourceFetch};
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use sqlx::PgPool;

/// One Wikidata search hit, pre-retrieval.
#[derive(Clone, Debug)]
pub struct WikidataHit {
    pub qid: String,
    pub label: String,
    pub description: String,
}

/// A retrieved Wikidata item: the claims that matter, already parsed by CODE (never a
/// model), plus the provenance row that proves them.
#[derive(Clone, Debug, Default)]
pub struct WikidataItem {
    pub qid: String,
    pub label: String,
    pub description: String,
    pub aliases: Vec<String>,
    /// English Wikipedia page title, when sitelinked.
    pub enwiki_title: Option<String>,
    /// P106 occupation labels are ids; we keep the raw QIDs and let the gate map the few
    /// that matter (basketball player/coach etc.).
    pub occupations: Vec<String>,
    /// P54 (member of sports team) target QIDs — career teams.
    pub member_of_teams: Vec<String>,
    /// P6087 (coach of sports team) target QIDs.
    pub coach_of_teams: Vec<String>,
    /// P569 date of birth, as the wire "+1988-12-30T00:00:00Z" shape (code trims to date).
    pub date_of_birth: Option<String>,
    /// P2067 mass in kilograms (unit-checked: only Q11570 kilogram amounts are kept).
    pub weight_kg: Option<f64>,
    /// P2048 height in centimeters (unit-checked: Q174728 cm / Q11573 m normalized to cm).
    pub height_cm: Option<f64>,
    /// P3647 NBA.com player id — the headshot URL derives from this.
    pub nba_id: Option<String>,
    /// The source_documents row the wbgetentities response landed as.
    pub source_document_id: i64,
}

const WIKIDATA_API: &str = "https://www.wikidata.org/w/api.php";

/// wikidata_search finds up to `limit` items for a name. The search response itself is
/// retrieved through the budgeted fetcher (provenance like everything else).
pub async fn wikidata_search(
    fetcher: &BudgetedFetcher,
    pool: &PgPool,
    policy: &FetchPolicy,
    name: &str,
    limit: usize,
) -> Result<Vec<WikidataHit>> {
    let url = format!(
        "{WIKIDATA_API}?action=wbsearchentities&search={}&language=en&type=item&limit={}&format=json",
        urlencode(name),
        limit.min(10)
    );
    let fetched = fetcher
        .fetch(pool, &url, policy)
        .await
        .map_err(|e| anyhow!("wikidata search fetch: {e}"))?;
    let v: Value = serde_json::from_str(&fetched.body).context("parse wbsearchentities")?;
    let mut out = Vec::new();
    if let Some(hits) = v.get("search").and_then(Value::as_array) {
        for h in hits {
            let Some(qid) = h.get("id").and_then(Value::as_str) else {
                continue;
            };
            out.push(WikidataHit {
                qid: qid.to_string(),
                label: str_at(h, "label"),
                description: str_at(h, "description"),
            });
        }
    }
    Ok(out)
}

/// wikidata_item retrieves one item's claims/labels/aliases/sitelinks and parses the claims
/// this rail consumes. Pure JSON → struct; the model never sees this.
pub async fn wikidata_item(
    fetcher: &BudgetedFetcher,
    pool: &PgPool,
    policy: &FetchPolicy,
    qid: &str,
) -> Result<WikidataItem> {
    let url = format!(
        "{WIKIDATA_API}?action=wbgetentities&ids={}&props=claims%7Clabels%7Cdescriptions%7Caliases%7Csitelinks&languages=en&format=json",
        urlencode(qid)
    );
    let fetched = fetcher
        .fetch(pool, &url, policy)
        .await
        .map_err(|e| anyhow!("wikidata item fetch: {e}"))?;
    let v: Value = serde_json::from_str(&fetched.body).context("parse wbgetentities")?;
    let entity = v
        .pointer(&format!("/entities/{qid}"))
        .ok_or_else(|| anyhow!("wikidata item {qid} missing from response"))?;
    Ok(parse_wikidata_entity(qid, entity, fetched.document_id))
}

/// parse_wikidata_entity is the pure claims parser — unit-tested against canned fixtures.
pub fn parse_wikidata_entity(qid: &str, entity: &Value, source_document_id: i64) -> WikidataItem {
    let mut item = WikidataItem {
        qid: qid.to_string(),
        label: entity
            .pointer("/labels/en/value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        description: entity
            .pointer("/descriptions/en/value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        enwiki_title: entity
            .pointer("/sitelinks/enwiki/title")
            .and_then(Value::as_str)
            .map(str::to_string),
        source_document_id,
        ..Default::default()
    };
    if let Some(aliases) = entity.pointer("/aliases/en").and_then(Value::as_array) {
        item.aliases = aliases
            .iter()
            .filter_map(|a| a.get("value").and_then(Value::as_str))
            .map(str::to_string)
            .collect();
    }
    item.occupations = claim_item_ids(entity, "P106", false);
    // P54 keeps the whole career — the discriminator wants history. P6087 keeps only
    // CURRENT tenures (no P582 end-time qualifier): a stale coaching stint must neither
    // classify someone a coach today nor mint a wrong coach_of edge (the Pat Riley case —
    // executive now, coach decades ago).
    item.member_of_teams = claim_item_ids(entity, "P54", false);
    item.coach_of_teams = claim_item_ids(entity, "P6087", true);
    item.date_of_birth = entity
        .pointer("/claims/P569/0/mainsnak/datavalue/value/time")
        .and_then(Value::as_str)
        .map(str::to_string);
    item.weight_kg = quantity_in(entity, "P2067", &[("Q11570", 1.0)]);
    item.height_cm = quantity_in(entity, "P2048", &[("Q174728", 1.0), ("Q11573", 100.0)]);
    item.nba_id = entity
        .pointer("/claims/P3647/0/mainsnak/datavalue/value")
        .and_then(Value::as_str)
        .map(str::to_string);
    item
}

/// claim_item_ids collects every wikibase-entityid target of a property's statements.
/// `current_only` drops statements carrying a P582 (end time) qualifier — ended tenures.
fn claim_item_ids(entity: &Value, prop: &str, current_only: bool) -> Vec<String> {
    let Some(claims) = entity
        .pointer(&format!("/claims/{prop}"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    claims
        .iter()
        .filter(|c| !current_only || c.pointer("/qualifiers/P582").is_none())
        .filter_map(|c| {
            c.pointer("/mainsnak/datavalue/value/id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

/// quantity_in reads a quantity claim, unit-gated: `accepted` maps unit QIDs to the factor
/// that converts the stored amount into OUR unit. Unknown units are DROPPED, never guessed
/// (a pound amount must not land in a kilogram column).
fn quantity_in(entity: &Value, prop: &str, accepted: &[(&str, f64)]) -> Option<f64> {
    let amount = entity
        .pointer(&format!("/claims/{prop}/0/mainsnak/datavalue/value/amount"))
        .and_then(Value::as_str)?;
    let unit = entity
        .pointer(&format!("/claims/{prop}/0/mainsnak/datavalue/value/unit"))
        .and_then(Value::as_str)?;
    let unit_qid = unit.rsplit('/').next().unwrap_or_default();
    let factor = accepted
        .iter()
        .find(|(q, _)| *q == unit_qid)
        .map(|(_, f)| *f)?;
    amount.trim_start_matches('+').parse::<f64>().ok().map(|a| a * factor)
}

/// wikipedia_summary retrieves the REST summary for a title — the prose the model may be
/// asked to describe on the mystery-candidate path. Returns the fetch (for provenance) —
/// the caller extracts `extract`/`description` from the JSON body.
pub async fn wikipedia_summary(
    fetcher: &BudgetedFetcher,
    pool: &PgPool,
    policy: &FetchPolicy,
    title: &str,
) -> Result<SourceFetch> {
    let url = format!(
        "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
        urlencode(&title.replace(' ', "_"))
    );
    fetcher
        .fetch(pool, &url, policy)
        .await
        .map_err(|e| anyhow!("wikipedia summary fetch: {e}"))
}

/// urlencode covers the query/path characters these APIs receive from names. Conservative
/// percent-encoding of everything outside unreserved.
pub(crate) fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn str_at(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entity() -> Value {
        json!({
            "labels": {"en": {"value": "Erik Spoelstra"}},
            "descriptions": {"en": {"value": "American basketball coach"}},
            "aliases": {"en": [{"value": "Erik Jon Spoelstra"}]},
            "sitelinks": {"enwiki": {"title": "Erik Spoelstra"}},
            "claims": {
                "P106": [{"mainsnak": {"datavalue": {"value": {"id": "Q5137571"}}}}],
                "P6087": [{"mainsnak": {"datavalue": {"value": {"id": "Q169138"}}}}],
                "P569": [{"mainsnak": {"datavalue": {"value": {"time": "+1970-11-01T00:00:00Z"}}}}],
                "P2067": [{"mainsnak": {"datavalue": {"value": {
                    "amount": "+83", "unit": "http://www.wikidata.org/entity/Q11570"
                }}}}],
                "P2048": [{"mainsnak": {"datavalue": {"value": {
                    "amount": "+1.85", "unit": "http://www.wikidata.org/entity/Q11573"
                }}}}]
            }
        })
    }

    #[test]
    fn parses_the_claims_this_rail_consumes() {
        let item = parse_wikidata_entity("Q433386", &entity(), 42);
        assert_eq!(item.label, "Erik Spoelstra");
        assert_eq!(item.description, "American basketball coach");
        assert_eq!(item.aliases, vec!["Erik Jon Spoelstra"]);
        assert_eq!(item.enwiki_title.as_deref(), Some("Erik Spoelstra"));
        assert_eq!(item.occupations, vec!["Q5137571"]);
        assert_eq!(item.coach_of_teams, vec!["Q169138"]);
        assert_eq!(item.date_of_birth.as_deref(), Some("+1970-11-01T00:00:00Z"));
        assert_eq!(item.weight_kg, Some(83.0));
        assert_eq!(item.height_cm, Some(185.0));
        assert_eq!(item.source_document_id, 42);
    }

    #[test]
    fn ended_coaching_tenures_are_dropped_from_coach_of() {
        // The Pat Riley shape: a P6087 statement with a P582 end qualifier is an ENDED
        // tenure — it must not classify someone a coach today or mint a coach_of edge.
        let mut e = entity();
        e["claims"]["P6087"] = serde_json::json!([
            {"mainsnak": {"datavalue": {"value": {"id": "Q121783"}}},
             "qualifiers": {"P582": [{"datavalue": {"value": {"time": "+1990-01-01T00:00:00Z"}}}]}},
            {"mainsnak": {"datavalue": {"value": {"id": "Q169138"}}}}
        ]);
        let item = parse_wikidata_entity("Q1", &e, 1);
        assert_eq!(item.coach_of_teams, vec!["Q169138"], "only the open tenure survives");
        // P54 career history keeps ended stints — the discriminator wants them.
        e["claims"]["P54"] = e["claims"]["P6087"].clone();
        let item = parse_wikidata_entity("Q1", &e, 1);
        assert_eq!(item.member_of_teams.len(), 2);
    }

    #[test]
    fn unknown_units_are_dropped_never_guessed() {
        let mut e = entity();
        // Pounds (Q100995) must not flow into a kg field.
        e["claims"]["P2067"][0]["mainsnak"]["datavalue"]["value"]["unit"] =
            json!("http://www.wikidata.org/entity/Q100995");
        let item = parse_wikidata_entity("Q1", &e, 1);
        assert_eq!(item.weight_kg, None);
    }

    #[test]
    fn urlencode_covers_names_and_qids() {
        assert_eq!(urlencode("Erik Spoelstra"), "Erik%20Spoelstra");
        assert_eq!(urlencode("Q433386"), "Q433386");
        assert_eq!(urlencode("Dončić"), "Don%C4%8Di%C4%87");
    }
}
