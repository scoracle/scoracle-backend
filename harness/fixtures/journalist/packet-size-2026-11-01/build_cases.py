"""Freeze nested real-data packets, retaining DB IDs and URLs. No network or inference."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
ANCHORS = {
    'AC Milan': ('ac-milan', 1276620, 'Tresoldi',
                 'Preserve LiveScore, both clubs and possibility/timing. No invented currency, agreed fee or completed move.'),
    'Chicago Bears': ('chicago-bears', 1341401, 'Donte Jackson',
                      'Preserve Da Windy City and the conditional trade opinion. Do not report a completed trade.'),
    'LA Clippers': ('la-clippers', 1289440, 'Max Strus',
                    'Preserve RotoBaller and road-trip unavailability. Do not add injury cause, duration or missed-game count.'),
}
manifest = {'instructions_sha256': hashlib.sha256((ROOT/'instructions.txt').read_bytes()).hexdigest(),
            'ordering': 'Anchor first, followed by all remaining reports in the production-loader order.',
            'passes': 'Ascending sizes then descending for Milan/Clippers; descending then ascending for Bears.',
            'subjects': []}
for line in (ROOT/'live-capture/results.jsonl').read_text().splitlines():
    capture = json.loads(line)
    parts = capture['parts']
    slug, article_id, anchor_name, rubric = ANCHORS[parts['subject']['name']]
    reports = parts['reports']
    assert len(reports) == 40 and len({r['id'] for r in reports}) == 40
    assert not capture.get('admission_error') and not capture.get('preparation_error')
    anchor = next(r for r in reports if r['id'] == article_id)
    assert anchor_name in anchor['title']
    assert all(r['classifier_world']['evidence_scope'] == 'headline' for r in reports)
    ordered = [anchor] + [r for r in reports if r['id'] != article_id]
    assert sorted(r['id'] for r in ordered) == sorted(r['id'] for r in reports)
    assert sum(anchor_name.lower() in r['title'].lower() for r in reports) == 1
    cases = []
    first = [40, 12, 4, 1] if slug == 'chicago-bears' else [1, 4, 12, 40]
    for pass_index, sizes in enumerate([first, first[::-1]], 1):
        for size in sizes:
            selected = ordered[:size]
            assert selected[0] == anchor
            cases.append({'name': f'{slug}-{size:02}-pass-{pass_index}',
                          'parts': {**parts, 'reports': selected},
                          'expect': {'narratives_empty': False, 'summary_includes': [anchor_name]},
                          'review': [rubric,
                                     'Judge every entry against only its cited visible reports. Separate attribution, invention, qualifications, metadata leakage, coverage and parser acceptance.']})
    for size in [1, 4, 12]:
        small = next(c['parts']['reports'] for c in cases if len(c['parts']['reports']) == size)
        assert small == ordered[:size]
    (ROOT/f'{slug}-cases.jsonl').write_text(''.join(json.dumps(c, ensure_ascii=False)+'\n' for c in cases))
    manifest['subjects'].append({'subject': parts['subject'], 'anchor_id': article_id,
        'anchor_original_position': reports.index(anchor)+1, 'anchor_rubric': rubric,
        'related_qualification_review': 'All 40 headlines inspected before inference; no separate denial or qualification of this named anchor found. Broad team availability or match headlines do not establish extra anchor details.',
        'reports': [{'report_key': f'report_{i}', 'article_id': r['id'],
                     'original_position': reports.index(r)+1, 'url': r['classifier_world']['url'],
                     'publisher': r['source'], 'headline': r['title']} for i,r in enumerate(ordered, 1)]})
(ROOT/'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2)+'\n')
print('Frozen 24 cases; exact nested prefixes, real DB IDs and source URLs retained.')
