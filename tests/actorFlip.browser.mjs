import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import { createServer } from 'vite'
import react from '@vitejs/plugin-react'

const require = createRequire(import.meta.url)
const { chromium } = process.env.PLAYWRIGHT_MODULE_DIR
  ? require(require.resolve('playwright', { paths: [process.env.PLAYWRIGHT_MODULE_DIR] })) : require('playwright')
const output = 'src-tauri/target/actor-flip-browser'
await fs.mkdir(output, { recursive: true })
const server = await createServer({ optimizeDeps: { entries: ['index.html', 'tests/helpers/actorFlipFixture.tsx'] }, plugins: [{ name: 'actor-flip-fixture', configureServer(vite) {
  vite.middlewares.use('/__actor-flip-test', async (_request, response) => {
    response.setHeader('Content-Type', 'text/html')
    response.end(await vite.transformIndexHtml('/__actor-flip-test', '<!doctype html><html><head><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="icon" href="data:,"></head><body><div id="root"></div><script type="module" src="/tests/helpers/actorFlipFixture.tsx"></script></body></html>'))
  })
} }], server: { host: '127.0.0.1', port: 0, watch: { ignored: ['**/src-tauri/target/**'] } } })
await server.listen()
const browser = await chromium.launch({ channel: 'msedge', headless: true })
try {
  const motionCheck = await browser.newPage({ viewport: { width: 1280, height: 800 } })
  await motionCheck.goto(`http://127.0.0.1:${server.httpServer.address().port}/__actor-flip-test`)
  await motionCheck.locator('.flip-card').first().waitFor()
  await motionCheck.waitForFunction(() => !document.getAnimations().some(animation => animation.playState === 'running' && animation.effect?.target?.closest?.('.flip-card')))
  await motionCheck.addStyleTag({ content: await fs.readFile('tests/helpers/actorFlipSweepReference.css', 'utf8') })
  const neutralPortrait = await motionCheck.addStyleTag({ content: '.actor-portrait{background:#302820!important}.portrait-photo,.portrait-light{visibility:hidden!important}' })
  await motionCheck.locator('.flip-card').first().click()
  await motionCheck.evaluate(() => {
    const card = document.querySelector('.flip-card')
    const reference = document.createElement('div')
    reference.className = 'reference-flip-sweep'
    reference.style.width = `${card.offsetWidth}px`; reference.style.height = `${card.offsetHeight}px`
    reference.innerHTML = '<span class="reference-flip-sweep-light"></span>'
    document.body.append(reference)
    document.getAnimations().filter(animation => animation.effect?.target?.closest?.('.flip-card,.reference-flip-sweep')).forEach(animation => animation.pause())
  })
  for (const time of [200, 300, 359.2, 450]) {
    const sample = await motionCheck.evaluate(time => {
      document.getAnimations().filter(animation => animation.effect?.target?.closest?.('.flip-card,.reference-flip-sweep')).forEach(animation => { animation.currentTime = time })
      const card = document.querySelector('.flip-card')
      const rotation = new DOMMatrix(getComputedStyle(card.querySelector('.flip-card-inner')).transform)
      const actual = getComputedStyle(card.querySelector('.flip-reveal-light'))
      const reference = getComputedStyle(document.querySelector('.reference-flip-sweep-light'))
      return { angle: Math.abs(Math.atan2(rotation.m13, rotation.m11) * 180 / Math.PI), actual: { opacity: Number(actual.opacity), x: new DOMMatrix(actual.transform).m41 }, reference: { opacity: Number(reference.opacity), x: new DOMMatrix(reference.transform).m41 } }
    }, time)
    assert.ok(sample.angle > 90, `at ${time}ms the sweep must illuminate the revealed front, without a first pass over the back`)
    assert.ok(Math.abs(sample.actual.opacity - sample.reference.opacity) < .005, `at ${time}ms sweep brightness matches the original CSS timing`)
    assert.ok(Math.abs(sample.actual.x - sample.reference.x) < .05, `at ${time}ms sweep position matches the original CSS timing`)
    const actualImage = await motionCheck.locator('.flip-card').first().screenshot()
    const referenceImage = await motionCheck.locator('.reference-flip-sweep').screenshot()
    await fs.writeFile(`${output}/sweep-current-${time}.png`, actualImage)
    await fs.writeFile(`${output}/sweep-reference-${time}.png`, referenceImage)
    const difference = await motionCheck.evaluate(async ({ actual, reference }) => {
      const pixels = async source => {
        const image = new Image(); image.src = `data:image/png;base64,${source}`; await image.decode()
        const canvas = document.createElement('canvas'); canvas.width = image.width; canvas.height = image.height
        const context = canvas.getContext('2d'); context.drawImage(image, 0, 0)
        return [.25, .35, .45, .55, .65, .75].flatMap(x => [...context.getImageData(Math.floor(x * image.width), Math.floor(image.height * .5), 1, 1).data].slice(0, 3))
      }
      const a = await pixels(actual); const b = await pixels(reference)
      return Math.max(...a.map((value, index) => Math.abs(value - b[index])))
    }, { actual: actualImage.toString('base64'), reference: referenceImage.toString('base64') })
    assert.ok(difference <= 4, `at ${time}ms one continuous sweep covers both halves of the card (RGB difference ${difference})`)
  }
  await neutralPortrait.evaluate(element => element.remove())
  await motionCheck.evaluate(() => {
    document.querySelector('.reference-flip-sweep').remove()
    document.getAnimations().filter(animation => animation.effect?.target?.closest?.('.flip-card')).forEach(animation => { animation.currentTime = 450; animation.play() })
  })
  await motionCheck.waitForTimeout(200)
  const sweep = await motionCheck.locator('.flip-reveal-light').first().evaluate(element => ({ opacity: Number(getComputedStyle(element).opacity), background: getComputedStyle(element).backgroundImage, running: element.getAnimations().some(animation => animation.playState === 'running') }))
  assert.ok(sweep.running && sweep.opacity > .05, 'the original broad sweep must remain visible after the card has settled')
  assert.ok(sweep.background.includes('115deg'), 'restore the previous diagonal sweep material')
  await motionCheck.evaluate(() => {
    window.shuffleGaps = []
    const grid = document.querySelector('.flip-grid')
    const round = document.querySelector('.flip-round-bar strong').textContent
    let packed = false
    const check = () => {
      const bounds = grid.getBoundingClientRect()
      const cards = [...grid.querySelectorAll('.flip-card')]
      const spread = Math.max(...cards.map(card => { const box = card.getBoundingClientRect(); return Math.hypot(box.x + box.width / 2 - bounds.x - bounds.width / 2, box.y + box.height / 2 - bounds.y - bounds.height / 2) }))
      if (grid.classList.contains('is-shuffling') && spread < cards[0].offsetWidth * .6) packed = true
      if (packed && document.querySelector('.flip-round-bar strong').textContent === round && grid.classList.contains('is-shuffling') && spread > cards[0].offsetWidth * 1.4) window.shuffleGaps.push(spread)
    }
    const observer = new MutationObserver(check)
    observer.observe(grid, { attributes: true, subtree: true, childList: true })
    const start = performance.now()
    const frame = () => { check(); if (performance.now() - start < 1100) requestAnimationFrame(frame); else observer.disconnect() }
    requestAnimationFrame(frame)
  })
  await motionCheck.getByRole('button', { name: '重新洗牌', exact: true }).click()
  await motionCheck.waitForTimeout(1150)
  assert.deepEqual(await motionCheck.evaluate(() => window.shuffleGaps), [], 'collected cards must stay in the pile until the new round is rendered, without flashing back to their slots')
  const recordBrightness = async (mode, activate) => {
    await motionCheck.evaluate(() => {
      window.cardBrightness = []; window.cardBrightnessDone = false
      const start = performance.now()
      const sample = () => {
        window.cardBrightness.push({ time: Math.round(performance.now() - start), cards: [...document.querySelectorAll('.flip-card')].map(card => ({ opacity: Number(getComputedStyle(card).opacity), revealed: card.classList.contains('is-revealed'), revealing: card.classList.contains('is-revealing') })) })
        if (performance.now() - start < 1250) requestAnimationFrame(sample)
        else window.cardBrightnessDone = true
      }
      requestAnimationFrame(sample)
    })
    await activate()
    await motionCheck.waitForFunction(() => window.cardBrightnessDone)
    const samples = await motionCheck.evaluate(() => window.cardBrightness)
    await fs.writeFile(`${output}/brightness-${mode}.json`, JSON.stringify(samples, null, 2))
    return samples.flatMap(sample => sample.cards.filter(card => card.opacity < .999).map(card => ({ time: sample.time, ...card })))
  }
  const batchDrops = await recordBrightness('batch', () => motionCheck.getByRole('button', { name: '翻开剩余', exact: true }).click())
  await motionCheck.getByRole('button', { name: '重新洗牌', exact: true }).click()
  await motionCheck.waitForFunction(() => !document.getAnimations().some(animation => animation.playState === 'running' && animation.effect?.target?.closest?.('.flip-card')))
  const singleDrops = await recordBrightness('single', () => motionCheck.locator('.flip-card').first().click())
  assert.deepEqual(batchDrops, [], 'batch reveal and sweep completion must keep every card at its normal brightness')
  assert.deepEqual(singleDrops, [], 'revealing one card must keep the other unopened cards at their normal brightness')
  await motionCheck.close()
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } })
  const errors = []
  page.on('pageerror', error => errors.push(error.message))
  await page.addInitScript(() => {
    window.__portraitStarts = 0
    const start = document.startViewTransition?.bind(document)
    if (start) document.startViewTransition = update => {
      window.__portraitStarts++
      return start(() => new Promise(resolve => setTimeout(() => resolve(update()), 60)))
    }
  })
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`)
  await page.getByRole('button', { name: '翻牌', exact: true }).click()
  const first = page.locator('.flip-card').first()
  await first.waitFor()
  await page.waitForFunction(() => !document.getAnimations().some(animation => animation.playState === 'running' && animation.effect?.target?.closest?.('.flip-card')))
  // A second press while a portrait is still being revealed finishes it, without navigating.
  await first.dblclick()
  assert.equal(await page.locator('.actor-profile-page:visible').count(), 0, 'rapid double press must keep the newly revealed portrait on the table')
  assert.equal(await page.locator('.flip-card.is-revealed').count(), 1)
  await page.waitForFunction(() => !document.querySelector('.flip-card.is-revealing'))
  await page.screenshot({ path: `${output}/after-table.png`, fullPage: true })
  await first.click()
  await page.locator('.actor-profile-page').waitFor({ state: 'visible' })
  await page.waitForFunction(() => !document.documentElement.dataset.portraitTravel)
  assert.equal(await page.locator('[style*="view-transition-name"]').count(), 0, 'portrait snapshots must release their names')
  await page.getByRole('button', { name: '返回', exact: true }).click()
  await first.waitFor({ state: 'visible' })
  await page.waitForFunction(() => !document.documentElement.dataset.portraitTravel)
  assert.equal(await first.evaluate(element => document.activeElement === element), true, 'return restores focus to the original card')
  assert.equal(await page.locator('.flip-card.is-revealed').count(), 1)
  await page.getByRole('button', { name: '翻开剩余', exact: true }).click()
  assert.equal(await page.locator('.flip-card.is-revealed').count(), 8)
  const delays = await page.evaluate(() => document.getAnimations().filter(animation => animation.effect?.target?.classList?.contains('flip-card-inner')).map(animation => animation.effect.getTiming().delay).sort((a, b) => a - b))
  assert.deepEqual(delays, [0, 45, 90, 135, 180, 225, 270], 'batch reveals form a short wave')
  assert.equal(await page.locator('.flip-reveal-ring').count(), 0)
  await page.getByRole('button', { name: '重新洗牌', exact: true }).click()
  await page.waitForFunction(() => document.querySelector('.flip-round-bar strong')?.textContent === '02')
  assert.equal(await page.locator('.flip-card.is-revealed').count(), 0)
  // Resize while collecting old cards used to strand a pending shuffle with disabled cards.
  await page.getByRole('button', { name: '重新洗牌', exact: true }).click()
  await page.setViewportSize({ width: 840, height: 900 })
  await page.waitForFunction(() => document.querySelector('.flip-round-bar strong')?.textContent === '03')
  assert.equal(await page.locator('.flip-card:disabled').count(), 0)
  await page.getByRole('button', { name: '重新洗牌', exact: true }).click()
  await page.getByRole('button', { name: '设置', exact: true }).click()
  await page.getByRole('button', { name: '翻牌', exact: true }).click()
  assert.equal(await page.locator('.flip-card:disabled').count(), 0)
  assert.equal(await page.locator('.flip-card.is-revealed').count(), 0)
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await page.locator('.actor-flip-page[data-motion="false"]').waitFor()
  await first.press('Space')
  assert.equal(await page.locator('.flip-card.is-revealed').count(), 1)
  const reducedAnimations = await first.evaluate(element => element.getAnimations({ subtree: true }).filter(animation => animation.effect.getKeyframes().some(frame => frame.transform && frame.transform !== 'none')).map(animation => ({ target: animation.effect.target.className, frames: animation.effect.getKeyframes() })))
  assert.deepEqual(reducedAnimations, [])
  // Keyboard reveal is instant, but a later mouse press still uses portrait travel.
  await page.emulateMedia({ reducedMotion: 'no-preference' })
  await page.locator('.actor-flip-page[data-motion="true"]').waitFor()
  const starts = await page.evaluate(() => window.__portraitStarts)
  await first.click()
  assert.equal(await page.evaluate(() => window.__portraitStarts), starts + 1)
  // Toggle the system preference while the native transition is awaiting its snapshot.
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await page.locator('.actor-profile-page').waitFor({ state: 'visible' })
  await page.waitForFunction(() => !document.documentElement.dataset.portraitTravel)
  await page.getByRole('button', { name: '返回', exact: true }).press('Enter')
  await first.waitFor({ state: 'visible' })
  await first.press('Enter')
  await page.locator('.actor-profile-page').waitFor({ state: 'visible' })
  assert.equal(await page.locator('html[data-portrait-travel]').count(), 0)
  assert.deepEqual(errors, [])
  await page.close()
  for (const viewport of [{ width: 1280, height: 800 }, { width: 840, height: 900 }, { width: 390, height: 844 }, { width: 320, height: 844 }]) {
    const fixture = await browser.newPage({ viewport })
    const fixtureErrors = []
    fixture.on('pageerror', error => fixtureErrors.push(error.message))
    await fixture.goto(`http://127.0.0.1:${server.httpServer.address().port}/__actor-flip-test`)
    await fixture.getByLabel('减少动态效果').check()
    for (const count of [8, 7, 5, 1]) {
      await fixture.getByLabel('人数', { exact: true }).selectOption(String(count))
      await fixture.waitForFunction(count => document.querySelectorAll('.flip-card').length === count, count)
      assert.equal(await fixture.locator('.flip-card').evaluateAll(cards => new Set(cards.map(card => card.dataset.actorId)).size), count)
      assert.equal(await fixture.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true)
      await fixture.locator('.flip-card').first().press('Space')
      assert.equal(await fixture.locator('.flip-card.is-revealed').count(), 1)
      assert.equal(await fixture.locator('.flip-card-rating').first().textContent(), '暂无评分')
      await fixture.getByRole('button', { name: '重新洗牌', exact: true }).press('Enter')
      assert.equal(await fixture.locator('.flip-card.is-revealed').count(), 0)
    }
    const normalWidth = await fixture.locator('.flip-card').first().evaluate(element => element.getBoundingClientRect().width)
    await fixture.getByLabel('人数', { exact: true }).selectOption('0')
    await fixture.getByRole('heading', { name: '暂无可翻牌演员' }).waitFor()
    assert.equal(await fixture.locator('.flip-card').count(), 0)
    const emptyCards = fixture.locator('.empty-card-table img')
    assert.equal(await emptyCards.count(), 8)
    assert.ok(await emptyCards.first().evaluate(element => element.getBoundingClientRect().width) <= normalWidth + 1)
    await fixture.getByLabel('状态', { exact: true }).selectOption('loading')
    await fixture.locator('.actor-flip-page [role="status"]').waitFor()
    await fixture.getByLabel('状态', { exact: true }).selectOption('error')
    await fixture.getByRole('alert').waitFor()
    await fixture.getByLabel('状态', { exact: true }).selectOption('ready')
    await fixture.getByLabel('人数', { exact: true }).selectOption('8')
    await fixture.screenshot({ path: `${output}/table-${viewport.width}.png`, fullPage: true })
    assert.deepEqual(fixtureErrors, [])
    await fixture.close()
  }
  console.log('Actor flip passed: rapid input, portrait travel/focus, batch wave, resize/navigation interruption, keyboard/reduced motion, 0/1/5/7/8 cards and four viewports.')
  if (process.env.RECORD_DEMO) {
    const demo = await browser.newPage({ viewport: { width: 1280, height: 800 }, recordVideo: { dir: output, size: { width: 1280, height: 800 } } })
    const video = demo.video()
    await demo.goto(`http://127.0.0.1:${server.httpServer.address().port}`)
    await demo.getByRole('button', { name: '翻牌', exact: true }).click()
    await demo.waitForTimeout(1100)
    await demo.locator('.flip-card').nth(2).hover()
    await demo.waitForTimeout(300)
    await demo.locator('.flip-card').nth(2).click()
    await demo.waitForTimeout(700)
    await demo.locator('.flip-card').nth(5).click()
    await demo.waitForTimeout(650)
    await demo.locator('.flip-card').nth(2).click()
    await demo.waitForTimeout(800)
    await demo.getByRole('button', { name: '返回', exact: true }).click()
    await demo.waitForTimeout(650)
    await demo.getByRole('button', { name: '重新洗牌', exact: true }).click()
    await demo.waitForTimeout(1100)
    await demo.getByRole('button', { name: '翻开剩余', exact: true }).click()
    await demo.waitForTimeout(1200)
    await demo.screenshot({ path: `${output}/revealed-table.png` })
    await demo.close()
    await video.saveAs(`${output}/actor-flip-demo.webm`)
  }
} finally { await browser.close(); await server.close() }

// Exercise the real-portrait rendering branch with synthetic IPC and in-memory image bytes.
const core = `export const isTauri=()=>true;export const convertFileSrc=p=>p;export async function invoke(command,args){switch(command){
case 'initialize_library_session':return {savedSources:0};
case 'load_actor_catalog':return window.fixtureCatalog;
case 'application_data_directory':return 'Z:/FICTIONAL_ONLY/software';
case 'load_display_preferences':return {defaultFilmRatio:'original',actorRatio:'original',libraryRatios:{}};
case 'load_interface_preferences':return {reduceMotion:false,thinScrollbar:true,filmCoverRatio:'original',actorPhotoRatio:'original',filmCoverFit:'cover',actorPhotoFit:'cover'};
case 'load_application_options':return {closeWindowExits:false};
case 'load_player_settings':return {playerMode:'system',playerPath:'',launchArguments:''};
case 'load_launch_history':return {films:[],events:[]};
case 'load_translation_settings':return {settings:{enabled:false,provider:'deepl',deepl_plan:'free',target_language:'ZH-HANS',azure_region:'',baidu_app_id:''},key_configured:false,stats:{pending:0,automatic:0,memory:0,local_reuse:0,stale:0,api_requests:0}};
case 'load_translation_key':return null;
case 'load_actor_thumbnail':window.imageReads.push(args);if(args.actorId==='missing')throw 'cover_unavailable';return Uint8Array.from(atob(window.fixtureImage),c=>c.charCodeAt(0)).buffer;
default:return [];}}`
const realServer = await createServer({ configFile: false, cacheDir: 'node_modules/.vite-actor-flip-real-test', optimizeDeps: { entries: ['index.html'] }, plugins: [react(), {
  name: 'synthetic-actor-ipc', enforce: 'pre', resolveId(id) { if (['@tauri-apps/api/core', '@tauri-apps/api/event'].includes(id)) return '\0' + id },
  load(id) { if (id === '\0@tauri-apps/api/core') return core; if (id === '\0@tauri-apps/api/event') return 'export const listen=async()=>()=>{};' },
}], server: { host: '127.0.0.1', port: 0, watch: { ignored: ['**/src-tauri/target/**'] } } })
await realServer.listen()
const realBrowser = await chromium.launch({ channel: 'msedge', headless: true })
try {
  const page = await realBrowser.newPage({ viewport: { width: 1280, height: 800 } })
  const errors = []
  page.on('pageerror', error => errors.push(error.message))
  await page.addInitScript(() => {
    window.imageReads = []
    const canvas = document.createElement('canvas'); canvas.width = 64; canvas.height = 90
    const context = canvas.getContext('2d'); context.fillStyle = '#302820'; context.fillRect(0, 0, 64, 90)
    window.fixtureImage = canvas.toDataURL('image/png').split(',')[1]
    const actor = (id, name, hasPortrait) => ({ id, name, hasPortrait, filmIds: [], tags: [], sources: [], favorite: false, portrait: 0, aliases: null, directoryName: name, library: '', directoryCreatedAt: null, birthMonth: '', height: '', cup: '', bust: '', waist: '', hips: '' })
    window.fixtureCatalog = { actors: [actor('photo', '合成演员甲', true), actor('other', '合成演员乙', true), actor('missing', '缺失照片演员', false)], films: [], tags: [], actorTags: [] }
    const start = document.startViewTransition?.bind(document)
    if (start) document.startViewTransition = update => start(async () => { await update(); window.profileSnapshotSrc = document.querySelector('.actor-profile-left .real-actor-photo')?.getAttribute('src') })
  })
  await page.goto(`http://127.0.0.1:${realServer.httpServer.address().port}`)
  await page.getByRole('button', { name: '翻牌', exact: true }).click()
  const card = page.locator('.flip-card[data-actor-id="photo"]')
  await card.locator('.real-actor-photo').waitFor({ state: 'attached' })
  await page.waitForFunction(() => !document.getAnimations().some(animation => animation.playState === 'running' && animation.effect?.target?.closest?.('.flip-card')))
  const source = await card.locator('.real-actor-photo').getAttribute('src')
  const reads = await page.evaluate(() => window.imageReads.filter(read => read.actorId === 'photo' && read.large).length)
  await card.press('Space')
  await card.click()
  await page.locator('.actor-profile-page').waitFor({ state: 'visible' })
  await page.waitForFunction(() => !document.documentElement.dataset.portraitTravel)
  assert.equal(await page.evaluate(() => window.profileSnapshotSrc), source, 'native destination snapshot already contains the cached real portrait')
  assert.equal(await page.evaluate(() => window.imageReads.filter(read => read.actorId === 'photo' && read.large).length), reads)
  await page.getByRole('button', { name: '返回', exact: true }).click()
  await card.waitFor({ state: 'visible' })
  await page.waitForFunction(() => !document.documentElement.dataset.portraitTravel)
  // Unsupported native transitions keep all original navigation and the revealed round.
  await page.evaluate(() => { document.startViewTransition = undefined })
  await card.click()
  await page.locator('.actor-profile-page').waitFor({ state: 'visible' })
  await page.getByRole('button', { name: '返回', exact: true }).click()
  await card.waitFor({ state: 'visible' })
  const missing = page.locator('.flip-card[data-actor-id="missing"]')
  await missing.press('Space'); await missing.press('Enter')
  await page.locator('.actor-profile-page').waitFor({ state: 'visible' })
  assert.equal(await page.locator('.actor-profile-left .actor-portrait-placeholder').count(), 1)
  await page.getByRole('button', { name: '返回', exact: true }).click()
  await page.getByRole('button', { name: '重新洗牌', exact: true }).click()
  await page.evaluate(() => { window.fixtureCatalog.actors = window.fixtureCatalog.actors.filter(actor => actor.id !== 'photo'); window.dispatchEvent(new Event('real-library-changed')) })
  await page.waitForFunction(() => document.querySelectorAll('.flip-card').length === 2 && !document.querySelector('.flip-grid.is-shuffling'))
  assert.equal(await card.count(), 0)
  assert.equal(await page.locator('.flip-card:disabled').count(), 0)
  assert.deepEqual(errors, [])
  await page.close()
  console.log('Synthetic IPC passed: real portrait snapshot reuse/no extra read, missing photos, native API fallback and actor removal during shuffle.')
} finally { await realBrowser.close(); await realServer.close() }
