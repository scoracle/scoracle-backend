# Other-person context ablation: 15 fixed cases

[Full prompts and outputs side by side](SIDE-BY-SIDE.html) · [Complete Markdown transcript](FULL-PROMPTS-AND-OUTPUTS.md) · [Machine-readable comparison](comparison.json) · [Database lookup receipts](identity-lookup.json)

**30/30 model requests completed; zero transport, decoding or lookup errors. No instruction edits, deployment or production writes.** Both arms use current v17/v8 shared-task relationship rules and exactly the same native questions, evidence and target attributes. Only `target.named_people` differs. Questions and all remaining payload fields were checked for equality.

## Coverage

The existing canonical lookup provides other-person context for **6/15 cases**. The other nine have identical prompts and produced **exactly identical scores**, serving as controls. Exact target identity/name matches were excluded because this experiment tests other entities. The lookup supports people, not teams, and does not supply detailed person roles. We did not infer missing roles or affiliations or manufacture new context fields.

| Case | Added context |
|---|---|
| Hornets | LaMelo Ball, player; Hornets roster affiliation observed July 3, 2026 |
| Eagles | Jerry Jones, person; no role/affiliation supplied |
| Seahawks | Troy Aikman, person; no role/affiliation supplied |
| Miami | Giannis Antetokounmpo, player; Milwaukee roster affiliation observed July 3, 2026 |
| Buccaneers | Baker Mayfield, player; Buccaneers affiliation observed October 23, 2026 |
| Chelsea | Josh Acheampong, player; Chelsea affiliation observed July 3, 2026 |

“Verified” means matched canonical database identities and dated roster records from a read-only query. It does not mean independent confirmation of current affiliation. In particular, Giannis’s July Milwaukee record conflicts with the supplied headline’s Heat-teammate framing; the historical date is explicitly retained. Current lookup results are captured in `identity-lookup.json` alongside the exact SQL.

## Results

- **No improved routing decisions; one regression.** Chelsea movement falls **83.39 → 73.16**, crossing below Insider’s 80 cutoff despite explicit dated Chelsea affiliation. Its baseline without other-person context routes to Insider under the current shared-task rules.
- Seahawks subject falls **63.75 → 56.09**, directionally better for an incidental broadcast story but still above the 55 admission cutoff. “Troy Aikman (person)” does not tell the model he is a broadcaster.
- Eagles stays correctly rejected; adding only “Jerry Jones (person)” causes little score movement.
- Hornets, Miami and Buccaneers have no routing changes.
- Krejci remains a false Influencer route at **21.35 emotion** in both arms. No other-person context was found, so this case provides no evidence about the effect of adding relationships.
- Admission is **13/15 in both arms**: Eagles and Tatum are rejected. Thresholds are unchanged: subject 55, emotion 15, movement 80, availability 85.

This small trial does **not** establish that better entity context is unhelpful. It tests the existing compact lookup, including generic `person` labels, incomplete coverage and historical affiliations. It does not test richer verified roles such as broadcaster/owner/coach, other-team descriptions, or evidence-specific speaker/action roles. We cannot infer why the model changed scores from its native probability outputs.

## Method and limitations

Same 15 frozen targets (eight teams, seven players), exact d3 fingerprint, headlines, publishers and target records. One request per variant, sequential calls, no retries. Without-context arm ran first, followed by with-context arm. Nine identical controls had identical scores. Shared server timings are not a dedicated benchmark; no gold labels were supplied, so zero label mistakes in the runner is not an accuracy measure.

Both arms were rerun with current shared-task instructions, rather than comparing newly enriched inputs against an older instruction version. No new production entity-enrichment code was written. The build uses existing request assembly and response validation. Exact sources, input cases, native requests/responses, raw scores, routes and errors are retained. All database access used READ ONLY transactions. The temporary tunnel was closed after the replay.

## Per-case review

- **01-charlotte-hornets:** Dated LaMelo/Hornets affiliation causes small score changes and no routing change. This does not establish the reason he is absent.
- **02-philadelphia-eagles:** Jerry Jones is identified only as person. The incidental Eagles story remains correctly rejected in both arms; role and affiliation are still missing.
- **03-seattle-seahawks:** Troy Aikman is identified only as person. Relevance drops toward the cutoff but the broadcast story remains falsely admitted; broadcaster role is not supplied.
- **04-arsenal:** No other-person context found. Identical scores are a control result, not evidence for or against added relationships.
- **05-roma:** No Allegri match found. Identical scores; the relevant speaker’s role and relationship were not tested.
- **06-miami-heat:** Dated Milwaukee roster context conflicts with the headline’s Heat-teammate framing. Scores change modestly and routes do not. The test does not establish whether the model resolves that conflict correctly.
- **07-tampa-bay-buccaneers:** Baker’s dated Buccaneers affiliation produces no route change. Vague rare-feat framing still routes to Influencer.
- **08-chelsea:** Regression: dated Chelsea affiliation does not help the personnel-interest classification. Movement crosses below 80, dropping Insider.
- **09-luka-doncic:** No other named person resolved from this headline, which says Raptors Coach without naming the coach. Identical scores.
- **10-jayson-tatum:** No other-person context found. Historical-trade admission remains blocked in both arms; high movement remains masked by admission.
- **11-dak-prescott:** No Brian Schottenheimer match found. Identical scores; coach relationship context was not tested.
- **12-baker-mayfield:** No other-person context found after excluding the target. Opposing teams are not covered by this lookup. Identical scores.
- **13-virgil-van-dijk:** No other-person context found after excluding the target. Opposing teams are not covered by this lookup. Identical scores.
- **14-gessime-yassine:** No other-person context found after excluding the target. Identical scores.
- **15-vit-krejci:** No other-person context found after excluding Krejci himself. Emotion remains 21.35, above 15, so the false Influencer route persists; this pair does not test added context.
