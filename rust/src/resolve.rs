//! Resolve — the embedding-backed same-name disambiguation gate (Plan §1.3), turned REAL in L4.
//!
//! This is the foundational value the candle pivot buys: ONE tested disambiguation capability,
//! reused by the scrub gate (`resolve_set`) and the transfer subject-test (`resolve_one`), instead
//! of Go's scattered per-stage prompt-stuffing. It is an ASYMMETRIC HYBRID (Plan §8), evidence-based
//! on the L4 experiment (AUC 0.88) + the L5 at-scale shadow:
//!
//!   * a cheap **CPU embedding cosine** between the article context and each candidate's identity
//!     card may **fast-track an obvious keep** — `≥ keep_threshold` → keep, with NO model call;
//!   * **everything below the keep line goes to the model.** The cheap proxy has NO authority to
//!     EXCLUDE — the L5 shadow proved an auto-drop band loses distinct, non-redundant truth (0 of 9
//!     dropped stories were captured elsewhere; one player erased), so the diviner, not the sieve,
//!     makes every exclusion. *The sieve surfaces; the diviner judges; only the diviner excludes.*
//!
//! Net: the GPU still runs on only ~half the candidates (the auto-keeps skip it), disambiguation is
//! more accurate than prompt-stuffing, the gate is one place, and no genuine link is ever condemned
//! by a proxy. The embeddings run on the CPU (Plan §1.4), so this REDUCES GPU load rather than adding.
//!
//! Fail-closed (the §1.2 invariant, here too): an ambiguous candidate whose model adjudication
//! fails to parse is **dropped** (`resolve_set`) / yields **`None`** (`resolve_one`) — never a guess.
//!
//! These `impl Harness` methods drop in BEHIND the existing `resolve_one`/`resolve_set` signatures
//! (in [`crate::harness`]) with no change to them — the Plan §5 test that the library was drawn
//! right. Requires `Harness.embedder = Some(..)`; an embed-less Harness errors (a wiring bug).

use crate::bucket::ArticleBucket;
use crate::config::ResolveConfig;
use crate::embed::cosine_similarity;
use crate::harness::{Candidate, EntityType, Harness, IdentityCard, Parser, Resolution, Resolved};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use anyhow::{Context, Result};
use serde::Deserialize;
use sqlx::PgPool;

/// The relevance system prompt: disambiguate same-name candidates and return only the JSON contract.
const RESOLVE_SYSTEM_PROMPT: &str = "Task: choose which listed candidates the article is genuinely about.\n\nA candidate is relevant when the article concerns that exact person/team as a real subject or meaningful mention.\n\nA candidate is not relevant when:\n- the article is about a different same-name person; use current club, nationality, role, and position as tie-breakers.\n- the identity's current club/role/position contradicts the article.\n- the name appears only as incidental noise in a roundup or list.\n\nBe inclusive for genuine mentions and strict on same-name confusion. Return only this JSON object:\n{\"relevant\": [<candidate numbers>]}";

/// Scrub-only extension of the relevance prompt: while the model is already reading the article
/// to vet ambiguous candidate links, also tag the article bucket. The relevance verdict remains
/// fail-closed and independent: an absent/unparseable bucket is ignored by the caller, which falls
/// back to candle.
const RESOLVE_BUCKET_SYSTEM_PROMPT: &str = "Task: choose which listed candidates the article is genuinely about, and classify the article as transfer/trade-related or other sports news.\n\nA candidate is relevant when the article concerns that exact person/team as a real subject or meaningful mention.\n\nA candidate is not relevant when:\n- the article is about a different same-name person; use current club, nationality, role, and position as tie-breakers.\n- the identity's current club/role/position contradicts the article.\n- the name appears only as incidental noise in a roundup or list.\n\nBucket rules:\n- bucket \"transfer\" for transfer, trade, loan, free-agency, signing, contract-move, bid, medical, or move-rumor coverage.\n- bucket \"other\" for match reports, injuries, previews, recaps, betting, tactical analysis, performance features, and general team news.\n\nBe inclusive for genuine mentions and strict on same-name confusion. Return only this JSON object:\n{\"relevant\": [<candidate numbers>], \"bucket\": \"transfer|other\"}";

/// The model budget for one adjudication call. Temperature 0.2 mirrors the scrub's "tight but a
/// judgment call"; JSON mode tightens adherence to the `{"relevant":[…]}` contract.
fn adjudication_opts() -> GenerateOptions {
    GenerateOptions {
        system: Some(RESOLVE_SYSTEM_PROMPT.to_string()),
        temperature: Some(0.2),
        num_predict: 512,
        num_ctx: 0,
        json_mode: true,
    format_schema: None,
    }
}

fn adjudication_bucket_opts() -> GenerateOptions {
    GenerateOptions {
        system: Some(RESOLVE_BUCKET_SYSTEM_PROMPT.to_string()),
        temperature: Some(0.2),
        num_predict: 512,
        num_ctx: 0,
        json_mode: true,
    format_schema: None,
    }
}

/// A candidate's band decision from the cheap cosine pre-filter. ASYMMETRIC (Plan §8): the proxy may
/// fast-track an obvious keep, but it has NO authority to exclude — everything below the keep line is
/// `Ambiguous` and goes to the model. Only the model (or its fail-closed non-commitment) ever drops.
/// There is deliberately no `Drop` variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decision {
    Keep,
    Ambiguous,
}

/// classify bands a cosine for the ASYMMETRIC gate (Plan §8): `≥ keep_threshold` is an auto-KEEP (the
/// only thing the cheap proxy may decide on its own); everything else is `Ambiguous` and is routed to
/// the model. The proxy NEVER auto-drops — the L5 shadow proved the auto-drop band lost distinct,
/// non-redundant truth (0/9 dropped stories were captured elsewhere), so the diviner, not the sieve,
/// makes every exclusion. `drop_threshold` stays in config for the OFFLINE banding analysis (the
/// shadow/experiment bins); the live gate ignores it.
fn classify(cosine: f32, cfg: &ResolveConfig) -> Decision {
    if cosine >= cfg.keep_threshold {
        Decision::Keep
    } else {
        Decision::Ambiguous
    }
}

/// identity_text renders a candidate's identity card as a natural sentence — the text the embedder
/// compares against the article, and the line the model sees. Current club leads (the strongest
/// disambiguator); teams need no card. Mirrors the L4 experiment's framing so the live gate matches
/// what was measured.
/// load_identity_candidate builds the entity's identity card for embedding-relevance scoring —
/// shared by vibe's narrative weighting and narratives' per-article relevance tags (moved here
/// from vibe, its Phase-1 beachhead home). Teams get an empty card (name-only identity).
pub async fn load_identity_candidate(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport: &str,
) -> Result<Candidate> {
    let Some(kind) = EntityType::from_db_str(entity_type) else {
        anyhow::bail!("unknown entity type {entity_type:?}");
    };
    if kind == EntityType::Team {
        return Ok(Candidate {
            entity_type: kind,
            entity_id,
            name: entity_name.to_string(),
            identity: IdentityCard::default(),
        });
    }
    let row: Option<(String, String, String)> = sqlx::query_as(
        r#"
        SELECT COALESCE(p.nationality, '') AS nationality,
               COALESCE(ct.name, '') AS current_club,
               COALESCE(NULLIF(pci.position, 'Unknown'), '') AS position
        FROM players p
        LEFT JOIN public.player_current_identity pci ON pci.player_id = p.id AND pci.sport = p.sport
        LEFT JOIN teams ct ON ct.id = pci.team_id AND ct.sport = p.sport
        WHERE p.id = $1 AND p.sport = $2
        "#,
    )
    .bind(entity_id)
    .bind(sport)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load identity {entity_type}/{entity_id}"))?;
    let (nationality, current_club, position) = row.unwrap_or_default();
    let opt = |s: String| (!s.is_empty()).then_some(s);
    Ok(Candidate {
        entity_type: kind,
        entity_id,
        name: entity_name.to_string(),
        identity: IdentityCard {
            nationality: opt(nationality),
            current_club: opt(current_club),
            position: opt(position),
        },
    })
}

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

pub struct RelevanceBucket {
    pub relevant: Vec<usize>,
    pub bucket: Option<ArticleBucket>,
}

pub struct RelevanceBucketParser {
    pub n: usize,
}

impl Parser<RelevanceBucket> for RelevanceBucketParser {
    fn parse(&self, raw: &str) -> Result<Option<RelevanceBucket>> {
        let (start, end) = match (raw.find('{'), raw.rfind('}')) {
            (Some(s), Some(e)) if e > s => (s, e),
            _ => return Ok(None),
        };
        #[derive(Deserialize)]
        struct Reply {
            #[serde(default)]
            relevant: Vec<i64>,
            #[serde(default)]
            bucket: String,
        }
        let reply: Reply = match serde_json::from_str(&raw[start..=end]) {
            Ok(r) => r,
            Err(_) => return Ok(None),
        };
        let relevant = reply
            .relevant
            .into_iter()
            .filter(|&i| i >= 1 && (i as usize) <= self.n)
            .map(|i| i as usize)
            .collect();
        Ok(Some(RelevanceBucket {
            relevant,
            bucket: ArticleBucket::from_model_tag(&reply.bucket),
        }))
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
        let (resolutions, _) = self
            .resolve_set_with_bucket(role, context, candidates)
            .await?;
        Ok(resolutions)
    }

    /// resolve_set_with_bucket is scrub's Wave-5 extension over [`Self::resolve_set`]. It preserves
    /// the asymmetric relevance gate and returns a model bucket only when an ambiguous-band model
    /// call was already needed. Auto-kept articles get `None` here and use the candle fallback.
    pub async fn resolve_set_with_bucket(
        &self,
        role: Role,
        context: &str,
        candidates: &[Candidate],
    ) -> Result<(Vec<Resolution>, Option<ArticleBucket>)> {
        if candidates.is_empty() {
            return Ok((Vec::new(), None));
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

        let mut bucket = None;
        if !ambiguous.is_empty() {
            let amb: Vec<&Candidate> = ambiguous.iter().map(|&i| &candidates[i]).collect();
            let prompt = build_relevance_prompt(context, &amb);
            let extracted = self
                .extract(
                    role,
                    &prompt,
                    &adjudication_bucket_opts(),
                    &RelevanceBucketParser { n: amb.len() },
                )
                .await?;
            if let Some(parsed) = extracted.value {
                // relevant is 1-indexed over `amb`, in band order.
                for (k, &i) in ambiguous.iter().enumerate() {
                    out[i].kept = parsed.relevant.contains(&(k + 1));
                }
                bucket = parsed.bucket;
            }
            // else: fail-closed — the ambiguous candidates keep their `kept = false` placeholder.
        }
        Ok((out, bucket))
    }

    /// resolve_one settles which ONE candidate (if any) a mention is — the transfer subject-test
    /// shape (Plan §1.3). Embeds the context against each candidate identity, takes the best match,
    /// and either auto-keeps it (cosine `≥ keep_threshold`) or asks the model to confirm it (anything
    /// below — the ASYMMETRIC gate, Plan §8: the proxy never excludes on its own). Fail-closed: an
    /// unconfirmed / fail-closed model verdict ⇒ `None`, never a guess. `subject` records who it
    /// resolved to.
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
            Decision::Ambiguous => {
                // Confirm the single best candidate with the model. The raw token names the mention
                // being resolved, so the model judges THIS surface form.
                let prompt = format!(
                    "Mention to resolve: \"{raw_token}\"\n\n{}",
                    build_relevance_prompt(context, &[chosen])
                );
                let extracted = self
                    .extract(
                        role,
                        &prompt,
                        &adjudication_opts(),
                        &RelevanceParser { n: 1 },
                    )
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

    fn player(
        name: &str,
        nationality: Option<&str>,
        club: Option<&str>,
        position: Option<&str>,
    ) -> Candidate {
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
    fn classify_is_asymmetric() {
        // Plan §8: ≥ keep auto-keeps; everything below is Ambiguous (→ the model). The proxy never
        // auto-drops, so even a very low cosine is Ambiguous, NOT a drop.
        let cfg = ResolveConfig {
            keep_threshold: 0.75,
            drop_threshold: 0.60,
        };
        assert_eq!(classify(0.80, &cfg), Decision::Keep);
        assert_eq!(classify(0.75, &cfg), Decision::Keep); // inclusive
        assert_eq!(classify(0.70, &cfg), Decision::Ambiguous);
        assert_eq!(classify(0.10, &cfg), Decision::Ambiguous); // no auto-drop, even very low
    }

    #[test]
    fn identity_text_player_and_team() {
        let p = player(
            "João Silva",
            Some("Portuguese"),
            Some("Benfica"),
            Some("midfielder"),
        );
        assert_eq!(
            identity_text(&p),
            "João Silva, a Portuguese midfielder, currently at Benfica."
        );
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
        assert_eq!(
            p.parse("here:\n{\"relevant\":[2]}\nok").unwrap(),
            Some(vec![2])
        );
    }
}
