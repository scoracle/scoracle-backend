//! Evidence and continuity supplied to the character.

use super::{Form, Mood, Snapshot};

fn dated_movement(
    slope: Option<f64>,
    samples: i32,
    start: Option<&str>,
    end: Option<&str>,
    sample_name: &str,
) -> String {
    let Some(s) = slope else {
        return "not measured".to_string();
    };
    let change = if s > 0.0 {
        format!("rose by {:.1} points", s.abs())
    } else if s < 0.0 {
        format!("fell by {:.1} points", s.abs())
    } else {
        "was unchanged".to_string()
    };
    let window = match (start, end) {
        (Some(start), Some(end)) => format!(" from {start} to {end}"),
        _ => " over an unavailable date window".to_string(),
    };
    format!("{change}{window}, across {samples} {sample_name}")
}

fn render_reading(out: &mut String, label: &str, reading: Option<&Form>) {
    out.push_str(&format!("=== {label} ===\n"));
    let Some(reading) = reading else {
        out.push_str("Not available.\n");
        return;
    };
    if let Some(date) = reading.generated_at.as_deref() {
        out.push_str(&format!("Written: {date}\n"));
    }
    if let Some(season) = reading.season {
        out.push_str(&format!("Season: {season}\n"));
    }
    if let Some(headline) = reading.headline.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!("Headline: {headline}\n"));
    }
    out.push_str(&format!("Reading: {}\n", reading.body));
}

fn render_mood(out: &mut String, reading: Option<&Mood>) {
    out.push_str("=== INFLUENCER READING ===\n");
    let Some(reading) = reading else {
        out.push_str("Not available.\n");
        return;
    };
    if let Some(date) = reading.generated_at.as_deref() {
        out.push_str(&format!("Written: {date}\n"));
    }
    if let Some(headline) = reading.headline.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!("Headline: {headline}\n"));
    }
    if let Some(sentiment) = reading.sentiment {
        out.push_str(&format!(
            "Current sentiment level: {sentiment} on a 1–100 scale\n"
        ));
    }
    out.push_str(&format!("Reading: {}\n", reading.body));
}

pub fn build_momentum_prompt(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    rating: Option<&Form>,
    vibe: Option<&Mood>,
    mom: &Snapshot,
    memory: Option<&str>,
) -> String {
    let mut b = String::new();
    b.push_str(&format!("Entity: {entity_name} ({sport} {entity_type})\n"));
    if let Some(card) = memory {
        b.push('\n');
        b.push_str(card);
        b.push('\n');
    }
    b.push('\n');
    render_reading(&mut b, "SCOUT READING", rating);
    b.push('\n');
    render_mood(&mut b, vibe);
    b.push_str("\n=== DATED TRAJECTORY STUDY ===\n");
    if let Some(date) = mom.generated_at.as_deref() {
        b.push_str(&format!("Computed: {date}\n"));
    }
    b.push_str(&format!(
        "Statistical form: {}.\n",
        dated_movement(
            mom.rating_slope,
            mom.rating_samples,
            mom.rating_window_start.as_deref(),
            mom.rating_window_end.as_deref(),
            "rated games",
        )
    ));
    b.push_str(&format!(
        "Reported mood: {}.\n",
        dated_movement(
            mom.vibe_slope,
            mom.vibe_samples,
            mom.vibe_window_start.as_deref(),
            mom.vibe_window_end.as_deref(),
            "scored readings",
        )
    ));
    b.push_str("The two windows are independent. Missing means unmeasured, not flat. The study records change, not its cause.\n");
    b
}
