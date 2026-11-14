# Task-first prompt samples

All eight entity-targeted prompts now use `task → target_context → target → evidence_context → evidence`. Tasks briefly describe the inputs and job; detailed writing rules remain in system instructions. Classifier relationship rules accompany its native scoring questions. No model calls or deployment were performed for this layout change.

## Classifier — exact current payloads

- [Player: Virgil van Dijk](classifier-player-payload.json) · [complete native request, including all four scoring questions](classifier-player-native-request.json)
- [Team: Tampa Bay Buccaneers](classifier-team-payload.json) · [complete native request, including all four scoring questions](classifier-team-native-request.json)

These are the exact ordered briefs rendered inside native `state`, exported from the same frozen inputs used in the 15-case trials. Classifier's legacy recorded background remains a string, including its JSON-like text. It has not been silently converted or simplified for display.

## Journalist — structured-attribute comparison

- [Player payload](journalist-player-payload.json) · [request including system rules](journalist-player-request.json)
- [Team payload](journalist-team-payload.json) · [request including system rules](journalist-team-request.json)

Journalist already uses structured recorded attributes. These samples show that existing representation in the same new top-level order. Historical experiment artifacts remain unchanged. Current quality snapshots were recaptured, and evidence readers retain compatibility with old `new_content` and `fresh` fields.
