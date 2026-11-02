# Complete outputs and manual source review

Development cases, no held-out accuracy claim. Mechanical acceptance is not factual correctness. Shared-server timings are not throughput measurements.

## narratives-person-86-FOOTBALL

Current source uses the **final** arm for this character.

### baseline — accepted — 4596 input tokens

```json
{
  "narratives": [
    {
      "body": "OneFootball reports that Martin Demichelis praised Yan Diomande ahead of the Champions League match between Real Madrid and RB Leipzig at the Bernabeu. Yardbarker and Yahoo both headline that the RB Leipzig boss raves about Diomande before the clash.",
      "headline": "Martin Demichelis praises Yan Diomande ahead of Real Madrid clash",
      "report_keys": [
        "report_1",
        "report_2",
        "report_3"
      ]
    },
    {
      "body": "Goal.com reports that Martin Demichelis insists Franco Mastantuono is built for Real Madrid despite his loan spell at Fiorentina, noting that the player needs time.",
      "headline": "Martin Demichelis on Franco Mastantuono",
      "report_keys": [
        "report_4"
      ]
    }
  ]
}
```

Keeps the two distinct discussions separate and attributes each to its selected publishers. Repeats similar headlines, but no material actor error detected.

[Receipt](batch-1/baseline/narratives-person-86-FOOTBALL/01/result.json)

### candidate — accepted — 4232 input tokens

```json
{
  "narratives": [
    {
      "body": "OneFootball reports that RB Leipzig boss Martín Demichelis praised Yan Diomande ahead of the Champions League tie between Real Madrid and Leipzig at the Bernabéu. Yardbarker and Yahoo repeat this headline, noting Demichelis's high regard for Diomande before the match. Goal.com reports separately that Demichelis insists Franco Mastantuono is built for Real Madrid despite his loan spell at Fiorentina, stating he needs time to develop.",
      "headline": "Martín Demichelis: praise for Diomande and Mastantuono",
      "report_keys": [
        "report_1",
        "report_2",
        "report_3",
        "report_4"
      ]
    }
  ]
}
```

Regression: merges Diomande praise and Mastantuono discussion into one entry. Adds 'to develop' to 'needs time'; the single headline also calls both discussions praise. Parser acceptance misses this grouping error.

[Receipt](batch-1/candidate/narratives-person-86-FOOTBALL/01/result.json)

### final — accepted — 4247 input tokens

```json
{
  "narratives": [
    {
      "body": "OneFootball reports that RB Leipzig boss Martín Demichelis praised Yan Diomande ahead of the Champions League tie between Real Madrid and Leipzig at the Bernabéu. Yardbarker and Yahoo repeat this headline, noting Demichelis's high regard for Diomande before the match.",
      "headline": "Martín Demichelis praises Yan Diomande",
      "report_keys": [
        "report_1",
        "report_2",
        "report_3"
      ]
    },
    {
      "body": "Goal.com reports that Martín Demichelis insists Franco Mastantuono is built for Real Madrid despite his loan spell at Fiorentina, stating he needs time to develop.",
      "headline": "Martín Demichelis on Franco Mastantuono",
      "report_keys": [
        "report_4"
      ]
    }
  ]
}
```

Restores two separate discussions. Attribution remains attached to the correct reports; 'to develop' still expands 'needs time'.

[Receipt](final/narratives-person-86-FOOTBALL/01/result.json)

## narratives-player-37-NBA

Current source uses the **final** arm for this character.

### baseline — rejected — 5344 input tokens

```json
{
  "narratives": [
    {
      "body": "NBC Sports reports that Bradley Beal remains out against the Lakers due to a knee injury. NBC Sports previously ruled him out for the game against Sacramento, citing the same knee issue. ABC30 Fresno reports that the Clippers indicated Beal will wear the No. 3 jersey, a number previously worn by Chris Paul.",
      "headline": "Bradley Beal: injury and jersey number",
      "report_keys": [
        "report_1",
        "report_2",
        "report_3"
      ]
    }
  ]
}
```

Merges injury and jersey-number stories. Rejected by the unchanged numerical/source check involving Chris Paul and 3; the source does in fact mention his old No. 3, so that rejection alone is not a factual verdict.

[Receipt](batch-2/baseline/narratives-player-37-NBA/01/result.json)

### candidate — accepted — 4941 input tokens

```json
{
  "narratives": [
    {
      "body": "NBC Sports reports that Bradley Beal remains out against the Lakers due to a knee injury. NBC Sports previously ruled him out for the game against Sacramento. ABC30 Fresno reports that the Clippers indicated Beal will wear Chris Paul's old No. 3 jersey.",
      "headline": "Bradley Beal: injury and jersey",
      "report_keys": [
        "report_1",
        "report_2",
        "report_3"
      ]
    }
  ]
}
```

Still merges injury and jersey-number stories. Different phrasing passes the parser; this is not a quality improvement in topic separation.

[Receipt](batch-2/candidate/narratives-player-37-NBA/01/result.json)

### final — rejected — 4956 input tokens

```json
{
  "narratives": [
    {
      "body": "NBC Sports reports that Bradley Beal remains out against the Lakers due to a knee injury. NBC Sports previously ruled him out for the game against Sacramento, citing the same knee issue. ABC30 Fresno reports that the Clippers have indicated Beal will wear the No. 3 jersey, a number previously worn by Chris Paul.",
      "headline": "Bradley Beal: injury status and jersey number",
      "report_keys": [
        "report_1",
        "report_2",
        "report_3"
      ]
    }
  ]
}
```

Still merges injury and jersey stories. Rejected by the unchanged numerical/source check; no parser relaxation.

[Receipt](final/narratives-player-37-NBA/01/result.json)

## vibe-person-23-FOOTBALL

Current source uses the **candidate** arm for this character.

### baseline — accepted — 3766 input tokens

```json
{
  "score": 75,
  "headline": "De Zerbi expresses delight and hope over Spurs' resilience",
  "body": "The supplied coverage reports that Roberto De Zerbi feels delighted and hopeful regarding Tottenham's performance. Headlines indicate he praised the team's resilience, attitude, and mentality, while noting his own happiness and satisfaction with the players' display and bonding."
}
```

Keeps the expressed positive reactions attached to De Zerbi. Broadly summarizes several headlines; happiness about bonding is looser than the supplied attribution that he credited it.

[Receipt](batch-1/baseline/vibe-person-23-FOOTBALL/01/result.json)

### candidate — accepted — 3434 input tokens

```json
{
  "score": 75,
  "headline": "De Zerbi expresses delight and hope after Spurs' victory",
  "body": "Coverage reports that Roberto De Zerbi is delighted by Tottenham's win and hopes their positive attitude remains intact. He praised the team's resilience, described the squad as 'like a family,' and expressed happiness with the performance and Kudus's display."
}
```

Keeps De Zerbi as the speaker of positive reactions and preserves the quoted family description. Remains scoped to the supplied coverage, without inventing a fan-wide mood.

[Receipt](batch-1/candidate/vibe-person-23-FOOTBALL/01/result.json)

### final — accepted — 3447 input tokens

```json
{
  "score": 75,
  "headline": "De Zerbi expresses delight and hope after Spurs' victory",
  "body": "Coverage reports that Roberto De Zerbi is delighted by Tottenham's win and hopes their positive attitude remains intact. He praised the team's resilience, described the squad as 'like a family,' and expressed happiness with the performance and Kudus's display."
}
```

Same scoped positive reaction as the initial lean version; no material change detected.

[Receipt](final/vibe-person-23-FOOTBALL/01/result.json)

### followup — accepted — 3508 input tokens

```json
{
  "score": 75,
  "headline": "De Zerbi expresses delight and hope after Spurs' win",
  "body": "Coverage reports that Roberto De Zerbi is delighted by Tottenham's victory and hopes their positive attitude remains intact. He praised the team's resilience, described the squad as 'like a family,' and expressed happiness with the players' performance and bonding."
}
```

Keeps the target as speaker of positive reactions, but again loosens the bonding attribution into happiness with bonding. No clear benefit over the shorter initial guide.

[Receipt](followup/vibe-person-23-FOOTBALL/01/result.json)

## vibe-person-959-NFL

Current source uses the **candidate** arm for this character.

### baseline — accepted — 1957 input tokens

```json
{
  "score": 50,
  "headline": "Mixed reactions to Jerry Jones amid Cowboys struggles",
  "body": "Coverage presents conflicting views on Jerry Jones. Yahoo Sports headlines suggest he faces criticism from former players, while The New York Times headline frames him as 'disappointed' and focused on roster improvements. The supplied reports offer no consensus or unified emotional tone, only these distinct, unverified perspectives."
}
```

Keeps criticism directed at Jones, but pluralizes one former player and treats two negative reports as conflicting/mixed. Neither headline establishes disagreement between these reactions.

[Receipt](batch-2/baseline/vibe-person-959-NFL/01/result.json)

### candidate — accepted — 1625 input tokens

```json
{
  "score": 50,
  "headline": "Mixed reactions to Jerry Jones amid Cowboys struggles",
  "body": "Coverage presents conflicting views on Jerry Jones. One headline frames his comments as a 'swipe' amidst team struggles, while another quotes him as 'disappointed' and focused on roster improvements. The tone reflects uncertainty and disagreement regarding his leadership and the team's direction."
}
```

Regression: makes the swipe Jones's own comments, reversing the critic and target. Adds disagreement about leadership and direction; still mislabels the negative reports as mixed/conflicting.

[Receipt](batch-2/candidate/vibe-person-959-NFL/01/result.json)

### final — accepted — 1638 input tokens

```json
{
  "score": 50,
  "headline": "Mixed reactions to Jerry Jones amid Cowboys struggles",
  "body": "Coverage presents conflicting views on Jerry Jones. One headline frames his comments as a 'swipe' amidst team struggles, while another quotes him as 'disappointed' and focused on roster improvements. The tone reflects uncertainty and disagreement regarding his leadership and the team's direction."
}
```

Same regression: makes the criticism Jones's own comments and calls the reports conflicting/mixed. Added reminder does not fix it.

[Receipt](final/vibe-person-959-NFL/01/result.json)

### followup — accepted — 1699 input tokens

```json
{
  "score": 50,
  "headline": "Mixed reactions to Jerry Jones amid Cowboys struggles",
  "body": "Coverage presents conflicting views on Jerry Jones. One headline frames his comments as a 'swipe' at the owner amidst team struggles, while another quotes him as 'disappointed' and eager to improve the roster before the trade deadline. These opposing angles create a mixed tone regarding his current standing and attitude."
}
```

Still assigns the swipe to Jones, now describing it as his swipe at the owner. Continues to call the negative reports opposing/mixed. The added relationship paragraph does not repair actor ownership.

[Receipt](followup/vibe-person-959-NFL/01/result.json)

## rating-team-2-NBA

Current source uses the **candidate** arm for this character.

### baseline — accepted — 2036 input tokens

```json
{
  "body": "The Boston Celtics recorded 89 games played in the 2025 season. Their rebounding output of 46.4 total rebounds placed them at the 93.1st percentile in an elite band among 30 peers, a rise from the 72.4th percentile the prior season. Rim protection (5.0 blocks) sat at the 58.6th percentile in an average band, falling from the 79.3th percentile previously. Steals (7.0), foul drawing (18.8 free throws attempted), and playmaking (24.6 assists) all ranked in the poor band at the 3.4th, 0.0th, and 10.3th percentiles respectively, with assists and scoring declining from prior standings. Scoring (114.2 points) remained in the poor band at the 31.0th percentile, down from the 69.0th percentile. The team's composite score of 54.2 reflects this overall profile."
}
```

Preserves 89 target games versus 30 peers, per-measure amounts, bands and comparison directions. Long but no reversed measurement detected.

[Receipt](batch-1/baseline/rating-team-2-NBA/01/result.json)

### candidate — accepted — 1596 input tokens

```json
{
  "body": "The Boston Celtics recorded 89 games played in the 2025 season. Rebounding totaled 46.4 points, placing the team at the 93.1st percentile in an elite band among 30 peers, a rise from the 72.4th percentile last season. Rim protection totaled 5.0 blocks, ranking at the 58.6th percentile in an average band, a fall from the 79.3th percentile. Steals totaled 7.0, ranking at the 3.4th percentile in a poor band, unchanged from the prior season. Foul drawing totaled 18.8, ranking at the 0.0th percentile in a poor band, unchanged from the prior season. Playmaking totaled 24.6 assists, ranking at the 10.3th percentile in a poor band, a fall from the 31.0th percentile. Scoring totaled 114.2 points, ranking at the 31.0th percentile in a poor band, a fall from the 69.0th percentile. The composite score is 54.2. No availability data is supplied."
}
```

Regression: changes 46.4 total rebounds to '46.4 points'. Other supplied percentiles, bands and comparison directions survive. Adds an unnecessary no-availability statement; acceptance misses the wrong measurement unit.

[Receipt](batch-1/candidate/rating-team-2-NBA/01/result.json)

### final — accepted — 1498 input tokens

```json
{
  "body": "The Boston Celtics recorded 89 games played in the 2025 season. Rebounding totaled 46.4 points, ranking at the 93.1st percentile in the elite band among 30 peers, a rise from the 72.4th percentile prior. Rim Protection totaled 5.0 points, ranking at the 58.6th percentile in the average band among 30 peers, a fall from the 79.3th percentile prior. Steals totaled 7.0 points, ranking at the 3.4th percentile in the poor band among 30 peers, held steady from the 3.4th percentile prior. Foul Drawing totaled 18.8 points, ranking at the 0.0th percentile in the poor band among 30 peers, held steady from the 0.0th percentile prior. Playmaking totaled 24.6 points, ranking at the 10.3th percentile in the poor band among 30 peers, a fall from the 31.0th percentile prior. Scoring totaled 114.2 points, ranking at the 31.0th percentile in the poor band among 30 peers, a fall from the 69.0th percentile prior. The composite score is 54.2. No availability headlines are supplied."
}
```

Further regression: labels rebounds, blocks, steals, free throws and assists all as points. Correct ranks do not make this faithful; parser accepts the wrong units.

[Receipt](final/rating-team-2-NBA/01/result.json)

### followup — accepted — 1536 input tokens

```json
{
  "body": "The Boston Celtics recorded 89 games played in the 2025 season. Rebounding totaled 46.4 points, ranking at the 93.1st percentile in the elite band among 30 peers, a rise from the 72.4th percentile. Rim Protection totaled 5.0 points, ranking at the 58.6th percentile in the average band among 30 peers, a fall from the 79.3th percentile. Steals totaled 7.0 points, ranking at the 3.4th percentile in the poor band among 30 peers, held steady from the 3.4th percentile. Foul Drawing totaled 18.8 points, ranking at the 0.0th percentile in the poor band among 30 peers, held steady from the 0.0th percentile. Playmaking totaled 24.6 points, ranking at the 10.3th percentile in the poor band among 30 peers, a fall from the 31.0th percentile. Scoring totaled 114.2 points, ranking at the 31.0th percentile in the poor band among 30 peers, a fall from the 69.0th percentile. The composite score is 54.2. No availability headlines are supplied."
}
```

Still labels every measurement as points, despite an explicit units reminder. Rejected as a useful refinement on source review, although the mechanical parser accepts it.

[Receipt](followup/rating-team-2-NBA/01/result.json)

## rating-player-744-NFL

Current source uses the **candidate** arm for this character.

### baseline — accepted — 1890 input tokens

```json
{
  "body": "Sam LaPorta's 2026 profile shows elite relative standing in Air Yards Responsible (90.6th percentile) and strong Points Responsible For (89.4th percentile) within a cohort of 828 peers. These measurements are constrained by a thin sample of five appearances, with a minimum threshold of ten required for reliable comparison."
}
```

Preserves the supplied elite/strong bands, ranks, 828 peers and five-appearance limit.

[Receipt](batch-2/baseline/rating-player-744-NFL/01/result.json)

### candidate — accepted — 1439 input tokens

```json
{
  "body": "Sam LaPorta's 2026 profile shows 316.0 total yards responsible and 18.0 points responsible, ranking at the 90.6th and 89.4th percentiles respectively within an elite and strong band among 828 peers. These readings rest on a thin sample of five appearances, which falls below the minimum threshold of ten required for reliable comparison."
}
```

Preserves 316.0 yards and 18.0 points, their separate ranks/bands, 828 peers and the five-appearance limit. Does not relabel the player cohort as teams.

[Receipt](batch-2/candidate/rating-player-744-NFL/01/result.json)

### final — accepted — 1341 input tokens

```json
{
  "body": "Sam LaPorta's 2026 profile shows 316.0 air yards responsible and 18.0 points responsible for, placing the air yards measure at the 90.6th percentile in the elite band and the points measure at the 89.4th percentile in the strong band. Both readings rest on a thin-sample limit of five recorded appearances, which falls below the minimum threshold of ten required for cross-season comparison."
}
```

Keeps the separate values and ranks, 828 omitted rather than misstated, and five appearances. Narrows the minimum threshold to cross-season comparison without that scope being explicit in limit.

[Receipt](final/rating-player-744-NFL/01/result.json)

### followup — accepted — 1379 input tokens

```json
{
  "body": "Sam LaPorta's 2026 profile shows 316.0 yards responsible for and 18.0 points responsible for, placing the yards measure at the 90.6th percentile in the elite band and the points measure at the 89.4th percentile in the strong band. Both readings rest on a thin-sample limit of five appearances, which falls below the minimum threshold of ten required for cross-season comparison."
}
```

Preserves the two distinct values and ranks plus five appearances, but again unnecessarily narrows the threshold to cross-season comparison. No established benefit from more wording.

[Receipt](followup/rating-player-744-NFL/01/result.json)

## momentum-player-744-NFL

Current source uses the **candidate** arm for this character.

### baseline — accepted — 1522 input tokens

```json
{
  "blurb": "Sam LaPorta's 2026 performance metrics show elite-level distinctiveness despite a thin sample, while coverage reflects extreme frustration and a desire to leave the team. The strong performance profile and critical coverage tone differ; neither establishes that the other is wrong or explains a change over time."
}
```

Strengthens preparedness to get paid elsewhere into a desire to leave the team. Current performance/coverage contrast is otherwise supported.

[Receipt](batch-1/baseline/momentum-player-744-NFL/01/result.json)

### candidate — accepted — 1089 input tokens

```json
{
  "blurb": "Sam LaPorta's 2026 performance metrics show elite percentile rankings despite a thin sample of five appearances, while coverage reflects his frustration over stalled extension talks and a lack of team support. These observations describe distinct current states—statistical standing versus reported sentiment—without a dated comparison to establish a trend or causal link between the two."
}
```

Improvement: omits the unsupported desire to leave, preserves five appearances, contract frustration and the absence of a dated trend.

[Receipt](batch-1/candidate/momentum-player-744-NFL/01/result.json)

### final — accepted — 1056 input tokens

```json
{
  "blurb": "Sam LaPorta's 2026 performance metrics show elite percentile rankings despite a thin sample of five appearances, while coverage reflects his frustration over stalled extension talks and a lack of team support. These observations describe the current state of his production and reported sentiment separately; without dated comparisons, no trend or causal link between his performance and the coverage tone can be established."
}
```

Preserves the current contrast, five appearances, contract frustration and absence of dated change. Avoids the baseline's unsupported desire to leave.

[Receipt](final/momentum-player-744-NFL/01/result.json)

### followup — accepted — 1056 input tokens

```json
{
  "blurb": "Sam LaPorta's 2026 performance metrics show elite percentile rankings despite a thin sample of five appearances, while coverage reflects his frustration over stalled extension talks and a lack of team support. These observations describe the current state of his production and reported sentiment separately; without dated comparisons, no trend or causal link between his performance and the coverage tone can be established."
}
```

Same supported contrast as the preceding refinement; no new benefit.

[Receipt](followup/momentum-player-744-NFL/01/result.json)

## oracle-team-2-NBA

Current source uses the **candidate** arm for this character.

### baseline — accepted — 1483 input tokens

```json
{
  "reading": "The Boston Celtics present a fractured profile where elite defensive metrics and rebounding are overshadowed by poor performance in scoring, turnovers, and free throws. This internal dissonance creates a tension between defensive solidity and offensive inefficiency, suggesting a team that is structurally sound but functionally unbalanced. The available data, limited to a single season's aggregate, leaves the trajectory of this specific imbalance and the team's immediate future uncertain."
}
```

Reverses elite turnovers into poor performance and invents structural/functional diagnoses.

[Receipt](batch-1/baseline/oracle-team-2-NBA/01/result.json)

### candidate — accepted — 1002 input tokens

```json
{
  "reading": "The Boston Celtics present a fractured profile where elite defensive metrics clash with poor offensive output and turnover rates. This internal tension suggests a team capable of containing opponents but struggling to generate its own scoring, leaving the overall trajectory uncertain."
}
```

Still reverses elite turnovers into poor turnover rates. Broad defensive/offensive framing obscures the mixed measures and adds a capability diagnosis. Shorter is not more faithful here.

[Receipt](batch-1/candidate/oracle-team-2-NBA/01/result.json)

### final — accepted — 1025 input tokens

```json
{
  "reading": "The Boston Celtics present a fractured profile where elite defensive metrics clash with poor offensive output and turnover rates. This internal tension suggests a team capable of containing opponents but struggling to generate its own scoring, leaving the overall trajectory uncertain."
}
```

Still reverses elite turnovers into poor turnover rates, despite the new measure-standing reminder.

[Receipt](final/oracle-team-2-NBA/01/result.json)

### followup — accepted — 1071 input tokens

```json
{
  "reading": "The Boston Celtics present a picture of elite defensive and rebounding strength contrasted by significant weaknesses in turnover management and shooting efficiency. While their ability to limit opponents and secure boards remains a dominant feature, the team's poor performance in three-point shooting and high turnover rates create a meaningful tension that limits their overall ceiling."
}
```

Further regression: reverses both elite turnovers and elite three-point shooting into weaknesses, invents high turnover rates and a ceiling. This longer guide is not retained.

[Receipt](followup/oracle-team-2-NBA/01/result.json)

## oracle-player-744-NFL

Current source uses the **candidate** arm for this character.

### baseline — accepted — 1674 input tokens

```json
{
  "reading": "Sam LaPorta presents a striking divergence in the 2026 season, where elite percentile rankings derived from a thin sample of five appearances contrast sharply with his expressed frustration and willingness to seek payment elsewhere. This tension between high statistical output and off-field dissatisfaction suggests a complex disconnect between his on-field contributions and his sentiment toward the Detroit Lions, leaving the stability of his future uncertain."
}
```

Preserves the five-appearance limit and contract frustration, but embellishes the contrast into a complex disconnect and uncertain future stability.

[Receipt](batch-2/baseline/oracle-player-744-NFL/01/result.json)

### candidate — accepted — 1300 input tokens

```json
{
  "reading": "Sam LaPorta's available picture centers on elite percentile rankings derived from a thin sample of five appearances, which contrasts sharply with reported frustration regarding stalled contract negotiations. This divergence between measured performance and expressed sentiment creates a meaningful tension, yet the limited data prevents confirmation of his actual trajectory or the team's readiness to resolve the situation."
}
```

Preserves the measured/sentiment contrast and five appearances without claiming rising raw metrics. Adds unsupported speculation about the team's readiness to resolve the situation.

[Receipt](batch-2/candidate/oracle-player-744-NFL/01/result.json)

### final — accepted — 1323 input tokens

```json
{
  "reading": "Sam LaPorta's available picture centers on elite statistical performance within a thin sample of five appearances, yet this is sharply contrasted by reported frustration and a willingness to seek payment elsewhere. The divergence between his high percentile rankings and his expressed dissatisfaction with the team's support creates a meaningful tension, while the limited sample size and lack of broader context leave the overall trajectory and durability of this situation unknown."
}
```

Preserves five appearances, frustration and willingness to seek payment elsewhere. Broad trajectory/durability phrasing remains more expansive than necessary.

[Receipt](final/oracle-player-744-NFL/01/result.json)

### followup — accepted — 1369 input tokens

```json
{
  "reading": "Sam LaPorta's available picture centers on elite statistical standing within a thin sample of five appearances, yet this measured performance is set against a backdrop of expressed frustration and a willingness to seek payment elsewhere. The divergence between his high percentile rankings and his reported dissatisfaction creates a tension where on-field contributions do not yet resolve the uncertainty surrounding his future with the Detroit Lions."
}
```

Keeps the thin sample and expressed frustration, but embellishes the relationship by implying contributions could resolve uncertainty about the future. No clear benefit from more wording.

[Receipt](followup/oracle-player-744-NFL/01/result.json)

## transfer-team-37-FOOTBALL

Current source uses the **final** arm for this character.

### baseline — accepted — 4853 input tokens

```json
{
  "body": "Roma are looking to make two signings in January, according to Yahoo Sports. The club has rejected three offers for Pisilli this summer, and reports indicate a crisis ahead involving the departure of Pellegrini and Svilar. Additionally, Roma are reportedly linked to a €50m package for El Aynaoui, though the headline suggests Leipzig is likely to retain him.",
  "findings": []
}
```

Turns injury absences into departures and attaches the package deal too loosely to Roma/El Aynaoui; the source names Leipzig as retaining two players.

[Receipt](batch-1/baseline/transfer-team-37-FOOTBALL/01/result.json)

### candidate — accepted — 4434 input tokens

```json
{
  "body": "Roma are likely to retain Gruda and El Aynaoui from Brighton for a €50m package, according to Bulinews. The club is also looking to make two signings in January, while rejecting three offers for Pisilli this summer. Meanwhile, Roma have lost coach Pellegrini for at least six to eight weeks due to a serious injury, and the situation is described as a crisis ahead of Napoli.",
  "findings": []
}
```

Regression: substitutes Roma for Leipzig as the retaining club and puts both players at Brighton. Also invents coach as Pellegrini's role and includes unrelated injury coverage.

[Receipt](batch-1/candidate/transfer-team-37-FOOTBALL/01/result.json)

### final — accepted — 4372 input tokens

```json
{
  "body": "Roma are looking to make two signings in January, according to Yahoo Sports. The club has rejected three offers for Pisilli this summer and faces a crisis with Pellegrini and Svilar out due to injury. Reports also mention a €50m package involving Gruda and El Aynaoui, though no specific agreement with Roma is confirmed.",
  "findings": []
}
```

Avoids making Roma the retaining club and no longer invents coach as Pellegrini's role. Still includes unrelated injury coverage, extends injury to Svilar without explicit evidence, and omits Leipzig from the package story.

[Receipt](final/transfer-team-37-FOOTBALL/01/result.json)

## transfer-player-83946-NFL

Current source uses the **final** arm for this character.

### baseline — rejected — 3992 input tokens

```json
{
  "body": "Evan Neal, previously a draft bust for the New York Giants, has signed with the Philadelphia Eagles to join their practice squad. Multiple outlets confirm the Eagles have signed him as an offensive lineman, following his release by the Las Vegas Raiders.",
  "findings": []
}
```

Preserves Raiders release before Eagles signing, but adopts the publisher's draft-bust characterization. Rejected by the unchanged source/alias checks; no finding meets the full-name quotation contract.

[Receipt](batch-2/baseline/transfer-player-83946-NFL/01/result.json)

### candidate — rejected — 3569 input tokens

```json
{
  "body": "Evan Neal, formerly with the New York Giants, has signed with the Philadelphia Eagles to join their practice squad. Multiple outlets confirm the Eagles have officially signed him as an offensive lineman, following his release from the Las Vegas Raiders.",
  "findings": []
}
```

Improvement: removes the draft-bust characterization while preserving release then practice-squad signing. Still rejected by the existing New York Giants source/alias check; parser unchanged.

[Receipt](batch-2/candidate/transfer-player-83946-NFL/01/result.json)

### final — rejected — 3507 input tokens

```json
{
  "body": "Evan Neal, formerly with the New York Giants, has signed with the Philadelphia Eagles to join their practice squad. Multiple outlets confirm the Eagles have officially signed him as an offensive lineman, following his release from the Las Vegas Raiders.",
  "findings": []
}
```

Same cleaner signing/release summary without adopting draft-bust language. Existing source/alias parser still rejects it.

[Receipt](final/transfer-player-83946-NFL/01/result.json)

## momentum-synthetic-dated-divergence

Current source uses the **candidate** arm for this character.

### baseline — accepted — 1572 input tokens

```json
{
  "blurb": "Harbor's measured rebounding profile is strong while its coverage leans cautious, creating a tension between performance and mood. Statistical distinctiveness increased by eight points and coverage tone decreased by twelve points over the same two-observation window, indicating a divergence in how the team is perceived versus how it is performing."
}
```

Incorrectly claims a shared window, although statistical distinctiveness starts October 20 and coverage tone starts October 19. Also conflates distinctiveness with performance/perception.

[Receipt](batch-2/baseline/momentum-synthetic-dated-divergence/01/result.json)

### candidate — accepted — 1138 input tokens

```json
{
  "blurb": "Harbor's measured rebounding is strong while coverage leans cautious, and rating notability increased by eight points over a two-observation window while coverage tone decreased by twelve points. These distinct metrics describe separate aspects of the current picture; neither establishes that the other is wrong or explains a change over time."
}
```

Avoids explicitly saying the windows are identical, but omits both date ranges and exposes rating notability rather than its plain meaning. It does not establish faithful handling of separate windows.

[Receipt](batch-2/candidate/momentum-synthetic-dated-divergence/01/result.json)

### final — accepted — 1125 input tokens

```json
{
  "blurb": "Harbor's measured rebounding profile is strong while the supplied coverage leans cautious, creating a tension between performance and tone. Statistical distinctiveness increased by eight points and coverage tone decreased by twelve points over two-week windows, but these small samples do not establish a durable trend or explain the divergence."
}
```

Regression: invents two-week windows. The supplied starts are October 20 and October 19, both ending October 22. The added date reminder is ineffective.

[Receipt](final/momentum-synthetic-dated-divergence/01/result.json)

### followup — accepted — 1146 input tokens

```json
{
  "blurb": "Harbor's measured rebounding profile is strong while the supplied coverage leans cautious, creating a tension between performance and tone. A notability increase of 8 points and a coverage tone decrease of 12 points are observed over two separate windows in late October, but these small samples do not establish a durable trend or explain the divergence."
}
```

Correctly says two separate windows in late October and avoids the invented two-week duration, but omits both exact date ranges and uses notability instead of statistical distinctiveness. No evidence that the longer reminder reliably establishes window fidelity; the shorter initial variant is selected for both Analyst cases.

[Receipt](followup/momentum-synthetic-dated-divergence/01/result.json)
