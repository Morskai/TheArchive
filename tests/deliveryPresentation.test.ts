import test from 'node:test'
import assert from 'node:assert/strict'
import { actorPhotoSummary, activePlaybackDays, actorAliases, sourceSizeLabel, videoResolutionLabel } from '../src/data/deliveryPresentation.ts'
test('aliases use bracket contents while keeping unbracketed names empty', () => {
  assert.equal(actorAliases('爱世梦罗（愛世くらら）'), '愛世くらら')
  assert.equal(actorAliases('演员 (Alias A)（别名 B）'), 'Alias A、别名 B')
  assert.equal(actorAliases('演员'), '')
})
test('source video sizes use decimal GB and TB with one decimal', () => {
  assert.equal(sourceSizeLabel(0), '0.0 GB')
  assert.equal(sourceSizeLabel(999_000_000_000), '999.0 GB')
  assert.equal(sourceSizeLabel(1_000_000_000_000), '1.0 TB')
  assert.equal(sourceSizeLabel(null), '未记录')
})
test('unavailable or invalid resolutions leave no extra separator', () => {
  assert.equal(videoResolutionLabel({width:1920,height:1080}), ' · 1920*1080')
  assert.equal(videoResolutionLabel(null), '')
  assert.equal(videoResolutionLabel({width:0,height:1080}), '')
})


test('photo availability handles empty libraries and explicit missing portraits', () => {
  assert.deepEqual(actorPhotoSummary([]), { percent: 0, missing: 0 })
  assert.deepEqual(actorPhotoSummary([{hasPortrait:true}, {hasPortrait:false}, {hasPortrait:true}]), { percent:67, missing:1 })
})
test('30-day activity counts unique local calendar days including today and excludes future days', () => {
  assert.equal(activePlaybackDays(['2026-09-09','2026-09-08','2026-10-08','2026-10-08','2026-10-09','invalid'], new Date(2026,9,8)), 2)
  assert.equal(activePlaybackDays([], new Date(2026,9,8)), 0)
})
