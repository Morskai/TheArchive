import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import {createRequire} from 'node:module'
import {fileURLToPath} from 'node:url'
import {createServer} from 'vite'
import react from '@vitejs/plugin-react'
const require=createRequire(import.meta.url)
const {chromium}=require(require.resolve('playwright',{paths:[process.env.PLAYWRIGHT_MODULE_DIR||'node_modules']}))
const output=process.env.RATIO_TEST_OUTPUT||'src-tauri/target/library-ratios-browser'
await fs.mkdir(output,{recursive:true})
const core=`export const isTauri=()=>true;export const convertFileSrc=p=>p;export async function invoke(c,args){
window.ratioCommands??=[];window.ratioCommands.push({c,args});
const read=()=>JSON.parse(localStorage.getItem('synthetic-ratios')||'{"defaultFilmRatio":"original","actorRatio":"original","libraryRatios":{}}');
switch(c){
case 'load_display_preferences':if(window.failRatioRead)throw 'synthetic';return read();
case 'save_film_cover_ratios':case 'save_actor_photo_ratio':{
 if(window.failRatioSave)throw 'synthetic';if(window.delayRatioSave)await new Promise(r=>setTimeout(r,250));
 const value=read();if(c==='save_actor_photo_ratio')value.actorRatio=args.ratio;else for(const id of args.libraryIds)value.libraryRatios[id]=args.ratio;localStorage.setItem('synthetic-ratios',JSON.stringify(value));return;}
case 'initialize_library_session':return {savedSources:2};
case 'load_actor_catalog':return window.fixtureCatalog;
case 'application_data_directory':return 'Z:/FICTIONAL_ONLY/software';
case 'load_interface_preferences':return {reduceMotion:true,thinScrollbar:true,filmCoverRatio:'original',actorPhotoRatio:'original',filmCoverFit:'cover',actorPhotoFit:'cover'};
case 'load_application_options':return {closeWindowExits:false};
case 'load_player_settings':return {playerMode:'internal',playerPath:'',launchArguments:'',internalDisplay:'overlay'};
case 'load_launch_history':return {films:[],events:[]};
case 'load_translation_settings':return {settings:{enabled:false,provider:'deepl',deepl_plan:'free',target_language:'ZH-HANS',azure_region:'',baidu_app_id:''},key_configured:false,stats:{pending:0,automatic:0,memory:0,local_reuse:0,stale:0,api_requests:0}};
case 'diagnostic_log_status':return {available:true,maxFileBytes:5242880,maxFiles:5};
case 'load_translation_key':return null;
case 'load_cover_thumbnail':case 'load_actor_thumbnail':throw 'cover_missing';
default:return [];}}`
const server=await createServer({configFile:false,root:fileURLToPath(new URL('..',import.meta.url)),cacheDir:'node_modules/.vite-display-ratios-test',optimizeDeps:{entries:['index.html']},plugins:[react(),{name:'mock',enforce:'pre',resolveId(id){if(['@tauri-apps/api/core','@tauri-apps/api/event'].includes(id))return '\0'+id},load(id){if(id==='\0@tauri-apps/api/core')return core;if(id==='\0@tauri-apps/api/event')return 'export const listen=async()=>()=>{};'}}],server:{host:'127.0.0.1',port:0,watch:{ignored:['**/output/**','**/src-tauri/target/**']}}})
await server.listen();const browser=await chromium.launch({channel:'msedge',headless:true})
try {
 for(const width of [1440,960,390]) {
  const page=await browser.newPage({viewport:{width,height:900}});const errors=[]
  page.on('pageerror',e=>errors.push(e.message));await page.route('**/favicon.ico',r=>r.fulfill({status:200,body:''}))
  await page.addInitScript(()=>{
   const film=(id,source)=>({id,title:id,manual_code:'',favorite:false,rating:null,tags:[],actors:[],has_cover:false,videos:[{id:'v'+id,observed_path:id+'/movie.mp4',source_id:source,source_name:source}]})
   const actor={id:'actor',realActorId:'actor',name:'测试演员',aliases:'',birthMonth:'',height:'',cup:'',bust:'',waist:'',hips:'',notes:'',favorite:false,portrait:0,hasPortrait:false,filmIds:[],tags:[],directoryCreatedAt:null}
   window.fixtureCatalog={actors:[actor],actorTags:[],tags:[],films:[film('Movie A','A'),film('Movie B','B')]}
  })
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`)
  const films=page.locator('section[aria-labelledby="library-heading"]');const actors=page.locator('.actor-library')
  const choose=async(name,value)=>{
    const control=page.getByRole('combobox',{name,exact:true});await control.scrollIntoViewIfNeeded();await control.click()
    const labels=await page.getByRole('option').allTextContents();const index=labels.findIndex(label=>label.trim()===value)
    assert.ok(index>=0);await page.keyboard.press('Home');for(let i=0;i<index;i++)await page.keyboard.press('ArrowDown');await page.keyboard.press('Enter')
  }
  const ratio=films.locator('.ratio-button')
  await page.locator('.film-card').first().waitFor();await page.waitForFunction(()=>window.ratioCommands?.some(x=>x.c==='load_display_preferences'))
  await choose('片库','A');assert.equal(await ratio.textContent(),'800:543')
  await ratio.click();await page.waitForFunction(()=>JSON.parse(localStorage.getItem('synthetic-ratios')).libraryRatios.A==='16:9')
  await ratio.click();await page.waitForFunction(()=>JSON.parse(localStorage.getItem('synthetic-ratios')).libraryRatios.A==='5:7')
  await choose('片库','B');assert.equal(await ratio.textContent(),'800:543');await ratio.click()
  await page.waitForFunction(()=>JSON.parse(localStorage.getItem('synthetic-ratios')).libraryRatios.B==='16:9')
  await choose('片库','A');assert.equal(await ratio.textContent(),'5:7')
  await page.getByRole('searchbox',{name:'搜索影片名或编号'}).fill('Movie');await page.getByRole('button',{name:/^清除筛选/}).click()
  assert.equal(await ratio.textContent(),'5:7');assert.equal(await page.getByRole('combobox',{name:'片库',exact:true}).textContent(),'A')
  await choose('片库','全部片库');assert.equal(await ratio.textContent(),'800:543');assert.equal(await ratio.isDisabled(),false)
  const ratios=await page.locator('.film-card').evaluateAll(cards=>cards.map(c=>[c.querySelector('.card-title').textContent,getComputedStyle(c.querySelector('.card-cover')).aspectRatio]))
  assert.deepEqual(ratios.sort(),[['Movie A','800 / 543'],['Movie B','800 / 543']])
  const savedBefore=await page.evaluate(()=>localStorage.getItem('synthetic-ratios'))
  const writesBefore=await page.evaluate(()=>window.ratioCommands.filter(x=>x.c==='save_film_cover_ratios').length)
  for(const [label,css] of [['16:9','16 / 9'],['5:7','5 / 7'],['800:543','800 / 543']]) {
    await ratio.click();assert.equal(await ratio.textContent(),label)
    assert.deepEqual(await page.locator('.card-cover').evaluateAll(covers=>covers.map(c=>getComputedStyle(c).aspectRatio)),[css,css])
  }
  assert.equal(await page.evaluate(()=>localStorage.getItem('synthetic-ratios')),savedBefore)
  assert.equal(await page.evaluate(()=>window.ratioCommands.filter(x=>x.c==='save_film_cover_ratios').length),writesBefore)
  await page.getByRole('searchbox',{name:'搜索影片名或编号'}).fill('Movie');await page.getByRole('button',{name:/^清除筛选/}).click()
  assert.equal(await ratio.textContent(),'800:543')
  await choose('片库','A');assert.equal(await ratio.textContent(),'5:7')
  await choose('片库','B');assert.equal(await ratio.textContent(),'16:9')
  await choose('片库','全部片库');assert.equal(await ratio.textContent(),'800:543')
  assert.equal(await ratio.getAttribute('title'),null)
  await ratio.click();assert.equal(await ratio.textContent(),'16:9')
  await page.screenshot({path:`${output}/films-${width}.png`,fullPage:true})
  const nav=page.getByRole('navigation',{name:'主导航'});await nav.getByRole('button',{name:'演员库',exact:true}).click()
  const photo=actors.locator('.ratio-button');await photo.waitFor();assert.equal(await photo.textContent(),'5:7')
  await photo.click();await page.waitForFunction(()=>JSON.parse(localStorage.getItem('synthetic-ratios')).actorRatio==='3:4');assert.equal(await photo.textContent(),'3:4')
  await page.getByRole('searchbox',{name:'搜索演员姓名或别名'}).fill('测试');await page.getByRole('button',{name:/^清除筛选/}).click();assert.equal(await photo.textContent(),'3:4')
  await photo.click();await page.waitForFunction(()=>JSON.parse(localStorage.getItem('synthetic-ratios')).actorRatio==='2:3')
  await photo.click();await page.waitForFunction(()=>JSON.parse(localStorage.getItem('synthetic-ratios')).actorRatio==='original')
  await photo.focus();await page.keyboard.press('Enter');await page.waitForFunction(()=>JSON.parse(localStorage.getItem('synthetic-ratios')).actorRatio==='3:4')
  await page.screenshot({path:`${output}/actors-${width}.png`,fullPage:true})
  await nav.getByRole('button',{name:'设置',exact:true}).click()
  assert.equal(await page.getByRole('combobox',{name:'影片封面比例',exact:true}).count(),0)
  assert.equal(await page.getByRole('combobox',{name:'演员照片比例',exact:true}).count(),0)
  assert.equal(await page.getByRole('combobox',{name:'影片封面显示方式',exact:true}).count(),1)
  assert.equal(await page.getByRole('combobox',{name:'演员照片显示方式',exact:true}).count(),1)
  const imageRows=page.locator('.settings-image-pair .settings-image-preference')
  await imageRows.first().scrollIntoViewIfNeeded()
  const pair=await imageRows.evaluateAll(rows=>rows.map(r=>{const rect=r.getBoundingClientRect();return {top:rect.top,left:rect.left,right:rect.right}}))
  assert.equal(pair.length,2);assert.ok(Math.abs(pair[0].top-pair[1].top)<1);assert.ok(pair[0].right<pair[1].left)
  await page.screenshot({path:`${output}/settings-${width}.png`})
  await choose('演员照片显示方式','完整显示')
  assert.ok(await page.evaluate(()=>window.ratioCommands.some(x=>x.c==='save_interface_preferences'&&x.args.preserveRatios===true)))
  await page.reload();await page.locator('.film-card').first().waitFor();await page.waitForFunction(()=>!document.querySelector('.ratio-button').disabled)
  assert.equal(await ratio.textContent(),'16:9','overview ratio persists across reload')
  await choose('片库','A');assert.equal(await ratio.textContent(),'5:7')
  await page.evaluate(()=>{window.failRatioSave=true});await ratio.click();await films.getByRole('alert').waitFor();assert.equal(await ratio.textContent(),'5:7')
  await page.evaluate(()=>{window.failRatioSave=false;window.delayRatioSave=true});await ratio.dblclick();await page.waitForTimeout(350);assert.equal(await ratio.textContent(),'800:543')
  await page.reload();await page.locator('.film-card').first().waitFor();await choose('片库','A');assert.equal(await ratio.textContent(),'800:543')
  await nav.getByRole('button',{name:'演员库',exact:true}).click();assert.equal(await photo.textContent(),'3:4')
  for(const section of [films,actors]) {
    const order=await section.locator('.toolbar-actions').evaluate(e=>[...e.children].map(c=>c.className))
    assert.ok(order.indexOf('direction-button')<order.indexOf('card-ratio-control')&&order.indexOf('card-ratio-control')<order.indexOf('density-control'))
    assert.equal(order.filter(name=>name==='toolbar-divider').length,2)
    assert.equal(await section.locator('.card-ratio-control>span').textContent(),'卡片比例')
    assert.equal(await section.locator('.ratio-button').getAttribute('title'),null)
  }
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);assert.deepEqual(errors,[])
  await page.addInitScript(()=>{window.failRatioRead=true})
  await page.reload();await films.getByRole('alert').filter({hasText:'显示比例暂时无法读取，请重试。'}).waitFor()
  assert.equal(await ratio.isDisabled(),true)
  await page.evaluate(()=>{window.failRatioRead=false});await page.getByRole('button',{name:'重试',exact:true}).click()
  await choose('片库','A');await page.waitForFunction(()=>!document.querySelector('.ratio-button').disabled)
  assert.equal(await ratio.textContent(),'800:543');assert.deepEqual(errors,[])
  console.log(`PASS ${width}: remembered overview/no library writes/reload restored, source persistence, ratio cycles/filter preservation, fit pair, write failure/read retry, guard, keyboard, separated toolbar/no tooltip/overflow`)
  await page.close()
 }
} finally {await browser.close();await server.close()}
