import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'
import react from '@vitejs/plugin-react'
const require = createRequire(import.meta.url)
const { chromium } = require(require.resolve('playwright', { paths: [process.env.PLAYWRIGHT_MODULE_DIR || 'node_modules'] }))
const output = process.env.CONTROL_FEEDBACK_OUTPUT || 'src-tauri/target/control-feedback-browser'
await fs.mkdir(output, { recursive: true })
const server = await createServer({ configFile: false, root: fileURLToPath(new URL('..', import.meta.url)), cacheDir: 'node_modules/.vite-control-feedback', optimizeDeps: { entries: ['tests/helpers/controlFeedbackFixture.tsx'] }, plugins: [react(), {
  name: 'feedback-fixture', enforce: 'pre', resolveId(id) { if (id === '@tauri-apps/api/core') return '\0feedback-core' },
  load(id) { if (id === '\0feedback-core') return `export const isTauri=()=>false;export async function invoke(command){if(command==='open_application_data_directory')window.configOpens=(window.configOpens||0)+1;return;} ` },
  configureServer(vite) { vite.middlewares.use('/__feedback', async (_request, response) => { response.setHeader('Content-Type', 'text/html'); response.end(await vite.transformIndexHtml('/__feedback', '<!doctype html><html><head><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="icon" href="data:,"></head><body><div id="root"></div><script type="module" src="/tests/helpers/controlFeedbackFixture.tsx"></script></body></html>')) }) },
}], server: { host: '127.0.0.1', port: 0, watch: { ignored: ['**/output/**', '**/src-tauri/target/**'] } } })
await server.listen()
const browser = await chromium.launch({ channel: 'msedge', headless: true })
try {
  for (const width of [1280, 390]) {
    const page = await browser.newPage({ viewport: { width, height: 800 } }); const errors = []
    page.setDefaultTimeout(10000)
    page.on('pageerror', error => errors.push(error.message))
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/__feedback`)
    const path = page.getByRole('button', { name: '打开配置目录' }), select = page.getByRole('combobox'), wrapper = page.locator('.app-select')
    await path.click()
    const pathFeedback = await path.evaluate(element => { const animations = element.getAnimations(); animations.forEach(animation => { animation.pause(); animation.currentTime = 80 }); return { outline: getComputedStyle(element).outlineWidth, outlineStyle: getComputedStyle(element).outlineStyle, outlineColor: getComputedStyle(element).outlineColor, frames: animations.flatMap(animation => animation.effect.getKeyframes()) } })
    await page.screenshot({ path: `${output}/path-${width}.png` })
    await select.click()
    const selectFeedback = await select.evaluate(element => { const animations = element.getAnimations(); animations.forEach(animation => { animation.pause(); animation.currentTime = 80 }); return { outline: getComputedStyle(element).outlineWidth, outlineStyle: getComputedStyle(element).outlineStyle, outlineColor: getComputedStyle(element).outlineColor, wrapperOutline: getComputedStyle(element.parentElement).outlineStyle, frames: animations.flatMap(animation => animation.effect.getKeyframes()) } })
    await page.screenshot({ path: `${output}/select-${width}.png` })
    if (process.env.BASELINE) { console.log(JSON.stringify({ width, pathFeedback, selectFeedback })); await page.close(); continue }
    for (const state of [pathFeedback, selectFeedback]) {
      assert.equal(state.outlineStyle, 'none', 'mouse click must not flash an outline')
      assert.ok(state.frames.some(frame => frame.filter?.includes('brightness')), 'press uses a restrained brightness cue')
      assert.ok(state.frames.every(frame => !('outline' in frame) && !('outlineWidth' in frame)))
    }
    assert.equal(selectFeedback.wrapperOutline, 'none')
    assert.equal(await page.evaluate(() => window.configOpens), 1)
    await page.getByRole('option', { name: '完整显示', exact: true }).click(); assert.equal(await select.textContent(), '完整显示')
    await page.locator('#action').click(); await page.keyboard.press('Shift+Tab')
    assert.ok(await select.evaluate(element => element.matches(':focus-visible')))
    assert.equal(await wrapper.evaluate(element => getComputedStyle(element).outlineWidth), '2px')
    await page.keyboard.press('ArrowDown'); await page.keyboard.press('Home'); await page.keyboard.press('Enter')
    assert.equal(await select.textContent(), '铺满裁切')
    await page.keyboard.press('Shift+Tab'); assert.ok(await path.evaluate(element => element.matches(':focus-visible')))
    assert.equal(await path.evaluate(element => getComputedStyle(element).outlineWidth), '2px')
    await page.screenshot({ path: `${output}/keyboard-${width}.png` })
    await page.locator('#reduce').click(); await select.click()
    assert.equal(await wrapper.evaluate(element => getComputedStyle(element).outlineStyle), 'none')
    assert.equal(await select.evaluate(element => getComputedStyle(element).outlineStyle), 'none')
    await page.keyboard.press('Escape'); await page.waitForTimeout(280)
    assert.equal(await select.evaluate(element => element.classList.contains('button-acknowledged')), false)
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false); assert.deepEqual(errors, [])
    console.log(`PASS ${width}: mouse click/selection without frame, brightness feedback, keyboard focus/selection, reduced motion, cleanup and overflow`)
    await page.close()
  }
} finally { await browser.close(); await server.close() }
