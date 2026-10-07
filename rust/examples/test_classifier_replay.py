import unittest
import json

from classifier_replay import SCHEMA_PATH, head_labels, model_text, source_windows, validate_head
from classifier_review import prepare, validate


class Coverage(unittest.TestCase):
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
        with self.assertRaisesRegex(ValueError, "named reviewer"):
            validate(record, item, schema)
        unit["reviewer"] = "synthetic contract check"
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


if __name__ == "__main__":
    unittest.main()
