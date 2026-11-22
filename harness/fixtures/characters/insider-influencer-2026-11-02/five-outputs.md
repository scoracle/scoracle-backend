# Five-article instruction comparison

Exact raw outputs, including failures. These are development cases.

## explicit-denial-clears-prior-rumor

Input tokens 878 → 894; rejected → accepted.

Before:
```json
{
  "body": "TV2 reports that Everton denied talks to sign Lina Foss, contradicting earlier links between the player and the club. The denial comes from the club itself, which explicitly stated no discussions were taking place regarding her transfer.",
  "findings": [
    {
      "counterparty": "Everton",
      "evidence_quote": "Everton denied talks to sign Lina Foss after earlier reports linked the player with the club.",
      "report_index": 0,
      "stage": "speculation",
      "status": "reported"
    }
  ]
}
```

After:
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

Input tokens 882 → 898; accepted → accepted.

Before:
```json
{
  "body": "Kai Sorensen is a football player currently under monitoring by Chelsea, according to a report from TransferTavern. The outlet states that Chelsea are among clubs keeping tabs on the forward, but explicitly notes that no bid or talks have been mentioned.",
  "findings": [
    																]
  }
```

After:
```json
{
  "body": "Kai Sorensen is a football player. TransferTavern reports that Chelsea are among clubs monitoring the forward, though no bid or talks are mentioned.",
  "findings": [
    		]
  }
```

Shorter, but still adds an unnecessary identity sentence. Monitoring/no-bid qualification is preserved.

## fully-named-advanced-talks

Input tokens 1043 → 1059; rejected → rejected.

Before:
```json
{
  "body": "Bayern Munich and Flamengo are negotiating a five-year deal for Mateus Andrade, with personal terms reportedly already agreed on. The club's board has opened formal negotiations with Flamengo, identifying the Brazilian winger as their top priority.",
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

After:
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

Input tokens 881 → 897; rejected → rejected.

Before:
```json
{
  "body": "Chelsea are monitoring Kai Sorensen, a forward, according to TransferTavern, though no bid or talks have been reported. The outlet notes the club is among those monitoring the player but explicitly states no financial offer or negotiation discussions exist.",
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

After:
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

Input tokens 920 → 936; accepted → accepted.

Before:
```json
{
  "body": "Aaron Gordon remains on the Denver Nuggets roster with no reported move, as Synthetic Wire explicitly states the report concerns DeMar DeRozan and contains nothing about Gordon. The outlet clarifies that while DeRozan is linked to a trade, this information does not extend to Gordon.",
  "findings": []
}
```

After:
```json
{
  "body": "Aaron Gordon remains on the Denver Nuggets roster with no reported movement in this report. Synthetic Wire notes that while DeMar DeRozan is linked with a trade to Denver, the report explicitly states nothing about a move for Gordon.",
  "findings": []
}
```

Target boundary preserved: the supplied excerpt explicitly says Gordon remains on the roster and reports no move for him. New body narrows that limit to this report.

## headline-unnamed-signing

Input tokens 916 → 932; rejected → accepted.

Before:
```json
{
  "body": "Example Wire reports that the Cedar Bears have announced the signing of a former Maple Foxes player, though the specific individual and contract details remain undisclosed in the headline. This move is presented as a confirmed announcement rather than speculation or interest.",
  "findings": [
    																]
  }
```

After:
```json
{
  "body": "Cedar Bears have announced the signing of a former Maple Foxes player, per Example Wire. The move is confirmed as a completed agreement.",
  "findings": [
    																]
  }
```

Now accepted after starting with the target; does not identify the unnamed player. Redundant confirmed/completed wording adds little and is stronger than needed.

## reliable-source-heating-steam

Input tokens 1035 → 1051; rejected → rejected.

Before:
```json
{
  "body": "Mateus Andrade is the subject of active negotiations between Bayern Munich and Flamengo, with Fabrizio Romano reporting that personal terms for a five-year deal are already agreed. Kicker confirms that Bayern's board has opened formal talks and considers Andrade their top priority.",
  "findings": [
    																]
  }
```

After:
```json
{
  "body": "Mateus Andrade is the subject of active negotiations between Bayern Munich and Flamengo, with both sources confirming formal talks are underway. Fabrizio Romano reported that personal terms for a five-year deal are already agreed, while Kicker added that Bayern's board has identified Andrade as their top priority.",
  "findings": [
    																]
  }
```

Still rejected: expands Bayern to Bayern Munich from co_mentions rather than article wording. Confirming language overstates the source relationship.

## unnamed-signing-no-identity-join

Input tokens 871 → 887; accepted → accepted.

Before:
```json
{
  "body": "Aaron Gordon attended a community event, which does not establish a move or appointment. Synthetic Wire reported that the Denver Nuggets signed an unnamed forward, but Gordon is not named in that deal.",
  "findings": []
}
```

After:
```json
{
  "body": "Aaron Gordon attended a community event while the Denver Nuggets announced the signing of an unnamed forward, per Synthetic Wire.",
  "findings": []
}
```

No false identity join, but loses the baseline explicit warning that Gordon is not named in the signing and combines his unrelated community appearance with it.

## clearly-negative

Input tokens 795 → 794; accepted → accepted.

Before:
```json
{"score":25,"headline":"Fans booed Cedar Comets over poor performance","body":"Example Wire reports that Cedar Comets fans expressed fury and booed the team regarding its performance. This coverage indicates strong negative reactions from the supporters."}
```

After:
```json
{
  "body": "Fans of the Cedar Comets expressed fury and booed the team's performance, according to Example Wire.",
  "score": 25,
  "headline": "Cedar Comets fans criticize the performance"
}
```

Supported negative score remains 25. Body is shorter and names the outlet.

## clearly-positive

Input tokens 792 → 791; accepted → accepted.

Before:
```json
{"score":75,"headline":"Fans delight over Cedar Comets victory","body":"Example Wire reports that Cedar Comets supporters were delighted and cheered the win. The coverage reflects a joyful mood from the fans themselves."}
```

After:
```json
{
  "body": "Supporters of the Cedar Comets expressed delight following their victory, with one outlet noting the fans' enthusiastic reaction to the result.",
  "score": 75,
  "headline": "Fans delight as Cedar Comets secure win"
}
```

Supported positive score remains 75, but replaces the actual publisher name with one outlet, losing requested attribution.

## gordon-derozan-wrong-subject

Input tokens 808 → 807; accepted → accepted.

Before:
```json
{"score":null,"headline":null,"body":null}
```

After:
```json
{
  "body": null,
  "score": null,
  "headline": null
}
```

Correct all-null abstention: reactions concern DeRozan, not Gordon.

## quiet-mixed

Input tokens 795 → 794; accepted → accepted.

Before:
```json
{"score":null,"headline":null,"body":null}
```

After:
```json
{
  "body": null,
  "score": null,
  "headline": null
}
```

Correct all-null abstention: schedule information supplies no emotional evidence.

## warm-memory-cold-coverage

Input tokens 787 → 786; accepted → accepted.

Before:
```json
{"score":null,"headline":null,"body":null}
```

After:
```json
{
  "body": null,
  "score": null,
  "headline": null
}
```

Correct all-null abstention: historical warmth does not turn current schedule coverage into positive emotion.

## transfer-player-83946-NFL

Input tokens 1595 → 1611; rejected → rejected.

Before:
```json
{
  "body": "Evan Neal has been confirmed by the Philadelphia Eagles as a signing, with reports indicating he joined the practice squad after being released by the Las Vegas Raiders. Multiple outlets, including Yahoo Sports and NBC Sports, have reported the Eagles' confirmation of his signing to their roster.",
  "findings": [
    																]
  }
```

After:
```json
{
  "body": "Evan Neal has joined the Philadelphia Eagles, confirmed by Yahoo Sports and NBC Sports, which report the Eagles signed the offensive tackle. New York Post adds that Neal came from the Las Vegas Raiders to the practice squad after his release.",
  "findings": [
    																]
  }
```

Still rejected: expands team names from co_mentions. Yahoo/NBC attribution is compressed into confirmed language; the New York Post source supports practice squad after Raiders release.

## transfer-team-37-FOOTBALL

Input tokens 1490 → 1506; rejected → accepted.

Before:
```json
{
  "body": "Roma is facing a significant crisis with head coach Pellegrini out for 6-8 weeks due to a serious injury, a situation La Gazzetta dello Sport describes as decimating the squad ahead of a match against Napoli. Additionally, reports indicate Roma is looking to make two signings in January, though no specific players or clubs were named in the available headlines.",
  "findings": []
}
```

After:
```json
{
  "body": "Roma is seeking two January signings, as reported by Yahoo Sports. The club also faces a coaching crisis with Pellegrini and Svilar out due to injury, according to La Gazzetta dello Sport. Meanwhile, reports suggest Leipzig may retain Roma's El Aynaoui for a €50m package, per Bulinews.",
  "findings": [
    		]
  }
```

Accepted but factually wrong: invents a coaching crisis and assigns the two-player €50m package solely to El Aynaoui. Acceptance is not a quality gain.

## vibe-person-23-FOOTBALL

Input tokens 1474 → 1473; accepted → accepted.

Before:
```json
{
  "score": 75,
  "headline": "De Zerbi expresses delight over Spurs' resilience and style after Coventry victory.",
  "body": "Roberto De Zerbi is described as 'very happy' and 'delighted' by multiple outlets, including Flashscore.co.za, VAVEL.com, Yellow Advertiser, Vodacom Soccer, and TribalFootball. He praised Tottenham's 'attitude,' 'mentality,' and ability to 'turn on the style' following their crucial win against Coventry."
}
```

After:
```json
{
  "body": "Roberto De Zerbi expressed strong satisfaction with Tottenham's performance, stating he was 'very happy' and 'delighted' after their victory over Coventry. He emphasized that the team's resilience and positive mentality remain intact following the crucial win.",
  "score": 75,
  "headline": "De Zerbi delighted as Tottenham turn on the style"
}
```

Still misstates a hope as an assertion that mentality remains intact and omits publisher names. Positive mood is supported, but the reporting is not fully faithful.

## vibe-person-959-NFL

Input tokens 1249 → 1248; accepted → accepted.

Before:
```json
{"score":50,"headline":"Jerry Jones faces criticism from ex-players and expresses disappointment","body":"The New York Times reports Jerry Jones is 'disappointed' and wants to improve the Cowboys roster before the trade deadline. Yahoo Sports notes an ex-Cowboys star took a swipe at Jones amid team struggles. These sources indicate mixed reactions: Jones feels let down, while a former player is critical of his leadership."}
```

After:
```json
{
  "body": "The New York Times reports Jerry Jones expressed disappointment regarding the Cowboys' roster before the trade deadline, while Yahoo Sports notes an ex-Cowboys star criticized Jones amid team struggles.",
  "score": 25,
  "headline": "Jerry Jones faces criticism and disappointment over Cowboys roster struggles ahead of trade deadline."
}
```

Improved: score 25 correctly treats disappointment and criticism as negative, preserves separate speakers and publishers, and uses one concise sentence.
