# Approved recap and plain-brief comparison

Baseline: clear-payload trial. Final: one Journalist hook after sourced paragraphs, plus plain Insider and Influencer tasks. Identical saved source parts, one generation per case; parser acceptance is not factual correctness.

## 1. chelsea-live-live-journalist-preview

Input: 1224 → 1182; validation: accepted → accepted.

The single-hook structure works, and the contract claim is now attributed to the boss. The model puts five separate reports into one five-sentence paragraph despite the one-development instruction. It again identifies Manu Kone as the record signing, which the supplied headline does not establish. The baseline misidentified the boss claim and the Kone link too. Both pass the parser.

Before:
```json
{"narratives":[{"body":"Chelsea FC Latest News .com reports that the club promoted Penders and Sharman-Lowe following Emi Martinez's injury.","headline":"Chelsea promote Penders and Sharman-Lowe after Martinez injury","report_keys":["report_1"]},{"body":"FootballTransfers.com headlines suggest Manu Kone is a record signing target for Chelsea.","headline":"Chelsea eye record signing Manu Kone","report_keys":["report_2"]},{"body":"Talk Chelsea reports that Chelsea's £100m target claims his boss has not given up on signing a new contract.","headline":"Chelsea's £100m target claims new contract possible","report_keys":["report_3"]}]
}
```

After:
```json
{"paragraphs":[{"body":"Chelsea FC Latest News .com reports that the club has promoted Penders and Sharman-Lowe following an injury to Emi Martinez. FootballTransfers.com notes that Manu Kone is being viewed as a record signing. Talk Chelsea states that a £100m target could yet sign a new contract, a claim made by his boss. London Evening Standard provides an injury update regarding Martinez, Chavarria, and James. Flashscore.com indicates Chelsea are searching for new right-back options amid Gusto's uncertainty.", "report_ids":[1,2,3,4,5]}],"headline":"Chelsea moves on injuries, transfer targets, and squad searches"}
```

## 2. dak-prescott-live-journalist-preview

Input: 1188 → 1146; validation: rejected → accepted.

The model chooses personal news and the New York Post quote topic, avoiding the incompatible ranking windows found in the baseline. It does not invent the substance of the personal announcement. It combines two separate stories in one paragraph and paraphrases “sucks” as frustration. New acceptance versus baseline rejection is not a held-out quality result; coverage is narrower.

Before:
```json
{"narratives":[{"body":"Athlon Sports reports that Cowboys quarterback Dak Prescott announced personal news following the team's fourth loss.","headline":"Cowboys QB Dak Prescott Announces Personal News After 4th Loss","report_keys":["report_1"]},{"body":"Multiple outlets, including ESPN and NFL.com, note that Dak Prescott's season rankings have hit a low point while other players have risen.","headline":"Dak Prescott's Season Rankings Hit a Low Point","report_keys":["report_2","report_3"]},{"body":"MARCA reports that Dak Prescott faces backlash after the Cowboys posted a 0-2 record since he told C.J. Stroud to 'go get some wins'.","headline":"Dak Prescott Faces Backlash After Cowboys 0-2 Record","report_keys":["report_4"]}]
}
```

After:
```json
{"paragraphs":[{"body":"Dak Prescott announced personal news following the team's fourth loss, a development reported by Athlon Sports. Additionally, Prescott revealed what he finds most frustrating after another difficult game for the Cowboys, according to the New York Post.", "report_ids":[1, 5]}],"headline":"Dak Prescott addresses personal news and frustration amid Cowboys' struggles"}
```

## 3. chelsea-target-preview

Input: 1213 → 1171; validation: accepted → rejected.

The model creates five separately cited paragraphs and writes one hook last. It invents Chelsea as Estevao's adviser, adds coach/assurances, changes 114 charges guilty verdict into 114 guilty verdicts, and expands Man City to Manchester City. The existing guard rejects the name expansion. The hook adds player departures and conflicting transfer advice that are not established. The baseline had actor errors but preserved the numerical wording.

Before:
```json
{"narratives":[{"body":"Football London reported that Estevao received brutally honest transfer advice from Chelsea despite a promise from Xabi Alonso.","headline":"Estevao urged to leave Chelsea despite Alonso promise","report_keys":["report_3"]},{"body":"CaughtOffside urged a Chelsea player with 23 minutes of playing time this season to leave the club.","headline":"Chelsea player urged to leave after limited playing time","report_keys":["report_2"]},{"body":"Football London stated Chelsea were labelled 'bigger and worse offenders' than Man City despite 114 charges guilty verdict.","headline":"Chelsea labelled worse offenders than Man City despite charges","report_keys":["report_5"]}]
}
```

After:
```json
{"paragraphs":[{"body":"Chelsea faced Aston Villa in a match concluded on December 21, 2026, as reported by FOX Sports.","report_ids":[4]},{"body":"Estevao received blunt transfer advice from Chelsea despite assurances from coach Xabi Alonso, according to Football London.","report_ids":[3]},{"body":"Chelsea was found to be 'bigger and worse offenders' than Manchester City following 114 guilty verdicts, per Football London.","report_ids":[5]},{"body":"A Chelsea player with only 23 minutes of playing time this season has been urged to leave the club, as noted by CaughtOffside.","report_ids":[2]},{"body":"A match between Bournemouth and Chelsea was covered by Flashscore.com.","report_ids":[1]}],"headline":"Chelsea faces scrutiny over conduct, player departures, and match results amid conflicting transfer advice."}
```

## 4. narratives-person-86-FOOTBALL

Input: 1322 → 1283; validation: accepted → accepted.

The concise baseline kept the supported Diomande praise. The recap adds French defender and exceptional without supplied support, and omits the separate Mastantuono story. It is mechanically accepted, illustrating a factual gap in validation. The native output still has paragraphs before the single hook.

Before:
```json
{"narratives":[{"body":"Martin Demichelis, RB Leipzig's coach, praised Yan Diomande ahead of the Champions League clash against Real Madrid at the Bernabeu.", "headline":"Demichelis praises Diomande ahead of Real Madrid clash", "report_keys":["report_1", "report_2", "report_3"]}]
}
```

After:
```json
{"paragraphs":[{"body":"RB Leipzig boss Martín Demichelis praised Yan Diomande ahead of the Champions League clash against Real Madrid at the Bernabéu, describing the French defender as exceptional despite the daunting nature of the upcoming fixture. The comments were reported by Yahoo, Yardbarker, and OneFootball, who all highlighted Demichelis's high regard for Diomande in the context of the European tie.","report_ids":[1,2,3]}],"headline":"Demichelis hails Diomande ahead of Real Madrid clash"}
```

## 5. narratives-player-37-NBA

Input: 1069 → 1033; validation: rejected → rejected.

The model merges injuries and the unrelated jersey-number story, expands Sacramento and Clippers names, and changes a number going to Beal into a retirement in his honor. The hook repeats the invented retirement. Baseline had a too-definitive assignment headline but did not invent retirement. The guard rejects Sacramento Kings, leaving the semantic defect visible in the saved raw reply.

Before:
```json
{"narratives":[{"body":"NBC Sports reports that Bradley Beal is ruled out for the game against Sacramento due to a knee injury and remains unavailable for the match against the Lakers.","headline":"Bradley Beal ruled out for Sacramento and Lakers games","report_keys":["report_1","report_2"]},{"body":"ABC30 Fresno reports that the Los Angeles Clippers have indicated the jersey number previously worn by Chris Paul will be assigned to Bradley Beal.","headline":"Clippers assign Chris Paul's old No. 3 to Bradley Beal","report_keys":["report_3"]}]
}
```

After:
```json
{"paragraphs":[{"body":"Bradley Beal is ruled out to play against the Sacramento Kings due to a knee injury, according to NBC Sports. He remains unavailable for the upcoming game against the Lakers as well, with NBC Sports confirming his continued absence. Additionally, the Los Angeles Clippers have indicated that Chris Paul's old jersey number will be retired in Beal's honor, as reported by ABC30 Fresno.", "report_ids":[1, 2, 3]}],"headline":"Bradley Beal ruled out for two games as Clippers honor legacy with jersey retirement"}
```

## 6. transfer-player-83946-NFL

Input: 1556 → 1595; validation: rejected → rejected.

The unsupported acquisition-from-Raiders and unconfirmed contract details from the baseline disappear. The main signing/practice-squad-after-release story is supported, but full Eagles and Raiders names are expanded beyond headline wording, and prose repeats the signing. The existing alias/name guard still rejects Philadelphia Eagles. Findings remain empty under the unchanged exact-quote contract.

Before:
```json
{
  "body": "Evan Neal has been confirmed by the Philadelphia Eagles as a signing, with reports indicating he joined the practice squad after being released by the Las Vegas Raiders. Multiple outlets describe the move as the Eagles acquiring the offensive tackle from the Raiders, though specific contract details remain unconfirmed.",
  "findings": [
    																]
  }
```

After:
```json
{
  "body": "Evan Neal has been confirmed by the Philadelphia Eagles as a signing, with reports indicating he joined the practice squad after being released by the Las Vegas Raiders. Multiple outlets, including Yahoo Sports and NBC Sports, have reported the Eagles' confirmation of his signing to their roster.",
  "findings": [
    																]
  }
```

## 7. transfer-team-37-FOOTBALL

Input: 1451 → 1490; validation: rejected → rejected.

The model calls Pellegrini the head coach and attributes LiveScore's 6-8 week injury duration to La Gazzetta dello Sport. The existing source/number guard catches that attribution error. It emphasizes injuries rather than movements and omits the rejected offers and retention package. The baseline also failed, after leaking different coach metadata and assigning the full package price to one player.

Before:
```json
{
  "body": "Roma is a football team in Serie A with Mirko Vučinić recorded as coach. Reports indicate Roma is looking to make two signings in January and rejected three offers for Pisilli this summer, while also facing a crisis with Pellegrini injured for 6-8 weeks. A separate report suggests Leipzig might retain Roma's El Aynaoui for €50m, though no other movement stories are supported.",
  "findings": []
}
```

After:
```json
{
  "body": "Roma is facing a significant crisis with head coach Pellegrini out for 6-8 weeks due to a serious injury, a situation La Gazzetta dello Sport describes as decimating the squad ahead of a match against Napoli. Additionally, reports indicate Roma is looking to make two signings in January, though no specific players or clubs were named in the available headlines.",
  "findings": []
}
```

## 8. vibe-person-23-FOOTBALL

Input: 1435 → 1474; validation: accepted → accepted.

The model keeps a positive score of 75 and two sentences. It no longer moves “great news” from Kulusevski's return to the match result. However, it treats hopes for attitude/mentality as praise, presents publisher wording as direct quoted praise, and pools outlets that do not all supply the happy/delighted description. Mechanically accepted in both versions.

Before:
```json
{
  "score": 75,
  "headline": "De Zerbi expresses joy over Spurs' resilience and style",
  "body": "Roberto De Zerbi is portrayed as delighted and happy following Tottenham's victory over Coventry. Multiple sources report that he praised the team's attitude, mentality, and resilience, describing the result as 'great news' and noting how Spurs turned on their style."
}
```

After:
```json
{
  "score": 75,
  "headline": "De Zerbi expresses delight over Spurs' resilience and style after Coventry victory.",
  "body": "Roberto De Zerbi is described as 'very happy' and 'delighted' by multiple outlets, including Flashscore.co.za, VAVEL.com, Yellow Advertiser, Vodacom Soccer, and TribalFootball. He praised Tottenham's 'attitude,' 'mentality,' and ability to 'turn on the style' following their crucial win against Coventry."
}
```

## 9. vibe-person-959-NFL

Input: 1210 → 1249; validation: accepted → accepted.

The baseline abstained despite explicit disappointment and criticism. The new text reports both sources, but calls two negative reactions mixed (50), pluralizes one ex-player in its hook, adds criticism of leadership, and uses three sentences. The unchanged parser accepts this; the semantic/calibration and length problems remain.

Before:
```json
{"score":null,"headline":null,"body":null}
```

After:
```json
{"score":50,"headline":"Jerry Jones faces criticism from ex-players and expresses disappointment","body":"The New York Times reports Jerry Jones is 'disappointed' and wants to improve the Cowboys roster before the trade deadline. Yahoo Sports notes an ex-Cowboys star took a swipe at Jones amid team struggles. These sources indicate mixed reactions: Jones feels let down, while a former player is critical of his leadership."}
```
