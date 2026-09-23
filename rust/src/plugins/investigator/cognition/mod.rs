//! Investigator judgment over prepared evidence. No database, queue, or retrieval handles.
use crate::studio::{Extracted, Studio};
use anyhow::Result;
pub mod gate;
pub mod prompt;
use prompt::{build_prose_prompt, page_text, prose_opts, ProseRead, ProseReadParser};

pub struct Assignment<'a> {
    pub sought_name: &'a str,
    pub descriptor: Option<&'a str>,
    pub sport: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub extract: &'a str,
}

pub async fn investigate_prose(
    studio: &Studio<'_>,
    a: &Assignment<'_>,
) -> Result<Extracted<ProseRead>> {
    let prompt = build_prose_prompt(
        a.sought_name,
        a.descriptor,
        a.sport,
        a.title,
        a.description,
        a.extract,
    );
    let mut result = studio
        .extract(&prompt, &prose_opts(), &ProseReadParser)
        .await?;
    if let Some(read) = result.value.as_mut() {
        let shown = page_text(a.title, a.description, a.extract);
        if !gate::contains_normalized(&shown, &read.sought_name_evidence) {
            read.sought_name_evidence.clear();
        }
        if !gate::contains_normalized(&shown, &read.occupation_phrase) {
            read.occupation_phrase.clear();
        }
        read.team_names
            .retain(|t| gate::contains_normalized(&shown, t));
    }
    Ok(result)
}

impl ProseRead {
    /// Studio has already removed every quote not present in the shown page. Storage
    /// contributes only the sport-scoped team match; classification stays in Studio.
    pub fn screen(
        &self,
        sport: &str,
        descriptor: Option<&str>,
        team_matched: bool,
    ) -> gate::ProseScreen {
        let role = gate::prose_role_class(sport, &self.occupation_phrase, team_matched);
        let descriptor_role = descriptor
            .map(gate::descriptor_role_class)
            .unwrap_or(gate::RoleClass::Unknown);
        gate::ProseScreen {
            evidence_ok: self.subject_kind == "person" && !self.sought_name_evidence.is_empty(),
            role,
            team_matched,
            descriptor_conflict: descriptor_role != gate::RoleClass::Unknown
                && role != gate::RoleClass::Unknown
                && descriptor_role != role,
        }
    }
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
    /// P1830 (owner of) target QIDs for current tenures.
    pub owner_of_teams: Vec<String>,
    /// P569 date of birth, as the wire "+1988-12-30T00:00:00Z" shape (code trims to date).
    pub date_of_birth: Option<String>,
    /// P2067 mass in kilograms (unit-checked: only Q11570 kilogram amounts are kept).
    pub weight_kg: Option<f64>,
    /// P2048 height in centimeters (unit-checked: Q174728 cm / Q11573 m normalized to cm).
    pub height_cm: Option<f64>,
    /// P3647 NBA.com player id — the headshot URL derives from this.
    pub nba_id: Option<String>,
    /// P18 image: the sport-agnostic Wikimedia Commons portrait source.
    pub image_file: Option<String>,
    /// P115 home venue target QID, current tenure only — team-shaped (mig 236 dynamic
    /// metadata); None for person items.
    pub venue_qid: Option<String>,
    /// P154 logo image — Commons filename; team-shaped.
    pub logo_file: Option<String>,
    /// The source_documents row the wbgetentities response landed as.
    pub source_document_id: i64,
}

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

#[cfg(test)]
mod tests;
