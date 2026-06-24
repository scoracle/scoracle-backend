//! Resolve — the embedding-backed same-name disambiguation gate (Plan §1.3), turned REAL in L4.
//!
//! This is the foundational value the candle pivot buys: ONE tested disambiguation capability,
//! reused by the scrub gate (`resolve_set`) and the transfer subject-test (`resolve_one`), instead
//! of Go's scattered per-stage prompt-stuffing. It is a HYBRID, evidence-based on the L4 experiment
//! (AUC 0.88 on the live vetted-label set):
//!
//!   * a cheap **CPU embedding cosine** between the article context and each candidate's identity
//!     card auto-decides the confident tails — `≥ keep_threshold` → keep, `< drop_threshold` → drop,
//!     with NO model call;
//!   * only the **ambiguous middle band** spends a Gemma adjudication (the scrub relevance prompt).
//!
//! Net: the GPU runs on ~half the candidates it used to, disambiguation is more accurate than
//! prompt-stuffing, and the gate is one place. The embeddings run on the CPU (Plan §1.4), so this
//! REDUCES GPU load rather than adding to it.
//!
//! Fail-closed (the §1.2 invariant, here too): an ambiguous candidate whose Gemma adjudication
//! fails to parse is **dropped** (`resolve_set`) / yields **`None`** (`resolve_one`) — never a guess.
//!
//! These `impl Harness` methods drop in BEHIND the existing `resolve_one`/`resolve_set` signatures
//! (in [`crate::harness`]) with no change to them — the Plan §5 test that the library was drawn
//! right. Requires `Harness.embedder = Some(..)`; an embed-less Harness errors (a wiring bug).

use crate::harness::{Candidate, EntityType, Harness, Parser, Resolution, Resolved};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::config::ResolveConfig;
use crate::embed::cosine_similarity;
use anyhow::Result;
use serde::Deserialize;

/// The relevance system prompt — the same disambiguation framing the Go scrub uses (current club is
/// the strongest same-name tie-breaker), asking for the JSON `{"relevant":[…]}` contract.
const RESOLVE_SYSTEM_PROMPT: &str = "You decide which of the listed players/teams a news article is GENUINELY ABOUT, so we tag it correctly. Same-name people are common — use each player's identity (nationality, current club, position) to tell them apart; CURRENT CLUB is the strongest tie-breaker.\n\nA candidate is RELEVANT if the article genuinely concerns that EXACT person/team — a real subject or a meaningful mention. A candidate is NOT relevant when:\n- it is a DIFFERENT same-name person — the article's club/position/role contradicts the identity (e.g. a club president or a manager, or a different player at another club). When the identity's current club is contradicted by the article, it is a different person.\n- the name appears only as incidental noise (a long roundup where they are not actually discussed).\n\nBe inclusive for genuine mentions, strict on same-name confusion. Reply with ONLY a JSON object, no prose:\n{\"relevant\": [<the candidate numbers that are genuinely about this article>]}";

/// The model budget for one adjudication call. Temperature 0.2 mirrors the scrub's "tight but a
/// judgment call"; JSON mode tightens adherence to the `{"relevant":[…]}` contract.
fn adjudication_opts() -> GenerateOptions {
    GenerateOptions {
        system: Some(RESOLVE_SYSTEM_PROMPT.to_string()),
        temperature: Some(0.2),
        num_predict: 512,
        json_mode: true,
    }
}

/// A candidate's band decision from the cosine pre-filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decision {
    Keep,
    Drop,
    Ambiguous,
}

/// classify bands a cosine against the configured thresholds. `≥ keep` keep, `< drop` drop, else
/// ambiguous (→ the model). With `drop ≤ keep` (the config invariant) the three are exhaustive.
fn classify(cosine: f32, cfg: &ResolveConfig) -> Decision {
    if cosine >= cfg.keep_threshold {
        Decision::Keep
    } else if cosine < cfg.drop_threshold {
        Decision::Drop
    } else {
        Decision::Ambiguous
    }
}

/// identity_text renders a candidate's identity card as a natural sentence — the text the embedder
/// compares against the article, and the line the model sees. Current club leads (the strongest
/// disambiguator); teams need no card. Mirrors the L4 experiment's framing so the live gate matches
/// what was measured.
pub fn identity_text(c: &Candidate) -> String {
    if c.entity_type == EntityType::Team {
        return format!("{} (team)", c.name);
    }
    let nat = c.identity.nationality.as_deref().unwrap_or("");
    let pos = c.identity.position.as_deref().unwrap_or("");
    let club = c.identity.current_club.as_deref().unwrap_or("");
    let descriptor = match (nat.is_empty(), pos.is_empty()) {
        (false, false) => format!("a {nat} {pos}"),
        (false, true) => format!("a {nat} player"),
        (true, false) => format!("a {pos}"),
        (true, true) => String::new(),
    };
    let mut clauses = Vec::new();
    if !descriptor.is_empty() {
        clauses.push(descriptor);
    }
    if club.is_empty() {
        clauses.push("current club unknown".to_string());
    } else {
        clauses.push(format!("currently at {club}"));
    }
    format!("{}, {}.", c.name, clauses.join(", "))
}

/// build_relevance_prompt lays out the article + the numbered candidate identities for the model to
/// judge (1-indexed, matching the `{"relevant":[…]}` reply).
fn build_relevance_prompt(context: &str, candidates: &[&Candidate]) -> String {
    let mut b = String::from("Article:\n");
    b.push_str(context);
    b.push_str("\n\nCandidates (same-name people may appear — disambiguate by identity):\n");
    for (i, c) in candidates.iter().enumerate() {
        b.push_str(&format!("{}. {}\n", i + 1, identity_text(c)));
    }
    b.push_str("\nReturn the JSON now.");
    b
}

/// RelevanceParser turns the model's `{"relevant":[…]}` reply into the set of 1-indexed candidate
/// numbers judged relevant. Fail-closed: no JSON object, or unparseable, ⇒ `Ok(None)` (the caller
/// drops the ambiguous candidates). Out-of-range indices are ignored. Mirrors the Go
/// `parseScrubRelevant` contract.
pub struct RelevanceParser {
    pub n: usize,
}

impl Parser<Vec<usize>> for RelevanceParser {
    fn parse(&self, raw: &str) -> Result<Option<Vec<usize>>> {
        let (start, end) = match (raw.find('{'), raw.rfind('}')) {
            (Some(s), Some(e)) if e > s => (s, e),
            _ => return Ok(None), // no JSON object → fail-closed
        };
        #[derive(Deserialize)]
        struct Reply {
            #[serde(default)]
            relevant: Vec<i64>,
        }
        let reply: Reply = match serde_json::from_str(&raw[start..=end]) {
            Ok(r) => r,
            Err(_) => return Ok(None), // unparseable → fail-closed
        };
        let kept = reply
            .relevant
            .into_iter()
            .filter(|&i| i >= 1 && (i as usize) <= self.n)
            .map(|i| i as usize)
            .collect();
        Ok(Some(kept))
    }
}

impl Harness {
    /// resolve_set vets WHICH of N linked candidates an article is genuinely about — the scrub gate
    /// shape (Plan §1.3), the clean 1:1 with `news_scrub.go::ScrubArticle`. Embeds the context + each
    /// identity once, bands each by cosine, and sends only the ambiguous band to the model in a
    /// SINGLE adjudication call. Fail-closed: if that call fails to parse, the ambiguous candidates
    /// are dropped (`kept = false`), never kept on a guess.
    ///
    /// `role` names the adjudicator (typically [`Role::EmotionalNews`], the news-relevance reasoner).
    pub async fn resolve_set(
        &self,
        role: Role,
        context: &str,
        candidates: &[Candidate],
    ) -> Result<Vec<Resolution>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        // One embed call: [context, identity_0, identity_1, …].
        let mut texts = Vec::with_capacity(candidates.len() + 1);
        texts.push(context.to_string());
        texts.extend(candidates.iter().map(identity_text));
        let vectors = self.embed(&texts).await?;
        let (ctx, identities) = vectors.split_first().expect("embed returns ≥1 vector here");

        let mut out = Vec::with_capacity(candidates.len());
        let mut ambiguous = Vec::new(); // indices needing the model
        for (i, c) in candidates.iter().enumerate() {
            let cosine = cosine_similarity(ctx, &identities[i]);
            let kept = match classify(cosine, &self.resolve) {
                Decision::Keep => true,
                Decision::Drop => false,
                Decision::Ambiguous => {
                    ambiguous.push(i);
                    false // placeholder; the adjudication below sets it
                }
            };
            out.push(Resolution {
                entity_id: c.entity_id,
                entity_type: c.entity_type,
                kept,
            });
        }

        if !ambiguous.is_empty() {
            let amb: Vec<&Candidate> = ambiguous.iter().map(|&i| &candidates[i]).collect();
            let prompt = build_relevance_prompt(context, &amb);
            let extracted = self
                .extract(role, &prompt, &adjudication_opts(), &RelevanceParser { n: amb.len() })
                .await?;
            if let Some(relevant) = extracted.value {
                // relevant is 1-indexed over `amb`, in band order.
                for (k, &i) in ambiguous.iter().enumerate() {
                    out[i].kept = relevant.contains(&(k + 1));
                }
            }
            // else: fail-closed — the ambiguous candidates keep their `kept = false` placeholder.
        }
        Ok(out)
    }

    /// resolve_one settles which ONE candidate (if any) a mention is — the transfer subject-test
    /// shape (Plan §1.3). Embeds the context against each candidate identity, takes the best match,
    /// and: auto-keeps it if confidently high, returns `None` if confidently low (not about any of
    /// them), or asks the model to confirm the best candidate in the ambiguous band. Fail-closed:
    /// ambiguous-and-unconfirmed ⇒ `None`, never a guess. `subject` records who it resolved to.
    pub async fn resolve_one(
        &self,
        role: Role,
        raw_token: &str,
        context: &str,
        candidates: &[Candidate],
    ) -> Result<Option<Resolved>> {
        if candidates.is_empty() {
            return Ok(None);
        }
        let mut texts = Vec::with_capacity(candidates.len() + 1);
        texts.push(context.to_string());
        texts.extend(candidates.iter().map(identity_text));
        let vectors = self.embed(&texts).await?;
        let (ctx, identities) = vectors.split_first().expect("embed returns ≥1 vector here");

        // Best-matching candidate by cosine.
        let mut best = (0usize, f32::NEG_INFINITY);
        for (i, ident) in identities.iter().enumerate() {
            let cosine = cosine_similarity(ctx, ident);
            if cosine > best.1 {
                best = (i, cosine);
            }
        }
        let chosen = &candidates[best.0];
        let resolved = || {
            Some(Resolved {
                entity_id: chosen.entity_id,
                entity_type: chosen.entity_type,
                subject: chosen.name.clone(),
            })
        };

        match classify(best.1, &self.resolve) {
            Decision::Keep => Ok(resolved()),
            Decision::Drop => Ok(None), // the text is not about any candidate
            Decision::Ambiguous => {
                // Confirm the single best candidate with the model. The raw token names the mention
                // being resolved, so the model judges THIS surface form.
                let prompt = format!(
                    "Mention to resolve: \"{raw_token}\"\n\n{}",
                    build_relevance_prompt(context, &[chosen])
                );
                let extracted = self
                    .extract(role, &prompt, &adjudication_opts(), &RelevanceParser { n: 1 })
                    .await?;
                match extracted.value {
                    Some(relevant) if relevant.contains(&1) => Ok(resolved()),
                    _ => Ok(None), // unconfirmed or fail-closed
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::{Candidate, IdentityCard};

    fn player(name: &str, nationality: Option<&str>, club: Option<&str>, position: Option<&str>) -> Candidate {
        Candidate {
            entity_type: EntityType::Player,
            entity_id: 1,
            name: name.to_string(),
            identity: IdentityCard {
                nationality: nationality.map(str::to_string),
                current_club: club.map(str::to_string),
                position: position.map(str::to_string),
            },
        }
    }

    #[test]
    fn classify_bands() {
        let cfg = ResolveConfig { keep_threshold: 0.75, drop_threshold: 0.60 };
        assert_eq!(classify(0.80, &cfg), Decision::Keep);
        assert_eq!(classify(0.75, &cfg), Decision::Keep); // inclusive
        assert_eq!(classify(0.70, &cfg), Decision::Ambiguous);
        assert_eq!(classify(0.60, &cfg), Decision::Ambiguous); // drop is exclusive
        assert_eq!(classify(0.59, &cfg), Decision::Drop);
    }

    #[test]
    fn identity_text_player_and_team() {
        let p = player("João Silva", Some("Portuguese"), Some("Benfica"), Some("midfielder"));
        assert_eq!(identity_text(&p), "João Silva, a Portuguese midfielder, currently at Benfica.");
        let sparse = player("X", None, None, None);
        assert_eq!(identity_text(&sparse), "X, current club unknown.");
        let team = Candidate {
            entity_type: EntityType::Team,
            entity_id: 9,
            name: "Chelsea".to_string(),
            identity: IdentityCard::default(),
        };
        assert_eq!(identity_text(&team), "Chelsea (team)");
    }

    #[test]
    fn relevance_parser_fail_closed() {
        let p = RelevanceParser { n: 3 };
        // Fail-closed cases → None (caller drops).
        assert_eq!(p.parse("").unwrap(), None);
        assert_eq!(p.parse("Sorry, I can't tell.").unwrap(), None);
        assert_eq!(p.parse("{not json").unwrap(), None);
        // Valid → the in-range relevant set; out-of-range filtered.
        assert_eq!(p.parse(r#"{"relevant":[1,3]}"#).unwrap(), Some(vec![1, 3]));
        assert_eq!(p.parse(r#"{"relevant":[2,9,0]}"#).unwrap(), Some(vec![2])); // 9,0 dropped
        // Missing key → empty set (everything ambiguous gets dropped, the conservative call).
        assert_eq!(p.parse(r#"{"other":true}"#).unwrap(), Some(vec![]));
        // Wrapped in prose → salvaged.
        assert_eq!(p.parse("here:\n{\"relevant\":[2]}\nok").unwrap(), Some(vec![2]));
    }
}
