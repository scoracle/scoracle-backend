# Other-entity context: full paired prompts and outputs

Current v17/v8 instructions are fixed in both arms, including shared relationship rules in task. Only `target.named_people` changes. Six cases receive other-person context; nine are unchanged controls. Context is backed by canonical database matches; dated roster records are not a claim of current affiliation.

## 01-charlotte-hornets — context added

**Headline:** Hornets blown out by Hawks 128-81 in first game home without LaMelo Ball, booed by sellout crowd

**Context:** LaMelo Ball (player); recorded affiliation: this target (Charlotte Hornets), observed 2026-07-03

**Review:** Dated LaMelo/Hornets affiliation causes small score changes and no routing change. This does not establish the reason he is absent.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 97.10 | 97.60 | +0.51 |
| emotion | 47.86 | 47.24 | -0.61 |
| movement | 3.11 | 7.78 | +4.68 |
| availability | 2.91 | 6.84 | +3.93 |

### Without other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Charlotte Hornets",
    "entity_type": "team",
    "sport": "NBA",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "Charlotte",
      "coach_on_record": {
        "name": "Steve Clifford",
        "observed_at": "2026-10-01"
      },
      "conference": "East",
      "country": "United States",
      "division": "Southeast",
      "home_venue_on_record": "Spectrum Center"
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Hornets blown out by Hawks 128-81 in first game home without LaMelo Ball, booed by sellout crowd",
    "publisher": "AP News"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Charlotte Hornets, the NBA team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Charlotte Hornets, the NBA team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Charlotte Hornets, the NBA team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Charlotte Hornets, the NBA team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.029050111192702628,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.47857143592291645,
      "type": "noul"
    },
    "movement": {
      "noul": 0.031058679920395266,
      "type": "noul"
    },
    "subject": {
      "noul": 0.9709641959387602,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2428,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](01/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Charlotte Hornets",
    "entity_type": "team",
    "sport": "NBA",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "Charlotte",
      "coach_on_record": {
        "name": "Steve Clifford",
        "observed_at": "2026-10-01"
      },
      "conference": "East",
      "country": "United States",
      "division": "Southeast",
      "home_venue_on_record": "Spectrum Center"
    },
    "named_people": "LaMelo Ball (player); recorded affiliation: this target (Charlotte Hornets), observed 2026-07-03"
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Hornets blown out by Hawks 128-81 in first game home without LaMelo Ball, booed by sellout crowd",
    "publisher": "AP News"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Charlotte Hornets, the NBA team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Charlotte Hornets, the NBA team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Charlotte Hornets, the NBA team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Charlotte Hornets, the NBA team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.06837074099005605,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.4724353173296722,
      "type": "noul"
    },
    "movement": {
      "noul": 0.07781044966272624,
      "type": "noul"
    },
    "subject": {
      "noul": 0.9760497052671678,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2572,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](01/with/request.json)

## 02-philadelphia-eagles — context added

**Headline:** ‘Knows Who He Makes His Money From,’ ‘So Exhausting’ — NFL Fans React to Stephen A. Smith Sitting Next to Jerry Jones During Cowboys-Eagles

**Context:** Jerry Jones (person)

**Review:** Jerry Jones is identified only as person. The incidental Eagles story remains correctly rejected in both arms; role and affiliation are still missing.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 25.51 | 26.12 | +0.61 |
| emotion | 45.71 | 47.08 | +1.37 |
| movement | 4.48 | 6.14 | +1.66 |
| availability | 0.69 | 0.53 | -0.16 |

### Without other-person context

**Errors:** none.

**Routes:** none

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Philadelphia Eagles",
    "entity_type": "team",
    "sport": "NFL",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "Philadelphia",
      "coach_on_record": {
        "name": "Dan Quinn",
        "observed_at": "2026-09-30"
      },
      "conference": "NFC",
      "country": "United States",
      "division": "EAST"
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "‘Knows Who He Makes His Money From,’ ‘So Exhausting’ — NFL Fans React to Stephen A. Smith Sitting Next to Jerry Jones During Cowboys-Eagles",
    "publisher": "Yahoo Sports"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Philadelphia Eagles, the NFL team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Philadelphia Eagles, the NFL team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Philadelphia Eagles, the NFL team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Philadelphia Eagles, the NFL team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.006937185979174601,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.4570844140190642,
      "type": "noul"
    },
    "movement": {
      "noul": 0.044816090378292095,
      "type": "noul"
    },
    "subject": {
      "noul": 0.2550520017808417,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2388,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](02/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** none

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Philadelphia Eagles",
    "entity_type": "team",
    "sport": "NFL",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "Philadelphia",
      "coach_on_record": {
        "name": "Dan Quinn",
        "observed_at": "2026-09-30"
      },
      "conference": "NFC",
      "country": "United States",
      "division": "EAST"
    },
    "named_people": "Jerry Jones (person)"
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "‘Knows Who He Makes His Money From,’ ‘So Exhausting’ — NFL Fans React to Stephen A. Smith Sitting Next to Jerry Jones During Cowboys-Eagles",
    "publisher": "Yahoo Sports"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Philadelphia Eagles, the NFL team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Philadelphia Eagles, the NFL team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Philadelphia Eagles, the NFL team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Philadelphia Eagles, the NFL team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.0053473985155672666,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.47075585243110984,
      "type": "noul"
    },
    "movement": {
      "noul": 0.06143569693294387,
      "type": "noul"
    },
    "subject": {
      "noul": 0.26119858339347346,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2436,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](02/with/request.json)

## 03-seattle-seahawks — context added

**Headline:** ‘Troy Aikman Looks Like He Hates Joe Buck,’ ‘I Can Feel the Tension’ — NFL Fans React to ESPN’s ‘Awkward’ Bears-Seahawks Broadcast

**Context:** Troy Aikman (person)

**Review:** Troy Aikman is identified only as person. Relevance drops toward the cutoff but the broadcast story remains falsely admitted; broadcaster role is not supplied.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 63.75 | 56.09 | -7.65 |
| emotion | 64.47 | 66.31 | +1.84 |
| movement | 5.96 | 3.97 | -1.99 |
| availability | 2.04 | 0.89 | -1.15 |

### Without other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Seattle Seahawks",
    "entity_type": "team",
    "sport": "NFL",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "Seattle",
      "coach_on_record": {
        "name": "Mike Macdonald",
        "observed_at": "2026-09-30"
      },
      "conference": "NFC",
      "country": "United States",
      "division": "WEST",
      "home_venue_on_record": "Lumen Field"
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "‘Troy Aikman Looks Like He Hates Joe Buck,’ ‘I Can Feel the Tension’ — NFL Fans React to ESPN’s ‘Awkward’ Bears-Seahawks Broadcast",
    "publisher": "Yahoo Sports"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Seattle Seahawks, the NFL team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Seattle Seahawks, the NFL team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Seattle Seahawks, the NFL team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Seattle Seahawks, the NFL team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.020393344527846625,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.6447342375709176,
      "type": "noul"
    },
    "movement": {
      "noul": 0.059597429707924504,
      "type": "noul"
    },
    "subject": {
      "noul": 0.6374690563939184,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2468,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](03/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Seattle Seahawks",
    "entity_type": "team",
    "sport": "NFL",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "Seattle",
      "coach_on_record": {
        "name": "Mike Macdonald",
        "observed_at": "2026-09-30"
      },
      "conference": "NFC",
      "country": "United States",
      "division": "WEST",
      "home_venue_on_record": "Lumen Field"
    },
    "named_people": "Troy Aikman (person)"
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "‘Troy Aikman Looks Like He Hates Joe Buck,’ ‘I Can Feel the Tension’ — NFL Fans React to ESPN’s ‘Awkward’ Bears-Seahawks Broadcast",
    "publisher": "Yahoo Sports"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Seattle Seahawks, the NFL team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Seattle Seahawks, the NFL team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Seattle Seahawks, the NFL team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Seattle Seahawks, the NFL team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.008901021231664823,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.6631435897817501,
      "type": "noul"
    },
    "movement": {
      "noul": 0.03971303855766416,
      "type": "noul"
    },
    "subject": {
      "noul": 0.5609275714615994,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2528,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](03/with/request.json)

## 04-arsenal — unchanged control

**Headline:** ‘What you live for’: Fleetwood revel in seismic Arsenal shock after Dowman sees red

**Context:** No other-person match. Identical prompt in both arms.

**Review:** No other-person context found. Identical scores are a control result, not evidence for or against added relationships.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 70.65 | 70.65 | +0.00 |
| emotion | 63.36 | 63.36 | +0.00 |
| movement | 46.07 | 46.07 | +0.00 |
| availability | 10.95 | 10.95 | +0.00 |

### Without other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Arsenal",
    "entity_type": "team",
    "sport": "FOOTBALL",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "London",
      "coach_on_record": {
        "name": "Régis Le Bris",
        "observed_at": "2026-09-30"
      },
      "country": "England",
      "home_venue_on_record": "Emirates Stadium",
      "league": {
        "country": "England",
        "name": "Premier League"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "‘What you live for’: Fleetwood revel in seismic Arsenal shock after Dowman sees red",
    "publisher": "The Guardian"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Arsenal, the FOOTBALL team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Arsenal, the FOOTBALL team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Arsenal, the FOOTBALL team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Arsenal, the FOOTBALL team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.10953645640553672,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.6336141904287121,
      "type": "noul"
    },
    "movement": {
      "noul": 0.4607333766753973,
      "type": "noul"
    },
    "subject": {
      "noul": 0.7065298725763065,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2432,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](04/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Arsenal",
    "entity_type": "team",
    "sport": "FOOTBALL",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "London",
      "coach_on_record": {
        "name": "Régis Le Bris",
        "observed_at": "2026-09-30"
      },
      "country": "England",
      "home_venue_on_record": "Emirates Stadium",
      "league": {
        "country": "England",
        "name": "Premier League"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "‘What you live for’: Fleetwood revel in seismic Arsenal shock after Dowman sees red",
    "publisher": "The Guardian"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Arsenal, the FOOTBALL team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Arsenal, the FOOTBALL team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Arsenal, the FOOTBALL team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Arsenal, the FOOTBALL team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.10953645640553672,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.6336141904287121,
      "type": "noul"
    },
    "movement": {
      "noul": 0.4607333766753973,
      "type": "noul"
    },
    "subject": {
      "noul": 0.7065298725763065,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2432,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](04/with/request.json)

## 05-roma — unchanged control

**Headline:** Allegri enraged after Napoli 3-3 Roma: ‘Maybe I shouldn’t be on the touchline’

**Context:** No other-person match. Identical prompt in both arms.

**Review:** No Allegri match found. Identical scores; the relevant speaker’s role and relationship were not tested.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 86.93 | 86.93 | +0.00 |
| emotion | 70.00 | 70.00 | +0.00 |
| movement | 10.48 | 10.48 | +0.00 |
| availability | 5.16 | 5.16 | +0.00 |

### Without other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Roma",
    "entity_type": "team",
    "sport": "FOOTBALL",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "Roma",
      "coach_on_record": {
        "name": "Mirko Vučinić",
        "observed_at": "2026-10-02"
      },
      "country": "Italy",
      "home_venue_on_record": "Stadio Olimpico",
      "league": {
        "country": "Italy",
        "name": "Serie A"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Allegri enraged after Napoli 3-3 Roma: ‘Maybe I shouldn’t be on the touchline’",
    "publisher": "Yahoo Sports"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Roma, the FOOTBALL team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Roma, the FOOTBALL team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Roma, the FOOTBALL team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Roma, the FOOTBALL team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.051607224545290466,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.7000113750529967,
      "type": "noul"
    },
    "movement": {
      "noul": 0.10478546937679838,
      "type": "noul"
    },
    "subject": {
      "noul": 0.8693126031262948,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2452,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](05/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Roma",
    "entity_type": "team",
    "sport": "FOOTBALL",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "Roma",
      "coach_on_record": {
        "name": "Mirko Vučinić",
        "observed_at": "2026-10-02"
      },
      "country": "Italy",
      "home_venue_on_record": "Stadio Olimpico",
      "league": {
        "country": "Italy",
        "name": "Serie A"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Allegri enraged after Napoli 3-3 Roma: ‘Maybe I shouldn’t be on the touchline’",
    "publisher": "Yahoo Sports"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Roma, the FOOTBALL team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Roma, the FOOTBALL team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Roma, the FOOTBALL team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Roma, the FOOTBALL team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.051607224545290466,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.7000113750529967,
      "type": "noul"
    },
    "movement": {
      "noul": 0.10478546937679838,
      "type": "noul"
    },
    "subject": {
      "noul": 0.8693126031262948,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2452,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](05/with/request.json)

## 06-miami-heat — context added

**Headline:** Giannis Antetokounmpo puts Miami Heat teammates on blast after heavy LA Lakers loss

**Context:** Giannis Antetokounmpo (player); team on record: Milwaukee Bucks, observed 2026-07-03

**Review:** Dated Milwaukee roster context conflicts with the headline’s Heat-teammate framing. Scores change modestly and routes do not. The test does not establish whether the model resolves that conflict correctly.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 90.11 | 88.83 | -1.28 |
| emotion | 40.59 | 43.05 | +2.46 |
| movement | 34.09 | 25.32 | -8.77 |
| availability | 1.87 | 1.95 | +0.08 |

### Without other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Miami Heat",
    "entity_type": "team",
    "sport": "NBA",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "Miami",
      "coach_on_record": {
        "name": "Anthony Carter",
        "observed_at": "2026-09-28"
      },
      "conference": "East",
      "country": "United States",
      "division": "Southeast"
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Giannis Antetokounmpo puts Miami Heat teammates on blast after heavy LA Lakers loss",
    "publisher": "NBA Analysis Network"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Miami Heat, the NBA team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Miami Heat, the NBA team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Miami Heat, the NBA team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Miami Heat, the NBA team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.01867369589621803,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.40588698660897754,
      "type": "noul"
    },
    "movement": {
      "noul": 0.34090997900926634,
      "type": "noul"
    },
    "subject": {
      "noul": 0.9010850396226084,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2336,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](06/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Miami Heat",
    "entity_type": "team",
    "sport": "NBA",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "Miami",
      "coach_on_record": {
        "name": "Anthony Carter",
        "observed_at": "2026-09-28"
      },
      "conference": "East",
      "country": "United States",
      "division": "Southeast"
    },
    "named_people": "Giannis Antetokounmpo (player); team on record: Milwaukee Bucks, observed 2026-07-03"
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Giannis Antetokounmpo puts Miami Heat teammates on blast after heavy LA Lakers loss",
    "publisher": "NBA Analysis Network"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Miami Heat, the NBA team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Miami Heat, the NBA team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Miami Heat, the NBA team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Miami Heat, the NBA team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.01950202149019337,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.4305363769664587,
      "type": "noul"
    },
    "movement": {
      "noul": 0.25323000517935024,
      "type": "noul"
    },
    "subject": {
      "noul": 0.8882917856975655,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2488,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](06/with/request.json)

## 07-tampa-bay-buccaneers — context added

**Headline:** Baker Mayfield, Buccaneers accomplish rare feat not seen since 1974

**Context:** Baker Mayfield (player); recorded affiliation: this target (Tampa Bay Buccaneers), observed 2026-10-23

**Review:** Baker’s dated Buccaneers affiliation produces no route change. Vague rare-feat framing still routes to Influencer.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 94.42 | 93.26 | -1.16 |
| emotion | 27.01 | 30.37 | +3.36 |
| movement | 18.58 | 20.97 | +2.39 |
| availability | 4.43 | 4.37 | -0.06 |

### Without other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Tampa Bay Buccaneers",
    "entity_type": "team",
    "sport": "NFL",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "Tampa Bay",
      "conference": "NFC",
      "country": "United States",
      "division": "SOUTH",
      "home_venue_on_record": "Raymond James Stadium"
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Baker Mayfield, Buccaneers accomplish rare feat not seen since 1974",
    "publisher": "Yahoo Sports"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Tampa Bay Buccaneers, the NFL team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Tampa Bay Buccaneers, the NFL team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Tampa Bay Buccaneers, the NFL team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Tampa Bay Buccaneers, the NFL team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.04431129015343121,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.2701188809486024,
      "type": "noul"
    },
    "movement": {
      "noul": 0.1858103819187256,
      "type": "noul"
    },
    "subject": {
      "noul": 0.9441975142802116,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2244,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](07/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Tampa Bay Buccaneers",
    "entity_type": "team",
    "sport": "NFL",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "Tampa Bay",
      "conference": "NFC",
      "country": "United States",
      "division": "SOUTH",
      "home_venue_on_record": "Raymond James Stadium"
    },
    "named_people": "Baker Mayfield (player); recorded affiliation: this target (Tampa Bay Buccaneers), observed 2026-10-23"
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Baker Mayfield, Buccaneers accomplish rare feat not seen since 1974",
    "publisher": "Yahoo Sports"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Tampa Bay Buccaneers, the NFL team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Tampa Bay Buccaneers, the NFL team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Tampa Bay Buccaneers, the NFL team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Tampa Bay Buccaneers, the NFL team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.0437046901940778,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.3036977193802879,
      "type": "noul"
    },
    "movement": {
      "noul": 0.2096884858000703,
      "type": "noul"
    },
    "subject": {
      "noul": 0.93256332728642,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2396,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](07/with/request.json)

## 08-chelsea — context added

**Headline:** Chelsea sweating over Josh Acheampong future as top clubs line up

**Context:** Josh Acheampong (player); recorded affiliation: this target (Chelsea), observed 2026-07-03

**Review:** Regression: dated Chelsea affiliation does not help the personnel-interest classification. Movement crosses below 80, dropping Insider.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 85.31 | 82.89 | -2.42 |
| emotion | 60.86 | 61.39 | +0.53 |
| movement | 83.39 | 73.16 | -10.23 |
| availability | 6.97 | 6.64 | -0.34 |

### Without other-person context

**Errors:** none.

**Routes:** influencer, insider, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Chelsea",
    "entity_type": "team",
    "sport": "FOOTBALL",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "London",
      "coach_on_record": {
        "name": "Xabi Alonso",
        "observed_at": "2026-09-30"
      },
      "country": "England",
      "league": {
        "country": "England",
        "name": "Premier League"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Chelsea sweating over Josh Acheampong future as top clubs line up",
    "publisher": "TribalFootball"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Chelsea, the FOOTBALL team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Chelsea, the FOOTBALL team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Chelsea, the FOOTBALL team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Chelsea, the FOOTBALL team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.06972140618237531,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.6086419932008875,
      "type": "noul"
    },
    "movement": {
      "noul": 0.8338520454883696,
      "type": "noul"
    },
    "subject": {
      "noul": 0.8530890124562688,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2356,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](08/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Chelsea",
    "entity_type": "team",
    "sport": "FOOTBALL",
    "team_category": "unknown",
    "recorded_attributes": {
      "city": "London",
      "coach_on_record": {
        "name": "Xabi Alonso",
        "observed_at": "2026-09-30"
      },
      "country": "England",
      "league": {
        "country": "England",
        "name": "Premier League"
      }
    },
    "named_people": "Josh Acheampong (player); recorded affiliation: this target (Chelsea), observed 2026-07-03"
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Chelsea sweating over Josh Acheampong future as top clubs line up",
    "publisher": "TribalFootball"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Chelsea, the FOOTBALL team (team category unknown) can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Chelsea, the FOOTBALL team (team category unknown), or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Chelsea, the FOOTBALL team (team category unknown)—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Chelsea, the FOOTBALL team (team category unknown)? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.06637005904647152,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.6139243973809133,
      "type": "noul"
    },
    "movement": {
      "noul": 0.7315991829360499,
      "type": "noul"
    },
    "subject": {
      "noul": 0.828907432605517,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2500,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](08/with/request.json)

## 09-luka-doncic — unchanged control

**Headline:** Raptors Coach Praised Luka Doncic For Destroying His Game Plan

**Context:** No other-person match. Identical prompt in both arms.

**Review:** No other named person resolved from this headline, which says Raptors Coach without naming the coach. Identical scores.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 89.42 | 89.42 | +0.00 |
| emotion | 83.70 | 83.70 | +0.00 |
| movement | 7.58 | 7.58 | +0.00 |
| availability | 1.65 | 1.65 | +0.00 |

### Without other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Luka Doncic",
    "entity_type": "player",
    "sport": "NBA",
    "recorded_attributes": {
      "date_of_birth": "1999-02-28",
      "nationality": "Slovenia",
      "position": "F-G",
      "recorded_facts": [
        {
          "attribute": "role",
          "recorded_at": "2026-09-05",
          "value": "player"
        }
      ],
      "team_on_record": {
        "name": "Los Angeles Lakers",
        "observed_on": "2026-07-04"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Raptors Coach Praised Luka Doncic For Destroying His Game Plan",
    "publisher": "Heavy.com"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Luka Doncic, the NBA player can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Luka Doncic, the NBA player, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Luka Doncic, the NBA player—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Luka Doncic, the NBA player? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.016512712973951103,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.8369885767383908,
      "type": "noul"
    },
    "movement": {
      "noul": 0.07583819246067146,
      "type": "noul"
    },
    "subject": {
      "noul": 0.8941728814816782,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2506,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](09/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Luka Doncic",
    "entity_type": "player",
    "sport": "NBA",
    "recorded_attributes": {
      "date_of_birth": "1999-02-28",
      "nationality": "Slovenia",
      "position": "F-G",
      "recorded_facts": [
        {
          "attribute": "role",
          "recorded_at": "2026-09-05",
          "value": "player"
        }
      ],
      "team_on_record": {
        "name": "Los Angeles Lakers",
        "observed_on": "2026-07-04"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Raptors Coach Praised Luka Doncic For Destroying His Game Plan",
    "publisher": "Heavy.com"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Luka Doncic, the NBA player can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Luka Doncic, the NBA player, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Luka Doncic, the NBA player—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Luka Doncic, the NBA player? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.016512712973951103,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.8369885767383908,
      "type": "noul"
    },
    "movement": {
      "noul": 0.07583819246067146,
      "type": "noul"
    },
    "subject": {
      "noul": 0.8941728814816782,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2506,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](09/with/request.json)

## 10-jayson-tatum — unchanged control

**Headline:** Nets still haunted by 2013 Celtics trade that led to Jayson Tatum

**Context:** No other-person match. Identical prompt in both arms.

**Review:** No other-person context found. Historical-trade admission remains blocked in both arms; high movement remains masked by admission.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 44.31 | 44.31 | +0.00 |
| emotion | 43.66 | 43.66 | +0.00 |
| movement | 81.44 | 81.44 | +0.00 |
| availability | 2.69 | 2.69 | +0.00 |

### Without other-person context

**Errors:** none.

**Routes:** none

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Jayson Tatum",
    "entity_type": "player",
    "sport": "NBA",
    "recorded_attributes": {
      "nationality": "USA",
      "position": "forward",
      "team_on_record": {
        "name": "Boston Celtics",
        "observed_on": "2026-07-04"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Nets still haunted by 2013 Celtics trade that led to Jayson Tatum",
    "publisher": "Yahoo Sports"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Jayson Tatum, the NBA player can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Jayson Tatum, the NBA player, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Jayson Tatum, the NBA player—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Jayson Tatum, the NBA player? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.026867784779071265,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.4365936547565422,
      "type": "noul"
    },
    "movement": {
      "noul": 0.8144364927681271,
      "type": "noul"
    },
    "subject": {
      "noul": 0.44314099752168046,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2222,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](10/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** none

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Jayson Tatum",
    "entity_type": "player",
    "sport": "NBA",
    "recorded_attributes": {
      "nationality": "USA",
      "position": "forward",
      "team_on_record": {
        "name": "Boston Celtics",
        "observed_on": "2026-07-04"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Nets still haunted by 2013 Celtics trade that led to Jayson Tatum",
    "publisher": "Yahoo Sports"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Jayson Tatum, the NBA player can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Jayson Tatum, the NBA player, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Jayson Tatum, the NBA player—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Jayson Tatum, the NBA player? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.026867784779071265,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.4365936547565422,
      "type": "noul"
    },
    "movement": {
      "noul": 0.8144364927681271,
      "type": "noul"
    },
    "subject": {
      "noul": 0.44314099752168046,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2222,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](10/with/request.json)

## 11-dak-prescott — unchanged control

**Headline:** ‘Shut Up,’ ‘Stop Taking Bullets for Him’ — Cowboys Fans Livid as Dak Prescott Shields Brian Schottenheimer After OT Loss vs. Cardinals

**Context:** No other-person match. Identical prompt in both arms.

**Review:** No Brian Schottenheimer match found. Identical scores; coach relationship context was not tested.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 96.05 | 96.05 | +0.00 |
| emotion | 70.92 | 70.92 | +0.00 |
| movement | 7.52 | 7.52 | +0.00 |
| availability | 1.60 | 1.60 | +0.00 |

### Without other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Dak Prescott",
    "entity_type": "player",
    "sport": "NFL",
    "recorded_attributes": {
      "position": "QB",
      "team_on_record": {
        "name": "Dallas Cowboys",
        "observed_on": "2026-10-23"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "‘Shut Up,’ ‘Stop Taking Bullets for Him’ — Cowboys Fans Livid as Dak Prescott Shields Brian Schottenheimer After OT Loss vs. Cardinals",
    "publisher": "Yahoo Sports"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Dak Prescott, the NFL player can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Dak Prescott, the NFL player, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Dak Prescott, the NFL player—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Dak Prescott, the NFL player? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.01597988568153348,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.7092132912792575,
      "type": "noul"
    },
    "movement": {
      "noul": 0.07515379617855142,
      "type": "noul"
    },
    "subject": {
      "noul": 0.9605148763782144,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2222,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](11/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Dak Prescott",
    "entity_type": "player",
    "sport": "NFL",
    "recorded_attributes": {
      "position": "QB",
      "team_on_record": {
        "name": "Dallas Cowboys",
        "observed_on": "2026-10-23"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "‘Shut Up,’ ‘Stop Taking Bullets for Him’ — Cowboys Fans Livid as Dak Prescott Shields Brian Schottenheimer After OT Loss vs. Cardinals",
    "publisher": "Yahoo Sports"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Dak Prescott, the NFL player can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Dak Prescott, the NFL player, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Dak Prescott, the NFL player—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Dak Prescott, the NFL player? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.01597988568153348,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.7092132912792575,
      "type": "noul"
    },
    "movement": {
      "noul": 0.07515379617855142,
      "type": "noul"
    },
    "subject": {
      "noul": 0.9605148763782144,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2222,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](11/with/request.json)

## 12-baker-mayfield — unchanged control

**Headline:** Falcons Match 1974 Infamy in Horrific No-Pass Baker Mayfield Loss

**Context:** No other-person match. Identical prompt in both arms.

**Review:** No other-person context found after excluding the target. Opposing teams are not covered by this lookup. Identical scores.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 84.57 | 84.57 | +0.00 |
| emotion | 62.53 | 62.53 | +0.00 |
| movement | 10.76 | 10.76 | +0.00 |
| availability | 11.71 | 11.71 | +0.00 |

### Without other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Baker Mayfield",
    "entity_type": "player",
    "sport": "NFL",
    "recorded_attributes": {
      "position": "QB",
      "team_on_record": {
        "name": "Tampa Bay Buccaneers",
        "observed_on": "2026-10-23"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Falcons Match 1974 Infamy in Horrific No-Pass Baker Mayfield Loss",
    "publisher": "Yahoo Sports"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Baker Mayfield, the NFL player can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Baker Mayfield, the NFL player, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Baker Mayfield, the NFL player—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Baker Mayfield, the NFL player? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.1171219019321696,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.6253179019687424,
      "type": "noul"
    },
    "movement": {
      "noul": 0.10758050018487716,
      "type": "noul"
    },
    "subject": {
      "noul": 0.8456949447814103,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2190,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](12/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Baker Mayfield",
    "entity_type": "player",
    "sport": "NFL",
    "recorded_attributes": {
      "position": "QB",
      "team_on_record": {
        "name": "Tampa Bay Buccaneers",
        "observed_on": "2026-10-23"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Falcons Match 1974 Infamy in Horrific No-Pass Baker Mayfield Loss",
    "publisher": "Yahoo Sports"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Baker Mayfield, the NFL player can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Baker Mayfield, the NFL player, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Baker Mayfield, the NFL player—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Baker Mayfield, the NFL player? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.1171219019321696,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.6253179019687424,
      "type": "noul"
    },
    "movement": {
      "noul": 0.10758050018487716,
      "type": "noul"
    },
    "subject": {
      "noul": 0.8456949447814103,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2190,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](12/with/request.json)

## 13-virgil-van-dijk — unchanged control

**Headline:** 'Very disappointed' - Virgil van Dijk's reaction to Liverpool's defeat by Arsenal

**Context:** No other-person match. Identical prompt in both arms.

**Review:** No other-person context found after excluding the target. Opposing teams are not covered by this lookup. Identical scores.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 95.63 | 95.63 | +0.00 |
| emotion | 92.85 | 92.85 | +0.00 |
| movement | 2.47 | 2.47 | +0.00 |
| availability | 0.79 | 0.79 | +0.00 |

### Without other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Virgil van Dijk",
    "entity_type": "player",
    "sport": "FOOTBALL",
    "recorded_attributes": {
      "date_of_birth": "1991-07-08",
      "nationality": "Netherlands",
      "position": "Defender",
      "team_on_record": {
        "league": "Premier League",
        "name": "Liverpool",
        "observed_on": "2026-07-04"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "'Very disappointed' - Virgil van Dijk's reaction to Liverpool's defeat by Arsenal",
    "publisher": "Liverpool FC"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Virgil van Dijk, the FOOTBALL player can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Virgil van Dijk, the FOOTBALL player, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Virgil van Dijk, the FOOTBALL player—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Virgil van Dijk, the FOOTBALL player? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.007909162062102744,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.9284872970823536,
      "type": "noul"
    },
    "movement": {
      "noul": 0.02468988027648883,
      "type": "noul"
    },
    "subject": {
      "noul": 0.9563392525555996,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2350,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](13/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Virgil van Dijk",
    "entity_type": "player",
    "sport": "FOOTBALL",
    "recorded_attributes": {
      "date_of_birth": "1991-07-08",
      "nationality": "Netherlands",
      "position": "Defender",
      "team_on_record": {
        "league": "Premier League",
        "name": "Liverpool",
        "observed_on": "2026-07-04"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "'Very disappointed' - Virgil van Dijk's reaction to Liverpool's defeat by Arsenal",
    "publisher": "Liverpool FC"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Virgil van Dijk, the FOOTBALL player can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Virgil van Dijk, the FOOTBALL player, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Virgil van Dijk, the FOOTBALL player—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Virgil van Dijk, the FOOTBALL player? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.007909162062102744,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.9284872970823536,
      "type": "noul"
    },
    "movement": {
      "noul": 0.02468988027648883,
      "type": "noul"
    },
    "subject": {
      "noul": 0.9563392525555996,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2350,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](13/with/request.json)

## 14-gessime-yassine — unchanged control

**Headline:** Gessime Yassine dominates in Strasbourg's lopsided Ligue 1 victory

**Context:** No other-person match. Identical prompt in both arms.

**Review:** No other-person context found after excluding the target. Identical scores.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 92.41 | 92.41 | +0.00 |
| emotion | 72.21 | 72.21 | +0.00 |
| movement | 0.61 | 0.61 | +0.00 |
| availability | 1.21 | 1.21 | +0.00 |

### Without other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Gessime Yassine",
    "entity_type": "player",
    "sport": "FOOTBALL",
    "recorded_attributes": {
      "nationality": "Morocco",
      "position": "Attacker",
      "team_on_record": {
        "league": "Ligue 1",
        "name": "Strasbourg",
        "observed_on": "2026-07-04"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Gessime Yassine dominates in Strasbourg's lopsided Ligue 1 victory",
    "publisher": "Pan Africa Football"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Gessime Yassine, the FOOTBALL player can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Gessime Yassine, the FOOTBALL player, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Gessime Yassine, the FOOTBALL player—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Gessime Yassine, the FOOTBALL player? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.012136143294349594,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.7220864828285091,
      "type": "noul"
    },
    "movement": {
      "noul": 0.006144399018890058,
      "type": "noul"
    },
    "subject": {
      "noul": 0.9240785498889144,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2294,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](14/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Gessime Yassine",
    "entity_type": "player",
    "sport": "FOOTBALL",
    "recorded_attributes": {
      "nationality": "Morocco",
      "position": "Attacker",
      "team_on_record": {
        "league": "Ligue 1",
        "name": "Strasbourg",
        "observed_on": "2026-07-04"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Gessime Yassine dominates in Strasbourg's lopsided Ligue 1 victory",
    "publisher": "Pan Africa Football"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Gessime Yassine, the FOOTBALL player can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Gessime Yassine, the FOOTBALL player, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Gessime Yassine, the FOOTBALL player—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Gessime Yassine, the FOOTBALL player? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.012136143294349594,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.7220864828285091,
      "type": "noul"
    },
    "movement": {
      "noul": 0.006144399018890058,
      "type": "noul"
    },
    "subject": {
      "noul": 0.9240785498889144,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2294,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](14/with/request.json)

## 15-vit-krejci — unchanged control

**Headline:** Vit Krejci News: Season-high 24 minutes Saturday

**Context:** No other-person match. Identical prompt in both arms.

**Review:** No other-person context found after excluding Krejci himself. Emotion remains 21.35, above 15, so the false Influencer route persists; this pair does not test added context.

| Score | Without context | With context | Change |
|---|---:|---:|---:|
| subject | 95.40 | 95.40 | +0.00 |
| emotion | 21.35 | 21.35 | +0.00 |
| movement | 1.07 | 1.07 | +0.00 |
| availability | 2.75 | 2.75 | +0.00 |

### Without other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Vit Krejci",
    "entity_type": "player",
    "sport": "NBA",
    "recorded_attributes": {
      "date_of_birth": "2000-06-19",
      "nationality": "Czech Republic",
      "position": "guard",
      "team_on_record": {
        "name": "Portland Trail Blazers",
        "observed_on": "2026-07-04"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Vit Krejci News: Season-high 24 minutes Saturday",
    "publisher": "RotoWire"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Vit Krejci, the NBA player can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Vit Krejci, the NBA player, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Vit Krejci, the NBA player—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Vit Krejci, the NBA player? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.027457228629088795,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.21351486945062068,
      "type": "noul"
    },
    "movement": {
      "noul": 0.010745639957164342,
      "type": "noul"
    },
    "subject": {
      "noul": 0.954039667332583,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2290,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](15/without/request.json)


### With other-person context

**Errors:** none.

**Routes:** influencer, journalist

Complete brief:

```json
{
  "task": {
    "assignment": "You will be given an entity and a headline found mentioning it. Your job is to look for a concrete development, emotion, personnel movement and participation issue involving that entity in the evidence.",
    "rules": [
      "Identify who is speaking, who is acting and who is affected. The target may be any of these, but a name appearing in the headline does not establish relevance.",
      "Keep each claim with the entity it concerns. Another person's feelings, move or injury do not automatically belong to the target.",
      "Use recorded attributes to understand identity and relationships, not as evidence of a new event. Preserve uncertainty and do not guess missing relationships or details."
    ]
  },
  "target_context": "The target below is a sports-related entity, such as a team, player, coach, owner or agent, associated with European football, NFL football or NBA basketball. Its name, entity type and sport identify who the reading concerns. Recorded attributes provide additional background about its role, affiliations and relationships.",
  "target": {
    "name": "Vit Krejci",
    "entity_type": "player",
    "sport": "NBA",
    "recorded_attributes": {
      "date_of_birth": "2000-06-19",
      "nationality": "Czech Republic",
      "position": "guard",
      "team_on_record": {
        "name": "Portland Trail Blazers",
        "observed_on": "2026-07-04"
      }
    }
  },
  "evidence_context": "This is a headline collected from a publisher's news feed. The full article is not supplied. The publisher identifies the outlet, not necessarily the speaker or subject.",
  "evidence": {
    "headline": "Vit Krejci News: Season-high 24 minutes Saturday",
    "publisher": "RotoWire"
  }
}
```

All scoring questions:

```json
{
  "availability": {
    "instructions": "Does the headline report whether Vit Krejci, the NBA player can participate because of injury, illness, fitness, suspension, eligibility, rest or return? For a team, include its own personnel. Uncertainty and denials can qualify; performance, playing time, tactical choices and transfers alone do not.",
    "type": "noul"
  },
  "emotion": {
    "instructions": "Does the headline express a feeling or judgment about Vit Krejci, the NBA player, or the target’s feelings about its own situation? Keep the emotion with its speaker and subject. A result, statistic or dramatic wording alone is insufficient.",
    "type": "noul"
  },
  "movement": {
    "instructions": "Does the headline report a personnel or contract development involving Vit Krejci, the NBA player—a transfer, trade, loan, signing, departure, contract discussion or coaching change? Include reported rumours and denials, and a team’s own personnel moves. Exclude historical moves and hypothetical trade advice.",
    "type": "noul"
  },
  "subject": {
    "instructions": "Does the headline state a concrete development directly involving Vit Krejci, the NBA player? Results, attributed opinions, reported rumours and denials can qualify. Incidental mentions, historical comparisons, listings, predictions and teasers that withhold the development do not.",
    "type": "noul"
  }
}
```

Complete native response:

```json
{
  "answers": {
    "availability": {
      "noul": 0.027457228629088795,
      "type": "noul"
    },
    "emotion": {
      "noul": 0.21351486945062068,
      "type": "noul"
    },
    "movement": {
      "noul": 0.010745639957164342,
      "type": "noul"
    },
    "subject": {
      "noul": 0.954039667332583,
      "type": "noul"
    }
  },
  "model": "vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707",
  "usage": {
    "input_tokens": 2290,
    "output_tokens": 0
  }
}
```

[Exact native request JSON](15/with/request.json)

