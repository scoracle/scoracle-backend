# Controlled outputs

Complete raw replies and validation receipts are retained. Parser acceptance is not factual correctness.

## 1. chelsea-live-live-journalist-preview

Native input: 2612 → 1224; validation: accepted → accepted.

The first Chelsea story now attributes the publisher and omits the unsupported first-team-squad detail. However, Manu Kone is identified as the record signing although the headline does not establish that link. The unnamed target is still made the speaker of the boss's claim. The baseline additionally imported right-back from a different story. Both pass validation despite factual errors.

Before:
```json
{"narratives":[{"body":"Chelsea promoted Penders and Sharman-Lowe to the first team squad following an injury to Emi Martinez.","headline":"Chelsea promote Penders and Sharman-Lowe after Emi Martinez injury","report_keys":["report_1"]},{"body":"Chelsea is actively searching for new right-back options as the club evaluates the future of current player Gusto.","headline":"Chelsea begin search for new right-back options amid Gusto uncertainty","report_keys":["report_5"]},{"body":"Chelsea's £100m target for a new right-back claims his boss has not given up on securing a new contract.","headline":"Chelsea's £100m target could yet sign new contract claims his boss","report_keys":["report_3"]}]}
```

After:
```json
{"narratives":[{"body":"Chelsea FC Latest News .com reports that the club promoted Penders and Sharman-Lowe following Emi Martinez's injury.","headline":"Chelsea promote Penders and Sharman-Lowe after Martinez injury","report_keys":["report_1"]},{"body":"FootballTransfers.com headlines suggest Manu Kone is a record signing target for Chelsea.","headline":"Chelsea eye record signing Manu Kone","report_keys":["report_2"]},{"body":"Talk Chelsea reports that Chelsea's £100m target claims his boss has not given up on signing a new contract.","headline":"Chelsea's £100m target claims new contract possible","report_keys":["report_3"]}]
}
```

## 2. dak-prescott-live-journalist-preview

Native input: 2360 → 1188; validation: rejected → rejected.

The backlash story now stays with MARCA and preserves 0-2 rather than combining separate New York Post coverage. Both versions merge ESPN rest-of-season rankings with NFL.com Week 7 QB rankings and blur their measurement/window meanings. Both are rejected by the existing named-claim guard; that mechanical rejection does not characterize the semantic problem fully.

Before:
```json
{"narratives":[{"body":"Athlon Sports reports that Cowboys quarterback Dak Prescott announced personal news following the team's fourth loss.","headline":"Dak Prescott Announces Personal News After 4th Loss","report_keys":["report_1"]},{"body":"ESPN and NFL.com indicate Dak Prescott's season rankings have dropped, with Isaiah Likely and Chuba Hubbard rising in the rest-of-season standings.","headline":"Dak Prescott Rankings Down as Isaiah Likely and Chuba Hubbard Rise","report_keys":["report_2","report_3"]},{"body":"MARCA and the New York Post note Dak Prescott faced backlash and expressed frustration after the Cowboys' poor record since he told C.J. Stroud to 'go get some wins'.","headline":"Dak Prescott Faces Backlash and Shares Frustration After Poor Start","report_keys":["report_4","report_5"]}]
}
```

After:
```json
{"narratives":[{"body":"Athlon Sports reports that Cowboys quarterback Dak Prescott announced personal news following the team's fourth loss.","headline":"Cowboys QB Dak Prescott Announces Personal News After 4th Loss","report_keys":["report_1"]},{"body":"Multiple outlets, including ESPN and NFL.com, note that Dak Prescott's season rankings have hit a low point while other players have risen.","headline":"Dak Prescott's Season Rankings Hit a Low Point","report_keys":["report_2","report_3"]},{"body":"MARCA reports that Dak Prescott faces backlash after the Cowboys posted a 0-2 record since he told C.J. Stroud to 'go get some wins'.","headline":"Dak Prescott Faces Backlash After Cowboys 0-2 Record","report_keys":["report_4"]}]
}
```

## 3. chelsea-target-preview

Native input: 2459 → 1213; validation: accepted → accepted.

The single unnamed player is no longer pluralized, and publishers are named. Chelsea is still invented as the adviser to Estevao, and advice to leave is unsupported. CaughtOffside is newly made the speaker urging departure, although the supplied headline names only the publisher. Both are mechanically accepted.

Before:
```json
{"narratives":[{"body":"Estevao received brutally honest transfer advice from Chelsea despite Xabi Alonso's promise.","headline":"Estevao urged to leave Chelsea despite Alonso promise","report_keys":["report_3"]},{"body":"Chelsea players with 23 minutes of playing time this season are urged to leave the club.","headline":"Chelsea player with 23 minutes urged to leave","report_keys":["report_2"]},{"body":"Chelsea was labelled 'bigger and worse offenders' than Man City despite 114 charges guilty verdict.","headline":"Chelsea labelled 'bigger and worse offenders' than Man City","report_keys":["report_5"]}]
}
```

After:
```json
{"narratives":[{"body":"Football London reported that Estevao received brutally honest transfer advice from Chelsea despite a promise from Xabi Alonso.","headline":"Estevao urged to leave Chelsea despite Alonso promise","report_keys":["report_3"]},{"body":"CaughtOffside urged a Chelsea player with 23 minutes of playing time this season to leave the club.","headline":"Chelsea player urged to leave after limited playing time","report_keys":["report_2"]},{"body":"Football London stated Chelsea were labelled 'bigger and worse offenders' than Man City despite 114 charges guilty verdict.","headline":"Chelsea labelled worse offenders than Man City despite charges","report_keys":["report_5"]}]
}
```

## 4. narratives-person-86-FOOTBALL

Native input: 2102 → 1322; validation: accepted → accepted.

The one retained Diomande story is supported and stays separate. The separate Mastantuono story is omitted rather than merged. Up to three stories permits fewer, but the baseline covered both; reduced coverage is not evidence of higher overall quality.

Before:
```json
{"narratives":[{"body":"RB Leipzig coach Martín Demichelis praised Yan Diomande ahead of the Champions League clash against Real Madrid at the Bernabéu.","headline":"Demichelis praises Yan Diomande ahead of Real Madrid clash","report_keys":["report_1","report_2","report_3"]},{"body":"Demichelis stated that Franco Mastantuono needs time to develop but believes he is built for Real Madrid despite his loan spell at Fiorentina.","headline":"Demichelis insists Franco Mastantuono is built for Real Madrid","report_keys":["report_4"]}] }
```

After:
```json
{"narratives":[{"body":"Martin Demichelis, RB Leipzig's coach, praised Yan Diomande ahead of the Champions League clash against Real Madrid at the Bernabeu.", "headline":"Demichelis praises Diomande ahead of Real Madrid clash", "report_keys":["report_1", "report_2", "report_3"]}]
}
```

## 5. narratives-player-37-NBA

Native input: 2811 → 1069; validation: accepted → rejected.

Injury reporting remains separate from the jersey story. The output expands Clippers to Los Angeles Clippers, which is absent from supplied wording and triggers the strict guard. The headline also changes an indication into a definitive assignment. Baseline was accepted; the parser was not loosened for the familiar alias.

Before:
```json
{"narratives":[{"body":"Bradley Beal is ruled out for the game against Sacramento due to a knee injury, according to NBC Sports.","headline":"Bradley Beal ruled out vs. Sacramento over knee injury","report_keys":["report_1"]},{"body":"NBC Sports reports that Bradley Beal remains unavailable for the upcoming game against the Lakers due to his knee injury.","headline":"Bradley Beal remains out vs. Lakers due to knee injury","report_keys":["report_2"]},{"body":"ABC30 Fresno reports that the Clippers have indicated Bradley Beal will wear Chris Paul's old No. 3 jersey.","headline":"Clippers indicate Bradley Beal will wear Chris Paul's old No. 3","report_keys":["report_3"]}]
}
```

After:
```json
{"narratives":[{"body":"NBC Sports reports that Bradley Beal is ruled out for the game against Sacramento due to a knee injury and remains unavailable for the match against the Lakers.","headline":"Bradley Beal ruled out for Sacramento and Lakers games","report_keys":["report_1","report_2"]},{"body":"ABC30 Fresno reports that the Los Angeles Clippers have indicated the jersey number previously worn by Chris Paul will be assigned to Bradley Beal.","headline":"Clippers assign Chris Paul's old No. 3 to Bradley Beal","report_keys":["report_3"]}]
}
```

## 6. transfer-player-83946-NFL

Native input: 2162 → 1556; validation: rejected → rejected.

The baseline reversal from signing with a rival to competing with a rival disappears. New text introduces acquiring Neal from the Raiders and unconfirmed contract details, neither supplied. Both versions expand Eagles/Raiders names and fail the named-claim guard. Empty findings remain appropriate to the exact-full-name quote requirement.

Before:
```json
{
  "body": "Evan Neal has been confirmed by the Philadelphia Eagles as a signee, with reports indicating he joined the practice squad after being released by the Las Vegas Raiders. Multiple outlets describe the move as the Eagles signing an ex-Giants draft bust to compete with an NFC East rival.",
  "findings": [
    																]
  }
```

After:
```json
{
  "body": "Evan Neal has been confirmed by the Philadelphia Eagles as a signing, with reports indicating he joined the practice squad after being released by the Las Vegas Raiders. Multiple outlets describe the move as the Eagles acquiring the offensive tackle from the Raiders, though specific contract details remain unconfirmed.",
  "findings": [
    																]
  }
```

## 7. transfer-team-37-FOOTBALL

Native input: 2456 → 1451; validation: accepted → rejected.

The new opening promotes stored coach metadata into the reading, triggering rejection. It retains unrelated injury coverage, assigns the entire two-player €50m package to El Aynaoui, and exceeds the requested two or three sentences. Baseline was accepted but invented coach Pellegrini and denied supported movement. Neither is factually sound.

Before:
```json
{
  "body": "Roma is looking to make two signings in January, though no specific targets or parties are named in the available reports. The club has rejected three offers for Pisilli this summer and will lose coach Pellegrini for at least six to eight weeks due to a serious injury. There is no reported movement involving Roma as a target or a mover in the provided data.",
  "findings": []
}
```

After:
```json
{
  "body": "Roma is a football team in Serie A with Mirko Vučinić recorded as coach. Reports indicate Roma is looking to make two signings in January and rejected three offers for Pisilli this summer, while also facing a crisis with Pellegrini injured for 6-8 weeks. A separate report suggests Leipzig might retain Roma's El Aynaoui for €50m, though no other movement stories are supported.",
  "findings": []
}
```

## 8. vibe-person-23-FOOTBALL

Native input: 2369 → 1435; validation: accepted → accepted.

Length improves from three sentences to two and valence stays positive (85 to 75). However, great news is transferred from Kulusevski's return to the result; hopes about attitude/mentality become praise. Both pass validation. The cleaner input does not prevent actor/object merging.

Before:
```json
{
  "score": 85,
  "headline": "De Zerbi expresses delight and hope over Spurs' resilience and style",
  "body": "Roberto De Zerbi is portrayed as deeply pleased with Tottenham's performance, describing himself as 'very happy' and 'delighted' after a crucial victory over Coventry. He specifically praises the team's resilience and hopes their positive 'attitude' and 'mentality' remain intact. While one source notes his focus on Kulusevski's return, the prevailing tone across reports is one of satisfaction and optimism regarding the team's style and response to the match."
}
```

After:
```json
{
  "score": 75,
  "headline": "De Zerbi expresses joy over Spurs' resilience and style",
  "body": "Roberto De Zerbi is portrayed as delighted and happy following Tottenham's victory over Coventry. Multiple sources report that he praised the team's attitude, mentality, and resilience, describing the result as 'great news' and noting how Spurs turned on their style."
}
```

## 9. vibe-person-959-NFL

Native input: 1412 → 1210; validation: accepted → accepted.

The baseline wrongly called two negative reactions mixed and scored 50. The new output abstains despite explicit disappointment and criticism. Mechanical acceptance of nulls is not a correct reading of this supplied evidence.

Before:
```json
{"score":50,"headline":"Jerry Jones faces criticism from ex-Cowboys star and media reports of disappointment","body":"Coverage indicates Jerry Jones is feeling disappointed and is seeking roster improvements before the trade deadline. An ex-Cowboys star has publicly criticized Jones amid reports of team struggles. The tone reflects mixed reactions: internal dissatisfaction from Jones and external criticism from a former player, without evidence of a unified fan or organizational consensus."}
```

After:
```json
{"score":null,"headline":null,"body":null}
```
