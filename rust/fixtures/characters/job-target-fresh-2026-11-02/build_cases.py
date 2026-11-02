"""Same frozen evidence, old/new assembly, metadata captured in one read-only snapshot per target."""
import copy
import json
import pathlib
import re
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parents[2]
WORKLOAD = ROOT.parent / 'useful-perspectives-2026-11-01/workload-final'
CONTROL = pathlib.Path('/private/tmp/scoracle-before-job-target-package')
CANDIDATE = REPO / 'target/release/examples/character_package'
ROLES = dict(narratives='journalist', vibe='influencer', rating='scout', transfer='insider', momentum='analyst', oracle='oracle')
OLD_VERSIONS = dict(narratives='n121-useful-perspectives', vibe='vibe-frame-v33-coverage-tone', rating='s80-measured-perspective', transfer='insider-source-v22-quote-eligibility', momentum='momentum-s43-dated-perspective', oracle='or31-five-perspectives')


def main():
    identities = {d['entity']:d for d in map(json.loads,(ROOT/'identity-capture/capture.jsonl').read_text().splitlines())}
    selection = [
        ('narratives','person-86-FOOTBALL'), ('narratives','player-37-NBA'),
        ('vibe','person-23-FOOTBALL'), ('vibe','person-959-NFL'),
        ('rating','team-2-NBA'), ('rating','player-744-NFL'),
        ('momentum','player-744-NFL'), ('oracle','team-2-NBA'), ('oracle','player-744-NFL')]
    sources = [(task, key, WORKLOAD/f'real-{task}-{key}'/'01/fixture.json') for task,key in selection]
    for key in ['team-37-FOOTBALL','player-83946-NFL']:
        path = next((ROOT.parent/'context-review-2026-11-02/eligible').glob('*'+key+'*/fixture.json'))
        sources.append(('transfer',key,path))
    sources.append(('momentum','synthetic-dated-divergence',REPO/'fixtures/quality/momentum/synthetic-dated-divergence.json'))
    manifest=[]
    for task,key,path in sources:
        original=json.loads(path.read_text())
        baseline=copy.deepcopy(original);candidate=copy.deepcopy(original)
        identity=identities.get(key.replace('-',':',2))
        for arm,fixture,binary in [('control',baseline,CONTROL),('candidate',candidate,CANDIDATE)]:
            fixture['system']=''
            fixture['temperature']=0
            fixture['name']=task+'-'+key
            if identity:
                target=fixture['parts']['subject'] if task=='oracle' else fixture['parts']
                target['database_record']=identity['legacy' if arm=='control' else 'attributes']
            if arm=='control': fixture['prompt_version']=OLD_VERSIONS[task]
            else:
                source=(REPO/'src/plugins'/ROLES[task]/'prompt.rs').read_text()
                fixture['prompt_version']=re.search(r'pub const \w*PROMPT_VERSION: &str = "([^"]+)',source)[1]
            target=ROOT/'cases'/arm/(task+'-'+key+'.json');target.parent.mkdir(parents=True,exist_ok=True)
            target.write_text(json.dumps(fixture,ensure_ascii=False,indent=2)+'\n')
            fixture['user_prompt']=subprocess.check_output([str(binary),str(target)],text=True)
            target.write_text(json.dumps(fixture,ensure_ascii=False,indent=2)+'\n')
        a=copy.deepcopy(baseline['parts']);b=copy.deepcopy(candidate['parts'])
        for parts in [a,b]:
            (parts['subject'] if task=='oracle' else parts).pop('database_record',None)
        assert a==b, 'fresh evidence and identity IDs must be unchanged'
        old=json.loads(baseline['user_prompt']);new=json.loads(candidate['user_prompt'])
        new_fresh=copy.deepcopy(new['fresh'])
        if task=='transfer':
            for i,report in enumerate(new_fresh['reports']):assert report.pop('report_index')==i
        assert old['fresh']==new_fresh, 'no source or measurement may change'
        assert old['tone']==new['tone']
        assert old['meta']['name']==new['meta']['name']
        assert new['job']==(REPO/'src/plugins'/ROLES[task]/'instructions.txt').read_text().strip().split('\n\n')[0]
        manifest.append(dict(name=task+'-'+key,task=task,source=str(path.relative_to(REPO)),identity_capture=identity['entity'] if identity else None,synthetic=identity is None))
    assert len(manifest)==12 and all(sum(m['task']==t for m in manifest)==2 for t in ROLES)
    (ROOT/'selection.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print('12 pairs, two per character; complete fresh evidence and tone unchanged.')


if __name__=='__main__':main()
