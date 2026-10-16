import assert from 'node:assert/strict'
import test from 'node:test'
import { sourceAddedNotice, sourcePreviewNotice, sourceRefreshNotice, type SourcePreview } from '../src/data/sources.ts'
const scanned: SourcePreview = { token: 'synthetic', video_count: 3, needs_review: 0, issue_count: 0, unreadable_directory_count: 0, complete: true, video_directory_count: 2, image_count: 0, subtitle_count: 0, cover_hint_count: 0 }
const confirmation = { library_id: 'synthetic', discovered: 3, inserted: 0, missing_count: 0 }
test('new and previously removed sources both announce fresh scan data', () => {
  assert.equal(sourcePreviewNotice(scanned), '发现 3 个视频 · 待确认添加')
  assert.equal(sourceAddedNotice(scanned, confirmation), '目录已添加 · 3 个视频')
})
test('artwork failure preserves a successfully committed source conclusion', () => {
  assert.equal(sourceRefreshNotice({ ...confirmation, artwork_failed: 0 }), '')
  assert.equal(sourceAddedNotice(scanned, { ...confirmation, artwork_failed: 1 }), '目录已添加 · 3 个视频；部分图片缓存未生成，可重新扫描。')
})
