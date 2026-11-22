# Classifier: 15 paired cases

Same headline, identity/background, criteria, routing and d3 model. Only the shared target description changes. Scores below are native 0–100 scores, not measured accuracy. No labels were supplied to the replay runner; its zero label mistakes does not establish correctness.

**30/30 requests completed. All 15 cases were admitted in both variants. One routing change: Eagles no longer routes to Influencer.**

Reviews below apply the frozen v13 criteria to supplied text; they are qualitative assessments, not independent ground-truth labels.

## 01-charlotte-hornets

**Target:** Charlotte Hornets (team, NBA)

**Headline:** Hornets blown out by Hawks 128-81 in first game home without LaMelo Ball, booed by sellout crowd

**Publisher:** AP News

| Score | Without context | With context |
|---|---:|---:|
| subject | 99.02 | 99.09 |
| emotion | 53.78 | 59.29 |
| movement | 6.14 | 7.93 |
| availability | 77.45 | 79.49 |

**Routes before:** influencer, journalist

**Routes after:** influencer, journalist

**Review:** Correct subject/emotion; absence of Ball plausibly supports availability.

**Request/response errors:** none.

## 02-philadelphia-eagles

**Target:** Philadelphia Eagles (team, NFL)

**Headline:** ‘Knows Who He Makes His Money From,’ ‘So Exhausting’ — NFL Fans React to Stephen A. Smith Sitting Next to Jerry Jones During Cowboys-Eagles

**Publisher:** Yahoo Sports

| Score | Without context | With context |
|---|---:|---:|
| subject | 90.45 | 91.90 |
| emotion | 16.04 | 14.06 |
| movement | 2.20 | 4.51 |
| availability | 5.97 | 6.98 |

**Routes before:** influencer, journalist

**Routes after:** journalist

**Review:** False-positive subject: reaction concerns Smith/Jones, not an Eagles development. Emotion routing improves (on to off).

**Request/response errors:** none.

## 03-seattle-seahawks

**Target:** Seattle Seahawks (team, NFL)

**Headline:** ‘Troy Aikman Looks Like He Hates Joe Buck,’ ‘I Can Feel the Tension’ — NFL Fans React to ESPN’s ‘Awkward’ Bears-Seahawks Broadcast

**Publisher:** Yahoo Sports

| Score | Without context | With context |
|---|---:|---:|
| subject | 95.58 | 95.55 |
| emotion | 45.94 | 41.41 |
| movement | 2.88 | 6.08 |
| availability | 8.70 | 14.23 |

**Routes before:** influencer, journalist

**Routes after:** influencer, journalist

**Review:** False-positive subject: broadcaster tension is not a Seahawks development. Remains admitted.

**Request/response errors:** none.

## 04-arsenal

**Target:** Arsenal (team, FOOTBALL)

**Headline:** ‘What you live for’: Fleetwood revel in seismic Arsenal shock after Dowman sees red

**Publisher:** The Guardian

| Score | Without context | With context |
|---|---:|---:|
| subject | 98.72 | 97.88 |
| emotion | 66.34 | 66.05 |
| movement | 65.27 | 67.11 |
| availability | 67.57 | 67.54 |

**Routes before:** influencer, journalist

**Routes after:** influencer, journalist

**Review:** Suspension-related participation is plausible from red card; movement score is a false positive for a match result.

**Request/response errors:** none.

## 05-roma

**Target:** Roma (team, FOOTBALL)

**Headline:** Allegri enraged after Napoli 3-3 Roma: ‘Maybe I shouldn’t be on the touchline’

**Publisher:** Yahoo Sports

| Score | Without context | With context |
|---|---:|---:|
| subject | 98.14 | 97.99 |
| emotion | 50.84 | 55.34 |
| movement | 17.19 | 20.08 |
| availability | 52.64 | 58.27 |

**Routes before:** influencer, journalist

**Routes after:** influencer, journalist

**Review:** Opponent coach emotion should not become Roma emotion; no participation issue is stated.

**Request/response errors:** none.

## 06-miami-heat

**Target:** Miami Heat (team, NBA)

**Headline:** Giannis Antetokounmpo puts Miami Heat teammates on blast after heavy LA Lakers loss

**Publisher:** NBA Analysis Network

| Score | Without context | With context |
|---|---:|---:|
| subject | 98.70 | 98.41 |
| emotion | 53.40 | 50.27 |
| movement | 27.26 | 36.07 |
| availability | 63.23 | 66.32 |

**Routes before:** influencer, journalist

**Routes after:** influencer, journalist

**Review:** Teammate criticism concerns Miami, but no participation issue is stated.

**Request/response errors:** none.

## 07-tampa-bay-buccaneers

**Target:** Tampa Bay Buccaneers (team, NFL)

**Headline:** Baker Mayfield, Buccaneers accomplish rare feat not seen since 1974

**Publisher:** Yahoo Sports

| Score | Without context | With context |
|---|---:|---:|
| subject | 98.08 | 97.27 |
| emotion | 62.35 | 58.60 |
| movement | 24.81 | 26.81 |
| availability | 40.39 | 38.90 |

**Routes before:** influencer, journalist

**Routes after:** influencer, journalist

**Review:** Concrete result/record is relevant; remarkable/rare feat alone does not establish explicit emotion.

**Request/response errors:** none.

## 08-chelsea

**Target:** Chelsea (team, FOOTBALL)

**Headline:** Chelsea sweating over Josh Acheampong future as top clubs line up

**Publisher:** TribalFootball

| Score | Without context | With context |
|---|---:|---:|
| subject | 97.85 | 96.65 |
| emotion | 40.95 | 35.13 |
| movement | 92.13 | 90.58 |
| availability | 44.55 | 39.51 |

**Routes before:** influencer, insider, journalist

**Routes after:** influencer, insider, journalist

**Review:** Contract interest is relevant movement; uncertainty alone does not establish explicit emotion.

**Request/response errors:** none.

## 09-luka-doncic

**Target:** Luka Doncic (player, NBA)

**Headline:** Raptors Coach Praised Luka Doncic For Destroying His Game Plan

**Publisher:** Heavy.com

| Score | Without context | With context |
|---|---:|---:|
| subject | 99.37 | 98.63 |
| emotion | 95.48 | 92.26 |
| movement | 6.59 | 9.36 |
| availability | 13.21 | 13.32 |

**Routes before:** influencer, journalist

**Routes after:** influencer, journalist

**Review:** Correct subject and explicit praise.

**Request/response errors:** none.

## 10-jayson-tatum

**Target:** Jayson Tatum (player, NBA)

**Headline:** Nets still haunted by 2013 Celtics trade that led to Jayson Tatum

**Publisher:** Yahoo Sports

| Score | Without context | With context |
|---|---:|---:|
| subject | 79.58 | 82.14 |
| emotion | 21.75 | 21.81 |
| movement | 85.48 | 87.63 |
| availability | 5.03 | 7.19 |

**Routes before:** influencer, insider, journalist

**Routes after:** influencer, insider, journalist

**Review:** Historical 2013 trade retrospective has no new Tatum development; subject and movement remain false positives.

**Request/response errors:** none.

## 11-dak-prescott

**Target:** Dak Prescott (player, NFL)

**Headline:** ‘Shut Up,’ ‘Stop Taking Bullets for Him’ — Cowboys Fans Livid as Dak Prescott Shields Brian Schottenheimer After OT Loss vs. Cardinals

**Publisher:** Yahoo Sports

| Score | Without context | With context |
|---|---:|---:|
| subject | 99.67 | 99.69 |
| emotion | 71.71 | 68.54 |
| movement | 6.70 | 11.52 |
| availability | 37.77 | 42.02 |

**Routes before:** influencer, journalist

**Routes after:** influencer, journalist

**Review:** Correct subject and anger involving Prescott; no participation issue is stated.

**Request/response errors:** none.

## 12-baker-mayfield

**Target:** Baker Mayfield (player, NFL)

**Headline:** Falcons Match 1974 Infamy in Horrific No-Pass Baker Mayfield Loss

**Publisher:** Yahoo Sports

| Score | Without context | With context |
|---|---:|---:|
| subject | 97.04 | 97.18 |
| emotion | 54.59 | 56.52 |
| movement | 28.75 | 49.35 |
| availability | 74.93 | 81.56 |

**Routes before:** influencer, journalist

**Routes after:** influencer, journalist

**Review:** Headline describes Falcons loss involving Mayfield; no injury/absence or transfer stated. Availability rises 74.9 to 81.6; movement rises 28.7 to 49.3.

**Request/response errors:** none.

## 13-virgil-van-dijk

**Target:** Virgil van Dijk (player, FOOTBALL)

**Headline:** 'Very disappointed' - Virgil van Dijk's reaction to Liverpool's defeat by Arsenal

**Publisher:** Liverpool FC

| Score | Without context | With context |
|---|---:|---:|
| subject | 98.62 | 98.22 |
| emotion | 80.99 | 75.50 |
| movement | 1.20 | 2.50 |
| availability | 28.30 | 26.20 |

**Routes before:** influencer, journalist

**Routes after:** influencer, journalist

**Review:** Correct direct subject emotion, though score declines.

**Request/response errors:** none.

## 14-gessime-yassine

**Target:** Gessime Yassine (player, FOOTBALL)

**Headline:** Gessime Yassine dominates in Strasbourg's lopsided Ligue 1 victory

**Publisher:** Pan Africa Football

| Score | Without context | With context |
|---|---:|---:|
| subject | 99.06 | 99.11 |
| emotion | 72.50 | 72.03 |
| movement | 1.70 | 2.96 |
| availability | 12.11 | 17.15 |

**Routes before:** influencer, journalist

**Routes after:** influencer, journalist

**Review:** Performance judgment plausibly supports emotion; direct subject remains correct.

**Request/response errors:** none.

## 15-vit-krejci

**Target:** Vit Krejci (player, NBA)

**Headline:** Vit Krejci News: Season-high 24 minutes Saturday

**Publisher:** RotoWire

| Score | Without context | With context |
|---|---:|---:|
| subject | 99.17 | 99.07 |
| emotion | 21.79 | 21.39 |
| movement | 0.97 | 2.22 |
| availability | 13.36 | 25.33 |

**Routes before:** influencer, journalist

**Routes after:** influencer, journalist

**Review:** Playing-time result is relevant but does not itself report availability or emotion.

**Request/response errors:** none.

## Raw receipts

[With context](classifier-with-context.jsonl) · [Without context](classifier-without-context.jsonl) · [Inputs](classifier-cases.jsonl)

Each JSONL receipt includes the complete submitted state/questions and raw native model response. An initial tunnel attempt failed because local port 18081 was already occupied; the test used temporary port 28081 instead. No model requests were lost or retried due to that setup error.
