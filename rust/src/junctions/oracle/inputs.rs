//! Evidence and continuity supplied to the character.

use super::{
    momentum_score, momentum_score_label, SynthMomentum, SynthNarrative, SynthRating, SynthVibe,
};
use crate::corpus::HeatItem;
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

fn write_transfer_evidence(b: &mut String, entity_name: &str, transfers: &[HeatItem]) {
    for transfer in transfers {
        let movement = match transfer.direction.as_str() {
            "incoming" => format!("From {} to {entity_name}", transfer.counterparty),
            "outgoing" => format!("From {entity_name} to {}", transfer.counterparty),
            _ => format!("{entity_name} and {}", transfer.counterparty),
        };
        let mut line = format!("- {movement}");
        if !transfer.stage.is_empty() {
            line.push_str("; ");
            line.push_str(&transfer.stage.replace('_', " "));
        }
        if !transfer.summary.is_empty() {
            line.push_str(" — \"");
            line.push_str(&transfer.summary.replace(['\n', '\r'], " "));
            line.push('"');
        }
        b.push_str(&line);
        b.push('\n');
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
        b.push('\n');
        let (shown, per_body) = match body_cap {
            Some(cap) => {
                let n = narratives.len().min(CROWN_MAX_NARRATIVES);
                (&narratives[..n], Some(cap / n.max(1)))
            }
            None => (narratives, None),
        };
        for n in shown {
            let mut tags = trajectory_label(&n.trajectory).to_string();
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
    }

    if let Some(r) = rating {
        if !r.body.is_empty() {
            b.push('\n');
            b.push_str(&capped(&descrub_z(&r.body), body_cap));
            b.push('\n');
        }
    }

    if let Some(v) = vibe {
        if !v.prompt.is_empty() {
            b.push('\n');
            b.push_str(&capped(&v.prompt, body_cap));
            b.push('\n');
        }
    }

    if let Some(blurb) = &mom.blurb {
        b.push('\n');
        b.push_str(&capped(blurb, body_cap));
        b.push('\n');
    } else if let Some(direction) = mom.direction.as_deref() {
        b.push_str(&format!("\nRecent movement is {direction}.\n"));
    } else if let Some(score) = momentum_score(mom) {
        b.push_str(&format!(
            "\nRecent movement is {}.\n",
            momentum_score_label(score)
        ));
    }

    if !transfers.is_empty() {
        b.push('\n');
        write_transfer_evidence(&mut b, entity_name, transfers);
    }

    b.push_str(&format!("\nPresent direction: {omen}.\n"));

    b
}
