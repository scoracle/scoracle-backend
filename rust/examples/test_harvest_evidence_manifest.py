"""Offline checks for the durable cohort indexes; no logs, models, or network needed."""
import copy
import json
from pathlib import Path
import unittest
from harvest_evidence_manifest import reduce_trace

FIXTURES = Path(__file__).resolve().parents[1] / 'fixtures' / 'harvester'


class EvidenceIndexTests(unittest.TestCase):
    def test_corrected_cohort_accounting_and_provenance(self):
        data = json.loads((FIXTURES / 'cohort-20260926-publisher-first.index.json').read_text())
        self.assertEqual(data['canonical_candidates'], 84)
        self.assertEqual(data['outcomes'], {'accepted': 67, 'rejected': 10, 'publisher_fetch_error': 7})
        self.assertEqual(data['all_four_assignments'], 53)
        self.assertEqual(data['accepted_context_bytes'], 42878)
        self.assertEqual(len({(r['article_id'], r['entity']) for r in data['articles']}), 84)
        for row in data['articles']:
            if row['outcome'] == 'publisher_fetch_error':
                self.assertNotIn('signals', row)
                continue
            self.assertEqual(row['model_input_excerpt']['selection'], 'bounded_verbatim_publisher_opening')
            for key in ('model_input_excerpt', 'excerpt'):
                if row[key]:
                    self.assertEqual(row[key]['end'] - row[key]['start'], row[key]['bytes'])
            for provider_id in row['provider_ids'].values():
                provider = data['providers'][provider_id]
                self.assertEqual(provider['revision'], '55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851')
                self.assertIn('tokenizer/tokenizer.json', provider['files'])
                self.assertIn('torch', provider['runtime'])

    def test_failed_rss_experiment_remains_distinguishable(self):
        data = json.loads((FIXTURES / 'cohort-20260926-rss-first.index.json').read_text())
        selections = {r['model_input_excerpt']['selection'] for r in data['articles'] if 'model_input_excerpt' in r}
        self.assertEqual(selections, {'bounded_verbatim_google_description'})

    def test_reducer_accounts_for_errors_without_copying_source_text(self):
        trace = {'articles': [{'article_id': 1, 'entity': 'Example FC', 'feed_rank': 0,
                              'outcome': 'publisher_fetch_error', 'error': 'PRIVATE_SOURCE'}],
                 'canonical_candidate_count': 1, 'outcomes': {'publisher_fetch_error': 1},
                 'cohort_contract': 'test', 'elapsed_ms': 2}
        result = reduce_trace(json.dumps(trace).encode())
        self.assertNotIn('PRIVATE_SOURCE', json.dumps(result))
        self.assertNotIn('signals', result['articles'][0])
        broken = copy.deepcopy(trace)
        broken['outcomes'] = {'rejected': 1}
        with self.assertRaises(ValueError):
            reduce_trace(json.dumps(broken).encode())


if __name__ == '__main__':
    unittest.main()
