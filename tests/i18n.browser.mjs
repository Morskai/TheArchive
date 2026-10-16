import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'
import react from '@vitejs/plugin-react'
import ts from 'typescript'

// Real App regression. Only the native IPC/event boundary is synthetic: no real
// application configuration, files, media, database or credentials are touched.
const require = createRequire(import.meta.url)
const { chromium } = require(require.resolve('playwright', { paths: [process.env.PLAYWRIGHT_MODULE_DIR || 'node_modules'] }))
const output = process.env.I18N_BROWSER_OUTPUT || 'src-tauri/target/i18n-browser'
await fs.mkdir(output, { recursive: true })
const locales = (process.env.I18N_BROWSER_LOCALES || 'zh-CN,zh-TW,en,ja').split(',')
const widths = (process.env.I18N_BROWSER_WIDTHS || '1280,960,390').split(',').map(Number)
const catalogs = {}
for (const [locale, filename] of [['zh-CN', 'zh-CN'], ['zh-TW', 'zh-TW'], ['en', 'en'], ['ja', 'ja']]) {
  const source = await fs.readFile(new URL(`../src/i18n/${filename}.ts`, import.meta.url), 'utf8')
  const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext } }).outputText
  catalogs[locale] = (await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`)).default
}
const extraSource = await fs.readFile(new URL('../src/i18n/extra.ts', import.meta.url), 'utf8')
const extraCompiled = ts.transpileModule(extraSource, { compilerOptions: { module: ts.ModuleKind.ESNext } }).outputText
const { extraCatalog } = await import(`data:text/javascript;base64,${Buffer.from(extraCompiled).toString('base64')}`)
for (const locale of Object.keys(catalogs)) Object.assign(catalogs[locale], extraCatalog(locale))
const label = (locale, key, values = []) => {
  assert.ok(locale === 'zh-CN' || Object.hasOwn(catalogs[locale], key), `Missing ${locale} catalog key: ${key}`)
  return (catalogs[locale][key] ?? key).replace(/\{(\d+)\}/g, (all, index) => index < values.length ? String(values[index]) : all)
}
const literalFilm = '设置'
const literalActor = '保存'
const longTitle = '设置 · 东京国际电影节修复特别版 The Complete Restored Director’s Edition (2026)'
const longPath = 'Z:/FICTIONAL_ONLY/电影收藏/电影节修复特别版/设置保存/东京国际电影节修复特别版/movie-with-original-title-and-multiple-subtitles.mkv'
const fixture = {
  actors: [{ id: 'a1', realActorId: 'a1', name: literalActor, aliases: '设置、Christopher Alexander Montgomery III', birthMonth: '1990-05', height: '170', cup: '', bust: '', waist: '', hips: '', notes: '', favorite: true, portrait: 0, hasPortrait: false, filmIds: ['f1', 'f2'], tags: ['设置', '保存'], directoryCreatedAt: null }],
  actorTags: ['设置', '保存'], tags: ['设置', '保存', '电影节修复特别收藏版本'],
  films: [literalFilm, longTitle].map((title, index) => ({ id: `f${index + 1}`, title, original_title: title, manual_code: `DEMO-00${index + 1}`, needs_organization: false, favorite: true, rating: 4, tags: ['设置', '保存'], has_cover: false, actors: [{ id: 'a1', name: literalActor }], videos: [{ id: `v${index + 1}`, observed_path: longPath, directory_path: 'Z:/FICTIONAL_ONLY/设置', source_id: 's1', source_name: '设置' }] })),
}
const core = `
export const isTauri=()=>true;export const convertFileSrc=p=>p;
export async function invoke(c,args={}){
 window.i18nCommands??=[];window.i18nCommands.push({c,args});
 switch(c){
 case 'load_interface_language':if(window.failLanguageRead)throw 'synthetic_language_read';return localStorage.getItem('synthetic-language')||'zh-CN';
 case 'save_interface_language':if(window.failLanguageSave)throw 'synthetic_language_save';localStorage.setItem('synthetic-language',args.language);window.emitNative('interface-language-changed',args.language);return;
 case 'initialize_library_session':if(window.failStartup)throw 'protection_failed';return {savedSources:1};
 case 'load_actor_catalog':return window.fixtureCatalog;
 case 'load_directory_facts':if(window.failDirectoryRead)throw 'directory_unavailable';return {path:'Z:/FICTIONAL_ONLY/设置',videos:1,images:0,subtitles:0};
 case 'apply_real_bulk_tag':return args.filmIds.length;
 case 'choose_actor_import_root':return 'Z:/FICTIONAL_ONLY/演员';
 case 'preview_actor_import':return {token:'synthetic',root:args.root,items:[...(window.partialActorImport?[{key:'new1',directory:'Z:/FICTIONAL_ONLY/演员/设置',name:'设置',aliases:[],candidateIds:[],canCreate:true,nameAliasSignature:'synthetic-new',status:'new',reason:null,filmography:[]}]:[]),{key:'failed1',directory:'Z:/FICTIONAL_ONLY/演员/保存',name:'保存',aliases:[],candidateIds:[],canCreate:false,nameAliasSignature:'',status:'failed',reason:'invalid_actor_profile'}]};
 case 'confirm_actor_scan_add':return {imported:1,updated:0,skipped:0,failed:1,conflicts:0,relationships:{available:0,existing:0,notFound:0,unmatched:[]},artwork_keys:[],successful_artwork_keys:[]};
 case 'load_library_problems':return window.fixtureProblems||[];
 case 'load_real_films':return window.fixtureCatalog.films;
 case 'list_real_tags':return window.fixtureCatalog.tags;
 case 'list_actor_tags':return window.fixtureCatalog.actorTags;
 case 'application_data_directory':return 'Z:/FICTIONAL_ONLY/software/设置/保存';
 case 'load_display_preferences':return {defaultFilmRatio:'original',actorRatio:'original',libraryRatios:{}};
 case 'load_interface_preferences':return {reduceMotion:true,thinScrollbar:true,filmCoverRatio:'original',actorPhotoRatio:'original',filmCoverFit:'cover',actorPhotoFit:'cover'};
 case 'load_application_options':return {closeWindowExits:false};
 case 'load_player_settings':return {playerMode:'internal',playerPath:'',launchArguments:'',internalDisplay:'overlay'};
 case 'load_launch_history':return {films:[],events:[]};
 case 'load_translation_settings':return {settings:{enabled:false,provider:'deepl',deepl_plan:'free',target_language:'ZH-HANS',azure_region:'',baidu_app_id:''},key_configured:false,stats:{pending:0,automatic:0,memory:0,local_reuse:0,stale:0,api_requests:0}};
 case 'diagnostic_log_status':return {available:true,maxFileBytes:5242880,maxFiles:5};
 case 'load_translation_key':return null;
 case 'list_library_sources':return [{id:'s1',name:'设置',root_path:'Z:/FICTIONAL_ONLY/保存/东京国际电影节修复特别收藏版本',observation_count:2,film_count:2,video_bytes:1234567890,last_scanned_at:null,enabled:true,removed:false}];
 case 'list_data_backups':return [{id:'synthetic-backup-0001',created_at_unix_ms:1760000000000}];
 case 'load_ignored_directory_names':return [];
 case 'load_cover_thumbnail':case 'load_actor_thumbnail':throw 'cover_missing';
 case 'window_titlebar_action':return false;
 case 'embedded_player_state':return window.syntheticPlayer||null;
 case 'embedded_player_layout':return;
 case 'embedded_player_close':window.syntheticPlayer=null;window.emitNative('embedded-player-changed',null);return;
 default:window.unhandledI18nCommands??=[];window.unhandledI18nCommands.push(c);return [];
 }}
`
const events = `export async function listen(name,callback){const fn=e=>{if(e.detail.name===name)callback({payload:e.detail.payload})};window.addEventListener('synthetic-native-event',fn);return()=>window.removeEventListener('synthetic-native-event',fn)};export async function emit(name,payload){window.emitNative(name,payload)}`
const server = await createServer({ configFile: false, root: fileURLToPath(new URL('..', import.meta.url)), cacheDir: 'node_modules/.vite-i18n-browser-test', optimizeDeps: { entries: ['index.html'] }, plugins: [react(), { name: 'i18n-native-fixture', enforce: 'pre', resolveId(id) { if (['@tauri-apps/api/core', '@tauri-apps/api/event'].includes(id)) return '\0' + id }, load(id) { if (id === '\0@tauri-apps/api/core') return core; if (id === '\0@tauri-apps/api/event') return events } }], server: { host: '127.0.0.1', port: 0, watch: { ignored: ['**/tmp/**', '**/src-tauri/target/**'] } } })
await server.listen()
const browser = await chromium.launch({ channel: 'msedge', headless: true })
const reports = []
const failures = []
async function choose(page, name, text) {
  const control = page.getByRole('combobox', { name, exact: true })
  await control.scrollIntoViewIfNeeded(); await control.click()
  const options = await page.getByRole('option').allTextContents()
  const index = options.findIndex(value => value.trim() === text)
  assert.ok(index >= 0, `No option ${text}: ${JSON.stringify(options)}`)
  await page.keyboard.press('Home')
  for (let i = 0; i < index; i++) await page.keyboard.press('ArrowDown')
  await page.keyboard.press('Enter')
}
async function noOverflow(page, context) {
  const state = await page.evaluate(() => {
    const visible = [...document.querySelectorAll('body *')].filter(element => {
      const rect = element.getBoundingClientRect(); const css = getComputedStyle(element)
      return rect.width && rect.height && css.visibility !== 'hidden' && !element.closest('[inert]') && rect.bottom > 0 && rect.top < innerHeight
    })
    const offenders = visible.filter(element => {
      const rect = element.getBoundingClientRect()
      if (rect.right <= innerWidth + 1 && rect.left >= -1) return false
      for (let parent = element.parentElement; parent; parent = parent.parentElement) {
        const css = getComputedStyle(parent); const box = parent.getBoundingClientRect()
        if (['hidden', 'clip', 'auto', 'scroll'].includes(css.overflowX) && box.right <= innerWidth + 1 && box.left >= -1) return false
      }
      return true
    }).map(element => ({ selector: element.tagName.toLowerCase() + (element.id ? '#' + element.id : '') + [...element.classList].map(name => '.' + name).join(''), text: element.textContent?.slice(0, 100), left: Math.round(element.getBoundingClientRect().left), right: Math.round(element.getBoundingClientRect().right) })).slice(0, 12)
    return { width: innerWidth, scrollWidth: document.documentElement.scrollWidth, offenders }
  })
  assert.equal(state.scrollWidth <= state.width + 1 && state.offenders.length === 0, true, `${context}: horizontal overflow ${JSON.stringify(state)}`)
}
async function snapshot(page, name) {
  assert.equal(await page.locator('.nav-label').textContent(), 'NAVIGATION BAR', 'navigation kicker stays in the original English in every language')
  if (await page.locator('.breadcrumb').count()) {
    const index = await page.locator('.nav-item').evaluateAll(items => items.findIndex(item => item.getAttribute('aria-current') === 'page'))
    const suffix = ['FILMS', 'ACTORS', 'ACTOR DRAW', 'HISTORY', 'ISSUES', 'SETTINGS'][index]
    assert.equal((await page.locator('.breadcrumb').textContent()).replace(/\s+/g, ' ').trim(), `COLLECTION / ${suffix}`, 'page breadcrumb retains the previous English format')
  }
  // Wait for committed layout and finite entrance animations, never mutate CSS.
  await page.evaluate(async () => {
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))
    await Promise.allSettled(document.getAnimations().filter(animation => Number.isFinite(Number(animation.effect?.getComputedTiming().endTime))).map(animation => animation.finished))
  })
  await noOverflow(page, name); await page.screenshot({ path: `${output}/${name}.png`, fullPage: true })
}
async function changeEvent(page, language) {
  await page.evaluate(language => { localStorage.setItem('synthetic-language', language); window.emitNative('interface-language-changed', language) }, language)
  await page.waitForFunction(language => document.documentElement.lang === language, language)
}
async function openNavigation(page, locale, key) {
  const nav = page.getByRole('navigation', { name: label(locale, '主导航'), exact: true })
  if (!await nav.isVisible()) await page.locator('.navigation-toggle').click()
  await nav.getByRole('button', { name: label(locale, key), exact: true }).click()
  await page.waitForFunction(name => [...document.querySelectorAll('.nav-item')].some(element => element.getAttribute('aria-label') === name && element.getAttribute('aria-current') === 'page'), label(locale, key))
}
async function newFixture(locale, width, extra = {}) {
  const page = await browser.newPage({ viewport: { width, height: 900 } })
  page.setDefaultTimeout(8000)
  const errors = []
  page.on('pageerror', error => errors.push(error.message))
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()) })
  await page.route('**/favicon.ico', route => route.fulfill({ status: 200, body: '' }))
  await page.addInitScript(({ fixture, locale, extra }) => {
    window.fixtureCatalog = fixture
    if (!localStorage.getItem('synthetic-language')) localStorage.setItem('synthetic-language', locale)
    Object.assign(window, extra)
    window.emitNative = (name, payload) => window.dispatchEvent(new CustomEvent('synthetic-native-event', { detail: { name, payload } }))
  }, { fixture, locale, extra })
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`)
  await page.locator('.film-card').first().waitFor()
  return { page, errors }
}

try {
  for (const locale of (process.env.I18N_BROWSER_FOCUS || process.env.I18N_BROWSER_RESIDUAL ? [] : locales)) for (const width of widths) {
    let page
    const name = `${locale}-${width}`
    try {
      const created = await newFixture(locale, width); page = created.page
      await page.waitForFunction(language => document.documentElement.lang === language, locale)
      // Independent literal anchors catch wrong language selection/catalog wiring.
      const anchor = { 'zh-CN': '影片库', 'zh-TW': '影片庫', en: 'Movies', ja: '動画ライブラリ' }[locale]
      assert.equal(await page.locator('#library-heading').textContent(), anchor)
      assert.equal(await page.title(), label(locale, 'THE ARCHIVE  私人影像收藏室').replace(/\s+/g, ' '), 'document title follows language')
      assert.equal(await page.getByRole('button', { name: label(locale, '最小化窗口'), exact: true }).count(), 1)
      assert.deepEqual(await page.locator('.card-title').allTextContents(), [literalFilm, longTitle])
      await snapshot(page, `${name}-films`)
      const next = locale === 'en' ? 'ja' : 'en'
      const search = page.getByRole('searchbox', { name: label(locale, '搜索影片名或编号'), exact: true })
      await search.fill('设置')
      await choose(page, label(locale, '片库'), '设置')
      await changeEvent(page, next)
      assert.equal(await page.title(), label(next, 'THE ARCHIVE  私人影像收藏室').replace(/\s+/g, ' '), 'document title updates live')
      assert.equal(await page.getByRole('searchbox', { name: label(next, '搜索影片名或编号'), exact: true }).inputValue(), '设置', 'search survives language event')
      assert.equal(await page.getByRole('combobox', { name: label(next, '片库'), exact: true }).textContent(), '设置', 'source selection survives language event')
      assert.deepEqual(await page.locator('.card-title').allTextContents(), [literalFilm, longTitle], 'Chinese user titles remain literal')
      await changeEvent(page, locale)
      await page.locator('.film-card').first().click()
      const detail = page.getByRole('dialog', { name: literalFilm, exact: true })
      await detail.waitFor(); assert.ok((await detail.innerText()).includes(literalActor))
      await detail.getByRole('button', { name: label(locale, '关闭详情'), exact: true }).waitFor()
      await snapshot(page, `${name}-film-detail`)
      await changeEvent(page, next)
      await detail.getByRole('button', { name: label(next, '关闭详情'), exact: true }).click()
      await changeEvent(page, locale)
      await openNavigation(page, locale, '演员库')
      assert.equal(await page.locator('#actors-heading').textContent(), label(locale, '演员库'))
      assert.equal(await page.locator('.actor-card-name').textContent(), literalActor)
      await snapshot(page, `${name}-actors`)
      await page.locator('.actor-card').first().click()
      await page.getByRole('button', { name: label(locale, '编辑演员资料'), exact: true }).click()
      const editor = page.locator('.actor-editor-form')
      await editor.waitFor()
      await editor.getByRole('textbox', { name: label(locale, '姓名'), exact: true }).fill('保存 · Aleksandra Wiśniewska-Kowalczyk')
      await changeEvent(page, next)
      assert.equal(await editor.getByRole('textbox', { name: label(next, '姓名'), exact: true }).inputValue(), '保存 · Aleksandra Wiśniewska-Kowalczyk', 'actor draft survives language event')
      await changeEvent(page, locale)
      assert.equal(await editor.getByRole('textbox', { name: label(locale, '姓名'), exact: true }).inputValue(), '保存 · Aleksandra Wiśniewska-Kowalczyk', 'actor draft survives language event')
      await snapshot(page, `${name}-actor-editor`)
      await editor.getByRole('button', { name: label(locale, '取消'), exact: true }).click()
      await page.locator('.actor-back').click()
      for (const [key, heading, headingKey] of [['翻牌', '#actor-flip-title', '演员翻牌'], ['播放历史', '#history-title', '播放历史'], ['问题中心', '#issues-title', '问题中心']]) {
        await openNavigation(page, locale, key)
        assert.equal(await page.locator(heading).textContent(), label(locale, headingKey))
        await snapshot(page, `${name}-${key}`)
      }
      await openNavigation(page, locale, '设置')
      await page.locator('#settings-title').waitFor()
      assert.equal(await page.locator('#settings-title').textContent(), label(locale, '设置'))
      await snapshot(page, `${name}-settings`)
      await page.getByRole('button', { name: label(locale, '移除'), exact: true }).click()
      const remove = page.getByRole('group', { name: label(locale, '确认移除目录'), exact: true })
      assert.ok((await remove.innerText()).includes('设置'))
      await snapshot(page, `${name}-remove-confirm`)
      await remove.getByRole('button', { name: label(locale, '取消'), exact: true }).click()
      await page.locator('input[name="restore-backup"]').check()
      for (const [action, heading] of [['恢复所选', '确认恢复软件资料'], ['删除所选', '删除所选备份'], ['恢复初始状态', '警告']]) {
        await page.getByRole('button', { name: label(locale, action), exact: true }).click()
        const dialog = page.getByRole('dialog', { name: label(locale, heading), exact: true })
        await dialog.waitFor(); await snapshot(page, `${name}-${action}`)
        await dialog.getByRole('button', { name: label(locale, '取消'), exact: true }).click()
      }
      // The selector applies only committed writes and reload reads that value.
      await choose(page, label(locale, '界面语言'), next === 'ja' ? '日本語' : 'English')
      await page.waitForFunction(language => document.documentElement.lang === language, next)
      assert.equal(await page.locator('#settings-title').textContent(), label(next, '设置'))
      await page.evaluate(() => { window.failLanguageSave = true })
      await choose(page, label(next, '界面语言'), '繁體中文')
      await page.getByRole('alert').filter({ hasText: label(next, '语言设置未保存，请重试。') }).waitFor()
      assert.equal(await page.evaluate(() => document.documentElement.lang), next, 'failed save retains current language')
      await page.reload(); await page.locator('.film-card').first().waitFor()
      await page.waitForFunction(language => document.documentElement.lang === language, next)
      assert.equal(await page.locator('#library-heading').textContent(), label(next, '影片库'), 'reload retains committed language')
      // Live player controls must retranslate without ending playback.
      await page.evaluate(({ title, path }) => {
        window.syntheticPlayer = { token: 1, window: 'main', filmId: 'f1', videoId: 'v1', title, videos: [{ id: 'v1', name: path }], tracks: [{ id: '1', kind: 'audio', title: '设置', selected: true }], position: 12, duration: 120, seeking: false, paused: true, stopped: false, loading: false, volume: 80, speed: 1, fullscreen: false, aspect: 'original', error: null }
        window.emitNative('embedded-player-opening', structuredClone(window.syntheticPlayer))
      }, { title: longTitle, path: longPath })
      await page.locator('.player-shell').waitFor()
      await page.getByRole('slider', { name: label(next, '播放进度'), exact: true }).waitFor()
      await changeEvent(page, locale)
      assert.equal(await page.getByRole('slider', { name: label(locale, '播放进度'), exact: true }).inputValue(), '12')
      assert.ok((await page.locator('.player-heading').innerText()).includes(longTitle))
      await snapshot(page, `${name}-player`)
      assert.deepEqual(created.errors, [], 'no application runtime errors')
      const unhandled = await page.evaluate(() => [...new Set(window.unhandledI18nCommands || [])])
      reports.push({ name, passed: true, unhandled }); console.log(`PASS ${name}: six pages, literal content, details, actor draft, dialogs, selector/reload/failure, live player, overflow`)
    } catch (error) {
      const detail = String(error.stack || error)
      failures.push({ name, error: detail }); console.error(`FAIL ${name}: ${detail}`)
      if (page) { await page.screenshot({ path: `${output}/${name}-FAIL.png`, fullPage: true }).catch(() => {}); await fs.writeFile(`${output}/${name}-FAIL.html`, await page.content()) }
    } finally { await page?.close() }
  }
  // A failed initial read is visible, must never overwrite storage with a default,
  // and retry must recover the saved language through the real setting component.
  for (const width of (process.env.I18N_BROWSER_FOCUS || process.env.I18N_BROWSER_RESIDUAL ? [] : widths)) {
    const { page, errors } = await newFixture('ja', width, { failLanguageRead: true })
    try {
      await openNavigation(page, 'zh-CN', '设置')
      await page.getByRole('alert').filter({ hasText: '语言设置读取失败，请重试。' }).waitFor()
      assert.equal(await page.getByRole('combobox', { name: '界面语言', exact: true }).isDisabled(), true)
      assert.equal(await page.evaluate(() => window.i18nCommands.filter(({ c }) => c === 'save_interface_language').length), 0, 'read failure must not save a default')
      await page.evaluate(() => { window.failLanguageRead = false })
      await page.locator('.settings-language').getByRole('button', { name: '重新读取', exact: true }).click()
      await page.waitForFunction(() => document.documentElement.lang === 'ja')
      assert.equal(await page.getByRole('combobox', { name: label('ja', '界面语言'), exact: true }).isDisabled(), false)
      await snapshot(page, `read-retry-${width}`); assert.deepEqual(errors, [])
      reports.push({ name: `read-retry-${width}`, passed: true }); console.log(`PASS read-retry-${width}`)
    } catch (error) { failures.push({ name: `read-retry-${width}`, error: String(error.stack || error) }); console.error(`FAIL read-retry-${width}: ${error}`) } finally { await page.close() }
  }
  // Dedicated cases for software-owned strings nested inside interpolation.
  // User tags/titles deliberately match Chinese catalog keys and stay literal.
  for (const width of (process.env.I18N_BROWSER_RESIDUAL ? [] : process.env.I18N_BROWSER_FOCUS ? [390] : widths)) {
    const { page, errors } = await newFixture('en', width)
    const check = async (key, run) => {
      const name = `nested-${key}-${width}`
      try { await run(); await snapshot(page, name); reports.push({ name, passed: true }); console.log(`PASS ${name}`) }
      catch (error) { failures.push({ name, error: String(error.stack || error) }); console.error(`FAIL ${name}: ${error}`); await page.screenshot({ path: `${output}/${name}-FAIL.png`, fullPage: true }) }
    }
    try {
      await page.waitForFunction(() => document.documentElement.lang === 'en')
      await check('ratio-aria', async () => {
        const aria = await page.locator('.motion-page:not([hidden]) .ratio-button').getAttribute('aria-label')
        assert.ok(!/[\u3400-\u9fff]/u.test(aria), `Software ratio accessible name must be English: ${aria}`)
        assert.match(aria, /16:9/, 'accessible name explains the next ratio')
      })
      await page.getByRole('button', { name: label('en', '影片管理'), exact: true }).click()
      await page.locator('.film-card').first().click()
      await choose(page, label('en', '批量操作目标标签'), '保存')
      await page.getByRole('button', { name: label('en', '添加标签'), exact: true }).click()
      await page.locator('.bulk-notice').waitFor()
      await check('batch-notice', async () => {
        const notice = await page.locator('.bulk-notice').textContent()
        assert.match(notice, /Added|added/, `successful batch notice describes adding in English: ${notice}`)
        assert.ok(notice.includes('保存'), `tag remains literal: ${notice}`)
        assert.ok(!/[\u3400-\u9fff]/u.test(notice.replaceAll('保存', '')), `only the user tag may contain Han characters: ${notice}`)
      })
      await page.getByRole('button', { name: label('en', '退出影片管理'), exact: true }).click()
      await page.evaluate(() => { window.failDirectoryRead = true })
      await page.locator('.film-card').first().click()
      const detail = page.getByRole('dialog', { name: literalFilm, exact: true })
      await detail.locator('.real-detail-empty').waitFor()
      await check('directory-error', async () => {
        assert.equal(await detail.locator('.real-detail-empty').textContent(), label('en', '目录无法完整读取，请检查访问权限。'))
      })
      await detail.getByRole('button', { name: label('en', '关闭详情'), exact: true }).click()
      await openNavigation(page, 'en', '设置')
      await check('scan-time-fallback', async () => {
        const status = await page.locator('.settings-library-row').innerText()
        assert.ok(status.includes(label('en', '未记录')), `source scan time fallback must be English: ${status}`)
      })
      await page.evaluate(() => { window.fixtureProblems = [{ id: 'p1', kind: 'film_missing_cover', subject_id: 'f1', title: '设置', reason: '', videos: [], directories: [], directory_options: [], expected_file: 'poster.jpg', sources: [{ id: 's1', name: '保存' }], handled: false }] })
      await openNavigation(page, 'en', '问题中心')
      await page.locator('.issue-type-tag').waitFor()
      await check('problem-aria', async () => {
        const aria = await page.locator('.issue-type-tag').getAttribute('aria-label')
        assert.ok(aria.includes('设置'), `user problem title stays literal: ${aria}`)
        assert.ok(!/[\u3400-\u9fff]/u.test(aria.replaceAll('设置', '')), `software problem kind must be English: ${aria}`)
      })
      await check('problem-search', async () => {
        await page.getByRole('searchbox', { name: label('en', '搜索问题或对象'), exact: true }).fill('cover')
        await page.locator('.issue-row').waitFor()
        await changeEvent(page, 'ja')
        assert.equal(await page.getByRole('searchbox', { name: label('ja', '搜索问题或对象'), exact: true }).inputValue(), 'cover')
        await page.waitForFunction(() => !document.querySelector('.issue-row'))
        await changeEvent(page, 'en')
        await page.locator('.issue-row').waitFor()
        assert.equal(await page.getByRole('searchbox', { name: label('en', '搜索问题或对象'), exact: true }).inputValue(), 'cover', 'issue search persists and reevaluates under locale changes')
      })
      await openNavigation(page, 'en', '演员库')
      await page.getByRole('button', { name: label('en', '新增演员'), exact: true }).click()
      await page.getByRole('button', { name: label('en', '扫描添加'), exact: true }).click()
      await page.getByRole('button', { name: label('en', '选择目录'), exact: true }).click()
      await page.locator('.actor-management-error').waitFor()
      await check('actor-import-error', async () => {
        const error = await page.locator('.actor-management-error').textContent()
        assert.equal(error, label('en', '{0}个目录无法导入。{1}', [1, label('en', '个人资料格式或范围不正确，请检查后重试。')]))
      })
      assert.deepEqual(errors, [], 'no application runtime or console errors')
    } catch (error) { failures.push({ name: `nested-scenario-${width}`, error: String(error.stack || error) }); console.error(`FAIL nested-scenario-${width}: ${error}`) } finally { await page.close() }
  }
  for (const [kind, extra] of [['startup-error', { failStartup: true }], ['partial-actor-import', { partialActorImport: true }]]) {
    const { page, errors } = await newFixture('en', 390, extra)
    const name = `residual-${kind}-390`
    try {
      await page.waitForFunction(() => document.documentElement.lang === 'en')
      if (kind === 'startup-error') {
        const alert = page.locator('.library-startup-feedback[role="alert"]')
        await alert.waitFor()
        const text = await alert.textContent()
        assert.ok(text.includes(label('en', '启动资料暂时无法读取，请重试读取资料。')), `startup failure message must follow English locale: ${text}`)
        assert.ok(!/[\u3400-\u9fff]/u.test(text), `startup prompt must contain only software-owned English: ${text}`)
        await snapshot(page, `${name}-error`)
        await page.evaluate(() => { window.failStartup = false })
        await alert.getByRole('button', { name: label('en', '重试读取资料'), exact: true }).click()
        await alert.waitFor({ state: 'hidden' })
      } else {
        await openNavigation(page, 'en', '演员库')
        await page.getByRole('button', { name: label('en', '新增演员'), exact: true }).click()
        await page.getByRole('button', { name: label('en', '扫描添加'), exact: true }).click()
        await page.getByRole('button', { name: label('en', '选择目录'), exact: true }).click()
        await page.getByRole('button', { name: label('en', '确认添加'), exact: true }).click()
        const notice = page.locator('.actor-management-notice')
        await notice.waitFor()
        const text = await notice.textContent()
        assert.ok(!/[\u3400-\u9fff]/u.test(text), `partial import success must translate its failed count suffix: ${text}`)
        assert.match(text, /1/, 'partial result displays imported/failed counts')
        assert.equal(text, label('en', '已新增 {0} 位演员，更新 {1} 位，跳过 {2} 位。{3}', [1, 0, 0, label('en', '另有 {0} 个目录无法导入。', [1])]))
      }
      await snapshot(page, name); assert.deepEqual(errors, [])
      reports.push({ name, passed: true }); console.log(`PASS ${name}`)
    } catch (error) { failures.push({ name, error: String(error.stack || error) }); console.error(`FAIL ${name}: ${error}`); await page.screenshot({ path: `${output}/${name}-FAIL.png`, fullPage: true }) } finally { await page.close() }
  }
} finally {
  await browser.close(); await server.close()
  await fs.writeFile(`${output}/report.json`, JSON.stringify({ reports, failures }, null, 2))
}
assert.equal(failures.length, 0, `${failures.length} browser regressions failed; see ${output}/report.json`)
