import assert from 'node:assert/strict'
import test from 'node:test'
import { directoryNumber, filmDisplayCode, filmSourceNames, videoNameAndFormat, videoSizeLabel } from '../src/data/realFilmPresentation.ts'
import type { RealFilm, RealVideo } from '../src/data/realFilms'

const video = (path: string, source = '虚构分类'): RealVideo => ({ id: path, observed_path: path, source_id: 'source', source_name: source })
const film = (videos: RealVideo[], manual_code: string | null = null): RealFilm => ({ id: 'film', title: '虚构影片', manual_code, videos, favorite: false, rating: null, tags: [], needs_organization: true })

test('directory code uses the immediate observed folder at any entry depth without inventing identities', () => {
  assert.deepEqual(directoryNumber(video('地区/分类/[DEMO-001] 作品/video.mp4')), ['DEMO-001'])
  assert.deepEqual(directoryNumber(video('[011713-240] 作品\\video.mp4')), ['011713-240'])
  assert.deepEqual(directoryNumber(video('video.mp4', '[DEMO-001] 作品')), ['DEMO-001'])
  assert.deepEqual(directoryNumber(video('[DEMO-001]/分类/video.mp4')), [])
  assert.deepEqual(directoryNumber(video('[中文字幕]/video.mp4')), ['中文字幕'])
  assert.equal(filmDisplayCode(film([video('video.mp4')])), '')
  assert.equal(filmDisplayCode(film([video('[DEMO-001]/a.mp4'), video('[DEMO-002]/b.mp4')])), '编号待核对')
  assert.equal(filmDisplayCode(film([video('[DEMO-001]/a.mp4')], '人工编号')), '人工编号')
  assert.equal(filmDisplayCode(film([video('[DEMO-001]/a.mp4')], '')), 'DEMO-001')
  assert.equal(filmSourceNames(film([video('a.mp4'), video('b.mp4'), video('c.mp4', '另一来源')])), '虚构分类、另一来源')
})

test('version facts preserve names, uppercase formats and the MB/GB boundary', () => {
  assert.deepEqual(videoNameAndFormat('目录/虚构 长文件名.mp4'), { name: '虚构 长文件名', format: 'MP4' })
  assert.deepEqual(videoNameAndFormat('目录\\sample.Wmv'), { name: 'sample', format: 'WMV' })
  assert.equal(videoSizeLabel(2.7 * 1024 ** 3), '2.7GB')
  assert.equal(videoSizeLabel(800 * 1024 ** 2), '800MB')
  assert.equal(videoSizeLabel(1024 ** 3), '1GB')
  assert.equal(videoSizeLabel(0), '0MB')
  assert.equal(videoSizeLabel(undefined), '大小未知')
  assert.equal(videoSizeLabel(NaN), '大小未知')
})

test('folder number accepts literal bracket contents and absence stays blank', () => {
 assert.deepEqual(directoryNumber(video('[自定中文编号]/FILE-999.mp4')), ['自定中文编号'])
 assert.deepEqual(directoryNumber(video('[CUSTOM] [SECOND]/FILE-999.mp4')), ['CUSTOM'])
 assert.deepEqual(directoryNumber(video('title/FILE-999.mp4')), [])
 assert.equal(filmDisplayCode(film([video('title/FILE-999.mp4')])), '')
})
