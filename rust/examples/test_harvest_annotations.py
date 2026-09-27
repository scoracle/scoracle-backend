"""Annotation gates exercise independent labels and source-bound UTF-8 offsets."""
import copy
import unittest
from harvest_annotations import validate, review_rows, sha


def case():
    headline = 'Équipe wins'
    opening = '  A team won. Équipe celebrated.'
    excerpt = {'text': opening, 'start': 0, 'end': len(opening.encode()),
               'selection': 'available_opening_sentences'}
    packet = {'headline': headline, 'body_hash': sha(opening),
              'model_input_excerpt': excerpt, 'excerpt': excerpt,
              'signals': {'relevance': 'model output must stay hidden'}}
    source = {'article_id': 7, 'entity': 'Équipe', 'outcome': 'accepted', 'packet': packet}
    durable = {'article_id': 7, 'entity': 'Équipe', 'body_hash': sha(opening),
               'headline_sha256': sha(headline),
               'model_input_excerpt': {'text_sha256': sha(opening), 'bytes': len(opening.encode())},
               'excerpt': {'text_sha256': sha(opening), 'bytes': len(opening.encode())}}
    row = {'schema_version': 'harvester-annotation-v1', 'article_id': 7,
           'query_entity': 'Équipe', 'cohort_date': '2026-09-26',
           'source_state': 'materialized', 'body_sha256': sha(opening),
           'headline_sha256': sha(headline), 'split': 'unassigned',
           'story_group': None, 'review_status': 'pending', 'annotator': None,
           'adjudicator': None,
           'labels': dict.fromkeys(('entity', 'journalist', 'influencer', 'insider', 'scout'))}
    return row, {(7, 'Équipe'): source}, {(7, 'Équipe'): durable}


class AnnotationTests(unittest.TestCase):
    def test_pending_and_source_only_review(self):
        row, trace, index = case()
        self.assertEqual(validate([row], trace, index), {'pending': 1})
        view = review_rows([row], trace)[0]
        self.assertNotIn('signals', str(view))
        self.assertNotIn('model output', str(view))
        self.assertEqual(view['publisher_openings'][0]['text'], trace[7, 'Équipe']['packet']['excerpt']['text'])

    def test_positive_requires_exact_retained_utf8_span_and_independent_adjudicator(self):
        row, trace, index = case()
        row['review_status'] = 'reviewed'
        row['annotator'] = 'reviewer-a'
        row['story_group'] = 'synthetic-7'
        row['labels']['entity'] = {'useful': True, 'reason': 'Reports the team result.',
                                   'evidence': [{'field': 'body', 'start': 14, 'end': 21}]}
        self.assertEqual(validate([row], trace, index), {'reviewed': 1})
        invalid = copy.deepcopy(row)
        invalid['labels']['entity']['evidence'][0]['start'] = 15  # middle of É
        with self.assertRaises(UnicodeDecodeError):
            validate([invalid], trace, index)
        invalid = copy.deepcopy(row)
        invalid['labels']['entity']['evidence'] = []
        with self.assertRaisesRegex(ValueError, 'source span'):
            validate([invalid], trace, index)
        adjudicated = copy.deepcopy(row)
        adjudicated['review_status'] = 'adjudicated'
        adjudicated['split'] = 'train'
        adjudicated['labels'].update({k: {'useful': False, 'reason': 'No such evidence.',
                                           'evidence': []}
                                      for k in ('journalist', 'influencer', 'insider', 'scout')})
        adjudicated['adjudicator'] = 'reviewer-b'
        self.assertEqual(validate([adjudicated], trace, index), {'adjudicated': 1})
        adjudicated['adjudicator'] = 'reviewer-a'
        with self.assertRaisesRegex(ValueError, 'independent adjudicator'):
            validate([adjudicated], trace, index)

    def test_failed_acquisition_cannot_be_negative_or_trainable(self):
        row, trace, index = case()
        trace[7, 'Équipe']['outcome'] = 'publisher_fetch_error'
        trace[7, 'Équipe']['packet'] = None
        index[7, 'Équipe'].pop('body_hash')
        index[7, 'Équipe'].pop('headline_sha256')
        row['body_sha256'] = row['headline_sha256'] = None
        row['source_state'] = 'acquisition_error'
        self.assertEqual(validate([row], trace, index), {'pending': 1})
        row['review_status'] = 'reviewed'
        row['annotator'] = 'reviewer-a'
        row['labels']['entity'] = {'useful': False, 'reason': 'No publisher page.', 'evidence': []}
        with self.assertRaisesRegex(ValueError, 'cannot be labeled'):
            validate([row], trace, index)

    def test_group_cannot_cross_splits(self):
        row, trace, index = case()
        second = copy.deepcopy(row)
        second['article_id'] = 8
        row['story_group'] = second['story_group'] = 'same-story'
        row['split'] = 'train'
        second['split'] = 'test'
        for r in (row, second):
            r['review_status'] = 'adjudicated'
            r['annotator'] = 'a'
            r['adjudicator'] = 'b'
            r['labels'] = {k: {'useful': False, 'reason': 'No relevant content.', 'evidence': []}
                           for k in r['labels']}
        trace[8, 'Équipe'] = copy.deepcopy(trace[7, 'Équipe'])
        index[8, 'Équipe'] = copy.deepcopy(index[7, 'Équipe'])
        with self.assertRaisesRegex(ValueError, 'crosses splits'):
            validate([row, second], trace, index)

    def test_ai_provisional_cannot_become_training_data(self):
        row, trace, index = case()
        row['review_status'] = 'ai_provisional'
        row['annotator'] = 'codex-ai-provisional'
        row['labels']['entity'] = {'useful': False, 'reason': 'Different team.', 'evidence': []}
        self.assertEqual(validate([row], trace, index), {'ai_provisional': 1})
        row['split'] = 'train'
        row['story_group'] = 'synthetic-7'
        with self.assertRaisesRegex(ValueError, 'AI labels cannot enter'):
            validate([row], trace, index)



if __name__ == '__main__':
    unittest.main()
