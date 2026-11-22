"""Freeze a 2x2 comparison: current/revised instructions × compact/readable layout."""
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent
SOURCE = ROOT.parent / 'insider-indices-2026-11-01/cases/control'
CASES = ['team-37-FOOTBALL', 'player-322-NBA', 'person-8-NFL', 'player-45-NFL']


def layout(original):
    packet = json.loads(original)
    reports = packet['fresh']['reports']
    readable = []
    for index, report in enumerate(reports):
        ordered = {'report_index': index}
        for key in ['publisher', 'published_at', 'headline', 'publisher_excerpt', 'co_mentions', 'classifier_world']:
            if key in report:
                ordered[key] = report[key]
        assert set(ordered) - {'report_index'} == set(report)
        assert {k: v for k, v in ordered.items() if k != 'report_index'} == report
        readable.append(ordered)
    packet['fresh']['reports'] = readable
    result = json.dumps(packet, ensure_ascii=False, indent=2)
    restored = json.loads(result)
    for index, report in enumerate(restored['fresh']['reports']):
        assert report.pop('report_index') == index
    assert restored == json.loads(original), 'layout must preserve every original field and value'
    return result


def main():
    control = (ROOT / 'control.txt').read_text().strip()
    candidate = (ROOT / 'insider.txt').read_text().strip()
    arms = ['control', 'layout', 'instructions', 'both']
    rows = []
    for index, name in enumerate(CASES):
        fixture = json.loads((SOURCE / (name + '.json')).read_text())
        assert fixture['temperature'] == 0
        for arm in arms[index:] + arms[:index]:
            rows.append(dict(name=name, arm=arm, fixture=fixture,
                             system=candidate if arm in ['instructions', 'both'] else control,
                             user_prompt=layout(fixture['user_prompt']) if arm in ['layout', 'both'] else fixture['user_prompt']))
    assert len(rows) == 16
    (ROOT / 'cases.jsonl').write_text(''.join(json.dumps(row, ensure_ascii=False) + '\n' for row in rows))
    (ROOT / 'control.txt').write_text(control + '\n')
    print('16 cases; all original source fields, metadata, tone and report order preserved.')


if __name__ == '__main__':
    main()
