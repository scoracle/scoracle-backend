//! Evidence and continuity supplied to the character.

use super::{
    momentum_score, momentum_score_label, trend_dir, SynthMomentum, SynthNarrative, SynthRating,
    SynthVibe,
};
use crate::corpus::{write_heat_lines, HeatItem};
use crate::trajectory::trajectory_label;

pub const CROWN_CARD_BODY_CAP: usize = 700;

const CROWN_MAX_NARRATIVES: usize = 3;

pub(crate) fn descrub_z(brief: &str) -> String {
    let b = brief.as_bytes();
    let mut out = String::with_capacity(brief.len());
    let mut i = 0usize;
    while i < b.len() {
        let is_z =
            (b[i] == b'z' || b[i] == b'Z') && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) && {
                let mut j = i + 1;
                while j < b.len() && b[j] == b' ' {
                    j += 1;
                }
                if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                    j += 1;
                }
                j < b.len() && b[j].is_ascii_digit()
            };
        if !is_z {
            let ch = brief[i..].chars().next().map(char::len_utf8).unwrap_or(1);
            out.push_str(&brief[i..i + ch]);
            i += ch;
            continue;
        }
        while out
            .chars()
            .next_back()
            .is_some_and(|c| c == ' ' || c == ',' || c == ';')
        {
            out.pop();
        }
        i += 1;
        while i < b.len() && (b[i] == b' ' || b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
            i += 1;
        }
    }
    out.replace(" )", ")")
        .replace("()", "")
        .replace(" .", ".")
        .replace(" ,", ",")
}

fn capped(s: &str, budget: Option<usize>) -> String {
    match budget {
        Some(max) => crate::util::truncate_bytes(s, max),
        None => s.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_crown_prompt(
    entity_type: &str,
    entity_name: &str,
    sport_raw: &str,
    narratives: &[SynthNarrative],
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
    transfers: &[HeatItem],
    omen: &str,
    omen_reason: &str,
    body_cap: Option<usize>,
    identity: Option<&str>,
) -> String {
    let mut b = String::new();

    b.push_str(&format!(
        "Entity: {entity_name} ({sport_raw} {entity_type})\n"
    ));

    if let Some(card) = identity {
        b.push('\n');
        b.push_str(card);
        b.push('\n');
    }

    if !narratives.is_empty() {
        b.push_str("\n=== THE JOURNALIST'S CARD (news storylines) ===\n");
        let (shown, per_body) = match body_cap {
            Some(cap) => {
                let n = narratives.len().min(CROWN_MAX_NARRATIVES);
                (&narratives[..n], Some(cap / n.max(1)))
            }
            None => (narratives, None),
        };
        for n in shown {
            let mut tags = format!(
                "impact {:.0}, {}",
                n.impact,
                trajectory_label(&n.trajectory)
            );
            if n.source_count > 0 {
                tags.push_str(&format!(", {} sources", n.source_count));
            }
            if let Some(d) = n.source_age_days {
                tags.push_str(&format!(", latest {d}d ago"));
            }
            b.push_str(&format!(
                "[{tags}] {}\n{}\n\n",
                n.title,
                capped(&n.body, per_body)
            ));
        }
        if narratives.len() > shown.len() {
            b.push_str(&format!(
                "(+{} more storyline(s) not shown — budget)\n\n",
                narratives.len() - shown.len()
            ));
        }
    } else {
        b.push_str("\n=== THE JOURNALIST'S CARD (news storylines) ===\n(no recent narratives)\n");
    }

    b.push_str("\n=== THE SCOUT'S CARD (scouting brief) ===\n");
    if let Some(r) = rating {
        b.push_str(&format!("Profile strength: {}/100\n", r.notability));
        if !r.body.is_empty() {
            b.push_str(&capped(&descrub_z(&r.body), body_cap));
            b.push('\n');
        }
    } else {
        b.push_str("(no stat commentary available)\n");
    }

    b.push_str("\n=== THE INFLUENCER'S CARD (the felt read) ===\n");
    if let Some(v) = vibe {
        b.push_str(&format!("Mood: {}/100\n", v.sentiment));
        if !v.prompt.is_empty() {
            b.push_str(&capped(&v.prompt, body_cap));
            b.push('\n');
        }
    } else {
        b.push_str("(no vibe prompt available)\n");
    }

    b.push_str("\n=== THE ANALYST'S CARD (momentum) ===\n");
    if mom.blurb.is_some() || mom.direction.is_some() {
        let direction = mom.direction.as_deref().unwrap_or("steady");
        if let Some(score) = momentum_score(mom) {
            b.push_str(&format!("Momentum: {direction} (score {score})\n"));
        } else {
            b.push_str(&format!("Momentum: {direction}\n"));
        }
        if let Some(blurb) = &mom.blurb {
            b.push_str(&capped(blurb, body_cap));
            b.push('\n');
        }
    } else if let Some(score) = momentum_score(mom) {
        b.push_str(&format!(
            "Momentum score: {score} ({})\n",
            momentum_score_label(score)
        ));
    }
    if let Some(s) = mom.vibe_slope {
        let dir = trend_dir(s);
        b.push_str(&format!(
            "Mood trend: {s:.1} over {} samples ({dir})\n",
            mom.vibe_samples
        ));
    }
    if let Some(s) = mom.rating_slope {
        let dir = trend_dir(s);
        b.push_str(&format!(
            "Form trend: {s:.1} over {} samples ({dir})\n",
            mom.rating_samples
        ));
    }
    if mom.empty() {
        b.push_str("(no momentum data)\n");
    }

    b.push_str("\n=== THE INSIDER'S CARD (transfer wire) ===\n");
    if transfers.is_empty() {
        b.push_str("(no active transfer rumors)\n");
    } else {
        write_heat_lines(&mut b, transfers);
    }

    b.push_str(&format!(
        "\n=== THE OMEN (computed) ===\nOmen: {omen} — {omen_reason}\n"
    ));

    b
}
