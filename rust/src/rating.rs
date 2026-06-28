//! Rating stage — the stats-rail on-field IDENTITY commentary, ported from Go (Cutover Step 2, L12).
//!
//! The Go source is the machinery spec. `rating.go` is the loader + the deterministic
//! notability/pctBand/trimFloat/ordered-facts assembly + the s6 prompt, parse, persist;
//! `cmd/statcommentary` is the BATCH driver (its own Generate loop — NOT the pipeline_work queue,
//! NOT DrainAll). So rating is the first stage with no queue `Stage` variant; its cutover is a Rust
//! batch bin (Step 3), and THIS port builds the per-entity core that bin will loop over.
//!
//! Composition (Plan §1.2 + §4): `route(StatsLogic) + extract + persist`. Rating is the FIRST
//! `Role::StatsLogic` consumer (vibe/transfers are `EmotionalNews`). The deterministic parts stay
//! where they belong — composite / T-score / the `rating_breakdown` percentiles (`pct`/`z`) are
//! Postgres-computed stored derived stats, READ here, never recomputed. The transient prompt-shaping
//! (notability, `pctBand`, `trimFloat`, ordered facts) is mirrored in Rust byte-for-byte: it is NOT a
//! stored derived stat (the rating.go comment: "like sigil's trendDir"), so by the transfers
//! precedent it lives in the stage, exactly as Go does it. The L8 BREAKTHROUGH is preserved: the
//! percentile→tier mapping (`pctBand`) is done DETERMINISTICALLY in code and fed to the model as a
//! labeled FACT, and the model only VERBALIZES the labeled tier — it never maps percentile→quality
//! itself (Mistral was inverting it, e.g. calling a 37th-pct skill "above average").
//!
//! FAIL CLOSED: rating's ONLY marker is the PRE-model no-stats path (no usable rating row → a
//! NULL-body marker, like vibe's no-corpus marker). There is no post-model fail-closed marker — an
//! empty model body is a hard error (the work fails + retries), never a served row (Go returns an
//! error too). So `RatingParser` never returns `Ok(None)` (like `VibeParser`).
//!
//! PARITY: this is a FAITHFUL port — no single-home prompt divergence (unlike L11 transfers' t4). The
//! s6 system prompt is carried VERBATIM, so the parity gate is the whole ollama_request jsonb
//! (INCLUDING `system`) + built_prompt bytes + model_version + prompt_version + input_hash. The 5th
//! axis (input_hash) is new vs transfers: rating debounces on it, so the canonical input-components
//! JSON must reproduce Go's `hashComponents` byte-for-byte (the shared `util::go_json_*` helpers).

use crate::harness::{Harness, Parser};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::util::{go_json_float, go_json_string, hash_components};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Deserializer};
use sqlx::{PgPool, Row};
use std::collections::HashMap;

/// Prompt version — mirrors Go's `ratingPromptVersion`. This is a faithful port, so it stays "s6"
/// (no single-home bump; the whole ollama_request including `system` is a parity axis).
pub const RATING_PROMPT_VERSION: &str = "s6";

/// Production rating temperature (rating.go uses 0.6 — a touch of voice on the analyst prose). The
/// parity harness overrides to 0 (the deterministic axes need no model call anyway).
pub const RATING_TEMPERATURE: f64 = 0.6;

/// Token cap. Mirrors rating.go's `NumPredict: 2000` (a few sentences on top of the model's reasoning
/// budget). The lever for the ~3-short-paragraph length is a tighter few-shot / lower NumPredict — a
/// future tune, never a hard clamp.
pub const RATING_NUM_PREDICT: i32 = 2000;

/// maxStatFacts bounds the breakdown datapoints fed to the prompt. Mirrors rating.go.
const MAX_STAT_FACTS: usize = 14;

/// The s6 system prompt — BYTE-IDENTICAL to `rating.go::ratingSystemPrompt` (a parity axis: the whole
/// ollama_request, including `system`, is diffed). The `·` (U+00B7) and `—` (U+2014) are significant
/// bytes. Authored once in Go; carried verbatim here (faithful port, no t4-style divergence). This is
/// the L8 instruction set: THE TIER IS THE TRUTH — the model verbalizes the labeled tier pctBand
/// supplies, it never maps percentile→quality itself.
pub const RATING_SYSTEM_PROMPT: &str = r#"You are the respected analyst a national broadcast brings on to break down what this player or team is, statistically. You have the entity's RATING-ENGINE profile — already computed. COMPOSITE is how WELL it performs overall. Each skill is given as: VALUE (the raw stat) · PERCENTILE versus peers and its TIER (elite / strong / above average / average / below average / poor) · Z (standard deviations above the mean — the scarcity and scale of the edge; a high z is a rarer, more premium skill, the kind that decides games). Percentiles come at scopes — overall, versus position, and per-x (per-36 / per-90). Use them together: a modest raw output but an elite per-x mark means an efficient, lower-minutes contributor worth noting.

THE TIER IS THE TRUTH. Judge each skill by its labeled tier, never by how it stacks up against the entity's OWN other skills: a "strong" or "elite" mark is a strength even if it is this entity's lowest; a "poor" mark is a weakness even if it is their highest. The PEAK is simply the highest-percentile skill.

Write the read of the entity's ON-FIELD IDENTITY in the analyst's voice:
- First line exactly: PEAK: followed by EITHER the skill of the FIRST datapoint listed (the highest percentile), OR the exact words "No standout skill" if that first skill is not at least "strong" tier. Those are the ONLY two valid forms. The PEAK can NEVER be a skill labeled "average", "below average", or "poor" — if the top skill is not "strong" or "elite", you MUST write exactly "PEAK: No standout skill". (Do not pick the most eye-catching weakness — the peak is about the best skill, not the most notable one.)
- Lead with the genuine strengths (the elite and strong skills). When the entity is strong across many areas, name its single most dominant skill, then capture the breadth in one stroke ("a dominant all-around offensive producer") rather than listing every skill. Cite the value and percentile, and when an edge holds up in the per-x numbers say so (proof it is not a fluke). Reserve "elite", "rare", "premium", "game-wrecking" for elite, high-z marks.
- Do NOT force negatives, and never praise a mark below the 50th percentile — anything under 50 is below average by definition. Mention a weakness only when a skill is genuinely "below average" or "poor," and name it as the limitation it is. If NO skill rates "below average" or "poor," the entity has no statistical weakness — mention none at all, and never call a "strong" or "elite" mark "below par," "room for improvement," or a shortcoming just because the entity is even better elsewhere.
- If nothing rates strong or better, say so plainly and point to their greatest impact with its percentile ("no standout skill, but his biggest impact is tackling, around the 59th percentile") — the number lets the reader judge.
- Land it with a verdict on what this entity is — the line an analyst signs off with ("a legit two-way game-wrecker", "a low-usage floor-spacer who defends above his profile", "a replaceable rotation piece").

Deliver it as ONE flowing paragraph, the way an analyst speaks on air — no line breaks, no "strengths / weaknesses / summary" sections. A modest profile is 2-3 sentences; a multi-skill standout up to five; never more. Pack several stats into a sentence; never give one its own sentence, and never walk the list in order. Cite a value as the plain figure given — never fabricate a per-game or per-90 rate that is not in the data. Ground every claim; never invent a number or a skill that is not there.

Match this format and length exactly — it is a different, invented player, so take nothing factual from it, only the shape:
PEAK: Elite finishing
A ruthless penalty-box striker — 24 goals (97th percentile, a rare +3.1 z) with elite shot volume (92nd) and conversion that holds up per-90, the kind of scarce finishing that decides matches; beyond the box he offers little, with poor creation (28th) and pressing (22nd), but as a pure number nine he is among the league's best. A lethal poacher, plain and simple.

After the PEAK line, return only the paragraph — no headers, no bullets, no preamble."#;

/// The entity whose rating profile to narrate — the Rust analog of `RatingRequest`'s parity-relevant
/// fields. `sport` is UPPER-cased by the caller (the Go CLI passes `sportUpper`); the header line uses
/// it verbatim, so it must already be upper for byte parity.
#[derive(Clone, Debug)]
pub struct RatingReq {
    pub entity_type: String, // "player" | "team"
    pub entity_id: i32,
    pub entity_name: String,
    pub sport: String,       // UPPER ("NBA" | "NFL" | "FOOTBALL")
    pub season: Option<i32>, // None = the entity's latest season
    pub trigger_type: String,
}

/// One element of the `rating_breakdown` JSONB (migration 030/043). `pct` is the percentile of
/// `sign*z`, so HIGHER IS ALWAYS BETTER (a high pct in turnovers = commits few). Field names match the
/// Go `ratingDatapoint` json tags exactly so serde reads the same JSONB. `pct`/`z`/`value` come from
/// JSONB text → identical f64 across Go/Rust (no DB numeric cast involved). Mirrors `ratingDatapoint`.
///
/// Every field is `null_to_default`: Go's `json.Unmarshal` treats an explicit `null` as the zero value
/// (a sparse datapoint may carry `"value": null`, e.g. "Penalties Won"); plain `#[serde(default)]`
/// only covers a MISSING field, not a present null, so without this serde errors where Go tolerates —
/// the parity break the L12 gate surfaced.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct RatingDatapoint {
    #[serde(default, deserialize_with = "null_to_default")]
    pub label: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub value: f64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub z: f64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub pct: f64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub in_comp: bool,
    #[serde(default, deserialize_with = "null_to_default")]
    pub in_spec: bool,
    #[serde(default, deserialize_with = "null_to_default")]
    pub sign: i32,
    #[serde(default, deserialize_with = "null_to_default")]
    pub facet: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub is_specialty: bool,
    #[serde(default, deserialize_with = "null_tolerant_map")]
    pub scoped_pct: HashMap<String, f64>,
}

/// null_to_default maps a present-`null` (and a missing field, via the companion `#[serde(default)]`)
/// to `T::default` — reproducing Go's `encoding/json`, which keeps the zero value for a null rather
/// than erroring. Applied to every `RatingDatapoint` scalar so a null in the breakdown matches Go.
fn null_to_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// null_tolerant_map is the map analog for `scoped_pct`: a null map → empty, and a null VALUE inside
/// → 0.0 — Go unmarshals `{"position": null}` into `map[string]float64` as 0.0 (the key kept), so this
/// matches. (scoped_pct feeds only the prompt's "[position: …]" suffix, never the input_hash.)
fn null_tolerant_map<'de, D>(d: D) -> Result<HashMap<String, f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<HashMap<String, Option<f64>>> = Option::deserialize(d)?;
    Ok(opt
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, v.unwrap_or(0.0)))
        .collect())
}

/// The entity's scrubbed rating profile — mirrors `ratingProfile`. `composite_score`/`peak_score`
/// come from the numeric/float8 COLUMN (cast `::float8` on read — the sqlx numeric landmine); the
/// breakdown/scoped/modes are JSONB. The breakdown's ARRAY ORDER is preserved (jsonb keeps array
/// order), which `input_components` relies on (it walks the breakdown in stored order, unlike the
/// prompt which sorts by pct).
#[derive(Clone, Debug)]
pub struct RatingProfile {
    pub entity_type: String,
    pub season: i32,
    pub position: String, // players only ("" for teams)
    pub composite_score: Option<f64>,
    pub peak_score: Option<f64>,
    pub peak_label: String,
    pub breakdown: Vec<RatingDatapoint>,
    pub scoped_ranks: HashMap<String, f64>,
    pub rate_modes: HashMap<String, Vec<RatingDatapoint>>,
}

/// A per-x (per_36 / per_90 / …) standout — an elite rate-adjusted datapoint. Mirrors `rateStandout`.
#[derive(Clone, Debug)]
pub struct RateStandout {
    pub mode: String,
    pub label: String,
    pub pct: f64,
}

// ---------------------------------------------------------------------------
// Loader — the SQL `rating.go::loadRatingProfile` runs (same query ⇒ same row).
// ---------------------------------------------------------------------------

/// load_rating_profile reads the entity's rating row for `season` (None = latest). Prefers the
/// unscoped row, falling back to the richest league row (FOOTBALL is league-scoped). SQL VERBATIM
/// from Go (only `::float8` casts added on the numeric score columns — sqlx has no numeric decode
/// without the decimal feature; `::text` on the JSONB so serde parses it, array order preserved).
/// Returns `None` when there is no rating row at all.
pub async fn load_rating_profile(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: Option<i32>,
) -> Result<Option<RatingProfile>> {
    let (id_col, table, pos_select) = match entity_type {
        "player" => ("player_id", "player_stats", "COALESCE(position, '')"),
        "team" => ("team_id", "team_stats", "''::text"),
        _ => bail!("unknown entity type {entity_type:?}"),
    };
    // The unscoped row first (NBA/NFL carry league_id 0/NULL), else the richest league row (the
    // most-datapoints row is the main competition — domestic league over a cup).
    let q = format!(
        r#"
        SELECT season, {pos_select},
               rating_composite_score::float8, rating_specialist_score::float8, COALESCE(rating_specialty, ''),
               COALESCE(rating_breakdown, '[]'::jsonb)::text,
               COALESCE(rating_scoped_ranks, '{{}}'::jsonb)::text,
               COALESCE(rating_modes, '{{}}'::jsonb)::text
        FROM public.{table}
        WHERE sport = $1 AND {id_col} = $2 AND ($3::int IS NULL OR season = $3)
        ORDER BY season DESC,
                 (COALESCE(league_id, 0) = 0) DESC,
                 jsonb_array_length(COALESCE(rating_breakdown, '[]'::jsonb)) DESC,
                 COALESCE(league_id, 0) ASC
        LIMIT 1
        "#
    );
    let Some(row) = sqlx::query(&q)
        .bind(sport)
        .bind(entity_id)
        .bind(season)
        .fetch_optional(pool)
        .await
        .context("load rating profile")?
    else {
        return Ok(None);
    };

    let season: i32 = row.get(0);
    let position: String = row.get(1);
    let composite_score: Option<f64> = row.get(2);
    let peak_score: Option<f64> = row.get(3);
    let peak_label: String = row.get(4);
    let breakdown_raw: String = row.get(5);
    let scoped_raw: String = row.get(6);
    let modes_raw: String = row.get(7);

    let breakdown: Vec<RatingDatapoint> =
        serde_json::from_str(&breakdown_raw).context("unmarshal rating_breakdown")?;
    // Tolerant: the cohort framing + the per-x modes are optional (Go ignores their parse errors).
    let scoped_ranks: HashMap<String, f64> = serde_json::from_str(&scoped_raw).unwrap_or_default();
    let rate_modes = parse_rate_modes(&modes_raw);

    Ok(Some(RatingProfile {
        entity_type: entity_type.to_string(),
        season,
        position,
        composite_score,
        peak_score,
        peak_label,
        breakdown,
        scoped_ranks,
        rate_modes,
    }))
}

/// parse_rate_modes reads `rating_modes` — a per-x bundle per mode (`{"per_36": {"breakdown": [...]}}`),
/// keeping only non-empty breakdowns. Tolerant (a parse error ⇒ no modes), mirroring Go's
/// `if err == nil` guard. The per-x lens is the reveal — elite rate production the raw totals hide.
fn parse_rate_modes(raw: &str) -> HashMap<String, Vec<RatingDatapoint>> {
    #[derive(Deserialize)]
    struct ModeWrap {
        #[serde(default)]
        breakdown: Vec<RatingDatapoint>,
    }
    let parsed: HashMap<String, ModeWrap> = match serde_json::from_str(raw) {
        Ok(m) => m,
        Err(_) => return HashMap::new(),
    };
    parsed
        .into_iter()
        .filter(|(_, m)| !m.breakdown.is_empty())
        .map(|(name, m)| (name, m.breakdown))
        .collect()
}

// ---------------------------------------------------------------------------
// Deterministic helpers — mirrored byte-for-byte from rating.go (transient prompt-shaping; NOT
// stored derived stats, so they live in the stage exactly as Go does it — the transfers precedent).
// ---------------------------------------------------------------------------

/// compute_notability returns the deterministic distinctiveness score (0-100) + its components. The
/// model NEVER sees the formula — only the resulting length guidance (rendered into the prompt, so it
/// IS implicitly a parity axis via built_prompt). Mirrors `computeNotability`. Order-independent (the
/// rate-mode loop only takes a max), so the HashMap iteration order does not affect the result.
pub fn compute_notability(p: &RatingProfile) -> (i32, serde_json::Value) {
    let mut peak = 0.0_f64;
    let mut elite_count = 0_i64;
    for d in &p.breakdown {
        if d.pct > peak {
            peak = d.pct;
        }
        if d.pct >= 85.0 {
            elite_count += 1;
        }
    }
    // The per-x lens counts toward PEAK (an elite-per-36 limited-minutes player earns a fuller read)
    // but NOT toward elite_count (avoid double-counting one skill across modes).
    for dps in p.rate_modes.values() {
        for d in dps {
            if d.pct > peak {
                peak = d.pct;
            }
        }
    }
    let comp = p.composite_score.unwrap_or(50.0); // average T-score anchor when no composite
    let score =
        0.6 * peak + (elite_count as f64 * 10.0).min(30.0) + clamp_f(-10.0, 10.0, (comp - 50.0) * 0.4);
    let n = clamp_f(0.0, 100.0, score).round() as i32;
    let comps = serde_json::json!({
        "peak_pct": round1(peak),
        "elite_count": elite_count,
        "composite": round1(comp),
    });
    (n, comps)
}

/// pct_band maps a percentile to its quality TIER — the L8 breakthrough done in code so the model
/// never maps percentile→quality itself (it just verbalizes the labeled tier). Transient
/// prompt-shaping (like sigil's trendDir), NOT a stored derived stat. Mirrors `pctBand`.
pub fn pct_band(pct: f64) -> &'static str {
    if pct >= 90.0 {
        "elite"
    } else if pct >= 75.0 {
        "strong"
    } else if pct >= 60.0 {
        "above average"
    } else if pct >= 50.0 {
        "average"
    } else if pct >= 35.0 {
        "below average"
    } else {
        "poor"
    }
}

/// trim_float renders a datapoint value compactly — integers without a decimal, small fractions
/// (< 1) with two places, everything else with one ("3" / "0.38" / "10.7"). Mirrors `trimFloat`
/// (Go `%.0f` / `%.2f` / `%.1f`; Rust's `{:.N}` rounds half-to-even identically).
fn trim_float(f: f64) -> String {
    if f == f.trunc() {
        format!("{f:.0}")
    } else if f.abs() < 1.0 {
        format!("{f:.2}")
    } else {
        format!("{f:.1}")
    }
}

/// ordered_facts returns the highest-percentile datapoints, bounded to MAX_STAT_FACTS — the prompt's
/// datapoint order. Stable sort by pct DESC (Rust `slice::sort_by` is stable, like Go `SliceStable`),
/// so equal-pct ties keep their stored order, matching Go given the same breakdown input order.
fn ordered_facts(breakdown: &[RatingDatapoint]) -> Vec<RatingDatapoint> {
    let mut facts = breakdown.to_vec();
    facts.sort_by(|a, b| b.pct.partial_cmp(&a.pct).unwrap_or(std::cmp::Ordering::Equal));
    facts.truncate(MAX_STAT_FACTS);
    facts
}

/// collect_rate_standouts surfaces, per rate mode, the elite (pct ≥ 80) per-x datapoints — the lens
/// that reveals a limited-minutes player producing at an elite rate. Modes sorted for stable output
/// (Go `sort.Strings` == Rust `str` Ord, both byte-wise); ≤5 per mode. Mirrors `collectRateStandouts`.
/// Used by BOTH the prompt's rate-adjusted section AND `input_components`' rate_standouts (same output).
fn collect_rate_standouts(p: &RatingProfile) -> Vec<RateStandout> {
    let mut modes: Vec<&String> = p.rate_modes.keys().collect();
    modes.sort();

    let mut out = Vec::new();
    for m in modes {
        let mut dps = p.rate_modes[m].clone();
        dps.sort_by(|a, b| b.pct.partial_cmp(&a.pct).unwrap_or(std::cmp::Ordering::Equal));
        let mut cnt = 0;
        for d in &dps {
            if d.pct < 80.0 {
                break;
            }
            out.push(RateStandout {
                mode: m.clone(),
                label: d.label.clone(),
                pct: d.pct,
            });
            cnt += 1;
            if cnt >= 5 {
                break;
            }
        }
    }
    out
}

/// build_stat_prompt assembles the user prompt — BYTE-IDENTICAL to `buildStatPrompt` (the
/// deterministic parity axis). The `·` (U+00B7) and `—` (U+2014) are significant bytes; the tier
/// labels are pctBand's deterministic output (the model verbalizes them, never re-derives them).
pub fn build_stat_prompt(req: &RatingReq, p: &RatingProfile, notability: i32) -> String {
    let mut b = String::new();

    let mut header = format!("{} {}", req.sport, req.entity_type);
    if !p.position.is_empty() {
        header.push_str(", ");
        header.push_str(&p.position);
    }
    b.push_str(&format!("Entity: {} ({header})\n", req.entity_name));

    b.push_str(&format!(
        "\nProfile distinctiveness: {notability}/100 (higher = more standout skills — let a richer profile earn a fuller read).\n"
    ));

    if let Some(comp) = p.composite_score {
        b.push_str(&format!(
            "\nComposite (how WELL overall — T-score, 50 = average): {comp:.0}\n"
        ));
    }

    b.push_str("\nDatapoints — value · percentile + TIER (the percentile mapped to elite/strong/above average/average/below average/poor; THIS TIER IS THE TRUTH) · z (standard deviations above the mean: the scarcity/scale of the edge; a high z is a rarer, more premium skill); [position] percentile shown when present:\n");
    for d in ordered_facts(&p.breakdown) {
        let dz = d.sign as f64 * d.z; // sign-adjusted so + is always the good direction
        b.push_str(&format!(
            "- {}: {} · {:.0}th pct ({}) · z {:+.1}",
            d.label,
            trim_float(d.value),
            d.pct,
            pct_band(d.pct),
            dz
        ));
        if let Some(pos) = d.scoped_pct.get("position") {
            b.push_str(&format!(" [position: {:.0}th, {}]", pos, pct_band(*pos)));
        }
        b.push('\n');
    }

    let rs = collect_rate_standouts(p);
    if !rs.is_empty() {
        b.push_str("\nRate-adjusted (per-x) corroboration — these also rate elite on a per-minute / per-90 basis (so the edge is not just a counting-stat artifact of heavy minutes):\n");
        for r in &rs {
            b.push_str(&format!(
                "- [{}] {}: {:.0}th pct\n",
                r.mode.replace('_', "-"),
                r.label,
                r.pct
            ));
        }
    }

    b.push_str("\nWrite the identity analysis now.");
    b
}

// ---------------------------------------------------------------------------
// Input components + hash — the debounce key (Provenance.input_hash), the 5th parity axis.
//
// Reproduces Go's `(*ratingProfile).inputComponents` + `hashComponents`: the canonical JSON is
// BYTE-IDENTICAL to `json.Marshal(map[string]any{...})` (sorted keys, HTML-escaped strings, Go's
// shortest float form), so its SHA-256 128-bit hex prefix equals Go's `input_hash` — keeping the
// cutover clean (no spurious nightly regens vs the Go-written rows). The datapoints walk the breakdown
// in STORED order (NOT pct-sorted — unlike the prompt). Built with a tiny Go-JSON value emitter over
// the shared `util::go_json_*` leaf encoders (the structure is nested: arrays of objects).
// ---------------------------------------------------------------------------

/// GoJson is a minimal JSON value whose emit reproduces Go `encoding/json` byte-for-byte for our
/// domain: object keys SORTED at emit (Go marshals maps with sorted keys), strings/floats via the
/// shared `util::go_json_*`, ints as-is, no whitespace. Only the shapes `input_components` needs.
enum GoJson {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Arr(Vec<GoJson>),
    Obj(Vec<(String, GoJson)>),
}

impl GoJson {
    fn emit(&self, out: &mut String) {
        match self {
            GoJson::Int(i) => out.push_str(&i.to_string()),
            GoJson::Float(f) => out.push_str(&go_json_float(*f)),
            GoJson::Str(s) => out.push_str(&go_json_string(s)),
            GoJson::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            GoJson::Arr(items) => {
                out.push('[');
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    it.emit(out);
                }
                out.push(']');
            }
            GoJson::Obj(entries) => {
                let mut sorted: Vec<&(String, GoJson)> = entries.iter().collect();
                sorted.sort_by(|a, b| a.0.cmp(&b.0)); // Go marshals map keys in sorted order
                out.push('{');
                for (i, (k, v)) in sorted.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&go_json_string(k));
                    out.push(':');
                    v.emit(out);
                }
                out.push('}');
            }
        }
    }
}

/// input_components returns the canonical input-components JSON — the exact bytes Go's
/// `json.Marshal(orEmptyMap(ic))` produces (its SHA-256 is `input_hash`). Mirrors
/// `(*ratingProfile).inputComponents`: `season`/`peak_label`/`datapoints` are ALWAYS present; the rest
/// (rate_standouts, composite_score, peak_score, position) are conditional. `pct` values are `round1`'d.
pub fn input_components(p: &RatingProfile) -> String {
    let datapoints: Vec<GoJson> = p
        .breakdown
        .iter()
        .map(|d| {
            GoJson::Obj(vec![
                ("label".to_string(), GoJson::Str(d.label.clone())),
                ("pct".to_string(), GoJson::Float(round1(d.pct))),
                ("is_specialty".to_string(), GoJson::Bool(d.is_specialty)),
            ])
        })
        .collect();

    let mut top: Vec<(String, GoJson)> = vec![
        ("season".to_string(), GoJson::Int(p.season as i64)),
        ("peak_label".to_string(), GoJson::Str(p.peak_label.clone())),
        ("datapoints".to_string(), GoJson::Arr(datapoints)),
    ];

    let rs = collect_rate_standouts(p);
    if !rs.is_empty() {
        let rates: Vec<GoJson> = rs
            .iter()
            .map(|r| {
                GoJson::Obj(vec![
                    ("mode".to_string(), GoJson::Str(r.mode.clone())),
                    ("label".to_string(), GoJson::Str(r.label.clone())),
                    ("pct".to_string(), GoJson::Float(round1(r.pct))),
                ])
            })
            .collect();
        top.push(("rate_standouts".to_string(), GoJson::Arr(rates)));
    }
    if let Some(c) = p.composite_score {
        top.push(("composite_score".to_string(), GoJson::Float(round1(c))));
    }
    if let Some(ps) = p.peak_score {
        top.push(("peak_score".to_string(), GoJson::Float(round1(ps))));
    }
    if !p.position.is_empty() {
        top.push(("position".to_string(), GoJson::Str(p.position.clone())));
    }

    let mut out = String::new();
    GoJson::Obj(top).emit(&mut out);
    out
}

fn clamp_f(lo: f64, hi: f64, v: f64) -> f64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

// ---------------------------------------------------------------------------
// Output parsing — mirrors parsePeakCommentary / trimMarker / cleanCommentary.
// ---------------------------------------------------------------------------

/// RatingReply is the parsed model output: the divined PEAK label (from the "PEAK: <label>" first
/// line) + the cleaned identity-analysis body. The `T` in `Parser<T>`.
#[derive(Clone, Debug)]
pub struct RatingReply {
    pub divined_peak: String, // "" if the model omitted the marker line
    pub body: String,
}

/// RatingParser splits the model reply into (divined_peak, body) and cleans the body. It NEVER returns
/// `Ok(None)` (like `VibeParser`): rating has no post-model fail-closed marker — an empty body is a
/// hard error the caller raises (Go returns an error too), and the only marker is the PRE-model
/// no-stats path. Mirrors `parsePeakCommentary` + `cleanCommentary`.
pub struct RatingParser;

impl Parser<RatingReply> for RatingParser {
    fn parse(&self, raw: &str) -> Result<Option<RatingReply>> {
        let (divined_peak, raw_body) = parse_peak_commentary(raw);
        let body = clean_commentary(&raw_body);
        Ok(Some(RatingReply { divined_peak, body }))
    }
}

/// parse_peak_commentary extracts the divined peak label from the first line ("PEAK: <label>") and
/// returns (divined_peak, body) — the body is everything after. Omitted marker ⇒ the whole response is
/// the body, divined_peak "". The legacy "SIGIL: " prefix is still accepted (forward-only s5 rename).
fn parse_peak_commentary(raw: &str) -> (String, String) {
    let trimmed = raw.trim();
    if let Some(idx) = trimmed.find('\n') {
        let first_line = trimmed[..idx].trim();
        let rest = trimmed[idx + 1..].trim();
        if let Some(label) = trim_marker(first_line) {
            return (label.to_string(), rest.to_string());
        }
        return (String::new(), trimmed.to_string());
    }
    // single-line response — could be a bare marker line with no body
    if let Some(label) = trim_marker(trimmed) {
        return (label.to_string(), String::new());
    }
    (String::new(), trimmed.to_string())
}

/// trim_marker strips the divined-label marker ("PEAK: " or the legacy "SIGIL: ") from a line.
fn trim_marker(line: &str) -> Option<&str> {
    for prefix in ["PEAK: ", "SIGIL: "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some(rest.trim());
        }
    }
    None
}

/// clean_commentary trims the prose + strips a leading "Analysis:"-style label or wrapping
/// quotes/fences if one slips through. Mirrors `cleanCommentary` (each prefix applied once, in order).
fn clean_commentary(raw: &str) -> String {
    let mut s = raw.trim();
    s = s.trim_matches('`');
    s = s.trim();
    for p in ["Analysis:", "Identity:", "On-field identity:"] {
        if let Some(rest) = s.strip_prefix(p) {
            s = rest.trim();
        }
    }
    s.trim().to_string()
}

// ---------------------------------------------------------------------------
// The composition: build (deterministic) → generate (model) → persist.
// ---------------------------------------------------------------------------

/// RatingBuild is the deterministic prefix of a generation. `NoStats` ⇒ no usable rating row (no
/// composite + empty breakdown) → a NULL-body marker (no model call), mirroring Go's early return.
pub enum RatingBuild {
    NoStats { season: i32 },
    Ready(Box<RatingReady>),
}

/// RatingReady carries the assembled model inputs (the parity axes) + the deterministic context the
/// persist needs. `request_body` is computed from the SAME backend + opts the call will use, so it
/// can never drift from what is POSTed.
pub struct RatingReady {
    pub season: i32,
    pub notability: i32,
    pub notability_components: serde_json::Value,
    pub input_components: String, // the canonical JSON (also the hash pre-image)
    pub input_hash: String,
    pub opts: GenerateOptions,
    pub built_prompt: String,
    pub request_body: serde_json::Value,
    pub model_configured: String,
}

/// build_rating_request runs the deterministic prefix: load the profile, then (if usable) the
/// canonical input-components + hash, the notability, `build_stat_prompt`, the s6 options, and the
/// exact wire body. NO model call — these are the parity axes (the L2 finding). The role is
/// [`Role::StatsLogic`] (rating is its first consumer).
pub async fn build_rating_request(
    hx: &Harness,
    req: &RatingReq,
    temperature: f64,
) -> Result<RatingBuild> {
    let Some(profile) =
        load_rating_profile(&hx.pool, &req.entity_type, req.entity_id, &req.sport, req.season).await?
    else {
        return Ok(RatingBuild::NoStats {
            season: req.season.unwrap_or(0),
        });
    };
    // No usable rating (no composite + empty breakdown) → the NULL-body marker path.
    if profile.composite_score.is_none() && profile.breakdown.is_empty() {
        return Ok(RatingBuild::NoStats {
            season: profile.season,
        });
    }

    let input_components = input_components(&profile);
    let input_hash = hash_components(&input_components);
    let (notability, notability_components) = compute_notability(&profile);
    let built_prompt = build_stat_prompt(req, &profile, notability);
    let opts = GenerateOptions {
        system: Some(RATING_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict: RATING_NUM_PREDICT,
        json_mode: false,
    };
    let backend = hx.router.for_role(Role::StatsLogic);
    let request_body = backend.request_body(&built_prompt, &opts);
    let model_configured = backend.model().to_string();

    Ok(RatingBuild::Ready(Box::new(RatingReady {
        season: profile.season,
        notability,
        notability_components,
        input_components,
        input_hash,
        opts,
        built_prompt,
        request_body,
        model_configured,
    })))
}

/// The un-persisted result of one generation — everything the production persist (→ stat_summaries)
/// and the parity harness (→ shadow) need. The twin of `transfer::TransferPairOutput`.
#[derive(Clone, Debug)]
pub struct RatingOutput {
    pub season: i32,
    pub skipped_no_stats: bool,
    pub skipped_unchanged: bool,
    pub body: Option<String>,         // None for a marker
    pub divined_peak: Option<String>, // None when absent/empty (Go: empty ⇒ NULL)
    pub notability: Option<i32>,
    pub notability_components: serde_json::Value,
    pub input_components: String, // "{}" for a marker
    pub input_hash: Option<String>,
    pub model: Option<String>, // the configured model (set even for the no-stats marker — Go parity)
    pub built_prompt: Option<String>,
    pub request_body: Option<serde_json::Value>,
    pub prompt_version: &'static str,
}

/// generate_rating runs the full per-entity generation (the analog of `RatingGenerator.Generate`,
/// minus persistence): `build_rating_request` → (skip-unchanged debounce) → `extract(StatsLogic)` →
/// parse → clean. The per-entity core the Step-3 batch bin loops over; also the parity `--vet` path.
/// `skip_unchanged` short-circuits (no model call) when the entity-season's last commentary was built
/// from the same rating snapshot (matching input_hash) — the nightly "work only on new data" gate.
pub async fn generate_rating(
    hx: &Harness,
    req: &RatingReq,
    temperature: f64,
    skip_unchanged: bool,
) -> Result<RatingOutput> {
    let ready = match build_rating_request(hx, req, temperature).await? {
        RatingBuild::NoStats { season } => {
            // The NULL-body marker. Go sets Model = ollama.Model() even here (so the read path sees
            // "no profile" with provenance), unlike vibe/transfer markers.
            let model = hx.router.for_role(Role::StatsLogic).model().to_string();
            return Ok(RatingOutput {
                season,
                skipped_no_stats: true,
                skipped_unchanged: false,
                body: None,
                divined_peak: None,
                notability: None,
                notability_components: serde_json::json!({}),
                input_components: "{}".to_string(),
                input_hash: None,
                model: Some(model),
                built_prompt: None,
                request_body: None,
                prompt_version: RATING_PROMPT_VERSION,
            });
        }
        RatingBuild::Ready(r) => *r,
    };

    if skip_unchanged {
        if let Some(last) =
            last_commentary_hash(&hx.pool, &req.entity_type, req.entity_id, &req.sport, ready.season)
                .await?
        {
            if last == ready.input_hash {
                return Ok(RatingOutput {
                    season: ready.season,
                    skipped_no_stats: false,
                    skipped_unchanged: true,
                    body: None,
                    divined_peak: None,
                    notability: None,
                    notability_components: serde_json::json!({}),
                    input_components: ready.input_components,
                    input_hash: Some(ready.input_hash),
                    model: Some(ready.model_configured),
                    built_prompt: None,
                    request_body: None,
                    prompt_version: RATING_PROMPT_VERSION,
                });
            }
        }
    }

    let extracted = hx
        .extract(Role::StatsLogic, &ready.built_prompt, &ready.opts, &RatingParser)
        .await?;
    let reply = extracted
        .value
        .ok_or_else(|| anyhow!("rating: parser returned None (RatingParser never fails closed)"))?;
    if reply.body.is_empty() {
        bail!(
            "rating: empty commentary ({}/{} {})",
            req.entity_type,
            req.entity_id,
            req.sport
        );
    }

    Ok(RatingOutput {
        season: ready.season,
        skipped_no_stats: false,
        skipped_unchanged: false,
        body: Some(reply.body),
        divined_peak: (!reply.divined_peak.is_empty()).then_some(reply.divined_peak),
        notability: Some(ready.notability),
        notability_components: ready.notability_components,
        input_components: ready.input_components,
        input_hash: Some(ready.input_hash),
        model: Some(extracted.model),
        built_prompt: Some(extracted.built_prompt),
        request_body: Some(extracted.request_body),
        prompt_version: RATING_PROMPT_VERSION,
    })
}

/// last_commentary_hash returns the input_hash of the entity-season's LATEST commentary — the nightly
/// skip signal. Canonical latest-generation rule (F-023): take the latest row regardless of
/// nullability; a no-stats marker has a NULL input_hash → None → the next run never wrongly skips
/// against an older real commentary the marker superseded. Mirrors `lastCommentaryHash`.
async fn last_commentary_hash(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: i32,
) -> Result<Option<String>> {
    let row: Option<Option<String>> = sqlx::query_scalar(
        "SELECT input_hash FROM stat_summaries \
         WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4 \
         ORDER BY generated_at DESC LIMIT 1",
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .fetch_optional(pool)
    .await
    .context("last commentary hash")?;
    Ok(row.flatten().filter(|s| !s.is_empty()))
}

/// persist_stat_summary writes ONE row to the LIVE stat_summaries table — the scored commentary and
/// the no-stats marker, which differ only in the bound values (body/notability/divined_peak NULL for
/// the marker; model_version is set for both). Mirrors `rating.go::persist`. Written + compiles;
/// NOT run this session (offline) — its first live run is the Step-3 cutover batch bin.
pub async fn persist_stat_summary(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    trigger_type: &str,
    trigger_payload: &serde_json::Value,
    out: &RatingOutput,
) -> Result<()> {
    let season: Option<i32> = (out.season > 0).then_some(out.season);
    let notability: Option<i16> = out.notability.map(|n| n as i16);
    let trigger_json = trigger_payload.to_string();
    let ncomp_json = out.notability_components.to_string();

    sqlx::query(
        r#"
        INSERT INTO stat_summaries (
            entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            body, notability, notability_components, input_components, input_hash,
            model_version, prompt_version, generated_at, divined_peak
        ) VALUES ($1,$2,$3,$4,$5,$6::jsonb, $7,$8,$9::jsonb,$10::jsonb,$11, $12,$13,NOW(),$14)
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .bind(trigger_type)
    .bind(&trigger_json)
    .bind(out.body.as_deref())
    .bind(notability)
    .bind(&ncomp_json)
    .bind(&out.input_components)
    .bind(out.input_hash.as_deref())
    .bind(out.model.as_deref())
    .bind(out.prompt_version)
    .bind(out.divined_peak.as_deref())
    .execute(pool)
    .await
    .context("persist stat summary")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dp(label: &str, value: f64, z: f64, pct: f64, sign: i32, specialty: bool) -> RatingDatapoint {
        RatingDatapoint {
            label: label.to_string(),
            value,
            z,
            pct,
            sign,
            is_specialty: specialty,
            ..Default::default()
        }
    }

    fn req(sport: &str, entity_type: &str, name: &str) -> RatingReq {
        RatingReq {
            entity_type: entity_type.to_string(),
            entity_id: 1,
            entity_name: name.to_string(),
            sport: sport.to_string(),
            season: None,
            trigger_type: "manual".to_string(),
        }
    }

    fn profile_player() -> RatingProfile {
        let mut scoring = dp("Scoring", 24.0, 3.1, 95.0, 1, true);
        scoring.scoped_pct.insert("position".to_string(), 88.0);
        RatingProfile {
            entity_type: "player".to_string(),
            season: 2025,
            position: "Guard".to_string(),
            composite_score: Some(67.0),
            peak_score: None,
            peak_label: "Scoring".to_string(),
            breakdown: vec![scoring, dp("Defense", 2.5, -0.5, 40.0, 1, false)],
            scoped_ranks: HashMap::new(),
            rate_modes: HashMap::new(),
        }
    }

    // --- build_stat_prompt byte-fixtures: the deterministic parity axis. The expected strings are
    // computed by hand from Go's buildStatPrompt, so a drift in the Rust assembly fails here (offline,
    // no model) before the live diff ever runs. ------------------------------------------------------

    #[test]
    fn prompt_player_composite_datapoints_and_scoped_position() {
        let p = profile_player();
        let prompt = build_stat_prompt(&req("NBA", "player", "Test Player"), &p, 70);
        assert_eq!(
            prompt,
            "Entity: Test Player (NBA player, Guard)\n\
\nProfile distinctiveness: 70/100 (higher = more standout skills — let a richer profile earn a fuller read).\n\
\nComposite (how WELL overall — T-score, 50 = average): 67\n\
\nDatapoints — value · percentile + TIER (the percentile mapped to elite/strong/above average/average/below average/poor; THIS TIER IS THE TRUTH) · z (standard deviations above the mean: the scarcity/scale of the edge; a high z is a rarer, more premium skill); [position] percentile shown when present:\n\
- Scoring: 24 · 95th pct (elite) · z +3.1 [position: 88th, strong]\n\
- Defense: 2.5 · 40th pct (below average) · z -0.5\n\
\nWrite the identity analysis now."
        );
    }

    #[test]
    fn prompt_team_no_composite_no_position() {
        // Team: position "" (no ", Guard" in the header), no composite line, one datapoint.
        let p = RatingProfile {
            entity_type: "team".to_string(),
            season: 2025,
            position: String::new(),
            composite_score: None,
            peak_score: None,
            peak_label: "Defense".to_string(),
            breakdown: vec![dp("Defense", 0.38, 1.2, 78.0, 1, false)],
            scoped_ranks: HashMap::new(),
            rate_modes: HashMap::new(),
        };
        let prompt = build_stat_prompt(&req("FOOTBALL", "team", "Test FC"), &p, 55);
        assert_eq!(
            prompt,
            "Entity: Test FC (FOOTBALL team)\n\
\nProfile distinctiveness: 55/100 (higher = more standout skills — let a richer profile earn a fuller read).\n\
\nDatapoints — value · percentile + TIER (the percentile mapped to elite/strong/above average/average/below average/poor; THIS TIER IS THE TRUTH) · z (standard deviations above the mean: the scarcity/scale of the edge; a high z is a rarer, more premium skill); [position] percentile shown when present:\n\
- Defense: 0.38 · 78th pct (strong) · z +1.2\n\
\nWrite the identity analysis now."
        );
    }

    // --- input_components canonical JSON: the input_hash pre-image (must match Go json.Marshal). -----

    #[test]
    fn input_components_matches_go_marshal_bytes() {
        // Datapoints walk the breakdown in STORED order (NOT pct-sorted): Scoring then Defense.
        // Top keys sorted: composite_score, datapoints, peak_label, position, season. Datapoint keys
        // sorted: is_specialty, label, pct. composite round1(67.0)=67 → "67"; pct round1 → "95"/"40".
        let ic = input_components(&profile_player());
        assert_eq!(
            ic,
            r#"{"composite_score":67,"datapoints":[{"is_specialty":true,"label":"Scoring","pct":95},{"is_specialty":false,"label":"Defense","pct":40}],"peak_label":"Scoring","position":"Guard","season":2025}"#
        );
        // The hash is a deterministic function of those exact bytes.
        assert_eq!(hash_components(&ic), hash_components(&ic));
    }

    #[test]
    fn input_components_omits_absent_optional_keys() {
        // No composite, no position, no peak_score, no rate modes → only season/peak_label/datapoints.
        let p = RatingProfile {
            entity_type: "team".to_string(),
            season: 2024,
            position: String::new(),
            composite_score: None,
            peak_score: None,
            peak_label: String::new(),
            breakdown: vec![],
            scoped_ranks: HashMap::new(),
            rate_modes: HashMap::new(),
        };
        // Empty breakdown → "datapoints":[] (Go marshals a non-nil empty slice as []); peak_label "".
        assert_eq!(
            input_components(&p),
            r#"{"datapoints":[],"peak_label":"","season":2024}"#
        );
    }

    #[test]
    fn rating_datapoint_tolerates_null_like_go() {
        // A sparse datapoint with an explicit null value + a null scoped entry — Go's json.Unmarshal
        // keeps zero values; serde must too (the L12 gate caught this: player:268's "Penalties Won"
        // carries "value": null, which plain #[serde(default)] would reject).
        let d: RatingDatapoint = serde_json::from_str(
            r#"{"label":"Penalties Won","value":null,"z":0.0,"pct":12.4,"scoped_pct":{"position":11.6,"x":null}}"#,
        )
        .expect("null tolerated like Go");
        assert_eq!(d.value, 0.0);
        assert_eq!(d.pct, 12.4);
        assert_eq!(d.scoped_pct.get("position"), Some(&11.6));
        assert_eq!(d.scoped_pct.get("x"), Some(&0.0)); // null map value → 0.0 (Go parity)
    }

    // --- deterministic helpers ----------------------------------------------------------------------

    #[test]
    fn pct_band_boundaries() {
        assert_eq!(pct_band(90.0), "elite");
        assert_eq!(pct_band(89.9), "strong");
        assert_eq!(pct_band(75.0), "strong");
        assert_eq!(pct_band(60.0), "above average");
        assert_eq!(pct_band(50.0), "average");
        assert_eq!(pct_band(49.9), "below average");
        assert_eq!(pct_band(35.0), "below average");
        assert_eq!(pct_band(34.9), "poor");
        assert_eq!(pct_band(0.0), "poor");
    }

    #[test]
    fn trim_float_formats_like_go() {
        assert_eq!(trim_float(3.0), "3"); // integral → %.0f
        assert_eq!(trim_float(24.0), "24");
        assert_eq!(trim_float(0.38), "0.38"); // abs < 1 → %.2f
        assert_eq!(trim_float(0.4), "0.40");
        assert_eq!(trim_float(10.7), "10.7"); // else → %.1f
        assert_eq!(trim_float(2.5), "2.5");
        assert_eq!(trim_float(-3.0), "-3");
    }

    #[test]
    fn compute_notability_known_case() {
        // peak 95, elite_count 1 (only Scoring ≥ 85), comp 67.
        // score = 0.6*95 + min(30, 10) + clamp(-10,10,(67-50)*0.4=6.8) = 57 + 10 + 6.8 = 73.8 → 74.
        let (n, comps) = compute_notability(&profile_player());
        assert_eq!(n, 74);
        assert_eq!(comps["peak_pct"], 95.0);
        assert_eq!(comps["elite_count"], 1);
        assert_eq!(comps["composite"], 67.0);
    }

    #[test]
    fn ordered_facts_sorts_desc_and_truncates() {
        let p = RatingProfile {
            entity_type: "player".to_string(),
            season: 2025,
            position: String::new(),
            composite_score: None,
            peak_score: None,
            peak_label: String::new(),
            breakdown: vec![
                dp("low", 1.0, 0.0, 20.0, 1, false),
                dp("high", 1.0, 0.0, 90.0, 1, false),
                dp("mid", 1.0, 0.0, 55.0, 1, false),
            ],
            scoped_ranks: HashMap::new(),
            rate_modes: HashMap::new(),
        };
        let ordered = ordered_facts(&p.breakdown);
        assert_eq!(
            ordered.iter().map(|d| d.label.as_str()).collect::<Vec<_>>(),
            vec!["high", "mid", "low"]
        );
    }

    // --- output parsing (mirror Go parsePeakCommentary / cleanCommentary) ---------------------------

    #[test]
    fn parse_peak_commentary_splits_marker_and_body() {
        let (peak, body) = parse_peak_commentary("PEAK: Elite scoring\nA lethal scorer who...");
        assert_eq!(peak, "Elite scoring");
        assert_eq!(body, "A lethal scorer who...");
    }

    #[test]
    fn parse_peak_commentary_accepts_legacy_sigil_prefix() {
        let (peak, body) = parse_peak_commentary("SIGIL: Rim protection\nDominant inside.");
        assert_eq!(peak, "Rim protection");
        assert_eq!(body, "Dominant inside.");
    }

    #[test]
    fn parse_peak_commentary_no_marker_is_all_body() {
        let (peak, body) = parse_peak_commentary("Just prose, no marker line\nsecond line");
        assert_eq!(peak, "");
        assert_eq!(body, "Just prose, no marker line\nsecond line");
    }

    #[test]
    fn clean_commentary_strips_fences_and_labels() {
        assert_eq!(clean_commentary("`Analysis: A solid two-way wing.`"), "A solid two-way wing.");
        assert_eq!(clean_commentary("  Identity: A poacher.  "), "A poacher.");
        assert_eq!(clean_commentary("Plain prose."), "Plain prose.");
    }

    #[test]
    fn rating_parser_never_fails_closed() {
        // Even garbage parses to Some (rating's only marker is pre-model); an empty body is the
        // caller's hard error, not a served UNKNOWN.
        let reply = RatingParser.parse("PEAK: X\nbody").unwrap().expect("always Some");
        assert_eq!(reply.divined_peak, "X");
        assert_eq!(reply.body, "body");
        assert!(RatingParser.parse("").unwrap().is_some());
    }
}
