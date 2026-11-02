"""Copy validity is necessary, not proof that selected spans preserve meaning."""
import copy

def validate(note, text):
    exact = lambda value: isinstance(value, str) and bool(value.strip()) and value in text
    assert set(note) == {'statements', 'unspecified'}
    assert 1 <= len(note['statements']) <= 3 and len(note['unspecified']) <= 4
    for statement in note['statements']:
        assert set(statement) == {'about', 'speaker', 'kind', 'claim', 'qualifications'}
        assert len(statement['about']) <= 4 and all(exact(v) for v in statement['about'])
        assert statement['speaker'] is None or exact(statement['speaker'])
        assert statement['kind'] in ['assertion', 'opinion', 'question', 'listing']
        assert exact(statement['claim'])
        assert len(statement['qualifications']) <= 5 and all(exact(v) for v in statement['qualifications'])
    assert all(exact(v) for v in note['unspecified'])

def self_check():
    text = 'Could Harbor recall Vale? Two approvals needed'
    note = {'statements': [{'about': ['Harbor', 'Vale'], 'speaker': None,
            'kind': 'question', 'claim': text, 'qualifications': ['Two approvals needed']}], 'unspecified': ['Two approvals']}
    validate(note, text)
    for field in ['claim', 'unspecified']:
        bad = copy.deepcopy(note)
        if field == 'claim': bad['statements'][0]['claim'] = 'Harbor will recall Vale.'
        else: bad['unspecified'] = ['transfer fee']
        try: validate(bad, text)
        except AssertionError: pass
        else: raise AssertionError('invented prose was accepted')
    # A copied fragment can still lose its question and qualification; review must catch this.
    fragment = copy.deepcopy(note)
    fragment['statements'][0].update(claim='Harbor recall Vale', kind='assertion', qualifications=[])
    validate(fragment, text)

if __name__ == '__main__':
    self_check()
    print('Copy guard rejects invented claims/unknowns; exact copying does not establish semantic fidelity.')
