# Evidence-led paragraph outputs

Qwen3.5-4B-Q4_K_M; temperature 0; thinking disabled; concurrency 1; no retries. Human development review, not held-out accuracy.

## explicit-denial-clears-prior-rumor

1 articles; 930 native input tokens; rejected; 1 prose paragraphs.

```json
{
  "body": "Lina Foss has been linked to Everton, but the club has denied any talks to sign her. TV2 reported that Everton explicitly rejected discussions to bring the player in after earlier stories suggested a connection between the two sides.\n\n",
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

The denial finding remains correct. The body chronology is supported by the supplied publisher excerpt, but the existing prose chronology guard rejects it. This is a validator limitation, not proof of fabrication.

## fizzled-prior-lowrel-skeptic

1 articles; 934 native input tokens; rejected; 1 prose paragraphs.

```json
{
  "body": "Chelsea are among clubs monitoring Kai Sorensen, though no bid or talks are mentioned. (TransferTavern)\n",
  "findings": []
}
```

Rejected for beginning with Chelsea rather than the exact target. Monitoring and no reported bid/talks remain supported.

## fully-named-advanced-talks

2 articles; 1095 native input tokens; rejected; 1 prose paragraphs.

```json
{
  "body": "Mateus Andrade is the subject of active negotiations between Bayern Munich and Flamengo, with reports indicating that personal terms for a five-year deal have already been agreed upon. Bayern Munich has opened formal talks with Flamengo and has identified the Brazilian winger as their top priority.",
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

Still rejected: finding quotes omit the full target. Personal-terms agreement is still promoted to here_we_go although fee negotiations continue; attribution is missing.

## fully-named-interest

1 articles; 933 native input tokens; rejected; 2 prose paragraphs.

```json
{
  "body": "Chelsea are among clubs monitoring Kai Sorensen, though no bid or talks are mentioned.\n\nTransferTavern\n",
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

Still rejected: wrong opening subject, standalone publisher line and finding quote missing the full target. Not a useful additional paragraph.

## gordon-derozan-wrong-subject

1 articles; 972 native input tokens; accepted; 2 prose paragraphs.

```json
{
  "body": "Aaron Gordon remains on the Denver Nuggets roster, with no reported movement or trade interest involving him in the current reporting cycle.\n\nSynthetic Wire reported that DeMar DeRozan is linked with a trade to the Denver Nuggets, but explicitly stated that this report says nothing about a move for Gordon.\n\n",
  "findings": []
}
```

Two paragraphs retain the different subjects, but broaden this report says nothing about Gordon into no movement or trade interest in the current reporting cycle. Accepted does not establish that wider claim.

## headline-unnamed-signing

1 articles; 968 native input tokens; accepted; 1 prose paragraphs.

```json
{
  "body": "Cedar Bears have announced the signing of a former Maple Foxes player, according to Example Wire.\n",
  "findings": [
    																]
  }
```

Concise and attributed; preserves the unnamed player without inventing identity. One paragraph is sufficient for this one report.

## reliable-source-heating-steam

2 articles; 1087 native input tokens; rejected; 1 prose paragraphs.

```json
{
  "body": "Mateus Andrade is the subject of active negotiations between Bayern Munich and Flamengo, with both parties reportedly in formal talks regarding a five-year deal. Bayern Munich has identified Andrade as their top priority and has opened formal negotiations with Flamengo, while personal terms for the winger have already been agreed upon.",
  "findings": [
    																]
  }
```

Still rejected for expanding Bayern into Bayern Munich. Body conflates the five-year personal terms with negotiations between clubs and omits explicit publishers.

## unnamed-signing-no-identity-join

1 articles; 923 native input tokens; accepted; 1 prose paragraphs.

```json
{
  "body": "Aaron Gordon attended a community event, while the Denver Nuggets announced the signing of an unnamed forward. (Synthetic Wire)\n",
  "findings": []
}
```

No false identity join, but still pairs an unrelated community appearance with the unnamed signing without explaining the evidence limit.

## clearly-negative

1 articles; 867 native input tokens; accepted; 1 prose paragraphs.

```json
{
  "body": "Fans of the Cedar Comets expressed intense anger and disappointment, booing the team and stating they were furious with its performance, according to Example Wire.",
  "score": 25,
  "headline": "Fans boo and voice fury over Cedar Comets performance"
}
```

Negative score remains supported. Adds disappointment to explicit fury/booing rather than sticking to the supplied reaction.

## clearly-positive

1 articles; 864 native input tokens; accepted; 1 prose paragraphs.

```json
{
  "body": "Supporters of the Cedar Comets expressed delight following their victory, with one outlet reporting that fans cheered the win and voiced their happiness about the result.",
  "score": 75,
  "headline": "Fans cheer delight as Cedar Comets secure victory"
}
```

Positive score and delight are supported, but still says one outlet instead of naming Example Wire.

## gordon-derozan-wrong-subject

1 articles; 880 native input tokens; accepted; 0 prose paragraphs.

```json
{
  "body": null,
  "score": null,
  "headline": null
}
```

Correct all-null abstention: the reaction concerns DeRozan, not Gordon.

## quiet-mixed

1 articles; 867 native input tokens; accepted; 0 prose paragraphs.

```json
{
  "body": null,
  "score": null,
  "headline": null
}
```

Correct all-null abstention: scheduling supplies no emotional reading.

## warm-memory-cold-coverage

1 articles; 859 native input tokens; accepted; 0 prose paragraphs.

```json
{
  "body": null,
  "score": null,
  "headline": null
}
```

Correct all-null abstention: historical sentiment is not reused as current evidence.

## transfer-player-83946-NFL

7 articles; 1925 native input tokens; rejected; 1 prose paragraphs.

```json
{
  "body": "Evan Neal has been signed by the Philadelphia Eagles to their practice squad, a move confirmed by multiple outlets including Yahoo Sports, NBC Sports, and the official Philadelphia Eagles organization. Reports from Yahoo Sports and the Eagles themselves state that Neal, along with Jaydon Blue and Anfernee Orji, has joined the practice squad. The New York Post adds context, noting that Neal was signed after being released by the Las Vegas Raiders and was previously a draft bust for the New York Giants. Heavy.com and another Yahoo Sports report describe the transaction as an ex-Giants player signing with an NFC East rival.",
  "findings": [
    																]
  }
```

Still rejected for expanding names. Longer single paragraph lists outlets and actors, but loosely pools confirmation. Not evidence that more words improve fidelity.

## transfer-team-37-FOOTBALL

9 articles; 1954 native input tokens; accepted; 5 prose paragraphs.

```json
{
  "body": "Roma is looking to make two signings in January, according to Yahoo Sports.\n\nRoma rejected three offers for Pisilli this summer, as reported by Yahoo Sports.\n\nRoma faces a crisis ahead of Napoli, with both Pellegrini and Svilar out, per La Gazzetta dello Sport.\n\nRoma will lose Pellegrini for at least 6-8 weeks with a serious injury, confirmed by LiveScore, Bulinews, and Football Italia.\n\nThuram deal, 3 Milan names, Roma-Bakola, ADL won't sell: today's news, as noted by OneFootball and Yahoo Sports.",
  "findings": []
}
```

Now emits five paragraphs and avoids the invented coaching role and two-player fee misassignment by omitting that story. Still includes injury-only reporting, wrongly credits Bulinews with the injury duration, and copies an unrelated roundup fragment. Accepted but not fully faithful.

## vibe-person-23-FOOTBALL

8 articles; 1785 native input tokens; accepted; 2 prose paragraphs.

```json
{
  "body": "Roberto De Zerbi expressed strong satisfaction with his team's performance, stating he was 'very happy' and praising Tottenham's resilience following a crucial victory over Coventry, as reported by Flashscore.co.za. He emphasized that the squad's 'attitude' and 'mentality' remained intact during the match, a sentiment echoed by VAVEL.com which highlighted his hope for these qualities to persist. De Zerbi also voiced delight at the team's ability to play with style, according to the Yellow Advertiser, while calling the return of player Kulusevski 'great news' for the upcoming fixture against Coventry, per Vodacom Soccer.\n\nThe manager specifically commended individual player contributions, noting he liked Mohammed Kudus' display against Coventry City, as quoted by GhanaSoccernet. Beyond the match result, De Zerbi credited the players' communication and a bonding trip for fostering a sense of family within the squad, a point made by The Guardian. However, Football London noted an ironic element to De Zerbi's post-match comments, suggesting a contrast between his expressed sentiments and the reality of the situation, though the specific irony remains unexplained in the headlines.",
  "score": 75,
  "headline": "De Zerbi expresses delight and satisfaction with Tottenham's resilience, style, and team bonding following a vital win over Coventry."
}
```

Two paragraphs correctly keep Kulusevski and Kudus separate and name publishers. Still turns hope into an asserted state and invents a contrast with reality from an unspecified irony headline. Positive score 75 is supported; narrative is not fully faithful.

## vibe-person-959-NFL

2 articles; 1321 native input tokens; accepted; 1 prose paragraphs.

```json
{
  "body": "The New York Times headline indicates that Jerry Jones expressed disappointment regarding the Cowboys' roster, a sentiment he hopes to address before the trade deadline. This reaction is attributed directly to Jones himself, as reported by The Athletic.",
  "score": 25,
  "headline": "Jerry Jones expresses disappointment over Cowboys roster, seeking improvements before trade deadline"
}
```

Negative score 25 remains supported. Drops the former-player criticism, so the reading is narrower than the preceding version; no claim that it covers all reactions.

## two-distinct-movement-stories

2 articles; 969 native input tokens; accepted; 2 prose paragraphs.

```json
{
  "body": "Cedar United have agreed a deal to sign Alex Reed, according to Example Wire.\n\nCedar United deny talks to sign Rowan Vale, according to Cedar Daily.",
  "findings": [
    {
      "counterparty": "Alex Reed",
      "evidence_quote": "Cedar United agree deal to sign Alex Reed",
      "report_index": 0,
      "stage": "here_we_go",
      "status": "reported"
    },
    {
      "counterparty": "Rowan Vale",
      "evidence_quote": "Cedar United deny talks to sign Rowan Vale",
      "report_index": 1,
      "stage": null,
      "status": "denied"
    }
  ]
}
```

Two separate sourced paragraphs and two exact findings preserve the agreement for Alex Reed and the denial for Rowan Vale. The stricter stage/source expectations were added to the canonical fixture after this call; separate saved-output assertions verify them without another generation.

## distinct-positive-and-negative-reactions

2 articles; 931 native input tokens; accepted; 3 prose paragraphs.

```json
{
  "body": "Example Wire reports that Cedar Comets supporters expressed delight regarding the stadium opening, highlighting a positive reaction from the fanbase to the venue's inauguration.\n\nIn contrast, Cedar Daily notes that fans have voiced anger and criticism specifically concerning a recent increase in ticket prices, indicating a negative sentiment focused on financial costs.\n\nThe coverage presents two distinct emotional threads: one celebrating the physical infrastructure of the team's home, and another expressing frustration over the affordability of access to that venue.",
  "score": 50,
  "headline": "Cedar Comets fans celebrate stadium opening but criticize ticket price hikes"
}
```

Separates positive stadium-opening and negative ticket-price reactions, then summarizes them. Mixed score 50 is supported. The third paragraph adds unnecessary interpretive language about infrastructure and affordability; no broader fan-consensus accuracy claim.
