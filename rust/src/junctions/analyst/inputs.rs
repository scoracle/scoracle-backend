//! Evidence and continuity supplied to the character.

use super::{momentum_conviction_from_score, momentum_direction_from_score};
use crate::junctions::oracle::{SynthMomentum, SynthRating, SynthVibe};

fn rail_movement(slope: Option<f64>, samples: i32) -> String {
    let Some(s) = slope else {
        return "not measured".to_string();
    };
    let size = match s.abs() {
        x if x < 5.0 => "flat",
        x if x < 15.0 => "drifting",
        x if x < 30.0 => "moving clearly",
        _ => "moving hard",
    };
    let way = if s > 0.0 { "up" } else { "down" };
    let confidence = match samples {
        0..=3 => "on a thin sample",
        4..=8 => "on a modest sample",
        _ => "on a healthy sample",
    };
    if size == "flat" {
        format!("flat {confidence}")
    } else {
        format!("{size} {way}, {confidence}")
    }
}

fn mood_level(sentiment: i32) -> &'static str {
    match sentiment {
        i32::MIN..=20 => "very low",
        21..=40 => "low",
        41..=60 => "middling",
        61..=75 => "warm",
        76..=90 => "high",
        _ => "very high",
    }
}

pub fn build_momentum_prompt(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
    identity: Option<&str>,
) -> String {
    let mut b = String::new();
    b.push_str(&format!("Entity: {entity_name} ({sport} {entity_type})\n"));
    if let Some(card) = identity {
        b.push('\n');
        b.push_str(card);
        b.push('\n');
    }
    b.push('\n');
    b.push_str("=== THE FORM RAIL (recent statistical performance) ===\n");
    match mom.rating_slope {
        Some(_) => b.push_str(&format!(
            "Form is: {}\n",
            rail_movement(mom.rating_slope, mom.rating_samples)
        )),
        None => match rating {
            Some(r) if !r.rating_trajectory_label.trim().is_empty() => {
                b.push_str(&format!("Form is: {}\n", r.rating_trajectory_label))
            }
            Some(r) if !r.rating_trajectory.trim().is_empty() => {
                b.push_str(&format!("Form is: {}\n", r.rating_trajectory))
            }
            _ => b.push_str("Form is: not measured\n"),
        },
    }
    b.push_str("\n=== THE MOOD RAIL (the feeling around them) ===\n");
    match vibe {
        Some(v) => b.push_str(&format!("Mood stands: {}\n", mood_level(v.sentiment))),
        None => b.push_str("Mood level: not measured\n"),
    }
    b.push_str(&format!(
        "Mood is: {}\n",
        rail_movement(mom.vibe_slope, mom.vibe_samples)
    ));
    if mom.empty() {
        b.push_str("\n(no durable momentum snapshot)\n");
    }
    match mom.momentum_score {
        Some(score) => {
            let strength = match momentum_conviction_from_score(Some(score)).abs() {
                0 => "genuinely flat — no measured lean",
                1 => "barely visible — a lean, not yet a move",
                2 => "modest but real",
                3 => "clean and well supported",
                4 => "strong — one of the clearer moves on the slate",
                _ => "emphatic — as hard as the scale measures",
            };
            b.push_str(&format!(
                "\nDirection (decided upstream, final): {} — strength of the move, also decided upstream: {}\n",
                momentum_direction_from_score(Some(score)),
                strength
            ));
        }
        None => b.push_str(
            "\nDirection (decided upstream, final): steady (no durable momentum snapshot)\n",
        ),
    }
    b
}
