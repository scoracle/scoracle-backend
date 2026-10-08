import unittest
import json

from classifier_replay import SCHEMA_PATH, head_labels, model_text, source_windows, validate_head
from classifier_review import prepare, training_rows, validate


class Coverage(unittest.TestCase):
    def test_source_selection_binds_exact_repeated_unicode_and_rejects_drift(self):
        from classifier_infer import bind_selection, source_units
        body = 'Équipe won. Équipe won. But not today.'
        units = source_units(body)
        self.assertEqual(''.join(units), body)
        claim = {'evidence': {'first': 1, 'last': 1}, 'target_evidence': None,
                 'qualifiers': {'speaker': [{'first': 0, 'last': 1}]}}
        raw = json.dumps({'claims': [claim]})
        bound = json.loads(bind_selection(raw, body, units))['claims'][0]
        self.assertEqual(bound['evidence'], {'quote': 'Équipe won. ', 'occurrence': 1})
        self.assertEqual(bound['qualifiers']['speaker'][0]['quote'], 'Équipe won. Équipe won. ')
        with self.assertRaisesRegex(ValueError, 'drift'):
            bind_selection(raw, body + 'changed', units)
        for selection in ({'first': 2, 'last': 1}, {'first': -1, 'last': 0},
                          {'first': 0, 'last': len(units)}, {'first': True, 'last': 1},
                          {'first': 0, 'last': 0, 'quote': 'invented'}):
            claim['evidence'] = selection
            with self.assertRaises(ValueError):
                bind_selection(json.dumps({'claims': [claim]}), body, units)

    def test_decision_reference_keeps_time_and_failures_separate(self):
        from classifier_decision_accuracy import expected, scores
        from classifier_gliner import quote, source_coverage
        ref = {"checks": [{"relation": "direct_subject", "kind": ["emotion"], "time": ["historical"]},
                          {"relation": "other_subject", "kind": ["emotion"], "time": ["current"]}]}
        truth = expected(ref)
        self.assertTrue(truth["historical_emotion"])
        self.assertFalse(truth["current_emotion"])
        self.assertEqual(scores({"status": "error"}), ({}, None))
        self.assertEqual(quote("éé Alex Alex", {"start": 8, "end": 12, "text": "Alex"}),
                         {"quote": "Alex", "occurrence": 1})
        with self.assertRaises(ValueError):
            source_coverage("Complete late denial.", "Complete")
        with self.assertRaises(ValueError):
            quote("Alex", {"start": 0, "end": 5, "text": "Alex."})

    def test_model_bound_qualification_refuses_overflow_and_incomplete_generation(self):
        from classifier_infer import budget, complete, proposal_schema, request_hash
        from classifier_replay import laya_questions
        budget([1, 2], 3, 5)
        with self.assertRaisesRegex(ValueError, "exceeds context"):
            budget([1, 2, 3], 3, 5)
        for response in ({"stop": False, "stop_type": "eos", "truncated": False},
                         {"stop": True, "stop_type": "limit", "truncated": False},
                         {"stop": True, "stop_type": "eos", "truncated": True}, {}):
            with self.assertRaisesRegex(ValueError, "incomplete generation"):
                complete(response)
        self.assertEqual(complete({"stop": True, "stop_type": "eos", "truncated": False, "content": "{}"}), "{}")
        self.assertEqual(request_hash({"é": "value", "b": 1}), request_hash({"b": 1, "é": "value"}))
        schema = json.loads(SCHEMA_PATH.read_text())
        labels = schema["vectors"]["emotion"]["labels"]
        for form in ("noul", "choice"):
            bank = laya_questions(labels, form)
            self.assertEqual(set(bank), set(labels))
            self.assertIn("no expressed emotion", bank["neutral"]["instructions"])
            self.assertEqual(bank["neutral"]["type"], form)
        request = {"output_contract": {"emotion_labels": labels, "claims": [{
            "target_relation": ["unknown"], "kind": ["information"], "time_scope": ["unknown"],
            "qualifiers": {key: None for key in schema["qualifiers"]}}]}}
        form = proposal_schema(request)
        qualifiers = form["properties"]["claims"]["items"]["properties"]["qualifiers"]
        self.assertEqual(set(qualifiers["required"]), set(schema["qualifiers"]))
        emotion_enum = form["properties"]["claims"]["items"]["properties"]["candidate_dimensions"]["items"]["enum"]
        self.assertEqual(emotion_enum, list(labels))
        selected = proposal_schema(request, 3)['properties']['claims']['items']['properties']['evidence']
        self.assertEqual(selected['required'], ['first', 'last'])
        self.assertEqual(selected['properties']['last']['maximum'], 2)

    def test_accuracy_requires_the_frozen_relationship_and_qualifiers(self):
        from classifier_accuracy import matches
        check = {"start": 10, "end": 30, "relation": "other_subject", "kind": ["emotion"],
                 "time": ["historical"], "speaker": "Mira", "qualifier": "reported_event_time"}
        claim = {"evidence": {"start": 10, "end": 30}, "target_relation": "other_subject",
                 "kind": "emotion", "time_scope": "historical", "qualifiers": {
                     "speaker": [{"quote": "Mira"}], "reported_event_time": [{"quote": "Last year"}]}}
        self.assertTrue(matches(claim, check))
        claim["target_relation"] = "direct_subject"
        self.assertFalse(matches(claim, check))
        claim["target_relation"] = "other_subject"
        claim["qualifiers"]["reported_event_time"] = None
        self.assertFalse(matches(claim, check))

    def test_exact_unicode_bytes_and_missing_qualification(self):
        text = "Équipe won.\n\nBut not today."
        encoded = [{"start": 0, "end": 12, "text": "Équipe won."},
                   {"start": 14, "end": 28, "text": "But not today."}]
        windows = source_windows(text, encoded)
        for window in windows:
            self.assertEqual(text.encode()[window["start"]:window["end"]].decode(), window["text"])
        encoded.pop()
        with self.assertRaisesRegex(ValueError, "complete source"):
            source_windows(text, encoded)

    def test_review_and_head_cannot_relabel_or_invent_evidence(self):
        schema = json.loads(SCHEMA_PATH.read_text())
        topic = head_labels(schema, ["relevance", "topic"])
        with self.assertRaisesRegex(ValueError, "trained head"):
            validate_head(list(schema["vectors"]["emotion"]["labels"]), topic)
        with self.assertRaisesRegex(ValueError, "input scopes"):
            head_labels(schema, ["emotion", "topic"])
        target = {"name": "Équipe", "sport": "FOOTBALL", "entity_type": "team", "entity_id": 1}
        text = "Équipe won."
        item = {"article_id": 1, "body": text, "query_entities": [target],
                "windows": [{"start": 0, "end": len(text.encode()), "text": text}]}
        with self.assertRaisesRegex(ValueError, "canonical query target"):
            model_text(item, text, "target_window")
        record = prepare(item, schema)
        self.assertEqual(validate(record, item, schema), 0)
        unit = record["units"][1]
        self.assertIn(text, unit["model_text"])
        unit["labels"]["topic.match_event"] = 1
        with self.assertRaisesRegex(ValueError, "pending row"):
            validate(record, item, schema)
        unit["reviewer"] = "synthetic contract check"
        unit["review_status"] = "reviewed"
        with self.assertRaisesRegex(ValueError, "supporting evidence"):
            validate(record, item, schema)
        unit["evidence"] = [{"label": "topic.match_event", "start": 0, "end": len(text.encode()),
                             "quote": text, "qualifiers": {key: None for key in schema["qualifiers"]}}]
        self.assertEqual(validate(record, item, schema), 1)
        unit["evidence"][0]["quote"] = "An invented match result"
        with self.assertRaisesRegex(ValueError, "exact model-visible"):
            validate(record, item, schema)
        unit["evidence"][0]["quote"] = text
        unit["ordinal_annotations"]["affect.valence"] = 101
        with self.assertRaisesRegex(ValueError, "declared scale"):
            validate(record, item, schema)
        unit["ordinal_annotations"]["affect.valence"] = 50
        with self.assertRaisesRegex(ValueError, "ordinal annotation lacks"):
            validate(record, item, schema)
        unit["ordinal_annotations"]["affect.valence"] = None
        unit["review_status"] = "ai_provisional"
        unit["reviewer"] = "codex-ai-provisional"
        self.assertEqual(validate(record, item, schema), 1)
        record["split"] = "train"
        record["syndication_group"] = "synthetic-result"
        with self.assertRaisesRegex(ValueError, "AI labels cannot enter"):
            validate(record, item, schema)
        unit["review_status"] = "adjudicated"
        unit["adjudicator"] = unit["reviewer"]
        with self.assertRaisesRegex(ValueError, "independent adjudicator"):
            validate(record, item, schema)
        unit["reviewer"] = "synthetic contract check"
        unit["adjudicator"] = "separate synthetic contract check"
        validate(record, item, schema)
        with self.assertRaisesRegex(ValueError, "usable extraction"):
            training_rows([record], schema, ["topic"])
        record["extraction_review"].update(usable=True, reviewer="synthetic extraction check")
        dataset = training_rows([record], schema, ["topic"])
        self.assertEqual(len(dataset["train"]), 1)
        self.assertEqual(dataset["train"][0]["labels"][0], 1)
        self.assertIsNone(dataset["train"][0]["labels"][1])

    def test_articulation_world_keeps_spectrum_separate_from_claims(self):
        from classifier_articulation import world, decode_reply
        from classifier_replay import digest
        schema = json.loads(SCHEMA_PATH.read_text())
        text = 'Cedar player Alex said, "I am not relieved."'
        item = {"article_id": -99, "source": "synthetic check", "body": text,
                "query_entities": [{"name": "Cedar Club"}]}
        measurement = {"article_id": -99, "status": "measured", "body_sha256": digest(text),
                       "coverage": {"truncated": False}, "provenance": {"model": "check", "revision": "check-v1"},
                       "windows": [{"start": 0, "end": len(text.encode()), "text": text,
                                    "scores": {"emotion": dict.fromkeys(schema["vectors"]["emotion"]["labels"], 0.1)}}]}
        packet = world(item, measurement, True)
        self.assertEqual(packet["FRESH EVIDENCE"][0]["publisher_text"], text)
        self.assertFalse(packet["CLASSIFIER MEASUREMENTS"]["calibrated"])
        self.assertEqual(len(packet["CLASSIFIER MEASUREMENTS"]["windows"][0]["emotion"]), 28)
        self.assertIn("target relevance", packet["CLASSIFIER MEASUREMENTS"]["unknown"])
        self.assertNotIn("CLASSIFIER MEASUREMENTS", world(item, measurement, False))
        measurement["body_sha256"] = "changed"
        with self.assertRaisesRegex(ValueError, "source-bound"):
            world(item, measurement, True)
        reply = {"done": True, "done_reason": "stop", "message": {"content": '{"headline":null,"body":null}'}}
        self.assertIsNone(decode_reply(reply)["body"])
        reply["message"]["content"] = '{"headline":"Invented","body":null}'
        with self.assertRaisesRegex(ValueError, "inconsistent abstention"):
            decode_reply(reply)
        reply.update(done_reason="length", message={"content": '{"headline":'})
        with self.assertRaisesRegex(ValueError, "incomplete generation"):
            decode_reply(reply)

    def test_unknown_training_labels_have_no_gradient(self):
        try:
            import torch
        except ImportError:
            self.skipTest("uses the existing Archbox torch runtime")
        from classifier_train import masked_loss
        logits = torch.tensor([[0.0, 9.0, 0.0]], requires_grad=True)
        loss = masked_loss(torch, logits, [[1, None, 0]])
        loss.backward()
        self.assertAlmostEqual(loss.item(), 0.693147, places=5)
        self.assertLess(logits.grad[0, 0].item(), 0)
        self.assertEqual(logits.grad[0, 1].item(), 0)
        self.assertGreater(logits.grad[0, 2].item(), 0)
        with self.assertRaisesRegex(ValueError, "no known labels"):
            masked_loss(torch, logits, [[None, None, None]])

    def test_qualified_claims_keep_target_time_denial_and_source_bindings(self):
        from copy import deepcopy
        from classifier_articulation import qualified_world, expression_world
        from classifier_replay import digest
        schema = json.loads(SCHEMA_PATH.read_text())
        controls = {item["article_id"]: item for item in map(json.loads, SCHEMA_PATH.with_name("controls.jsonl").read_text().splitlines())}
        records = list(map(json.loads, SCHEMA_PATH.with_name("qualified-claims-controls.jsonl").read_text().splitlines()))
        packets = {}
        for record in records:
            item = controls[record["article_id"]]
            measurement = {"article_id": item["article_id"], "status": "measured", "body_sha256": digest(item["body"]),
                           "coverage": {"truncated": False}, "provenance": {"model": "check", "revision": "check-v1"},
                           "windows": [{"start": 0, "end": len(item["body"].encode()), "text": item["body"],
                                        "scores": {"emotion": dict.fromkeys(schema["vectors"]["emotion"]["labels"], 0.1)}}]}
            packets[item["article_id"]] = qualified_world(item, measurement, record)
            self.assertNotIn("publisher_text", packets[item["article_id"]]["FRESH EVIDENCE"][0])
            reader = expression_world(packets[item["article_id"]])
            self.assertNotIn("CLASSIFIER MEASUREMENTS", reader)
            self.assertNotIn("CLAIM REVIEW", reader)
            for claim in reader["QUALIFIED CLAIMS"]:
                self.assertIn(claim["publisher_text"], item["body"])
                self.assertNotIn("candidate_dimensions", claim)
            if item["article_id"] == -3:
                changed = deepcopy(record)
                changed["claims"][0]["qualifiers"]["negation"][0]["quote"] = "I am relieved."
                with self.assertRaisesRegex(ValueError, "exact model-visible"):
                    qualified_world(item, measurement, changed)
                changed = deepcopy(record)
                changed["claims"][0]["qualifiers"]["reported_event_time"] = None
                with self.assertRaisesRegex(ValueError, "known claim time"):
                    qualified_world(item, measurement, changed)
                changed = deepcopy(record)
                changed["claims"][0]["target_relation"] = "unknown"
                with self.assertRaisesRegex(ValueError, "needs review"):
                    qualified_world(item, measurement, changed)
        for key in (-1, -6, -10, -11):
            self.assertEqual(packets[key]["QUALIFIED CLAIMS"], [])
        self.assertEqual(len(packets[-9]["QUALIFIED CLAIMS"]), 1)
        self.assertEqual(set(packets[-9]["CLASSIFIER MEASUREMENTS"]["windows"][0]["emotion"]), {"excitement"})
        self.assertEqual(packets[-5]["QUALIFIED CLAIMS"][0]["time_scope"], "historical")
        self.assertEqual(packets[-13]["QUALIFIED CLAIMS"][0]["time_scope"], "current")
        self.assertEqual(len(packets[-12]["QUALIFIED CLAIMS"]), 1)
        self.assertNotIn("relief", packets[-12]["CLASSIFIER MEASUREMENTS"]["windows"][0]["emotion"])
        self.assertEqual(packets[-12]["SOURCE CONTEXT"][0]["quote"], controls[-12]["body"])
        self.assertIn("The first report said", expression_world(packets[-12])["SOURCE CONTEXT"][0])
        self.assertNotIn("counterparty", expression_world(packets[-12])["QUALIFIED CLAIMS"][0])


if __name__ == "__main__":
    unittest.main()
