import assert from 'node:assert/strict'
import test from 'node:test'
import { groupDirectoryProblems, filterProblemGroups, selectedProblemGroups, problemGroupIssueIds, toggleAllProblemGroups, filterLibraryProblems, openProblemDirectory, problemDirectoryRequest, problemDirectoryError, problemLabel, visibleProblemSelection, type LibraryProblem } from '../src/data/problems.ts'

const problems: LibraryProblem[] = [
  { id: 'open-a', kind: 'film_missing_cover', subject_id: 'film-a', title: '晨光', reason: '无法找到 cover.jpg', videos: [{ id: 'video-a', name: 'ABC-101.MP4' }], expected_file: 'cover.jpg', sources: [{ id: 'source-a', name: '同名片库' }], handled: false },
  { id: 'open-b', kind: 'film_code_conflict', subject_id: 'film-b', title: '夜景', reason: '无法找到 poster.jpg', videos: [{ id: 'video-b', name: 'ABC-102.mp4' }], expected_file: 'poster.jpg', sources: [{ id: 'source-b', name: '同名片库' }], handled: false },
  { id: 'handled', kind: 'film_missing_cover', subject_id: 'film-c', title: '已核对', reason: '无法找到 cover.jpg', videos: [], expected_file: 'cover.jpg', sources: [{ id: 'source-a', name: '同名片库' }], handled: true },
  { id: 'duplicate-a', kind: 'film_code_conflict', subject_id: 'directory-a', title: '编号 TEST-01', reason: '同一片库有效编号重复', videos: [{ id: 'video-c', name: 'main.mp4' }], directories: ['分类/[TEST-01] 影片'], expected_file: '支持的影片文件', sources: [{ id: 'source-c', name: '虚构片库' }], handled: false },
]

test('localized issue text participates in search while source, type and handled filters still apply', () => {
  const row = { ...problems[0], reason: '仅有中文说明', directory_targets: [{ source_id: 'source-a', relative: '虚构目录', label: '虚构目录' }] }
  const groups = groupDirectoryProblems([row])
  const softwareText = () => ['Missing cover', 'No usable poster image']
  assert.equal(filterProblemGroups(groups, 'open', 'source-a', 'missing cover', 'film_missing_cover', softwareText).length, 1)
  assert.equal(filterProblemGroups(groups, 'open', 'source-a', 'poster image', 'film_missing_cover', softwareText).length, 1)
  assert.equal(filterProblemGroups(groups, 'open', 'source-b', 'missing cover', 'film_missing_cover', softwareText).length, 0)
  assert.equal(filterProblemGroups(groups, 'resolved', 'source-a', 'missing cover', 'film_missing_cover', softwareText).length, 0)
  assert.equal(filterProblemGroups(groups, 'open', 'source-a', 'missing cover', 'film_code_conflict', softwareText).length, 0)
})

test('directory groups combine types without mixing same names across sources or handled states', () => {
  const cover = { ...problems[0], directory_targets: [{ source_id: 'a', relative: '演员/影片', label: '演员/影片' }], directory_options: ['演员/影片'] }
  const video = { ...problems[3], directory_targets: [{ source_id: 'a', relative: '演员/影片', label: '演员/影片' }], directory_options: ['演员/影片'] }
  const other = { ...cover, id: 'other', directory_targets: [{ source_id: 'b', relative: '演员/影片', label: '演员/影片' }] }
  const groups = groupDirectoryProblems([cover, { ...video, handled: true }, other])
  assert.equal(groups.length, 2)
  assert.deepEqual(groups[0].items.map(item => item.problem.id), ['open-a', 'duplicate-a'])
  assert.deepEqual(filterProblemGroups(groups, 'open', 'all', '').flatMap(group => group.items.map(item => item.problem.id)), ['open-a', 'other'])
  assert.deepEqual(filterProblemGroups(groups, 'resolved', 'all', '').flatMap(group => group.items.map(item => item.problem.id)), ['duplicate-a'])
  assert.equal(filterProblemGroups(groups, 'open', 'a', '影片').length, 1)
  assert.equal(filterProblemGroups(groups, 'open', 'a', cover.videos[0].name).length, 1)
})

test('problem type filter intersects source, status and search while retaining directory indexes', () => {
  const target = (relative: string) => ({ source_id: 'a', relative, label: relative })
  const cover: LibraryProblem = { ...problems[0], directory_targets: [target('甲')], directory_options: ['甲'] }
  const duplicate: LibraryProblem = { ...problems[3], kind: 'film_code_conflict', directory_targets: [target('乙'), target('甲')], directory_options: ['乙', '甲'], handled: false }
  const groups = groupDirectoryProblems([cover, duplicate])
  const covers = filterProblemGroups(groups, 'open', 'a', '甲', 'film_missing_cover')
  assert.equal(covers.length, 1)
  assert.deepEqual(problemGroupIssueIds(covers), [cover.id])
  const duplicates = filterProblemGroups(groups, 'open', 'a', '甲', 'film_code_conflict')
  assert.deepEqual(problemGroupIssueIds(duplicates), [duplicate.id])
  assert.equal(duplicates[0].items[0].directoryIndex, 1)
  assert.deepEqual(filterProblemGroups(groups, 'resolved', 'a', '甲', 'film_code_conflict'), [])
  assert.deepEqual(filterProblemGroups(groups, 'open', 'other', '甲', 'film_missing_cover'), [])
})

test('multi-directory conflicts retain original directory indexes and mark each identity only once', () => {
  const conflict: LibraryProblem = { ...problems[0], id: 'shared', kind: 'film_code_conflict', directory_options: ['甲', '乙'], directory_targets: [{ source_id: 'a', relative: '甲', label: '甲' }, { source_id: 'a', relative: '乙', label: '乙' }] }
  const groups = groupDirectoryProblems([conflict])
  assert.deepEqual(groups.map(group => group.items[0].directoryIndex), [0, 1])
  assert.equal(filterProblemGroups(groups, 'open', 'all', '乙').length, 1)
  const visible = groups.flatMap(group => group.items.map(item => item.problem))
  assert.deepEqual(problemGroupIssueIds(selectedProblemGroups(toggleAllProblemGroups([], groups), groups)), ['shared'])
  assert.deepEqual(visibleProblemSelection(['shared', 'shared'], visible), ['shared'])
  assert.deepEqual(problemDirectoryRequest(groups[1].items[0].problem, groups[1].items[0].directoryIndex), { problemId: 'shared', directoryIndex: 1 })
})

test('unknown directory identity stays separate instead of guessing from titles or source names', () => {
  assert.equal(groupDirectoryProblems(problems.slice(0, 2)).length, 2)
})

test('select all toggles only directories in the pending page, including partial selection', () => {
  const groups = groupDirectoryProblems(problems)
  const visible = filterProblemGroups(groups, 'open', 'source-a', '')
  const pending = filterProblemGroups(groups, 'open', 'all', '')
  assert.deepEqual(toggleAllProblemGroups([], visible), ['unknown:open-a'])
  assert.deepEqual(toggleAllProblemGroups(['unknown:open-a'], visible), [])
  assert.deepEqual(toggleAllProblemGroups(['unknown:open-a'], pending), ['unknown:open-a', 'unknown:open-b', 'unknown:duplicate-a'])
  assert.deepEqual(toggleAllProblemGroups(['unknown:open-b', 'unknown'], visible), ['unknown:open-a'])
  assert.deepEqual(toggleAllProblemGroups(['unknown:open-a'], filterProblemGroups(groups, 'open', 'all', '已核对')), [])
})

test('cover label is concise without changing problem identity', () => {
  assert.equal(problemLabel(problems[0]), '封面缺失')
  assert.equal(problems[0].kind, 'film_missing_cover')
})

test('missing artwork filters use source IDs, states, type and fixed filename evidence', () => {
  assert.deepEqual(filterLibraryProblems(problems, 'open', 'source-a', '').map(item => item.id), ['open-a'])
  assert.deepEqual(filterLibraryProblems(problems, 'resolved', 'all', '').map(item => item.id), ['handled'])
  assert.deepEqual(filterLibraryProblems(problems, 'open', 'all', ' POSTER.JPG ').map(item => item.id), ['open-b'])
  assert.deepEqual(filterLibraryProblems(problems, 'open', 'all', 'abc-102').map(item => item.id), ['open-b'])
  assert.deepEqual(filterLibraryProblems(problems, 'open', 'all', '封面缺失').map(item => item.id), ['open-a'])
  assert.deepEqual(filterLibraryProblems(problems, 'resolved', 'all', '封面缺失').map(item => item.id), ['handled'])
  assert.equal(filterLibraryProblems(problems, 'open', 'all', '固定封面归属冲突').length, 0)
})

test('duplicate number reminders can be found by type and saved relative directory', () => {
  assert.deepEqual(filterLibraryProblems(problems, 'open', 'source-c', '编号重复').map(item => item.id), ['duplicate-a'])
  assert.deepEqual(filterLibraryProblems(problems, 'open', 'all', '分类/[TEST-01]').map(item => item.id), ['duplicate-a'])
})

test('hidden or already handled problems leave selection and cannot enter scoped batches', () => {
  const selected = ['open-a', 'open-b', 'handled', 'unknown']
  const visible = filterLibraryProblems(problems, 'open', 'source-a', '')
  assert.deepEqual(visibleProblemSelection(selected, visible), ['open-a'])
  assert.deepEqual(visibleProblemSelection(selected, filterLibraryProblems(problems, 'open', 'all', '夜景')), ['open-b'])
  assert.deepEqual(visibleProblemSelection(selected, filterLibraryProblems(problems, 'resolved', 'all', '')), [])
})

test('code conflicts use the duplicate number label and both saved directory evidences', () => {
  const conflict: LibraryProblem = { id: 'conflict', kind: 'film_code_conflict', subject_id: 'conflict-a', title: 'TEST-01', reason: '同一来源的不同影片文件夹出现相同编号', videos: [], directories: ['分类/[TEST-01] 晨光', '演员/[TEST-01] 夜景'], expected_file: '', sources: [{ id: 'source-a', name: '虚构片库' }], handled: false }
  assert.equal(problemLabel(conflict), '编号重复')
  assert.deepEqual(filterLibraryProblems([...problems, conflict], 'open', 'source-a', '编号重复').map(item => item.id), ['conflict'])
  assert.deepEqual(filterLibraryProblems([...problems, conflict], 'open', 'source-a', '演员/[TEST-01]').map(item => item.id), ['conflict'])
  assert.deepEqual(visibleProblemSelection(['conflict', 'handled'], [conflict]), ['conflict'])
})

test('problem directory requests carry only the saved problem identity and selected index', () => {
  const multiple: LibraryProblem = { ...problems[0], directory_options: ['演员甲/[TEST-01] 第一部', '演员乙/[TEST-01] 第二部'] }
  assert.deepEqual(problemDirectoryRequest(multiple, 0), { problemId: 'open-a', directoryIndex: 0 })
  assert.deepEqual(problemDirectoryRequest(multiple, 1), { problemId: 'open-a', directoryIndex: 1 })
  assert.deepEqual(filterLibraryProblems([multiple], 'open', 'all', '演员乙').map(item => item.id), ['open-a'])
  for (const index of [-1, 2, 0.5, Number.NaN]) assert.throws(() => problemDirectoryRequest(multiple, index), /problem_directory_unavailable/)
  assert.throws(() => problemDirectoryRequest(problems[0], 0), /problem_directory_unavailable/)
})

test('directory opening errors give a concise retry or source explanation', () => {
  assert.match(problemDirectoryError('source_paused'), /停用/)
  assert.match(problemDirectoryError('root_unavailable'), /无法访问/)
  assert.match(problemDirectoryError('problem_not_found'), /重新读取/)
  assert.match(problemDirectoryError('problem_directory_unavailable'), /暂不可用/)
  assert.match(problemDirectoryError('access_denied'), /拒绝/)
})

test('directory IPC uses an identity and index and rejects invalid choices before invoking', async () => {
  const calls: unknown[] = []
  const runtime = globalThis as unknown as { window?: unknown }
  const previous = runtime.window
  runtime.window = { __TAURI_INTERNALS__: { invoke: async (command: string, args: unknown) => { calls.push({ command, args }) } } }
  try {
    const problem: LibraryProblem = { ...problems[0], directory_options: ['隔离目录甲', '隔离目录乙'] }
    await openProblemDirectory(problem, 1)
    assert.deepEqual(calls, [{ command: 'open_problem_directory', args: { problemId: 'open-a', directoryIndex: 1 } }])
    assert.throws(() => openProblemDirectory(problem, 2), /problem_directory_unavailable/)
    assert.equal(calls.length, 1)
  } finally {
    if (previous === undefined) delete runtime.window
    else runtime.window = previous
  }
})

test('resolved selections support select all and deduplicate shared directory issues', () => {
  const resolved = filterProblemGroups(groupDirectoryProblems(problems), 'resolved', 'all', '')
  assert.deepEqual(toggleAllProblemGroups([], resolved), ['unknown:handled'])
  assert.deepEqual(toggleAllProblemGroups(['unknown:handled'], resolved), [])
  assert.deepEqual(visibleProblemSelection(['handled','handled','open-a'], problems, 'resolved'), ['handled'])
})

test('directory selection counts rows rather than tags and clears only selected records', () => {
  const targets = (relative: string) => [{ source_id: 'source-a', relative, label: relative }]
  const rows = groupDirectoryProblems([
    { ...problems[0], directory_targets: targets('演员/影片甲') },
    { ...problems[3], directory_targets: targets('演员/影片甲') },
    { ...problems[1], directory_targets: targets('演员/影片乙') },
  ])
  const selected = selectedProblemGroups([rows[0].key, rows[0].key, 'hidden'], rows)
  assert.equal(selected.length, 1)
  assert.deepEqual(problemGroupIssueIds(selected), ['open-a', 'duplicate-a'])
  assert.ok(!problemGroupIssueIds(selected).includes('open-b'))
  assert.deepEqual(toggleAllProblemGroups([rows[0].key], rows), rows.map(row => row.key))
  assert.deepEqual(toggleAllProblemGroups(rows.map(row => row.key), rows), [])
  assert.deepEqual(selectedProblemGroups([rows[0].key], rows.slice(1)), [])
})

test('shared conflicts keep independent directory checkboxes while writes deduplicate IDs', () => {
  const conflict = { ...problems[0], id: 'shared', kind: 'film_code_conflict' as const, directory_targets: [
    { source_id: 'source-a', relative: '甲', label: '甲' },
    { source_id: 'source-a', relative: '乙', label: '乙' },
  ] }
  const rows = groupDirectoryProblems([conflict])
  assert.equal(selectedProblemGroups([rows[0].key], rows).length, 1)
  assert.equal(selectedProblemGroups([rows[0].key], rows)[0].key, rows[0].key)
  assert.equal(selectedProblemGroups(toggleAllProblemGroups([], rows), rows).length, 2)
  assert.deepEqual(problemGroupIssueIds(rows), ['shared'])
  const resolved = filterProblemGroups(groupDirectoryProblems([{ ...conflict, handled: true }]), 'resolved', 'all', '')
  assert.equal(selectedProblemGroups(toggleAllProblemGroups([], resolved), resolved).length, 2)
  assert.deepEqual(problemGroupIssueIds(selectedProblemGroups([resolved[1].key], resolved)), ['shared'])
})
