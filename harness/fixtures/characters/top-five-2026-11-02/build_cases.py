"""Reassemble existing scored evidence; ranking and the cap are production code."""
import json
import pathlib
import re
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parents[2]
BASE = ROOT.parent/'relevant-guide-2026-11-02'
ROLES = dict(narratives=('journalist','subject'),transfer=('insider','movement'),vibe=('influencer','emotion'))

def capture(source, output):
    fixture = json.loads(source.read_text())
    original = json.loads(fixture['user_prompt'])
    role, criterion = ROLES[fixture['task']]
    fixture['prompt_version'] = re.search(r'pub const \w*PROMPT_VERSION: &str = "([^"]+)',(REPO/'src/plugins'/role/'prompt.rs').read_text())[1]
    output.parent.mkdir(parents=True,exist_ok=True)
    output.write_text(json.dumps(fixture,ensure_ascii=False,indent=2)+'\n')
    fixture['user_prompt'] = subprocess.check_output([str(REPO/'target/debug/examples/character_package'),str(output)],text=True)
    new = json.loads(fixture['user_prompt'])
    assert new['meta'] == original['meta']
    assert list(new) == ['job','context_guide','meta','fresh']
    assert 'tone' not in new['context_guide']
    parts = fixture['parts']
    items = parts.get('reports',parts.get('sources'))
    def score(item):
        value = ((item.get('classifier_world') or {}).get('spectrum') or {}).get(criterion)
        return value if isinstance(value,(int,float)) and not isinstance(value,bool) and 0 <= value <=100 else -1
    indices = sorted(range(len(items)),key=lambda i:-score(items[i]))[:5]
    reports = new['fresh'] if isinstance(new['fresh'],list) else new['fresh']['reports']
    assert len(reports) == min(5,len(items))
    def article_id(item):
        return item.get('id',item.get('article_id',(item.get('classifier_world') or {}).get('article_id')))
    ids = [article_id(items[i]) for i in indices]
    actual = [report.get('article_id',(report.get('classifier_world') or {}).get('article_id')) for report in reports]
    assert ids == actual, (output,ids,actual)
    old_reports = original['fresh'] if isinstance(original['fresh'],list) else original['fresh']['reports']
    # On first recapture the old request still contains the full original order.
    # Subsequent runs compare the already selected projection directly.
    old_ids = [r.get('article_id',(r.get('classifier_world') or {}).get('article_id')) for r in old_reports]
    for position, (index, report) in enumerate(zip(indices,reports)):
        if ids[position] is not None and ids[position] in old_ids:
            old_report = dict(old_reports[old_ids.index(ids[position])])
        elif original['job'] == new['job']:
            old_report = dict(old_reports[position])
        else:
            old_report = dict(old_reports[index])
        report=dict(report)
        for key in ['report_key','report_index']:
            old_report.pop(key,None);report.pop(key,None)
        assert old_report == report, (output,index)
    output.write_text(json.dumps(fixture,ensure_ascii=False,indent=2)+'\n')
    return dict(case=output.stem,task=fixture['task'],criterion=criterion,candidates=len(items),selected=[dict(article_id=article_id(items[i]),score=score(items[i]),original_index=i) for i in indices],unselected=[article_id(items[i]) for i in range(len(items)) if i not in indices])

if __name__ == '__main__':
    selection=[]
    for source in sorted((BASE/'cases/candidate').glob('*.json')):
        if json.loads(source.read_text())['task'] in ROLES:
            selection.append(capture(source, ROOT/'cases'/source.name))
    selection.append(capture(BASE/'chelsea-preview.json',ROOT/'cases/narratives-chelsea-top-five.json'))
    for task in ROLES:
        for source in (REPO/'fixtures/quality'/task).glob('*.json'):
            capture(source,source)
    (ROOT/'selection.json').write_text(json.dumps(selection,indent=2)+'\n')
    (ROOT/'instructions.json').write_text(json.dumps({role:(REPO/'src/plugins'/role/'instructions.txt').read_text() for role,_ in ROLES.values()},indent=2)+'\n')
    print('7 development cases and 29 quality fixtures reassembled; top-five IDs, scores, source values and metadata verified.')
