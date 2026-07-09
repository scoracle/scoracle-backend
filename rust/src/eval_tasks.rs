//! Per-lens eval task registry (Multi-Lens Cognition Panel, Phase 3).
//!
//! `bin/eval` used to be hardwired to the vibe task (`Role::EmotionalNews`) and the live corpus.
//! A `LensTask` is the seam that generalizes it: each task knows its `Role`, its `GenerateOptions`
//! (system + num_predict + json_mode), how to build the exact PRODUCTION prompt for an entity, and
//! how to `evaluate` a raw reply into a `CaseVerdict`. It COMPOSES the capability library — the
//! stage loaders + prompt builders + parsers already in the lib — rather than reinventing them, so
//! the eval measures the real prompt with only the backend swapped.
//!
//! Two scoring axes, unified in `CaseVerdict`:
//!   - MAE (vibe live): `abs_err = |score - human_label|`.
//!   - property rubric (fixtures): named boolean `PropertyCheck`s from a fixture's `Expect`.
//!
//! The rubric lives in the fixture's `Expect`, not the task, so a task stays entity-agnostic
//! (task = the lens; a fixture SET like "disagreement" is a collection of `Expect`s over it).
//!
//! SAFETY: like `bin/eval` itself, tasks are read-only on the pipeline — they read corpus tables to
//! build a prompt and POST to the model; they NEVER claim `pipeline_work` or write a product table.

use crate::corpus::{load_transfer_heat, lookup_entity_name};
use crate::harness::Harness;
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::sigil::{
    build_synthesis_prompt, load_pillars, parse_synthesis_response, ParsedSynthesis,
    SIGIL_NUM_PREDICT, SIGIL_PROMPT_VERSION, SIGIL_SYSTEM_PROMPT,
};
use crate::vibe::{
    build_sentiment_prompt, load_latest_narratives, parse_sentiment_and_prompt, VIBE_NUM_PREDICT,
    VIBE_PROMPT_VERSION, VIBE_SYSTEM_PROMPT,
};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// EntitySpec is one entity a case scores. Lives here (not in the bin) so `build_prompt` and the
/// tests can construct it; the bin's CLI parser builds it from `entity_type:id:sport` tokens.
#[derive(Clone, Debug)]
pub struct EntitySpec {
    pub entity_type: String,
    pub entity_id: i32,
    pub sport: String,
}

impl EntitySpec {
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.entity_type, self.entity_id, self.sport)
    }
}

/// One named boolean assertion over a parsed reply (fixture property axis).
#[derive(Clone, Debug)]
pub struct PropertyCheck {
    pub name: String,
    pub pass: bool,
    /// Human-readable evidence for the ✓/✗ (e.g. `conv=70 ≤ 55`).
    pub detail: String,
}

/// CaseVerdict is one backend's scored answer for one case, task-agnostic: it carries BOTH the MAE
/// axis (`abs_err`, vibe live) and the property axis (`checks`, fixtures). `display` is the
/// one-line echo for the side-by-side. Perf metrics are held by the caller (identical per task).
#[derive(Clone, Debug)]
pub struct CaseVerdict {
    /// The reply parsed to the task's validated `T` (drives "scored N/n").
    pub parsed: bool,
    /// Mean-absolute-error axis: `Some` only when a numeric label AND a parsed score both exist.
    pub abs_err: Option<f64>,
    /// Property axis: empty for a pure-MAE (live, no expect) case.
    pub checks: Vec<PropertyCheck>,
    /// One-line score/prose echo.
    pub display: String,
}

impl CaseVerdict {
    pub fn all_checks_pass(&self) -> bool {
        self.checks.iter().all(|c| c.pass)
    }

    pub fn checks_passed(&self) -> usize {
        self.checks.iter().filter(|c| c.pass).count()
    }
}

/// Expect is the union of expected properties a fixture can assert. Each task reads only the subset
/// it understands and ignores the rest, so the fixture schema stays uniform and the loader
/// task-agnostic. `#[serde(default)]` lets a hand-authored fixture omit every field it does not use.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Expect {
    // vibe fixture score band (per-case boolean stand-in for the aggregate MAE axis).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_max: Option<i32>,
    // sigil panel-disagreement rubric.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub convergence_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub convergence_max: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disagreement_nonempty: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why_now_nonempty: Option<bool>,
    /// Catches example-parroting / asserts the real conflict is named (against the NORMALIZED
    /// disagreement — see `effective_disagreement`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disagreement_includes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disagreement_excludes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blurb_includes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blurb_excludes: Option<Vec<String>>,
}

/// A frozen eval case: the exact `system` + `user_prompt` (captured or hand-authored), the run
/// `temperature`, the `prompt_version` it was frozen under (drift-checked vs the live task), and
/// the expected properties. This is the reproducible regression unit — the same fixture yields the
/// same output every run (temperature 0).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fixture {
    pub name: String,
    pub task: String,
    pub prompt_version: String,
    pub system: String,
    pub user_prompt: String,
    pub temperature: f64,
    #[serde(default)]
    pub expect: Expect,
}

/// A lens eval task: the routing + prompt + scoring seam `bin/eval` runs against. Object-safe
/// (`build_prompt` boxed by `async_trait`), so tasks dispatch through `Box<dyn LensTask>`.
#[async_trait]
pub trait LensTask: Send + Sync {
    /// Registry key (`"vibe"`, `"sigil"`) — also the `fixtures/<name>/` dir.
    fn name(&self) -> &'static str;
    /// The role whose incumbent/candidate this task A/Bs.
    fn role(&self) -> Role;
    /// The stage's prompt-contract version — single-sourced from the stage const, drift-checked
    /// against a fixture's frozen `prompt_version`.
    fn prompt_version(&self) -> &'static str;
    /// system + num_predict + json_mode from the stage consts; the caller chooses `temperature`
    /// (live = 0.0 deterministic; fixture = the fixture's frozen value).
    fn gen_options(&self, temperature: f64) -> GenerateOptions;
    /// Build the EXACT production user-prompt for an entity. `Ok(None)` = no-corpus skip (the stage
    /// would write a marker without a model call — nothing to score).
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>>;
    /// Parse + score one raw reply. Pure/sync/offline. `label` drives the MAE axis (vibe live);
    /// `expect` drives the property axis (fixtures). Both optional and independent.
    fn evaluate(&self, raw: &str, label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict;
}

/// resolve_task maps a task name to its `LensTask`. Adding a task = a new unit struct + one arm.
pub fn resolve_task(name: &str) -> Option<Box<dyn LensTask>> {
    match name {
        "vibe" => Some(Box::new(VibeTask)),
        "sigil" => Some(Box::new(SigilTask)),
        _ => None,
    }
}

/// all_task_names lists the registered tasks (for usage output + unknown-task errors).
pub fn all_task_names() -> &'static [&'static str] {
    &["vibe", "sigil"]
}

/// fixture_drift returns a warning when a fixture was frozen under a different prompt contract than
/// the live task — the frozen `system`/`user_prompt` are then stale and the fixture should be
/// re-captured + re-annotated. Warn, never fail (a bump is a signal, not an error).
pub fn fixture_drift(fx: &Fixture, task: &dyn LensTask) -> Option<String> {
    if fx.prompt_version != task.prompt_version() {
        Some(format!(
            "fixture-rot: {} was frozen at prompt_version={} but task {} is now {} — re-capture + re-annotate",
            fx.name,
            fx.prompt_version,
            task.name(),
            task.prompt_version()
        ))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// VibeTask — behavior-preserving port of the original hardcoded eval path.
// ---------------------------------------------------------------------------

pub struct VibeTask;

#[async_trait]
impl LensTask for VibeTask {
    fn name(&self) -> &'static str {
        "vibe"
    }
    fn role(&self) -> Role {
        Role::EmotionalNews
    }
    fn prompt_version(&self) -> &'static str {
        VIBE_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> GenerateOptions {
        GenerateOptions {
            system: Some(VIBE_SYSTEM_PROMPT.to_string()),
            temperature: Some(temperature),
            num_predict: VIBE_NUM_PREDICT,
            json_mode: false,
        }
    }
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>> {
        let name = lookup_entity_name(&hx.pool, &e.entity_type, e.entity_id, &e.sport).await?;
        // Reads use the upper-cased sport; the prompt uses the request-case value, mirroring
        // generate_vibe (and the original build_vibe_prompt).
        let sport = e.sport.to_uppercase();
        let (narratives, _ids) =
            load_latest_narratives(&hx.pool, &e.entity_type, e.entity_id, &sport).await?;
        let heat = load_transfer_heat(&hx.pool, &e.entity_type, e.entity_id, &sport).await?;
        if narratives.is_empty() && heat.is_empty() {
            return Ok(None);
        }
        Ok(Some(build_sentiment_prompt(
            &e.entity_type,
            &name,
            &e.sport,
            &narratives,
            &heat,
        )))
    }
    fn evaluate(&self, raw: &str, label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        match parse_sentiment_and_prompt(raw) {
            Ok((s, v)) => {
                let mut checks = Vec::new();
                if let Some(x) = expect {
                    if let Some(min) = x.score_min {
                        checks.push(PropertyCheck {
                            name: "score_ge".into(),
                            pass: s >= min,
                            detail: format!("score={s} ≥ {min}"),
                        });
                    }
                    if let Some(max) = x.score_max {
                        checks.push(PropertyCheck {
                            name: "score_le".into(),
                            pass: s <= max,
                            detail: format!("score={s} ≤ {max}"),
                        });
                    }
                }
                CaseVerdict {
                    parsed: true,
                    abs_err: label.map(|l| (s as f64 - l).abs()),
                    checks,
                    display: format!("score={s} | {v}"),
                }
            }
            Err(_) => CaseVerdict {
                parsed: false,
                abs_err: None,
                checks: Vec::new(),
                display: "unparseable".into(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// SigilTask — panel synthesis + the disagreement rubric.
// ---------------------------------------------------------------------------

pub struct SigilTask;

/// effective_disagreement normalizes a placeholder DISAGREEMENT line to "absent". The parser keeps
/// the raw text (`Some("N/A")`), but mistral:7b emits `DISAGREEMENT: N/A` (often quoted) instead of
/// OMITTING the line when the lenses agree — so a literal `is_some()` would read an aligned case as
/// "disagreement present". Strips surrounding quotes too (the model wraps the line in `"..."`), so
/// `disagreement_includes`/`excludes` match the real content, not the quotes.
fn effective_disagreement(p: &ParsedSynthesis) -> Option<&str> {
    let raw = p.disagreement.as_deref()?;
    let t = raw.trim().trim_matches('"').trim();
    let low = t.to_ascii_lowercase();
    if t.is_empty() || matches!(low.as_str(), "n/a" | "na" | "none" | "-" | "n.a." | "null") {
        None
    } else {
        Some(t)
    }
}

/// disp_opt renders an optional convergence for the detail/echo lines.
fn disp_opt(o: Option<i32>) -> String {
    o.map(|c| c.to_string()).unwrap_or_else(|| "–".into())
}

#[async_trait]
impl LensTask for SigilTask {
    fn name(&self) -> &'static str {
        "sigil"
    }
    fn role(&self) -> Role {
        Role::StatsLogic
    }
    fn prompt_version(&self) -> &'static str {
        SIGIL_PROMPT_VERSION
    }
    fn gen_options(&self, temperature: f64) -> GenerateOptions {
        GenerateOptions {
            system: Some(SIGIL_SYSTEM_PROMPT.to_string()),
            temperature: Some(temperature),
            num_predict: SIGIL_NUM_PREDICT,
            json_mode: false,
        }
    }
    async fn build_prompt(&self, hx: &Harness, e: &EntitySpec) -> Result<Option<String>> {
        let name = lookup_entity_name(&hx.pool, &e.entity_type, e.entity_id, &e.sport).await?;
        let sport = e.sport.to_uppercase();
        let (_season, narratives, rating, vibe, momentum, transfers) =
            load_pillars(hx, &e.entity_type, e.entity_id, &sport).await?;
        // No-pillar path: the stage would persist a marker without a model call (sigil.rs) — no
        // synthesis to score.
        if narratives.is_empty()
            && rating.is_none()
            && vibe.is_none()
            && momentum.empty()
            && transfers.is_empty()
        {
            return Ok(None);
        }
        // prev_sigil = None: deterministic + reproducible, exactly as the parity path (sigil.rs).
        Ok(Some(build_synthesis_prompt(
            &e.entity_type,
            &name,
            &e.sport,
            &narratives,
            rating.as_ref(),
            vibe.as_ref(),
            &momentum,
            &transfers,
            None,
        )))
    }
    fn evaluate(&self, raw: &str, _label: Option<f64>, expect: Option<&Expect>) -> CaseVerdict {
        let p = parse_synthesis_response(raw);
        // Mirrors SigilParser's fail-closed gate: no parseable SCORE ⇒ score 0 ⇒ not a valid reply.
        let parsed = p.score != 0;
        let eff = effective_disagreement(&p);
        let mut checks = Vec::new();

        if let Some(x) = expect {
            if let Some(max) = x.convergence_max {
                checks.push(PropertyCheck {
                    name: "convergence_le".into(),
                    pass: p.convergence.is_some_and(|c| c <= max),
                    detail: format!("conv={} ≤ {max}", disp_opt(p.convergence)),
                });
            }
            if let Some(min) = x.convergence_min {
                checks.push(PropertyCheck {
                    name: "convergence_ge".into(),
                    pass: p.convergence.is_some_and(|c| c >= min),
                    detail: format!("conv={} ≥ {min}", disp_opt(p.convergence)),
                });
            }
            if let Some(want) = x.disagreement_nonempty {
                checks.push(PropertyCheck {
                    name: if want {
                        "disagreement_present".into()
                    } else {
                        "disagreement_absent".into()
                    },
                    pass: eff.is_some() == want,
                    detail: format!("disagreement={}", eff.unwrap_or("(none)")),
                });
            }
            for s in x.disagreement_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("disagreement_includes:{s}"),
                    pass: eff.is_some_and(|d| d.contains(s.as_str())),
                    detail: format!("disagreement={}", eff.unwrap_or("(none)")),
                });
            }
            for s in x.disagreement_excludes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("disagreement_excludes:{s}"),
                    pass: eff.is_none_or(|d| !d.contains(s.as_str())),
                    detail: format!("disagreement={}", eff.unwrap_or("(none)")),
                });
            }
            if let Some(want) = x.why_now_nonempty {
                checks.push(PropertyCheck {
                    name: if want {
                        "why_now_present".into()
                    } else {
                        "why_now_absent".into()
                    },
                    pass: p.why_now.is_some() == want,
                    detail: format!("why_now={}", p.why_now.as_deref().unwrap_or("(none)")),
                });
            }
            for s in x.blurb_includes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("blurb_includes:{s}"),
                    pass: p.blurb.contains(s.as_str()),
                    detail: String::new(),
                });
            }
            for s in x.blurb_excludes.iter().flatten() {
                checks.push(PropertyCheck {
                    name: format!("blurb_excludes:{s}"),
                    pass: !p.blurb.contains(s.as_str()),
                    detail: String::new(),
                });
            }
        }

        CaseVerdict {
            parsed,
            abs_err: None,
            checks,
            display: format!(
                "score={} conv={} | {}",
                p.score,
                disp_opt(p.convergence),
                p.blurb
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_resolves_known_tasks_and_rejects_unknown() {
        assert!(resolve_task("vibe").is_some());
        assert!(resolve_task("sigil").is_some());
        assert!(resolve_task("nope").is_none());
        assert_eq!(resolve_task("vibe").unwrap().name(), "vibe");
        assert_eq!(resolve_task("sigil").unwrap().name(), "sigil");
    }

    #[test]
    fn all_task_names_are_unique_and_resolvable() {
        let names = all_task_names();
        let mut seen = std::collections::HashSet::new();
        for n in names {
            assert!(seen.insert(*n), "duplicate task name {n}");
            assert!(resolve_task(n).is_some(), "{n} not resolvable");
        }
    }

    // --- sigil disagreement rubric ------------------------------------------------

    const CONFLICTED: &str = "SCORE: 68\nCONVERGENCE: 40\nDISAGREEMENT: strong PEAK vs sliding momentum and negative narrative\nWHY_NOW: trade-demand reports\nBLURB: Elite wing under pressure.";
    const CONVERGENT: &str = "SCORE: 87\nCONVERGENCE: 95\nBLURB: A rising guard drawing All-Star buzz.";

    #[test]
    fn sigil_rubric_passes_on_conflicted_reply() {
        let x = Expect {
            convergence_max: Some(55),
            disagreement_nonempty: Some(true),
            disagreement_includes: Some(vec!["PEAK".into()]),
            ..Default::default()
        };
        let v = SigilTask.evaluate(CONFLICTED, None, Some(&x));
        assert!(v.parsed);
        assert!(v.all_checks_pass(), "checks: {:?}", v.checks);
    }

    #[test]
    fn sigil_rubric_fails_convergent_reply_against_conflict_expect() {
        let x = Expect {
            convergence_max: Some(55),
            disagreement_nonempty: Some(true),
            ..Default::default()
        };
        let v = SigilTask.evaluate(CONVERGENT, None, Some(&x));
        // 95 is not <= 55, and there is no disagreement line.
        assert!(!v.all_checks_pass());
        assert_eq!(v.checks_passed(), 0);
    }

    #[test]
    fn sigil_aligned_expect_inverts_between_the_two_replies() {
        let x = Expect {
            convergence_min: Some(70),
            disagreement_nonempty: Some(false),
            ..Default::default()
        };
        assert!(SigilTask.evaluate(CONVERGENT, None, Some(&x)).all_checks_pass());
        assert!(!SigilTask.evaluate(CONFLICTED, None, Some(&x)).all_checks_pass());
    }

    #[test]
    fn disagreement_excludes_catches_parroted_example() {
        // The model parrots the system-prompt example for a case with no such conflict.
        let parroted = "SCORE: 65\nCONVERGENCE: 80\nDISAGREEMENT: \"strong PEAK vs sliding momentum and negative narrative\"\nBLURB: Role player amid trade talk.";
        let x = Expect {
            disagreement_excludes: Some(vec!["sliding momentum".into()]),
            ..Default::default()
        };
        let v = SigilTask.evaluate(parroted, None, Some(&x));
        assert!(!v.all_checks_pass(), "excludes should catch the parroted string");
    }

    #[test]
    fn effective_disagreement_normalizes_placeholders() {
        // DISAGREEMENT: N/A (and quoted / none / dash) must count as ABSENT.
        for raw in [
            "SCORE: 87\nCONVERGENCE: 95\nDISAGREEMENT: N/A\nBLURB: aligned.",
            "SCORE: 87\nCONVERGENCE: 95\nDISAGREEMENT: \"none\"\nBLURB: aligned.",
            "SCORE: 87\nCONVERGENCE: 95\nDISAGREEMENT: -\nBLURB: aligned.",
        ] {
            let x = Expect {
                disagreement_nonempty: Some(false),
                ..Default::default()
            };
            let v = SigilTask.evaluate(raw, None, Some(&x));
            assert!(v.all_checks_pass(), "placeholder should be absent for {raw:?}: {:?}", v.checks);
        }
        // An excludes check must not match a placeholder either.
        let x = Expect {
            disagreement_excludes: Some(vec!["sliding".into()]),
            ..Default::default()
        };
        let v = SigilTask.evaluate("SCORE: 50\nDISAGREEMENT: N/A\nBLURB: x.", None, Some(&x));
        assert!(v.all_checks_pass());
    }

    #[test]
    fn sigil_unparseable_reply_is_not_parsed() {
        let v = SigilTask.evaluate("the sigil feels like a 64 today", None, None);
        assert!(!v.parsed); // no SCORE line ⇒ score 0
    }

    // --- vibe MAE axis ------------------------------------------------------------

    #[test]
    fn vibe_evaluate_computes_abs_err() {
        let v = VibeTask.evaluate("SCORE: 30\nVIBE: grim outlook", Some(80.0), None);
        assert!(v.parsed);
        assert_eq!(v.abs_err, Some(50.0));
    }

    #[test]
    fn vibe_unparseable_has_no_abs_err() {
        let v = VibeTask.evaluate("no score here at all", Some(80.0), None);
        assert!(!v.parsed);
        assert_eq!(v.abs_err, None);
    }

    #[test]
    fn vibe_score_band_checks() {
        let x = Expect {
            score_max: Some(40),
            ..Default::default()
        };
        assert!(VibeTask
            .evaluate("SCORE: 30\nVIBE: grim", None, Some(&x))
            .all_checks_pass());
        assert!(!VibeTask
            .evaluate("SCORE: 70\nVIBE: bright", None, Some(&x))
            .all_checks_pass());
    }

    // --- fixture serde + drift ----------------------------------------------------

    #[test]
    fn fixture_round_trips_and_defaults_expect() {
        let json = r#"{
            "name": "aligned-convergent",
            "task": "sigil",
            "prompt_version": "s11",
            "system": "SYS",
            "user_prompt": "Entity: X",
            "temperature": 0.0,
            "expect": { "convergence_min": 70, "disagreement_nonempty": false }
        }"#;
        let fx: Fixture = serde_json::from_str(json).unwrap();
        assert_eq!(fx.name, "aligned-convergent");
        assert_eq!(fx.expect.convergence_min, Some(70));
        assert_eq!(fx.expect.disagreement_nonempty, Some(false));
        assert_eq!(fx.expect.score_min, None); // defaulted
                                                // A fixture may omit expect entirely.
        let bare = r#"{"name":"n","task":"sigil","prompt_version":"s11","system":"s","user_prompt":"u","temperature":0.0}"#;
        let fx2: Fixture = serde_json::from_str(bare).unwrap();
        assert_eq!(fx2.expect.convergence_min, None);
    }

    #[test]
    fn fixture_drift_flags_prompt_version_mismatch() {
        let mut fx = Fixture {
            name: "f".into(),
            task: "sigil".into(),
            prompt_version: SIGIL_PROMPT_VERSION.into(),
            system: "s".into(),
            user_prompt: "u".into(),
            temperature: 0.0,
            expect: Expect::default(),
        };
        assert!(fixture_drift(&fx, &SigilTask).is_none());
        fx.prompt_version = "s1".into();
        assert!(fixture_drift(&fx, &SigilTask).is_some());
    }
}
