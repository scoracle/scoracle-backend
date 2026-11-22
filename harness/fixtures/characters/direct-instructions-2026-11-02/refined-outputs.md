# Journalist controlled comparison

Identical saved evidence; one generation per case in each batch. Parser acceptance is not factual correctness.

## 1. chelsea-live-live-journalist-preview

Initial trial collapsed all five headlines into one roundup, made Manu Kone the record signing, and mangled the boss attribution. The refinement separates the promotion and contract stories, correctly attributes the contract quote to the boss, and omits the ambiguous Kone item. It still repeats the first claim and bundles injury updates with the separate right-back search. The earlier baseline also misidentified the contract speaker and Kone link. All three pass the mechanical parser.

baseline: 1224 input tokens; accepted.

```json
{
  "narratives": [
    {
      "body": "Chelsea FC Latest News .com reports that the club promoted Penders and Sharman-Lowe following Emi Martinez's injury.",
      "headline": "Chelsea promote Penders and Sharman-Lowe after Martinez injury",
      "report_keys": [
        "report_1"
      ]
    },
    {
      "body": "FootballTransfers.com headlines suggest Manu Kone is a record signing target for Chelsea.",
      "headline": "Chelsea eye record signing Manu Kone",
      "report_keys": [
        "report_2"
      ]
    },
    {
      "body": "Talk Chelsea reports that Chelsea's £100m target claims his boss has not given up on signing a new contract.",
      "headline": "Chelsea's £100m target claims new contract possible",
      "report_keys": [
        "report_3"
      ]
    }
  ]
}
```

initial: 1114 input tokens; accepted.

```json
{
  "narratives": [
    {
      "body": "Chelsea FC Latest News .com reports that the club promoted Penders and Sharman-Lowe following Emi Martinez's injury. FootballTransfers.com notes Manu Kone is a record signing target. Talk Chelsea states a £100m target's boss claims his boss has not given up on securing a new contract. London Evening Standard and Flashscore.com provide injury updates and search for new right-back options amid Gusto uncertainty.",
      "headline": "Chelsea injury updates and transfer news",
      "report_ids": [
        1,
        2,
        3,
        4,
        5
      ]
    }
  ]
}
```

refined: 1174 input tokens; accepted.

```json
{
  "narratives": [
    {
      "body": "Chelsea FC Latest News .com reports that the club promoted Penders and Sharman-Lowe following Emi Martinez's injury. The headline states the promotion occurred after Martinez's injury.",
      "headline": "Chelsea promote Penders and Sharman-Lowe after Martinez injury",
      "report_ids": [
        1
      ]
    },
    {
      "body": "Talk Chelsea notes that Chelsea's £100m target could yet sign a new contract, claims his boss. The headline quotes the boss saying, 'We haven't given up'.",
      "headline": "Chelsea's £100m target could sign new contract claims boss",
      "report_ids": [
        3
      ]
    },
    {
      "body": "London Evening Standard provides an injury update listing Martinez, Chavarria, and James with latest return dates. Flashscore.com adds that Chelsea are searching for new right-back options amid Gusto uncertainty.",
      "headline": "Chelsea injury updates and right-back search",
      "report_ids": [
        4,
        5
      ]
    }
  ]
}
```

## 2. dak-prescott-live-journalist-preview

Initial trial retained the merged ESPN rest-of-season and NFL.com Week 7 rankings, with added interpretation about a shift in standing and a difficult period. The refinement chooses other stories and avoids that conflation; it preserves MARCA's actors, quotation and 0-2 record. It adds “of the season” to the fourth-loss headline without explicit support. The current named-claim guard rejects the capitalized headline “Dak Prescott Reveals What Sucks Most”; this rejection is not itself evidence that the recap's names are wrong. All three versions are rejected, for different headline strings. The parser was not loosened.

baseline: 1188 input tokens; rejected.

```json
{
  "narratives": [
    {
      "body": "Athlon Sports reports that Cowboys quarterback Dak Prescott announced personal news following the team's fourth loss.",
      "headline": "Cowboys QB Dak Prescott Announces Personal News After 4th Loss",
      "report_keys": [
        "report_1"
      ]
    },
    {
      "body": "Multiple outlets, including ESPN and NFL.com, note that Dak Prescott's season rankings have hit a low point while other players have risen.",
      "headline": "Dak Prescott's Season Rankings Hit a Low Point",
      "report_keys": [
        "report_2",
        "report_3"
      ]
    },
    {
      "body": "MARCA reports that Dak Prescott faces backlash after the Cowboys posted a 0-2 record since he told C.J. Stroud to 'go get some wins'.",
      "headline": "Dak Prescott Faces Backlash After Cowboys 0-2 Record",
      "report_keys": [
        "report_4"
      ]
    }
  ]
}
```

initial: 1078 input tokens; rejected.

```json
{
  "narratives": [
    {
      "body": "Athlon Sports reports that Cowboys quarterback Dak Prescott announced personal news following the team's fourth loss. The headline indicates the announcement came after a difficult period for the franchise.",
      "headline": "Dak Prescott Announces Personal News After 4th Loss",
      "report_ids": [
        1
      ]
    },
    {
      "body": "MARCA states that Dak Prescott faces backlash after the Cowboys posted a 0-2 record since he reportedly told C.J. Stroud to 'go get some wins'. The article links the team's recent struggles to this interaction.",
      "headline": "Dak Prescott Faces Backlash After Cowboys 0-2 Record",
      "report_ids": [
        4
      ]
    },
    {
      "body": "ESPN and NFL.com note that Dak Prescott's rest-of-season rankings have declined, with headlines indicating he has hit a season low while other players rise. The reports suggest a shift in his standing relative to peers.",
      "headline": "Dak Prescott Rankings Decline Amid Season Low",
      "report_ids": [
        2,
        3
      ]
    }
  ]
}
```

refined: 1138 input tokens; rejected.

```json
{
  "narratives": [
    {
      "body": "Athlon Sports reports that Cowboys quarterback Dak Prescott announced personal news following the team's fourth loss of the season.",
      "headline": "Dak Prescott Announces Personal News After 4th Loss",
      "report_ids": [
        1
      ]
    },
    {
      "body": "MARCA states that Dak Prescott faces backlash after the Cowboys posted a 0-2 record since he told C.J. Stroud to 'go get some wins'.",
      "headline": "Dak Prescott Faces Backlash After Cowboys 0-2 Record",
      "report_ids": [
        4
      ]
    },
    {
      "body": "New York Post indicates Dak Prescott revealed what 'sucks' the most after another nightmare Cowboys game.",
      "headline": "Dak Prescott Reveals What 'Sucks' Most After Nightmare Game",
      "report_ids": [
        5
      ]
    }
  ]
}
```

## 3. chelsea-target-preview

Initial trial mixed transfer advice with the unnamed player, mixed charges with a match listing, and introduced coach background into news. The refinement removes the coach leak but still invents Chelsea as Estevao's adviser and CaughtOffside as the speaker urging departure. It newly joins transfer advice with the separate charges story and labels fixture listings as match results without a supplied result. Baseline already had actor errors but separated these developments. Final parser acceptance does not establish fidelity.

baseline: 1213 input tokens; accepted.

```json
{
  "narratives": [
    {
      "body": "Football London reported that Estevao received brutally honest transfer advice from Chelsea despite a promise from Xabi Alonso.",
      "headline": "Estevao urged to leave Chelsea despite Alonso promise",
      "report_keys": [
        "report_3"
      ]
    },
    {
      "body": "CaughtOffside urged a Chelsea player with 23 minutes of playing time this season to leave the club.",
      "headline": "Chelsea player urged to leave after limited playing time",
      "report_keys": [
        "report_2"
      ]
    },
    {
      "body": "Football London stated Chelsea were labelled 'bigger and worse offenders' than Man City despite 114 charges guilty verdict.",
      "headline": "Chelsea labelled worse offenders than Man City despite charges",
      "report_keys": [
        "report_5"
      ]
    }
  ]
}
```

initial: 1103 input tokens; rejected.

```json
{
  "narratives": [
    {
      "body": "Football London reported that Estevao received brutally honest transfer advice from Chelsea despite Xabi Alonso's promise, while CaughtOffside urged a player with only 23 minutes of playing time this season to leave the club. The articles do not specify which player received the advice or confirm the current status of the player urged to leave.",
      "headline": "Chelsea players urged to leave amid transfer advice and playing time concerns",
      "report_ids": [
        2,
        3
      ]
    },
    {
      "body": "Football London stated that Chelsea was labelled 'bigger and worse offenders' than Man City despite 114 charges guilty verdict. FOX Sports provided the final score for the Chelsea vs. Aston Villa match on December 21, 2026. The articles do not detail the specific charges or the match result.",
      "headline": "Chelsea faces scrutiny over charges despite match report",
      "report_ids": [
        4,
        5
      ]
    },
    {
      "body": "Flashscore.com listed a match between Bournemouth and Chelsea. The target's record attributes indicate Xabi Alonso as the coach observed at 2026-09-30, but the articles do not confirm his current role or the match outcome.",
      "headline": "Chelsea scheduled for Bournemouth match with Xabi Alonso listed as coach",
      "report_ids": [
        1
      ]
    }
  ]
}
```

refined: 1163 input tokens; accepted.

```json
{
  "narratives": [
    {
      "body": "Football London reported that Estevao received brutally honest transfer advice from Chelsea despite Xabi Alonso's promise. The club is also being labelled 'bigger and worse offenders' than Man City despite 114 charges guilty verdict.",
      "headline": "Estevao urged to leave Chelsea amid transfer advice and club criticism",
      "report_ids": [
        3,
        5
      ]
    },
    {
      "body": "CaughtOffside urged a Chelsea player with 23 minutes of playing time this season to leave the club.",
      "headline": "Chelsea player urged to leave after limited playing time this season",
      "report_ids": [
        2
      ]
    },
    {
      "body": "Flashscore.com and FOX Sports provided match data for Bournemouth - Chelsea and Chelsea vs. Aston Villa - Final Score - December 21, 2026.",
      "headline": "Chelsea match results: Bournemouth and Aston Villa fixtures",
      "report_ids": [
        1,
        4
      ]
    }
  ]
}
```

## 4. narratives-person-86-FOOTBALL

All three retain the supported Diomande story and omit the separate Mastantuono story. Initial trial repeats praise across three sentences and attributes the Bernabeu detail to Yahoo/Yardbarker, although that venue appears in OneFootball. The refinement provides a single concise sentence with all cited publishers. This improves attribution and removes padding without claiming full story coverage.

baseline: 1322 input tokens; accepted.

```json
{
  "narratives": [
    {
      "body": "Martin Demichelis, RB Leipzig's coach, praised Yan Diomande ahead of the Champions League clash against Real Madrid at the Bernabeu.",
      "headline": "Demichelis praises Diomande ahead of Real Madrid clash",
      "report_keys": [
        "report_1",
        "report_2",
        "report_3"
      ]
    }
  ]
}
```

initial: 1215 input tokens; accepted.

```json
{
  "narratives": [
    {
      "body": "Martin Demichelis, RB Leipzig coach, praised Yan Diomande ahead of the Real Madrid clash. Yahoo and Yardbarker reported that Demichelis raved about the player before the Champions League tie at the Bernabeu. OneFootball also noted Demichelis hailing Diomande before the match.",
      "headline": "Demichelis praises Yan Diomande ahead of Real Madrid clash",
      "report_ids": [
        1,
        2,
        3
      ]
    }
  ]
}
```

refined: 1275 input tokens; accepted.

```json
{
  "narratives": [
    {
      "body": "Yahoo, Yardbarker, and OneFootball report that RB Leipzig boss Martín Demichelis praised Yan Diomande ahead of the Champions League clash against Real Madrid at the Bernabéu.",
      "headline": "Demichelis praises Yan Diomande ahead of Real Madrid clash",
      "report_ids": [
        1,
        2,
        3
      ]
    }
  ]
}
```

## 5. narratives-player-37-NBA

The baseline retained the meaning that Chris Paul's jersey number was going to Beal, though its headline was too definitive. Both new trials change that into a jersey being retired for Beal, which the source does not say. The refinement also repeats the injury claim as confirmation. All versions expand Clippers to Los Angeles Clippers and fail the same existing guard. The rejection and raw erroneous text are retained.

baseline: 1069 input tokens; rejected.

```json
{
  "narratives": [
    {
      "body": "NBC Sports reports that Bradley Beal is ruled out for the game against Sacramento due to a knee injury and remains unavailable for the match against the Lakers.",
      "headline": "Bradley Beal ruled out for Sacramento and Lakers games",
      "report_keys": [
        "report_1",
        "report_2"
      ]
    },
    {
      "body": "ABC30 Fresno reports that the Los Angeles Clippers have indicated the jersey number previously worn by Chris Paul will be assigned to Bradley Beal.",
      "headline": "Clippers assign Chris Paul's old No. 3 to Bradley Beal",
      "report_keys": [
        "report_3"
      ]
    }
  ]
}
```

initial: 965 input tokens; rejected.

```json
{
  "narratives": [
    {
      "body": "NBC Sports reports Bradley Beal is ruled out against Sacramento due to a knee injury and remains unavailable for the Lakers game. The outlet states Beal (knee) is ruled out vs. Sacramento and remains out vs. Lakers.",
      "headline": "Bradley Beal (knee) ruled out vs. Sacramento",
      "report_ids": [
        1,
        2
      ]
    },
    {
      "body": "ABC30 Fresno reports the Los Angeles Clippers are indicating that Chris Paul's old No. 3 jersey will be retired for Bradley Beal. The article states the Clippers indicate Chris Paul's old No. 3 going to Bradley Beal.",
      "headline": "Clippers indicate Chris Paul's old No. 3 going to Bradley Beal",
      "report_ids": [
        3
      ]
    }
  ]
}
```

refined: 1025 input tokens; rejected.

```json
{
  "narratives": [
    {
      "body": "NBC Sports reports Bradley Beal is ruled out against Sacramento due to a knee injury and remains unavailable for the Lakers game. The player's absence is confirmed by NBC Sports.",
      "headline": "Bradley Beal ruled out vs. Sacramento and Lakers due to knee injury",
      "report_ids": [
        1,
        2
      ]
    },
    {
      "body": "ABC30 Fresno reports the Los Angeles Clippers are indicating that Chris Paul's old No. 3 jersey will be retired for Bradley Beal.",
      "headline": "Clippers indicate Chris Paul's old No. 3 going to Bradley Beal",
      "report_ids": [
        3
      ]
    }
  ]
}
```
