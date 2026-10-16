import assert from 'node:assert/strict'
import {createRequire} from 'node:module'
import {fileURLToPath} from 'node:url'
import {createServer} from 'vite'
import react from '@vitejs/plugin-react'
const require=createRequire(import.meta.url)
const {chromium}=require(require.resolve('playwright',{paths:[process.env.PLAYWRIGHT_MODULE_DIR||'node_modules']}))
const core=`export const isTauri=()=>true;export const convertFileSrc=p=>p;export async function invoke(c,args={}){switch(c){
case 'initialize_library_session':return {savedSources:0};
case 'load_actor_catalog':return window.fixtureCatalog;
case 'application_data_directory':return 'Z:/FICTIONAL_ONLY/software';
case 'open_application_data_directory':window.configOpens=(window.configOpens??0)+1;if(window.failConfigOpen)throw 'directory_open_failed';return;
case 'load_display_preferences':return {defaultFilmRatio:'original',actorRatio:'original',libraryRatios:{}};
case 'load_interface_preferences':return {reduceMotion:true,thinScrollbar:true,filmCoverRatio:'original',actorPhotoRatio:'original',filmCoverFit:'cover',actorPhotoFit:'cover'};
case 'load_application_options':return {closeWindowExits:false};
case 'load_player_settings':return window.savedPlayer??{playerMode:'system',playerPath:'',launchArguments:''};
case 'save_player_settings':if(window.failPlayerSave)throw 'settings_save_failed';window.savedPlayer=args.settings;return;
case 'restart_application':window.restartRequested=(window.restartRequested??0)+1;if(window.failRestart)throw 'restart_failed';return;
case 'load_launch_history':return {films:[],events:[]};
case 'load_translation_settings':return {settings:{enabled:false,provider:'deepl',deepl_plan:'free',target_language:'ZH-HANS',azure_region:'',baidu_app_id:''},key_configured:false,stats:{pending:0,automatic:0,memory:0,local_reuse:0,stale:0,api_requests:0}};
case 'diagnostic_log_status':return {available:true,maxFileBytes:5242880,maxFiles:5};
case 'load_translation_key':return null;
case 'load_cover_thumbnail':case 'load_actor_thumbnail':throw 'cover_missing';
default:return [];}}`
const server=await createServer({configFile:false,root:fileURLToPath(new URL('..',import.meta.url)),cacheDir:'node_modules/.vite-library-reset-test',optimizeDeps:{entries:['index.html']},plugins:[react(),{name:'mock',enforce:'pre',resolveId(id){if(['@tauri-apps/api/core','@tauri-apps/api/event'].includes(id))return '\0'+id},load(id){if(id==='\0@tauri-apps/api/core')return core;if(id==='\0@tauri-apps/api/event')return 'export const listen=async()=>()=>{};'}}],server:{host:'127.0.0.1',port:0,watch:{ignored:['**/output/**','**/src-tauri/target/**']}}})
await server.listen();const browser=await chromium.launch({channel:'msedge',headless:true})
try {
 for(const width of [1280,390]) {
  const page=await browser.newPage({viewport:{width,height:900}});const errors=[]
  await page.route('**/favicon.ico',r=>r.fulfill({status:200,body:''}));page.on('pageerror',e=>errors.push(e.message))
  await page.addInitScript(()=>{const film=(id,source,favorite,rating,tags)=>({id,title:id,manual_code:'',favorite,rating,tags,actors:[],has_cover:false,videos:[{id:'v'+id,observed_path:id+'/movie.mp4',source_id:source,source_name:source}]});window.fixtureCatalog={actors:[],tags:['中文字幕'],films:[film('Match','A',true,5,['中文字幕']),film('Other in A','A',false,null,[]),film('Other in B','B',true,5,['中文字幕'])]}})
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`);await page.locator('.film-card').first().waitFor()
  const choose=async(name,option)=>{await page.getByRole('combobox',{name,exact:true}).click();await page.getByRole('option',{name:option,exact:true}).click()}
  await choose('片库','A');await choose('评分','5星')
  await page.locator('.filter-row').getByRole('button',{name:/已收藏/}).click()
  await page.locator('.tag-filter-preview .tag-filter-choice').first().click()
  await page.getByRole('searchbox',{name:'搜索影片名或编号'}).fill('Match')
  assert.equal(await page.locator('.film-card').count(),1)
  await page.getByRole('button',{name:/^清除筛选/}).click()
  assert.equal(await page.getByRole('combobox',{name:'片库',exact:true}).textContent(),'A')
  assert.equal(await page.getByRole('combobox',{name:'评分',exact:true}).textContent(),'不限')
  assert.equal(await page.getByRole('searchbox',{name:'搜索影片名或编号'}).inputValue(),'')
  assert.equal(await page.locator('.filter-row').getByRole('button',{name:/已收藏/}).getAttribute('aria-pressed'),'false')
  assert.equal(await page.locator('.tag-filter-preview .tag-filter-choice').first().getAttribute('aria-pressed'),'false')
  assert.equal(await page.locator('.film-card').count(),2)
  await page.getByRole('button',{name:/^清除筛选/}).click();assert.equal(await page.locator('.film-card').count(),2)
  await page.getByRole('navigation',{name:'主导航'}).getByRole('button',{name:'设置',exact:true}).click()
  await page.getByRole('button',{name:'打开配置目录',exact:true}).click()
  assert.equal(await page.evaluate(()=>window.configOpens),1)
  await page.evaluate(()=>{window.failConfigOpen=true})
  await page.getByRole('button',{name:'打开配置目录',exact:true}).focus()
  await page.keyboard.press('Enter')
  await page.getByText('配置目录未能打开，请重试。',{exact:true}).waitFor()
  assert.equal(await page.evaluate(()=>window.configOpens),2)
  assert.equal(await page.getByRole('radio',{name:/系统默认播放器/}).count(),0)
  assert.equal(await page.getByText('关闭当前播放器后，下次打开时使用新的显示方式。',{exact:true}).count(),0)
  assert.equal(await page.getByRole('button',{name:'重启软件',exact:true}).count(),0)
  await page.getByRole('radio',{name:/内置播放器/}).check()
  await choose('内置播放器显示方式','独立窗口')
  await page.evaluate(()=>{window.failPlayerSave=true})
  await page.getByRole('button',{name:'保存播放器选项',exact:true}).click()
  await page.getByText(/^选项未保存：/).waitFor()
  assert.equal(await page.getByRole('button',{name:'重启软件',exact:true}).count(),0)
  await page.evaluate(()=>{window.failPlayerSave=false})
  await page.getByRole('button',{name:'保存播放器选项',exact:true}).click()
  await page.getByText('播放器选项已保存。',{exact:true}).waitFor()
  assert.equal(await page.getByRole('button',{name:'重启软件',exact:true}).getAttribute('class'),'settings-action')
  assert.deepEqual(await page.evaluate(()=>window.savedPlayer),{playerMode:'internal',playerPath:'',launchArguments:'',internalDisplay:'window'})
  await page.getByText('播放器选项已保存。',{exact:true}).waitFor()
  await page.evaluate(()=>{window.failRestart=true})
  await page.getByRole('button',{name:'重启软件',exact:true}).click()
  await page.getByText('软件未能重启，请重试。',{exact:true}).waitFor()
  assert.equal(await page.evaluate(()=>window.restartRequested),1)
  const section=page.getByRole('region',{name:'诊断日志',exact:true})
  assert.ok((await section.textContent()).includes('默认开启，帮助定位程序错误'))
  assert.equal(await section.locator('.settings-diagnostic-copy').textContent(),'日志仅保存在本机，最多约 25 MB。导出包含诊断记录，不含 API 密钥、影片名称、演员资料或完整媒体路径。')
  assert.deepEqual(errors,[])
  console.log(`PASS ${width}: reset keeps source while clearing search/rating/favorite/tags, repeated reset, exact diagnostic copy`)
  await page.close()
 }
} finally {await browser.close();await server.close()}
