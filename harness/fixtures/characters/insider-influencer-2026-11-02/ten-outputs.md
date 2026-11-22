# Ten-article batch

Raw responses and manual review. This batch predates the requested multi-paragraph Insider/Influencer instructions.

## explicit-denial-clears-prior-rumor

1 articles; 894 → 894 tokens; accepted → accepted.

```json
{
  "body": "Lina Foss has been linked to Everton, but the club has denied any talks to sign her, according to TV2.",
  "findings": [
    {
      "counterparty": "Everton",
      "evidence_quote": "Everton denied talks to sign Lina Foss after earlier reports linked the player with the club.",
      "report_index": 0,
      "stage": null,
      "status": "denied"
    }
  ]
}
```

Improved: explicit denial is now status denied with null stage; concise attributed body starts with the target.

## fizzled-prior-lowrel-skeptic

1 articles; 898 → 898 tokens; accepted → accepted.

```json
{
  "body": "Kai Sorensen is a football player. TransferTavern reports that Chelsea are among clubs monitoring the forward, though no bid or talks are mentioned.",
  "findings": [
    		]
  }
```

Shorter, but still adds an unnecessary identity sentence. Monitoring/no-bid qualification is preserved.

## fully-named-advanced-talks

2 articles; 1059 → 1059 tokens; rejected → rejected.

```json
{
  "body": "Mateus Andrade is the subject of active negotiations between Bayern Munich and Flamengo, with personal terms reportedly already agreed on a five-year deal. Both clubs are described as prioritizing the winger, with formal talks now open.",
  "findings": [
    {
      "counterparty": "Flamengo",
      "evidence_quote": "Bayern Munich and Flamengo are negotiating the fee, with personal terms already agreed on a five-year deal for the winger.",
      "report_index": 0,
      "stage": "here_we_go",
      "status": "reported"
    },
    {
      "counterparty": "Flamengo",
      "evidence_quote": "Bayern Munich's board have opened formal negotiations with Flamengo and see the Brazilian as their top target.",
      "report_index": 1,
      "stage": "concrete_interest",
      "status": "reported"
    }
  ]
}
```

Still rejected: quotes omit the full target and personal terms are classified as a completed agreement. Body newly attributes priority to both clubs although only Bayern is described that way.

## fully-named-interest

1 articles; 897 → 897 tokens; rejected → rejected.

```json
{
  "body": "Chelsea are among clubs monitoring Kai Sorensen, though no bid or talks are mentioned. TransferTavern reports the interest but offers no further details on the status of the move.",
  "findings": [
    {
      "counterparty": "Chelsea",
      "evidence_quote": "A report claims Chelsea are among clubs monitoring the forward, though no bid or talks are mentioned.",
      "report_index": 0,
      "stage": "speculation",
      "status": "reported"
    }
  ]
}
```

Still rejected: body starts with Chelsea rather than the target and finding quote omits the target. Baseline also turned no reported talks into no talks existing.

## gordon-derozan-wrong-subject

1 articles; 936 → 936 tokens; accepted → accepted.

```json
{
  "body": "Aaron Gordon remains on the Denver Nuggets roster with no reported movement in this report. Synthetic Wire notes that while DeMar DeRozan is linked with a trade to Denver, the report explicitly states nothing about a move for Gordon.",
  "findings": []
}
```

Target boundary preserved: the supplied excerpt explicitly says Gordon remains on the roster and reports no move for him. New body narrows that limit to this report.

## headline-unnamed-signing

1 articles; 932 → 932 tokens; accepted → accepted.

```json
{
  "body": "Cedar Bears have announced the signing of a former Maple Foxes player, per Example Wire. The move is confirmed as a completed agreement.",
  "findings": [
    																]
  }
```

Now accepted after starting with the target; does not identify the unnamed player. Redundant confirmed/completed wording adds little and is stronger than needed.

## reliable-source-heating-steam

2 articles; 1051 → 1051 tokens; rejected → rejected.

```json
{
  "body": "Mateus Andrade is the subject of active negotiations between Bayern Munich and Flamengo, with both sources confirming formal talks are underway. Fabrizio Romano reported that personal terms for a five-year deal are already agreed, while Kicker added that Bayern's board has identified Andrade as their top priority.",
  "findings": [
    																]
  }
```

Still rejected: expands Bayern to Bayern Munich from co_mentions rather than article wording. Confirming language overstates the source relationship.

## unnamed-signing-no-identity-join

1 articles; 887 → 887 tokens; accepted → accepted.

```json
{
  "body": "Aaron Gordon attended a community event while the Denver Nuggets announced the signing of an unnamed forward, per Synthetic Wire.",
  "findings": []
}
```

No false identity join, but loses the baseline explicit warning that Gordon is not named in the signing and combines his unrelated community appearance with it.

## clearly-negative

1 articles; 794 → 794 tokens; accepted → accepted.

```json
{
  "body": "Fans of the Cedar Comets expressed fury and booed the team's performance, according to Example Wire.",
  "score": 25,
  "headline": "Cedar Comets fans criticize the performance"
}
```

Supported negative score remains 25. Body is shorter and names the outlet.

## clearly-positive

1 articles; 791 → 791 tokens; accepted → accepted.

```json
{
  "body": "Supporters of the Cedar Comets expressed delight following their victory, with one outlet noting the fans' enthusiastic reaction to the result.",
  "score": 75,
  "headline": "Fans delight as Cedar Comets secure win"
}
```

Supported positive score remains 75, but replaces the actual publisher name with one outlet, losing requested attribution.

## gordon-derozan-wrong-subject

1 articles; 807 → 807 tokens; accepted → accepted.

```json
{
  "body": null,
  "score": null,
  "headline": null
}
```

Correct all-null abstention: reactions concern DeRozan, not Gordon.

## quiet-mixed

1 articles; 794 → 794 tokens; accepted → accepted.

```json
{
  "body": null,
  "score": null,
  "headline": null
}
```

Correct all-null abstention: schedule information supplies no emotional evidence.

## warm-memory-cold-coverage

1 articles; 786 → 786 tokens; accepted → accepted.

```json
{
  "body": null,
  "score": null,
  "headline": null
}
```

Correct all-null abstention: historical warmth does not turn current schedule coverage into positive emotion.

## transfer-player-83946-NFL

7 articles; 1611 → 1889 tokens; rejected → rejected.

```json
{
  "body": "Evan Neal has joined the Philadelphia Eagles' practice squad, a move confirmed by the team and reported by multiple outlets. The signing follows his release by the Las Vegas Raiders, with sources noting he was previously a draft bust for the New York Giants. Neal is joining alongside teammates Jaydon Blue and Anfernee Orji.",
  "findings": [
    																]
  }
```

Still rejected: expands Raiders to Las Vegas Raiders and Giants to New York Giants from identity hints rather than article text. Signing/practice squad is supported, but publisher attribution becomes vague.

## transfer-team-37-FOOTBALL

9 articles; 1506 → 1918 tokens; accepted → rejected.

```json
{
  "body": "Roma is seeking two January signings, with reports indicating interest in players from AC Milan and a potential €50m package for El Aynaoui to join RB Leipzig. The club has also rejected multiple offers for Pisilli this summer.",
  "findings": [
    		]
  }
```

Rejected: invents Roma interest in Milan players by joining separate items in a roundup. Still assigns a two-player €50m package solely to El Aynaoui and turns retain into join. Removing the invented coaching crisis does not make this faithful.

## vibe-person-23-FOOTBALL

8 articles; 1473 → 1712 tokens; accepted → accepted.

```json
{
  "body": "Roberto De Zerbi expressed strong satisfaction with Tottenham's performance, stating he was 'very happy' and 'delighted' after a crucial victory over Coventry. He praised the team's resilience, attitude, and bonding, while also highlighting the positive impact of player Mohammed Kudus's return and Mohammed Kudus's display.",
  "score": 75,
  "headline": "De Zerbi delighted as Tottenham turn on the style"
}
```

Accepted but wrong: transfers Kulusevski's return to Mohammed Kudus, turns a hope about attitude into praise, and omits publishers.

## vibe-person-959-NFL

2 articles; 1248 → 1248 tokens; accepted → accepted.

```json
{
  "body": "The New York Times reports Jerry Jones expressed disappointment regarding the Cowboys' roster before the trade deadline, while Yahoo Sports notes an ex-Cowboys star criticized Jones amid team struggles.",
  "score": 25,
  "headline": "Jerry Jones faces criticism and disappointment over Cowboys roster struggles ahead of trade deadline."
}
```

Same supported negative score 25 and separate speaker/source reading as the five-article candidate.

## chelsea-live-live-journalist-preview

5 articles; 1182 → 1182 tokens; accepted → accepted.

```json
{"paragraphs":[{"body":"Chelsea FC Latest News .com reports that the club has promoted Penders and Sharman-Lowe following an injury to Emi Martinez. FootballTransfers.com notes that Manu Kone is being viewed as a record signing. Talk Chelsea states that a £100m target could yet sign a new contract, a claim made by his boss. London Evening Standard provides an injury update regarding Martinez, Chavarria, and James. Flashscore.com indicates Chelsea are searching for new right-back options amid Gusto's uncertainty.", "report_ids":[1,2,3,4,5]}],"headline":"Chelsea moves on injuries, transfer targets, and squad searches"}
```

Same as the prior output: one crowded paragraph; assigns record-signing language to Kone although the headline does not establish that identity.

## dak-prescott-live-journalist-preview

5 articles; 1146 → 1146 tokens; accepted → accepted.

```json
{"paragraphs":[{"body":"Dak Prescott announced personal news following the team's fourth loss, a development reported by Athlon Sports. Additionally, Prescott revealed what he finds most frustrating after another difficult game for the Cowboys, according to the New York Post.", "report_ids":[1, 5]}],"headline":"Dak Prescott addresses personal news and frustration amid Cowboys' struggles"}
```

Same narrower but source-bound personal-news/frustration recap. Does not merge incompatible ranking windows.

## chelsea-target-preview

10 articles; 1171 → 1467 tokens; rejected → accepted.

```json
{"paragraphs":[],"headline":null}
```

Explicit empty recap despite usable news headlines. Mechanically accepted abstention, but a coverage failure rather than a factual-quality gain.

## narratives-person-86-FOOTBALL

4 articles; 1283 → 1283 tokens; accepted → accepted.

```json
{"paragraphs":[{"body":"RB Leipzig boss Martín Demichelis praised Yan Diomande ahead of the Champions League clash against Real Madrid at the Bernabéu, describing the French defender as exceptional despite the daunting nature of the upcoming fixture. The comments were reported by Yahoo, Yardbarker, and OneFootball, who all highlighted Demichelis's high regard for Diomande in the context of the European tie.","report_ids":[1,2,3]}],"headline":"Demichelis hails Diomande ahead of Real Madrid clash"}
```

Same unsupported French-defender description of Diomande; still accepted.

## narratives-player-37-NBA

3 articles; 1033 → 1033 tokens; rejected → rejected.

```json
{"paragraphs":[{"body":"Bradley Beal is ruled out to play against the Sacramento Kings due to a knee injury, according to NBC Sports. He remains unavailable for the upcoming game against the Lakers as well, per NBC Sports. The LA Clippers have indicated that Chris Paul's old jersey number will be assigned to Beal, as reported by ABC30 Fresno.", "report_ids":[1, 2, 3]}],"headline":"Bradley Beal ruled out for two games with knee injury; Clippers assign Paul's old number to him"}
```

Different output despite identical native input and temperature zero: avoids the previous invented retirement but expands Sacramento to Sacramento Kings and is rejected. Reported number-assignment certainty is stronger than indicates.
