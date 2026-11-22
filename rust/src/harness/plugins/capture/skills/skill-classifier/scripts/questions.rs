// Keys are also the required response fields. {subject} is filled from target identity.
pub(crate) const QUESTIONS: [(&str, &str); 4] = [
    (
        "subject",
        r#"Does the headline state a concrete development directly involving {subject}? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not."#,
    ),
    (
        "emotion",
        r#"Does the headline express a feeling or judgment about {subject}, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient."#,
    ),
    (
        "movement",
        r#"Does the headline report a personnel or contract development involving {subject}—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice."#,
    ),
    (
        "availability",
        r#"Does the headline report whether {subject} can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not."#,
    ),
];
