import assert from 'node:assert/strict'
import test from 'node:test'
import { catalogs } from '../src/i18n/messages.ts'
import { formatMessage, message } from '../src/i18n/core.ts'
import { sourceAddedMessage } from '../src/i18n/feedback.ts'

test('every offline catalog has matching keys and exact placeholder multiplicities', () => {
  const source = Object.keys(catalogs['zh-CN']).sort()
  const placeholders = (text: string) => text.match(/\{\d+\}/g)?.sort() ?? []
  for (const language of ['en', 'ja', 'zh-TW'] as const) {
    assert.deepEqual(Object.keys(catalogs[language]).sort(), source, language)
    for (const key of source) {
      assert.ok(catalogs[language][key].trim(), `${language}: ${key}`)
      assert.deepEqual(placeholders(catalogs[language][key]), placeholders(key), `${language}: ${key}`)
    }
  }
})

test('the same saved notice renders in each language without translating a user tag that matches UI text', () => {
  const notice = message('已为 {0} 部影片添加标签“{1}”。', [2, '保存'])
  assert.equal(formatMessage(catalogs, 'en', notice), 'Added “保存” to 2 movies.')
  assert.equal(formatMessage(catalogs, 'ja', notice), '2本の動画にタグ「保存」を追加しました。')
  assert.equal(formatMessage(catalogs, 'zh-TW', notice), '已為 2 部影片新增標籤「保存」。')
})

test('successful scans retain localized coordination and artwork warnings without losing the committed result', () => {
  const notice = sourceAddedMessage({ video_count: 3 } as any, { related_retained: 1, recovery_failed: true, artwork_failed: 1 } as any)
  const english = formatMessage(catalogs, 'en', notice)
  assert.match(english, /3/)
  assert.match(english, /cache|image/i)
  assert.doesNotMatch(english, /\p{Script=Han}/u)
})
