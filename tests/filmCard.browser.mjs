import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'
import react from '@vitejs/plugin-react'
const require=createRequire(import.meta.url)
const {chromium}=process.env.PLAYWRIGHT_MODULE_DIR?require(require.resolve('playwright',{paths:[process.env.PLAYWRIGHT_MODULE_DIR]})):require('playwright')
const output=process.env.FILM_CARD_OUTPUT||'src-tauri/target/film-card-browser'
await fs.mkdir(output,{recursive:true})
const server=await createServer({configFile:false,root:fileURLToPath(new URL('..',import.meta.url)),cacheDir:'node_modules/.vite-film-card-test',optimizeDeps:{entries:['tests/helpers/filmCardFixture.tsx']},plugins:[react(),{name:'fixture',configureServer(vite){vite.middlewares.use('/__film-card-test',async(_req,res)=>{res.setHeader('Content-Type','text/html');res.end(await vite.transformIndexHtml('/__film-card-test','<!doctype html><html><head><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="icon" href="data:,"></head><body><div id="root"></div><script type="module" src="/tests/helpers/filmCardFixture.tsx"></script></body></html>'))})}}],server:{host:'127.0.0.1',port:0,watch:{ignored:['**/output/**','**/src-tauri/target/**']}}})
await server.listen();const browser=await chromium.launch({channel:'msedge',headless:true})
try {
 for(const viewport of [{width:1280,height:900},{width:800,height:900},{width:390,height:844}]) {
  const page=await browser.newPage({viewport});const errors=[]
  page.on('pageerror',e=>errors.push(e.message));page.on('console',m=>{if(['error','warning'].includes(m.type()))errors.push(m.text())})
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/__film-card-test`)
  const cards=page.locator('.film-card');await cards.first().waitFor()
  const numbered=cards.nth(0),short=cards.nth(1),long=cards.nth(2),pending=cards.nth(3)
  if(process.env.BASELINE){await page.screenshot({path:`${output}/before-${viewport.width}.png`,fullPage:true});await page.close();continue}
  assert.equal(await short.locator('.card-code-row').count(),0)
  assert.equal(await long.locator('.card-code').count(),0)
  assert.equal(await pending.locator('.card-code').textContent(),'编号待核对')
  assert.equal(await short.locator('.card-title').count(),1)
  assert.equal(await long.locator('.card-source').getAttribute('title'),'经典电影与世界各地珍藏版本合集非常长的片库名称')
  for(const density of ['comfortable','standard','compact']) for(const ratio of ['800 / 543','16 / 9','5 / 7']) {
    await page.evaluate(({density,ratio})=>{document.getElementById('cards').className='film-grid density-'+density;document.documentElement.style.setProperty('--film-cover-ratio',ratio)},{density,ratio})
    const metrics=await cards.evaluateAll(elements=>elements.map(card=>{
      const meta=card.querySelector('.card-meta'),cover=card.querySelector('.card-cover'),title=card.querySelector('.card-title'),source=card.querySelector('.card-source'),code=card.querySelector('.card-code'),rating=card.querySelector('.card-rating'),tag=card.querySelector('.card-tags');const rect=e=>{if(!e)return null;const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom}};
      const style=e=>({font:getComputedStyle(e).font,color:getComputedStyle(e).color,letterSpacing:getComputedStyle(e).letterSpacing,userSelect:getComputedStyle(e).userSelect,lineHeight:parseFloat(getComputedStyle(e).lineHeight)})
      return {card:rect(card),meta:rect(meta),cover:rect(cover),title:rect(title),source:rect(source),code:rect(code),titleStyle:style(title),ratingStyle:style(rating),tagStyle:tag?style(tag):null}
    }))
    assert.equal(metrics[0].titleStyle.font,metrics[1].titleStyle.font)
    assert.equal(metrics[0].titleStyle.color,metrics[1].titleStyle.color)
    assert.equal(metrics[0].titleStyle.letterSpacing,metrics[1].titleStyle.letterSpacing)
    for(const index of [1,2,5]) {
      assert.ok(Math.abs(metrics[index].title.y-metrics[index].cover.bottom-(metrics[0].code.y-metrics[0].cover.bottom))<1)
      assert.ok(metrics[index].title.height<=2*metrics[index].titleStyle.lineHeight+1)
      if(metrics[index].source) assert.ok(metrics[index].source.y>=metrics[index].title.bottom)
    }
    for(const m of metrics) {
      assert.ok(m.title.right<=m.card.right+1)
      assert.ok(!m.source||m.source.right<=m.card.right+1)
      assert.notEqual(m.titleStyle.userSelect,'text')
    }
    assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false)
    if(density==='standard'&&ratio==='800 / 543') await page.screenshot({path:`${output}/mixed-${viewport.width}.png`,fullPage:true})
  }
  await page.evaluate(()=>{document.getElementById('cards').className='film-grid density-standard';document.documentElement.style.setProperty('--film-cover-ratio','800 / 543')})
  await numbered.locator('.card-cover').click();await numbered.locator('.card-cover').click({button:'right'})
  assert.deepEqual(await page.evaluate(()=>window.filmCardActions),['open:numbered','play:numbered'])
  await numbered.locator('.card-rating').click();await numbered.locator('.card-tags').click();await numbered.locator('.card-rating').click({button:'right'})
  assert.deepEqual(await page.evaluate(()=>window.filmCardActions),['open:numbered','play:numbered','open:numbered','open:numbered','play:numbered'])
  await numbered.focus();await page.keyboard.press('Enter');await page.keyboard.press('Space')
  assert.equal((await page.evaluate(()=>window.filmCardActions)).length,7)
  await page.locator('#manage').click();await short.locator('.card-rating').click();assert.equal(await short.getAttribute('aria-pressed'),'true')
  await short.locator('.card-cover').click({button:'right'});assert.equal((await page.evaluate(()=>window.filmCardActions)).filter(x=>x==='play:short').length,0)
  await short.locator('.card-tags').click();assert.equal(await short.getAttribute('aria-pressed'),'false')
  assert.equal(await numbered.locator('.favorite-mark').getAttribute('aria-label'),'已收藏')
  assert.equal(await long.locator('.card-title').getAttribute('title'),'一段很长的电影片名用于核对两行省略以及普通影视收藏中的完整标题查看方式')
  assert.deepEqual(errors,[])
  console.log(`PASS ${viewport.width}: 9 density/ratio combinations, mixed/empty/pending/title/source, original card metadata clicks, click/context/keyboard/management`)
  await page.close()
 }
} finally {await browser.close();await server.close()}
