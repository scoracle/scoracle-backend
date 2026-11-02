# Context guide: complete outputs and source review

Same evidence, schema and generation settings in all three arms. These are development cases; parser acceptance is not factual validation.

## narratives-person-86-FOOTBALL

### control — accepted

```json
{
  "narratives": [
    {
      "body": "OneFootball reports that Martin Demichelis praised Yan Diomande ahead of the Champions League match between Real Madrid and RB Leipzig at the Bernabeu. Yardbarker and Yahoo both headline that RB Leipzig boss Martin Demichelis raves about Yan Diomande before the clash with Real Madrid.",
      "headline": "Demichelis praises Diomande ahead of Madrid clash",
      "report_keys": [
        "report_1",
        "report_2",
        "report_3"
      ]
    },
    {
      "body": "Goal.com reports that Martin Demichelis insists Franco Mastantuono is built for Real Madrid despite his loan spell at Fiorentina, noting that the player needs time to develop.",
      "headline": "Demichelis: Mastantuono built for Real Madrid",
      "report_keys": [
        "report_4"
      ]
    }
  ]
}
```

Keeps two distinct discussions and the correct speakers and publishers. Some redundant attribution across duplicate headlines.

### initial — accepted

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
      "headline": "Martin Demichelis says Franco Mastantuono is built for Real Madrid",
      "report_keys": [
        "report_4"
      ]
    }
  ]
}
```

Preserves the same two stories and qualifications.

### refined — accepted

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

Preserves the same two stories; the Mastantuono headline stays a restrained topic label.

## narratives-player-37-NBA

### control — accepted

```json
{
  "narratives": [
    {
      "body": "NBC Sports reports that Bradley Beal remains out for the game against the Lakers due to a knee injury. NBC Sports previously ruled him out for the game against Sacramento, citing the same knee issue. ABC30 Fresno reports that the Clippers have indicated the number three jersey, formerly worn by Chris Paul, will be assigned to Bradley Beal.",
      "headline": "Bradley Beal: injury and jersey news",
      "report_keys": [
        "report_1",
        "report_2",
        "report_3"
      ]
    }
  ]
}
```

Combines knee absence and jersey-number news in one entry, contrary to distinct-story selection.

### initial — rejected

```json
{
  "narratives": [
    {
      "body": "NBC Sports reports that Bradley Beal remains out against the Lakers due to a knee injury. NBC Sports previously ruled him out for the game against Sacramento, citing the same knee issue. ABC30 Fresno reports that the Clippers have indicated Beal will wear the No. 3 jersey, a number previously associated with Chris Paul.",
      "headline": "Bradley Beal: injury and jersey number updates",
      "report_keys": [
        "report_1",
        "report_2",
        "report_3"
      ]
    }
  ]
}
```

Still bundles unrelated stories. Native number/subject guard rejects the Chris Paul/No. 3 construction; source does contain the old number, so guard rejection is not by itself a semantic verdict.

### refined — rejected

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

Same story bundling and number/subject guard rejection remain.

## vibe-person-23-FOOTBALL

### control — accepted

```json
{
  "score": 75,
  "headline": "De Zerbi expresses delight and hope over Spurs' resilience",
  "body": "The supplied coverage leans positive, reporting that Roberto De Zerbi is delighted and very happy following Tottenham's win, while also expressing hope that the team's attitude and mentality remain intact."
}
```

Supported delight and happiness, preserving hope about attitude and mentality.

### initial — accepted

```json
{
  "score": 75,
  "headline": "De Zerbi expresses delight and praise for Tottenham's resilience",
  "body": "The supplied coverage leans positive, reporting that Roberto De Zerbi is delighted with Tottenham's performance and praises the team's resilience, attitude, and bonding. Headlines describe him as 'very happy' and note his satisfaction with players' displays, while one report highlights his credit for the squad's improved talk and bonding."
}
```

Keeps positive tone but weakens the distinction between hope, praise and the report crediting a bonding trip for improvement.

### refined — accepted

```json
{
  "score": 75,
  "headline": "De Zerbi expresses delight and hope over Spurs' resilience",
  "body": "The supplied coverage reports that Roberto De Zerbi feels delighted and hopeful regarding Tottenham's performance. Headlines indicate he praised the team's resilience, attitude, and mentality, while noting his own happiness and satisfaction with the players' display and bonding."
}
```

Keeps the reported positive reactions but turns hope about attitude and mentality into praise of them.

## vibe-person-959-NFL

### control — accepted

```json
{
  "score": 25,
  "headline": "Jerry Jones faces criticism and disappointment",
  "body": "Yahoo Sports headlines frame Jerry Jones as a target of an ex-Cowboys star amid team struggles, while The New York Times reports him as 'disappointed' regarding roster improvements before the trade deadline. The coverage conveys a tone of concern and dissatisfaction without inventing broader consensus or fan reactions."
}
```

Correctly keeps the ex-player criticism aimed at Jones, but leaks instruction language into the output.

### initial — accepted

```json
{
  "score": 50,
  "headline": "Mixed reactions to Jerry Jones amid Cowboys struggles",
  "body": "Coverage presents conflicting views on Jerry Jones. One headline frames his criticism of the owner as a 'swipe' amidst team struggles, while another explicitly labels him 'disappointed' and notes his desire to improve the roster before the trade deadline. The tone reflects uncertainty and division regarding his current standing and future plans."
}
```

Misassigns criticism to Jones himself and invents a conflict/division between two negative reports.

### refined — accepted

```json
{
  "score": 50,
  "headline": "Mixed reactions to Jerry Jones amid Cowboys struggles",
  "body": "Coverage presents conflicting views on Jerry Jones. Yahoo Sports headlines suggest he faces criticism from former players, while The New York Times headline frames him as 'disappointed' and focused on roster improvements. The supplied reports offer no consensus or unified emotional tone, only these distinct, unverified perspectives."
}
```

Corrects the criticism direction but pluralizes one former star and still calls two negative reports conflicting/mixed.

## rating-team-2-NBA

### control — accepted

```json
{
  "body": "The Boston Celtics' 2025 season profile shows a dominant rebounding performance at the 93.1st percentile in the elite band, rising from the 72.4th percentile. This strength contrasts sharply with a 31.0th percentile scoring output in the poor band, which fell from the 69.0th percentile. Playmaking (10.3th percentile, poor band) and rim protection (58.6th percentile, average band) also declined from prior standings, while steals and foul drawing remained stagnant in the poor band. The composite score of 54.2 reflects this mixed profile. Readings are based on 89 games played."
}
```

Correct main ranks and supplied directions, with the 89-game sample; restrained interpretation still preferable to dominant/stagnant language.

### initial — rejected

```json
{
  "body": "The Boston Celtics recorded 46.4 total rebounds across 89 games, placing the team at the 93.1st percentile in the elite band, a marked improvement from the 72.4th percentile last season. In contrast, rim protection (blocks) sits at the 58.6th percentile in the average band, having declined from the 79.3th percentile. Playmaking, scoring, and foul drawing all rank in the poor band, with playmaking and scoring showing declines from prior standings while steals and foul drawing remained held at poor levels. The composite score of 54.2 reflects this mixed profile. Readings are based on 89 games."
}
```

Main ranks and directions survive, but phrasing 46.4 rebounds across 89 games can imply a total that the projection does not establish.

### refined — accepted

```json
{
  "body": "The Boston Celtics recorded 89 games played in the 2025 season. Their rebounding output of 46.4 total rebounds placed them at the 93.1st percentile in an elite band among 30 peers, a rise from the 72.4th percentile the prior season. Rim protection (5.0 blocks) sat at the 58.6th percentile in an average band, falling from the 79.3th percentile previously. Steals (7.0), foul drawing (18.8 free throws attempted), and playmaking (24.6 assists) all ranked in the poor band at the 3.4th, 0.0th, and 10.3th percentiles respectively, with assists and scoring declining from prior standings. Scoring (114.2 points) remained in the poor band at the 31.0th percentile, down from the 69.0th percentile. The team's composite score of 54.2 reflects this overall profile."
}
```

Keeps target sample separate from peer count, retains per-measure values, bands and directions. Longer than necessary; no wrong measurement detected.

## rating-player-744-NFL

### control — accepted

```json
{
  "body": "Sam LaPorta's 2026 season profile shows elite relative standing in air yards responsible (90.6th percentile) and strong relative standing in points responsible (89.4th percentile) within a cohort of 828 players. These measurements are based on only five recorded appearances, a thin sample that limits the reliability of the rankings. No prior season comparison is supplied."
}
```

Keeps 828 players, supplied ranks and five-appearance thin sample.

### initial — accepted

```json
{
  "body": "Sam LaPorta's 2026 profile shows elite relative standing in air yards responsible (90.6th percentile) and strong relative standing in points responsible (89.4th percentile) within a cohort of 828 teams. These measurements are based on only five recorded appearances, which falls below the ten-appearance threshold for a robust reading."
}
```

Incorrectly changes the comparison group to 828 teams despite a player target.

### refined — accepted

```json
{
  "body": "Sam LaPorta's 2026 profile shows elite relative standing in Air Yards Responsible (90.6th percentile) and strong Points Responsible For (89.4th percentile) within a cohort of 828 peers. These measurements are constrained by a thin sample of five appearances, with a minimum threshold of ten required for reliable comparison."
}
```

Uses 828 peers, preserves the player measurements and five-appearance limit. Threshold explanation concerns comparison, not a newly computed rank.

## momentum-player-744-NFL

### control — rejected

```json
{
  "blurb": "Sam LaPorta's 2026 performance metrics place him in the top 90th percentile among peers, yet his coverage reflects extreme frustration and a desire to leave the team. This creates a sharp tension between his elite statistical standing and his reported dissatisfaction with the organization."
}
```

Changes 91.7/90.8 percentiles to top 90th and strengthens preparedness to get paid elsewhere into desire to leave.

### initial — accepted

```json
{
  "blurb": "Sam LaPorta's 2026 performance metrics place him in the elite percentile band despite a thin sample, while coverage reflects his frustration with stalled extension talks and a lack of team support. With no dated comparisons provided, the relationship between his statistical distinctiveness and the tone of his coverage cannot be assessed over time."
}
```

Avoids the invented percentile and desire to leave; describes current contrast and lack of an observed time comparison.

### refined — accepted

```json
{
  "blurb": "Sam LaPorta's 2026 performance metrics show elite-level distinctiveness despite a thin sample, while coverage reflects extreme frustration and a desire to leave the team. The strong performance profile and critical coverage tone differ; neither establishes that the other is wrong or explains a change over time."
}
```

Again strengthens conditional preparedness into desire to leave, despite accurate current performance/coverage contrast.

## oracle-team-2-NBA

### control — accepted

```json
{
  "reading": "The Boston Celtics present a fractured profile where elite defensive metrics and offensive precision clash with a poor standardized composite score, suggesting a team that is technically proficient but strategically disjointed. This tension between specific strengths and a failing overall trajectory remains the only known reality, as the absence of fixture data prevents any conclusion about the causes of this decline or the stability of the roster."
}
```

Mislabels composite 54.2 as poor and invents strategic disjointedness.

### initial — accepted

```json
{
  "reading": "The Boston Celtics present a fractured defensive profile where elite shot prevention clashes with poor rebounding and turnover management, creating a defensive inconsistency that undermines their offensive strength. This internal tension between high-level perimeter defense and fundamental breakdowns suggests a team capable of scoring but struggling to contain opponents across the full spectrum of play."
}
```

Reverses elite rebounding and turnovers into poor measures and adds a defensive causal story.

### refined — accepted

```json
{
  "reading": "The Boston Celtics present a fractured profile where elite defensive metrics and rebounding are overshadowed by poor performance in scoring, turnovers, and free throws. This internal dissonance creates a tension between defensive solidity and offensive inefficiency, suggesting a team that is structurally sound but functionally unbalanced. The available data, limited to a single season's aggregate, leaves the trajectory of this specific imbalance and the team's immediate future uncertain."
}
```

Rebounding is restored but elite turnovers still become poor. Adds unsupported structural/functional diagnoses.

## oracle-player-744-NFL

### control — accepted

```json
{
  "reading": "Sam LaPorta presents a striking divergence in late 2026, where elite statistical performance coexists with palpable frustration over stalled contract talks. This tension between his rising on-field metrics and his expressed willingness to seek payment elsewhere suggests a disconnect between his contributions and his perceived standing with the Detroit Lions. However, the thin sample of five appearances limits the certainty of his trajectory, leaving the depth of his dissatisfaction and the ultimate resolution of his future uncertain."
}
```

Expands a supplied rising direction label into rising on-field metrics without dated underlying comparisons.

### initial — accepted

```json
{
  "reading": "Sam LaPorta presents a sharp divergence in the 2026 season, where elite percentile rankings derived from a thin sample of five appearances contrast with public expressions of extreme frustration and a willingness to seek payment elsewhere. This tension between his measured on-field contributions and his off-field sentiment toward the Detroit Lions highlights a significant disconnect, though the limited sample size and stalled extension talks leave the trajectory of his future uncertain."
}
```

Keeps elite relative ranks, five appearances and contract frustration, but its disconnect framing is more emphatic than necessary.

### refined — accepted

```json
{
  "reading": "Sam LaPorta presents a striking divergence in the 2026 season, where elite percentile rankings derived from a thin sample of five appearances contrast sharply with his expressed frustration and willingness to seek payment elsewhere. This tension between high statistical output and off-field dissatisfaction suggests a complex disconnect between his on-field contributions and his sentiment toward the Detroit Lions, leaving the stability of his future uncertain."
}
```

Keeps the core contrast and sample limit; complex disconnect and future stability are embellishments rather than useful evidence distinctions.

## transfer-team-37-FOOTBALL

### control — accepted

```json
{
  "body": "Roma are looking to make two signings in January, according to Yahoo Sports. The club has rejected three offers for Pisilli this summer, and reports indicate a crisis ahead involving the departure of Pellegrini and Svilar.",
  "findings": []
}
```

Turns reported injury absences of Pellegrini/Svilar into departures.

### initial — accepted

```json
{
  "body": "Roma are looking to make two signings in January, according to Yahoo Sports. The club has rejected three offers for Pisilli this summer, and reports indicate Pellegrini is out for at least six to eight weeks due to a serious injury.",
  "findings": []
}
```

Correctly calls the Pellegrini absence an injury, though including it is outside the movement brief.

### refined — accepted

```json
{
  "body": "Roma are looking to make two signings in January, according to Yahoo Sports. The club has rejected three offers for Pisilli this summer, and reports indicate a crisis ahead involving the departure of Pellegrini and Svilar. Additionally, Roma are reportedly linked to a €50m package for El Aynaoui, though the headline suggests Leipzig is likely to retain him.",
  "findings": []
}
```

Departure error returns; also attaches the 50m package too loosely to Roma/El Aynaoui although the source describes Leipzig retaining two players.

## transfer-player-83946-NFL

### control — rejected

```json
{
  "body": "Evan Neal, previously a draft bust for the New York Giants, has signed with the Philadelphia Eagles to join their practice squad. Multiple outlets confirm the Eagles have officially signed him, adding him alongside Jaydon Blue and Anfernee Orji.",
  "findings": []
}
```

Correct signing/practice-squad story but adopts publisher draft-bust characterization as its own.

### initial — rejected

```json
{
  "body": "Evan Neal, previously a New York Giants draft bust, has signed with the Philadelphia Eagles to join their practice squad. Multiple outlets confirm the Eagles have signed him as an offensive lineman, following his release by the Las Vegas Raiders.",
  "findings": []
}
```

Preserves Raiders release before Eagles signing, but still adopts draft-bust characterization without attribution.

### refined — rejected

```json
{
  "body": "Evan Neal, previously a draft bust for the New York Giants, has signed with the Philadelphia Eagles to join their practice squad. Multiple outlets confirm the Eagles have signed him as an offensive lineman, noting the move follows his release by the Las Vegas Raiders.",
  "findings": []
}
```

Same issue; source/alias parser also rejects New York Giants. No findings qualify in any arm under the full-name quote contract.

## momentum-synthetic-dated-divergence

### control — accepted

```json
{
  "blurb": "Harbor's measured rebounding profile is strong while the supplied coverage leans cautious, creating a tension between performance and mood. A rating_notability increase of 8 points and a coverage_tone decrease of 12 points over the same window indicate diverging trends in distinctiveness and tone, though neither proves a causal link between the two."
}
```

Calls different start dates the same window and prints internal measure names.

### initial — accepted

```json
{
  "blurb": "Harbor's measured rebounding profile is strong while its coverage leans cautious, creating a tension between performance and mood. Statistical distinctiveness increased by eight points over the last two days, whereas coverage tone decreased by twelve points in a similar window; neither change proves a causal link between the two."
}
```

Uses ordinary measure meanings, but last two days is loosely anchored and a similar window hides the exact separate date ranges.

### refined — accepted

```json
{
  "blurb": "Harbor's measured rebounding profile is strong while its coverage leans cautious, creating a tension between performance and mood. Statistical distinctiveness increased by eight points and coverage tone decreased by twelve points over the same two-observation window, indicating a divergence in how the team is perceived versus how it is performing."
}
```

Again claims a shared two-observation window and conflates statistical distinctiveness with performance. The windows have different start dates.
