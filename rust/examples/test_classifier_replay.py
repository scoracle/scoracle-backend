import unittest

from classifier_replay import laya_questions, source_windows


class Coverage(unittest.TestCase):
    def test_exact_unicode_bytes_and_missing_qualification(self):
        neutral = laya_questions()["neutral"]
        self.assertEqual(neutral["criteria"]["true"], "No emotion is expressed")
        self.assertEqual(neutral["criteria"]["false"], "Emotion is expressed")
        text = "Équipe won.\n\nBut not today."
        encoded = [{"start": 0, "end": 12, "text": "Équipe won."},
                   {"start": 14, "end": 28, "text": "But not today."}]
        windows = source_windows(text, encoded)
        for window in windows:
            self.assertEqual(text.encode()[window["start"]:window["end"]].decode(), window["text"])
        encoded.pop()
        with self.assertRaisesRegex(ValueError, "complete source"):
            source_windows(text, encoded)


if __name__ == "__main__":
    unittest.main()
