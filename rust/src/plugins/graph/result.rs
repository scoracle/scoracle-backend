//! Deterministic parsing of one verbatim, source-grounded final score line.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedResult {
    pub home: String,
    pub home_score: u32,
    pub away_score: u32,
    pub away: String,
}

/// A line must contain exactly one score token with a non-empty name on each side.
pub fn parse_result_line(line: &str) -> Option<ParsedResult> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut found: Option<(usize, u32, u32)> = None;
    for (i, tok) in tokens.iter().enumerate() {
        if let Some((h, a)) = parse_score_token(tok) {
            if found.is_some() {
                return None;
            }
            found = Some((i, h, a));
        }
    }
    let (i, home_score, away_score) = found?;
    if i == 0 || i == tokens.len() - 1 {
        return None;
    }
    Some(ParsedResult {
        home: tokens[..i].join(" "),
        home_score,
        away_score,
        away: tokens[i + 1..].join(" "),
    })
}

fn parse_score_token(tok: &str) -> Option<(u32, u32)> {
    let sep = tok.find(['-', '–', '—', ':'])?;
    let (h, a) = (&tok[..sep], &tok[sep..]);
    let a = a.trim_start_matches(['-', '–', '—', ':']);
    if h.is_empty() || a.is_empty() {
        return None;
    }
    let h: u32 = h.parse().ok()?;
    let a: u32 = a.parse().ok()?;
    if h > 150 || a > 150 {
        return None;
    }
    Some((h, a))
}
