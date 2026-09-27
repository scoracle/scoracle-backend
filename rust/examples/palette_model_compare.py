"""Compare local models on the finite Studio palette contract, without database writes."""

import json
import time
import urllib.request

ENDPOINT = "http://127.0.0.1:11435/api/generate"
MODELS = ["granite4.2:3b", "alibayram/smollm3"]
SYSTEM = "Arrange the plugin's approved statements. Return only the requested JSON choices. Your words are never published directly."
CASES = {
    "rich": [
        ("measure_0", ["Avery Chen ranks elite in Blocks per game: percentile 96.0 among 4470 eligible profiles.", "In Blocks per game, Avery Chen sits at percentile 96.0 among 4470 eligible profiles, an elite standing."]),
        ("measure_1", ["Avery Chen ranks strong in Defensive rebounds per game: percentile 81.0 among 4470 eligible profiles.", "In Defensive rebounds per game, Avery Chen sits at percentile 81.0 among 4470 eligible profiles, a strong standing."]),
        ("measure_2", ["Avery Chen ranks poor in Turnovers per game: percentile 22.0 among 4470 eligible profiles.", "In Turnovers per game, Avery Chen sits at percentile 22.0 among 4470 eligible profiles, a poor standing."]),
    ],
    "sparse": [
        ("measure_0", ["Noah Reed ranks poor in Goals per match: percentile 18.0.", "In Goals per match, Noah Reed sits at percentile 18.0, a poor standing."]),
    ],
    "paired": [
        ("measure_0", ["Mina Torres ranks strong in Assists per game: percentile 84.0 among 312 eligible profiles.", "In Assists per game, Mina Torres sits at percentile 84.0 among 312 eligible profiles, a strong standing."]),
        ("measure_1", ["Mina Torres ranks below average in Turnovers per game: percentile 39.0 among 312 eligible profiles.", "In Turnovers per game, Mina Torres sits at percentile 39.0 among 312 eligible profiles, a below average standing."]),
    ],
}
CASES["rich_with_form"] = CASES["rich"] + [
    ("recent_form", ["Recent form for Avery Chen: overall scores trending up over recent games; 5 scored events.",
                     "For Avery Chen, the recent-form record reads: overall scores trending up over recent games; 5 scored events."]),
]
CASES["oracle_five_cards"] = [
    ("reporting", ["For Northbridge FC, the leading reported storyline is A title challenge gathers.", "The current reporting around Northbridge FC is led by A title challenge gathers."]),
    ("rating", ["The performance profile for Northbridge FC has notability 82 and a rising trajectory.", "For Northbridge FC, the recorded rating is 82 in notability, with direction rising."]),
    ("vibe", ["The recorded mood for Northbridge FC is 72 out of 100.", "Around Northbridge FC, the mood card reads 72 out of 100."]),
    ("momentum", ["The recorded trajectory for Northbridge FC is rising.", "For Northbridge FC, recent momentum is rising."]),
    ("wire", ["The leading active wire names Vale United at heat 70.", "The active Vale United wire carries recorded heat 70."]),
    ("direction", ["The present direction for Northbridge FC is ascendant.", "Taken together, Northbridge FC is under the ascendant omen."]),
]
CASES["journalist_sources"] = [
    ("article_0", ["For Vale Kerr, BBC reports: Negotiations continued on Friday.", "BBC's report concerning Vale Kerr says: Negotiations continued on Friday."]),
    ("article_1", ["For Vale Kerr, AP reports: The club confirmed the meeting.", "AP's report concerning Vale Kerr says: The club confirmed the meeting."]),
    ("article_2", ["For Vale Kerr, Reuters reports: A decision is expected next week.", "Reuters's report concerning Vale Kerr says: A decision is expected next week."]),
]
CASES["insider_wire"] = [
    ("wire_0", ["The active wire lists Vale United as advanced talks (incoming), with heat 70.", "For Northbridge FC, the recorded Vale United wire is advanced talks (incoming) at heat 70."]),
    ("wire_1", ["The active wire lists Southport as speculation (outgoing), with heat 25.", "For Northbridge FC, the recorded Southport wire is speculation (outgoing) at heat 25."]),
]

def request(case, model):
    paints = CASES[case]
    prompt = "Choose one approved phrasing for each fact in order. Return only JSON with a choices array of zero-based phrasing indexes. The array must have one choice for each numbered fact.\n"
    for slot, (identity, phrasings) in enumerate(paints):
        prompt += f"\nFact {slot} ({identity}):\n"
        for index, phrase in enumerate(phrasings):
            prompt += f"  {index}: {phrase}\n"
    schema = {"type":"object","properties":{"choices":{"type":"array","minItems":len(paints),"maxItems":len(paints),"items":{"type":"integer","minimum":0}}},"required":["choices"],"additionalProperties":False}
    payload = {"model":model,"system":SYSTEM,"prompt":prompt,"stream":False,"think":False,"format":schema,"options":{"temperature":0,"num_ctx":4096,"num_predict":160}}
    started = time.monotonic()
    with urllib.request.urlopen(urllib.request.Request(ENDPOINT, json.dumps(payload).encode(), {"Content-Type":"application/json"}), timeout=180) as response:
        result = json.load(response)
    elapsed = time.monotonic() - started
    raw = result.get("response", "")
    try:
        choices = json.loads(raw)["choices"]
        valid = len(choices) == len(paints) and all(type(choice) is int and 0 <= choice < len(paints[index][1]) for index, choice in enumerate(choices))
        rendered = " ".join(paints[index][1][choice] for index, choice in enumerate(choices)) if valid else None
    except (KeyError, TypeError, ValueError, IndexError):
        valid, rendered = False, None
    return {"model":model,"case":case,"valid":valid,"seconds":round(elapsed, 3),"eval_count":result.get("eval_count"),"done_reason":result.get("done_reason"),"raw":raw,"rendered":rendered}

if __name__ == "__main__":
    for model in MODELS:
        for case in CASES:
            for repeat in range(3):
                print(json.dumps({"repeat": repeat, **request(case, model)}), flush=True)
