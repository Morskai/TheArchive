import test from 'node:test'
import assert from 'node:assert/strict'
import { castNameLabel, moveTag, placeTag } from '../src/data/tagPresentation.ts'
import { editText } from '../src/data/textEditing.ts'
test('cast labels preserve five full-width characters', () => {
  assert.equal(castNameLabel('张三李四王'), '张三李四王')
  assert.equal(castNameLabel('张三李四王六'), '张三李四王..')
  assert.equal(castNameLabel('ABCDEFGHIJK'), 'ABCDEFGHIJ..')
  assert.equal(castNameLabel('𠮷田美雪子六'), '𠮷田美雪子..')
})
test('moving tags preserves every name exactly once', () => {
  assert.deepEqual(moveTag(['A','B','C'], 'C', 'A'), ['C','A','B'])
  assert.deepEqual(moveTag(['A','B','C'], 'A', 'C'), ['B','C','A'])
  const original = ['A','B']; assert.equal(moveTag(original, 'missing', 'B'), original)
})
test('text edits replace selection and respect field length', () => {
  assert.deepEqual(editText('评分标签', 2, 4, '收藏'), { value: '评分收藏', caret: 4 })
  assert.deepEqual(editText('abcdef', 2, 4, ''), { value: 'abef', caret: 2 })
  assert.deepEqual(editText('abcdef', 2, 4, '123456', 7), { value: 'ab123ef', caret: 5 })
})

test('pointer drop inserts before or after a target from either direction', () => {
  const tags = ['A', 'B', 'C', 'D']
  assert.deepEqual(placeTag(tags, 'D', 'A', true), ['D', 'A', 'B', 'C'])
  assert.deepEqual(placeTag(tags, 'D', 'A', false), ['A', 'D', 'B', 'C'])
  assert.deepEqual(placeTag(tags, 'A', 'C', true), ['B', 'A', 'C', 'D'])
  assert.deepEqual(placeTag(tags, 'A', 'C', false), ['B', 'C', 'A', 'D'])
  assert.equal(placeTag(tags, 'A', 'B', true), tags)
  assert.equal(placeTag(tags, 'missing', 'B', true), tags)
  assert.equal(placeTag(tags, 'A', 'A', false), tags)
})
