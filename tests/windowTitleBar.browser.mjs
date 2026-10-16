import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'
import react from '@vitejs/plugin-react'

// Set PLAYWRIGHT_MODULE_DIR to the installed/bundled node_modules directory.
// Only window IPC is mocked; HTML dialog top-layer, focus and hit testing are native Chromium.
const require = createRequire(import.meta.url)
const { chromium } = process.env.PLAYWRIGHT_MODULE_DIR
  ? require(require.resolve('playwright', { paths: [process.env.PLAYWRIGHT_MODULE_DIR] }))
  : require('playwright')
const server = await createServer({
  configFile: false,
  root: fileURLToPath(new URL('..', import.meta.url)),
  cacheDir: 'node_modules/.vite-window-titlebar-test',
  optimizeDeps: { entries: ['tests/helpers/windowTitleBarFixture.tsx'] },
  plugins: [react(), { name: 'caption-fixture', configureServer(vite) {
    vite.middlewares.use('/__window-titlebar-test', async (_request, response) => {
      response.setHeader('Content-Type', 'text/html')
      response.end(await vite.transformIndexHtml('/__window-titlebar-test', '<!doctype html><html><body><div id="root"></div><script type="module" src="/tests/helpers/windowTitleBarFixture.tsx"></script></body></html>'))
    })
  } }],
  server: { host: '127.0.0.1', port: 0, watch: { ignored: ['**/src-tauri/target/**', '**/output/**', '**/tmp/**'] } },
})
await server.listen()
const address = server.httpServer.address()
const browser = await chromium.launch({ channel: process.env.PLAYWRIGHT_BROWSER_CHANNEL || 'msedge', headless: true })
try {
  for (const viewport of [{ width: 1280, height: 800 }, { width: 390, height: 844 }]) {
    const page = await browser.newPage({ viewport })
    page.setDefaultTimeout(10000)
    const errors = []
    page.on('pageerror', error => { errors.push(error.message); console.error(error.message) })
    await page.goto(`http://127.0.0.1:${address.port}/__window-titlebar-test`, { timeout: 30000, waitUntil: 'domcontentloaded' })
    const caption = page.locator('.window-caption')
    await caption.waitFor()
    assert.equal(await page.getByRole('checkbox', { name: '监控片库目录' }).count(), 0)
    assert.equal(await page.getByRole('checkbox', { name: '关闭窗口时退出程序' }).isChecked(), false)
    await page.locator('#open-first').click()
    await page.waitForFunction(() => document.querySelector('#first .window-caption'), null, { timeout: 5000 })
    assert.equal(await page.locator('#first').evaluate(element => element.matches(':modal')), true)
    await caption.click({ position: { x: 40, y: 18 } })
    assert.equal(await page.evaluate(() => window.captionTestActions.at(-1)), 'drag')
    assert.equal(await page.evaluate(() => document.activeElement.id), 'open-second')
    await page.locator('#background').evaluate(element => element.focus())
    assert.notEqual(await page.evaluate(() => document.activeElement.id), 'background')
    for (let index = 0; index < 8; index++) {
      await page.keyboard.press('Tab')
      // Native Chromium may briefly focus body while cycling via browser chrome;
      // background controls must remain inert throughout the cycle.
      assert.equal(await page.evaluate(() => document.activeElement === document.body || !!document.activeElement.closest('#first')), true)
    }
    assert.equal(await page.evaluate(() => document.elementFromPoint(40, 18)?.closest('.window-titlebar') !== null), true)
    await page.locator('#open-second').click()
    await page.waitForFunction(() => document.querySelector('#second .window-caption'))
    await page.getByRole('button', { name: '最小化窗口' }).click()
    assert.equal(await page.evaluate(() => window.captionTestActions.at(-1)), 'minimize')
    await page.keyboard.press('Escape')
    await page.waitForFunction(() => document.querySelector('#first .window-caption'))
    await caption.click({ position: { x: 40, y: 18 } })
    assert.equal(await page.evaluate(() => window.captionTestActions.at(-1)), 'drag')
    await page.getByRole('button', { name: '取消确认' }).click()
    await page.waitForFunction(() => document.querySelector('.window-titlebar')?.parentElement.tagName !== 'DIALOG')
    assert.equal(await page.evaluate(() => document.activeElement.id), 'open-first')
    await page.getByRole('button', { name: '添加忽略目录' }).click()
    await page.waitForFunction(() => document.querySelector('.ignore-names-dialog .window-caption'))
    await caption.click({ position: { x: 40, y: 18 } })
    assert.equal(await page.evaluate(() => window.captionTestActions.at(-1)), 'drag')
    assert.equal(await page.locator('.ignore-names-dialog').evaluate(element => element.matches(':modal')), true)
    await page.keyboard.press('Escape')
    await page.waitForFunction(() => !document.querySelector('.ignore-names-dialog'))
    await page.getByRole('radio').check()
    for (const action of ['恢复所选', '删除所选']) {
      await page.getByRole('button', { name: action }).click()
      await page.waitForFunction(() => document.querySelector('.settings-restore-dialog:modal .window-caption'))
      await caption.click({ position: { x: 40, y: 18 } })
      assert.equal(await page.evaluate(() => window.captionTestActions.at(-1)), 'drag')
      await page.getByRole('button', { name: '取消', exact: true }).click()
    }
    await page.evaluate(() => window.captionTestClose())
    await page.waitForFunction(() => document.querySelector('[aria-labelledby="exit-dialog-title"] .window-caption'))
    await caption.click({ position: { x: 40, y: 18 } })
    assert.equal(await page.evaluate(() => window.captionTestActions.at(-1)), 'drag')
    assert.equal(await page.getByRole('heading', { name: '托盘不可用，退出程序？' }).count(), 1)
    assert.equal(await page.getByRole('button', { name: '更新并退出' }).count(), 0)
    await page.getByRole('button', { name: '取消', exact: true }).click()
    await page.getByRole('button', { name: '关闭窗口', exact: true }).click()
    assert.equal(await page.evaluate(() => window.captionTestActions.at(-1)), 'close')
    await page.evaluate(() => window.captionTestClose())
    await page.getByRole('button', { name: '退出', exact: true }).click()
    assert.equal(await page.evaluate(() => window.captionTestActions.at(-1)), 'resolve-exit')
    await page.getByRole('button', { name: '取消', exact: true }).click()
    await page.locator('#open-first').click()
    await page.locator('#remove-first').click()
    await page.waitForFunction(() => document.querySelector('.window-titlebar')?.parentElement.tagName !== 'DIALOG')
    assert.equal(await page.locator('.window-titlebar').count(), 1)
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true)
    assert.deepEqual(errors, [])
    console.log(`PASS native modal caption, nested stack, focus, removal and viewport ${viewport.width}×${viewport.height}`)
    await page.close()
  }
} finally { await browser.close(); await server.close() }
