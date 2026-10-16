import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import { createServer } from 'vite'
import react from '@vitejs/plugin-react'
import { spawn, execFileSync } from 'node:child_process'
import path from 'node:path'
const require = createRequire(import.meta.url)
const { chromium } = require(require.resolve('playwright', { paths: [process.env.PLAYWRIGHT_MODULE_DIR || '.'] }))
const native = process.env.NATIVE_PLAYER_PROBE === '1'
const output = 'output/player-seek-20261015'
await fs.mkdir(output, { recursive: true })
const mock = `
export const isTauri=()=>true;
let display='overlay', token=0, current=null;
window.probeSnapshot=()=>structuredClone(current);
window.addEventListener('probe-timeline',event=>{Object.assign(current,event.detail);window.dispatchEvent(new CustomEvent('probe-player-changed',{detail:{name:'embedded-player-changed',payload:null}}))});
const videos=[{id:'v1',name:'第 1集 演示.avi'},{id:'v2',name:'第 2集 演示.avi'}];
export async function invoke(command,args={}){
 window.probeCalls??=[];window.probeCalls.push({command,...args});
 const emit=(name,payload)=>window.dispatchEvent(new CustomEvent('probe-player-changed',{detail:{name,payload}}));
 const notify=()=>emit('embedded-player-changed',null);
 if(command==='probe_catalog')return [{id:'f1',videos}];
 if(command==='probe_display'){display=args.display;return;}
 if(command==='window_titlebar_action')return false;
 if(command==='write_diagnostic_event')return;
 if(command==='load_display_preferences')return {defaultFilmRatio:'original',actorRatio:'original',libraryRatios:{}};
 if(command==='load_interface_preferences')return {reduceMotion:false};
 if(command==='embedded_player_state')return current?structuredClone(current):null;
 if(command==='launch_film_video'){current={token:++token,window:display==='window'?'player':'main',filmId:'f1',videoId:args.videoId,title:'合成片库 · 一个很长的中文影片标题',videos,tracks:[{id:'1',kind:'audio',title:'中文 · 音轨1',selected:true},{id:'2',kind:'audio',title:'English · 音轨2',selected:false},{id:'3',kind:'sub',title:'中文外挂字幕',selected:true}],position:12,duration:120,paused:true,stopped:false,volume:80,speed:1,fullscreen:false,aspect:'original',error:null,loading:true};emit('embedded-player-opening',structuredClone(current));await new Promise(resolve=>setTimeout(resolve,900));current.loading=false;if(window.probeFailLoad){current.stopped=true;current.error='internal_player_unavailable';notify();throw 'internal_player_unavailable'}current.paused=false;window.probeLoadedAt=performance.now();notify();return {historySaved:true};}
 if(command==='embedded_player_layout')return;
 if(command==='embedded_player_close'){current=null;notify();await new Promise(resolve=>setTimeout(resolve,1000));return;}
 if(command==='embedded_player_choose_subtitle')return;
 if(command==='embedded_player_control'){
  if(args.token!==current?.token)throw 'player_replaced';
  if(current.error==='internal_player_unavailable')throw 'player_unavailable';
  if(args.action==='stop')current.stopped=true;
  if(args.action==='pause'){current.paused=current.stopped?false:!current.paused;current.stopped=false;}
  if(args.action==='volume')current.volume=+args.value;
  if(args.action==='skip')current.position=Math.max(0,Math.min(current.duration,current.position+(+args.value)));
  if(args.action==='seek'){
   if(window.probeSeekFail)throw 'player_action_failed';
   const session=current, serial=session.seekSerial=(session.seekSerial??0)+1, delay=window.probeSeekDelay??0, decode=window.probeSeekDecodeDelay??0;
   session.seeking=true;
   const finish=()=>{if(session.seekSerial===serial){session.position=+args.value;session.seeking=false;notify()}};
   if(decode)setTimeout(finish,decode);else finish();
   if(delay)await new Promise(resolve=>setTimeout(resolve,delay));
  }
  if(args.action==='speed')current.speed=+args.value;
  if(args.action==='fullscreen')current.fullscreen=!current.fullscreen;
  if(args.action==='aspect')current.aspect=args.value;
  if(args.action==='audio'||args.action==='subtitle')current.tracks.filter(t=>t.kind===(args.action==='audio'?'audio':'sub')).forEach(t=>t.selected=t.id===args.value);
  return;
 }
 throw 'unsupported_probe_command';
}
`
const server = await createServer({ configFile: false, cacheDir: `node_modules/.vite-player-${native ? 'native' : 'mock'}`, plugins: [react(), {
  name: 'player-fixture', enforce: 'pre',
  resolveId(id) { if (!native && ['@tauri-apps/api/core', '@tauri-apps/api/event'].includes(id)) return '\0' + id },
  load(id) { if (id === '\0@tauri-apps/api/core') return mock; if (id === '\0@tauri-apps/api/event') return `export async function listen(name,callback){const fn=event=>{if(event.detail?.name===name)callback({payload:event.detail.payload})};window.addEventListener('probe-player-changed',fn);return ()=>window.removeEventListener('probe-player-changed',fn)}` },
  configureServer(vite) { vite.middlewares.use('/__native-player-probe', async (_request, response) => {
    response.setHeader('Content-Type', 'text/html')
    response.end(await vite.transformIndexHtml('/__native-player-probe', '<!doctype html><html><head><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="icon" href="data:,"></head><body><div id="root"></div><script type="module" src="/tests/helpers/embeddedPlayerFixture.tsx"></script></body></html>'))
  }) },
}], optimizeDeps: { entries: ['tests/helpers/embeddedPlayerFixture.tsx', 'index.html'] }, server: { host: '127.0.0.1', port: native ? 15173 : 15174, strictPort: true, watch: { ignored: ['**/src-tauri/target/**', '**/output/**', '**/.local-tools/**'] } } })
await server.listen()
let browser, processHandle
const errors = []
try {
  if (native) {
    await fs.writeFile(`${output}/restart-pids.txt`, '')
    processHandle = spawn('src-tauri/target/debug/examples/embedded_player_probe.exe', [], { env: { ...process.env, PLAYER_PROBE_RESTART_MARKER:path.resolve(output, 'restart-pids.txt'), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9444' }, windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] })
    processHandle.stderr.on('data', data => { void fs.appendFile(`${output}/native-stderr.log`, data) })
    for (let i = 0; i < 100; i++) { try { browser = await chromium.connectOverCDP('http://127.0.0.1:9444', { noDefaults: true }); break } catch { await new Promise(resolve => setTimeout(resolve, 200)) } }
    assert.ok(browser, 'native WebView2 CDP endpoint did not open')
  } else browser = await chromium.launch({ channel: 'msedge', headless: true })
  const page = native ? browser.contexts()[0].pages()[0] : await browser.newPage({ viewport: { width: 1280, height: 800 } })
  page.on('pageerror', error => errors.push(error.message))
  if (!native) await page.goto(`${server.resolvedUrls.local[0]}__native-player-probe`)
  await page.evaluate(()=>{window.playerTiming={start:performance.now()};document.addEventListener('click',()=>{window.playerTiming.click=performance.now()},{once:true});window.addEventListener('probe-launch-complete',event=>{window.playerTiming.completed=event.detail},{once:true});const observer=new MutationObserver(()=>{if(document.querySelector('.player-shell[open]')){window.playerTiming.drawer=performance.now();observer.disconnect()}});observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['open']});if(window.__TAURI_INTERNALS__)window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'embedded-player-opening',target:{kind:'Any'},handler:window.__TAURI_INTERNALS__.transformCallback(()=>{window.playerTiming.opening=performance.now()})}).catch(()=>{})})
  await page.getByRole('button', { name: '播放第一集', exact: true }).click()
  await page.getByRole('dialog').waitFor().catch(async error=>{console.error('Synthetic probe failed to open:',await page.locator('body').innerText(),errors);throw error})
  if(!native){assert.equal(await page.getByText('正在打开视频…',{exact:true}).count(),1);assert.equal(await page.evaluate(()=>window.probeLoadedAt??null),null)}
  await page.getByRole('button',{name:'暂停',exact:true}).waitFor()
  await page.waitForTimeout(600)
  await fs.writeFile(`${output}/${native?'native':'browser'}-opening-timing.json`,JSON.stringify(await page.evaluate(()=>({...window.playerTiming,clickToDrawer:window.playerTiming.drawer-window.playerTiming.click,clickToLoad:window.playerTiming.completed-window.playerTiming.click})),null,2))
  if (native) {
    const hwnd = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('probe_hwnd', { label: 'main' }))
    execFileSync('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'tests/helpers/capturePlayerWindow.ps1', '-WindowHandle', String(hwnd), '-OutputPath', path.resolve(output, 'native-video.png')], { windowsHide: true })
  }
  assert.equal(await page.locator('dialog:modal').count(), 1)
  if(native){const t=await page.evaluate(()=>window.playerTiming);assert.ok(t.drawer<t.completed,'native presentation must precede load completion')}
  assert.equal(await page.getByRole('button', { name: '暂停', exact: true }).count(), 1)
  if(!native){const timing=await page.evaluate(()=>({drawer:window.playerTiming.drawer,loaded:window.probeLoadedAt}));assert.ok(timing.loaded-timing.drawer>650,'drawer must open before slow load completes')}
  assert.equal(await page.locator('.player-heading>span').evaluate(e=>getComputedStyle(e).fontSize),'14px')
  assert.equal(await page.locator('.player-volume button svg').evaluate(e=>getComputedStyle(e).width),'30px')
  await page.getByRole('button', { name: '暂停', exact: true }).click()
  await page.getByRole('button', { name: '播放', exact: true }).waitFor()
  if(native && process.env.PLAYER_PROBE_LONG==='1'){
    const progress=page.getByRole('slider',{name:'播放进度',exact:true})
    assert.equal(+await progress.getAttribute('max'),7200)
    const measurements=[]
    for(const fraction of [0.83,0.12,0.95,0.49]){
      const box=await progress.boundingBox(), x=box.x+5+(box.width-10)*fraction, y=box.y+box.height/2
      await page.mouse.move(x,y);await page.mouse.down()
      const target=+await progress.inputValue()
      assert.ok(Math.abs(target-7200*fraction)<1,'two-hour native pointer mapping')
      assert.ok(Math.abs((await progress.boundingBox()).width-box.width)<0.1,'native long-video clock must not resize track')
      await page.mouse.up()
      await page.waitForFunction(async target=>{const s=await window.__TAURI_INTERNALS__.invoke('embedded_player_state');return !s.seeking && Math.abs(s.position-target)<1.1},target,{timeout:10000})
      await page.waitForFunction(target=>Math.abs(+document.querySelector('[aria-label=播放进度]').value-target)<1.1,target)
      const state=await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('embedded_player_state'))
      measurements.push({fraction,target,actual:state.position,errorSeconds:state.position-target})
    }
    await fs.writeFile(`${output}/native-two-hour-seeks.json`,JSON.stringify(measurements,null,2))
    const hwnd=await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('probe_hwnd',{label:'main'}))
    execFileSync('powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File','tests/helpers/capturePlayerWindow.ps1','-WindowHandle',String(hwnd),'-OutputPath',path.resolve(output,'native-two-hour-timeline.png')],{windowsHide:true})
    // Restore enough room for the existing fixed +/-20-second shortcut checks.
    await page.evaluate(async()=>{const s=await window.__TAURI_INTERNALS__.invoke('embedded_player_state');await window.__TAURI_INTERNALS__.invoke('embedded_player_control',{token:s.token,action:'seek',value:'12'})})
    await page.waitForFunction(()=>Math.abs(+document.querySelector('[aria-label=播放进度]').value-12)<1.1)
  }
  if(!native){
    await page.evaluate(()=>{window.probeSeekDelay=700;window.dispatchEvent(new CustomEvent('probe-timeline',{detail:{duration:7200}}))})
    // A two-hour timeline with a deliberately slow engine must keep the chosen
    // position after release instead of snapping back to the previous poll.
    const progress=page.getByRole('slider',{name:'播放进度',exact:true})
    await page.waitForFunction(()=>+document.querySelector('[aria-label=播放进度]').max===7200)
    const box=await progress.boundingBox()
    const x=box.x+5+(box.width-10)*0.83,y=box.y+box.height/2
    await page.mouse.move(x,y);await page.mouse.down()
    const chosen=+await progress.inputValue()
    assert.ok(Math.abs(chosen-7200*0.83)<1,'native range pointer mapping')
    const after=await progress.boundingBox()
    assert.ok(Math.abs(after.width-box.width)<0.1,'crossing one hour must not resize the seek track')
    await page.mouse.up()
    await page.waitForTimeout(80)
    assert.ok(Math.abs(+await progress.inputValue()-chosen)<0.2,'selected position must remain while seek is loading')
    await page.waitForTimeout(900)
    assert.ok(Math.abs(+await progress.inputValue()-chosen)<0.2,'actual engine position must match the chosen target')
    await page.evaluate(()=>{window.probeSeekDelay=0;window.probeSeekDecodeDelay=900})
    for(const fraction of [0.15,0.94]){
      const rect=await progress.boundingBox()
      await page.mouse.click(rect.x+5+(rect.width-10)*fraction,rect.y+rect.height/2)
      const target=+await progress.inputValue()
      await page.waitForTimeout(600)
      assert.ok(Math.abs(+await progress.inputValue()-target)<0.2,'accepted command must wait for actual decoding, including stale polls')
      await page.waitForTimeout(500)
      assert.ok(Math.abs(await page.evaluate(()=>window.probeSnapshot().position)-target)<0.2)
    }
    // An older command response cannot clear a later pointer selection.
    await page.evaluate(()=>{window.probeSeekDelay=700})
    await progress.click({position:{x:box.width*0.2,y:box.height/2}})
    await page.evaluate(()=>{window.probeSeekDelay=0})
    await progress.click({position:{x:box.width*0.7,y:box.height/2}})
    const latest=+await progress.inputValue()
    await page.waitForTimeout(780)
    assert.ok(Math.abs(+await progress.inputValue()-latest)<0.2)
    await page.waitForTimeout(350)
    assert.ok(Math.abs(await page.evaluate(()=>window.probeSnapshot().position)-latest)<0.2)
    // Native relative seeking can supersede the target between state polls.
    await progress.click({position:{x:box.width*0.3,y:box.height/2}})
    const superseded=+await progress.inputValue()
    await page.waitForTimeout(100)
    await page.evaluate(target=>window.dispatchEvent(new CustomEvent('probe-timeline',{detail:{position:target+20,seeking:false,seekSerial:999}})),superseded)
    await page.waitForFunction(target=>Math.abs(+document.querySelector('[aria-label=播放进度]').value-target-20)<0.2,superseded,{timeout:1800})
    // Pointer capture commits even when a drag is released above the track.
    await page.mouse.move(box.x+box.width*0.7,y);await page.mouse.down()
    await page.mouse.move(box.x+box.width*0.4,y-40);await page.mouse.up()
    const dragged=+await progress.inputValue()
    await page.waitForTimeout(1100)
    assert.ok(Math.abs(await page.evaluate(()=>window.probeSnapshot().position)-dragged)<0.2)
    await progress.press('Home')
    await page.waitForTimeout(1100)
    assert.equal(await page.evaluate(()=>window.probeSnapshot().position),0)
    await page.evaluate(()=>{window.probeSeekFail=true})
    await progress.click({position:{x:box.width*0.6,y:box.height/2}})
    await page.waitForTimeout(100)
    assert.equal(+await progress.inputValue(),0,'failed seek must reveal actual time')
    await page.evaluate(()=>{window.probeSeekFail=false;window.probeSeekDecodeDelay=0})
    for(const width of [800,390,1280]){
      await page.setViewportSize({width,height:800})
      const rect=await progress.boundingBox()
      await page.mouse.move(rect.x+5+(rect.width-10)*0.74,rect.y+rect.height/2);await page.mouse.down()
      const target=+await progress.inputValue()
      assert.ok(Math.abs(target-7200*0.74)<1,`pointer mapping at width ${width}`)
      assert.ok(Math.abs((await progress.boundingBox()).width-rect.width)<0.1)
      await page.mouse.up()
      await page.waitForFunction(target=>Math.abs(window.probeSnapshot().position-target)<0.2,target)
    }
    await page.evaluate(()=>{window.probeSeekFail=false;window.probeSeekDecodeDelay=0;window.dispatchEvent(new CustomEvent('probe-timeline',{detail:{duration:120,position:12}}))})
  }
  assert.equal(await page.getByRole('button', { name: '上一集', exact: true }).isDisabled(), true)
  assert.equal(await page.locator('.player-engine').count(), 0)
  const backdrop=await page.locator('.player-shell').evaluate(e=>getComputedStyle(e,'::backdrop').backgroundColor)
  assert.equal(backdrop,'rgba(0, 0, 0, 0)','exit must expose the library instead of an opaque backdrop')
  if(!native){await page.locator('.player-heading').dispatchEvent('mousedown',{button:0,detail:1});assert.equal(await page.evaluate(()=>window.probeCalls.filter(c=>c.command==='window_titlebar_action').at(-1).action),'drag')}
  else {
    await page.locator('.player-heading').dispatchEvent('mousedown',{button:0,detail:1})
    await page.waitForFunction(async()=>(await window.__TAURI_INTERNALS__.invoke('probe_window_actions')).includes('drag'))
  }
  if(!native){
    const before=await page.evaluate(()=>window.probeCalls.filter(c=>c.command==='window_titlebar_action'&&c.action==='drag').length)
    await page.locator('.player-heading').dispatchEvent('mousedown',{button:2,detail:1})
    await page.getByRole('button',{name:'关闭播放器',exact:true}).dispatchEvent('mousedown',{button:0,detail:1})
    assert.equal(await page.evaluate(()=>window.probeCalls.filter(c=>c.command==='window_titlebar_action'&&c.action==='drag').length),before)
  }
  assert.equal(await page.getByRole('group', { name:'窗口控制', exact:true }).count(), 0, 'overlay must not expose application window controls')
  assert.equal(await page.locator('.player-shell').evaluate(e => e.getBoundingClientRect().top), 0)
  assert.equal(await page.locator('.player-shell').evaluate(e => Math.abs(e.getBoundingClientRect().right - innerWidth) < 1), true)
  assert.equal(await page.evaluate(() => getComputedStyle(document.documentElement).overflowY), 'hidden')
  assert.ok(Math.abs(await page.getByRole('slider', { name: '播放进度', exact: true }).evaluate(e => e.getBoundingClientRect().height) - 64/3) < 0.1)
  assert.deepEqual(await page.locator('.player-transport>button').evaluateAll(nodes => nodes.map(n => n.getAttribute('aria-label'))), ['停止', '上一集', '播放', '下一集'])
  const stage = page.locator('.player-stage')
  await stage.focus()
  const beforeSeek = await page.getByRole('slider', { name: '播放进度', exact: true }).inputValue()
  await page.keyboard.press('ArrowRight')
  await page.waitForFunction(before => +document.querySelector('[aria-label="播放进度"]').value >= +before + 19, beforeSeek)
  await page.keyboard.press('ArrowLeft')
  await page.waitForFunction(before => Math.abs(+document.querySelector('[aria-label="播放进度"]').value - +before) < 1, beforeSeek)
  await page.keyboard.press('Space')
  await page.getByRole('button', { name: '暂停', exact: true }).waitFor()
  await stage.dispatchEvent('dblclick')
  await page.getByRole('button', { name: '播放', exact: true }).waitFor()
  await stage.focus()
  await page.keyboard.press('Enter')
  await page.getByRole('button', { name: '取消全屏', exact: true }).waitFor()
  await page.keyboard.press('Enter')
  await page.getByRole('button', { name: '全屏', exact: true }).waitFor()
  await page.getByRole('button', { name: '静音', exact: true }).hover()
  await page.getByRole('slider', { name: '音量', exact: true }).press('Home')
  if (native) {
    const hwnd = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('probe_hwnd', { label: 'main' }))
    execFileSync('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'tests/helpers/capturePlayerWindow.ps1', '-WindowHandle', String(hwnd), '-OutputPath', path.resolve(output, 'native-volume.png')], { windowsHide: true })
    const nativeInput = action => execFileSync('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'tests/helpers/playerNativeInput.ps1', '-WindowHandle', String(hwnd), '-Action', action], { windowsHide:true })
    nativeInput('doubleClick')
    await page.getByRole('button', { name:'暂停', exact:true }).waitFor()
    await page.waitForTimeout(700)
    nativeInput('doubleClick')
    await page.getByRole('button', { name:'播放', exact:true }).waitFor()
    await page.waitForTimeout(700)
    nativeInput('doubleClick')
    await page.getByRole('button', { name:'暂停', exact:true }).waitFor()
    nativeInput('space')
    await page.getByRole('button', { name:'播放', exact:true }).waitFor()
    await page.evaluate(async () => {const v=await window.__TAURI_INTERNALS__.invoke('embedded_player_state');await window.__TAURI_INTERNALS__.invoke('embedded_player_control',{token:v.token,action:'seek',value:'5'})})
    nativeInput('right')
    await page.waitForFunction(async () => (await window.__TAURI_INTERNALS__.invoke('embedded_player_state')).position > 24)
    nativeInput('left')
    await page.waitForFunction(async () => Math.abs((await window.__TAURI_INTERNALS__.invoke('embedded_player_state')).position - 5) < 1)
    nativeInput('enter')
    await page.getByRole('button', { name:'取消全屏', exact:true }).waitFor()
    nativeInput('enter')
    await page.getByRole('button', { name:'全屏', exact:true }).waitFor()
    await page.getByRole('button', { name:'倍速', exact:true }).click()
    nativeInput('escape')
    await page.locator('.player-panel').waitFor({state:'detached'})
  }
  await page.getByRole('button', { name: '取消静音', exact: true }).waitFor()
  await page.getByRole('button', { name: '取消静音', exact: true }).click()
  await page.getByRole('button', { name: '静音', exact: true }).waitFor()
  if (!native) {
    await page.evaluate(() => document.getElementById('probe-confirm').showModal())
    await page.waitForTimeout(100)
    assert.equal(await page.evaluate(() => window.probeCalls.filter(c => c.command === 'embedded_player_layout').at(-1).bounds.visible), false)
    const before = await page.evaluate(() => window.probeCalls.filter(c => c.command === 'embedded_player_control').length)
    await page.getByRole('button', { name: '取消退出', exact: true }).focus()
    await page.keyboard.press('Space')
    await page.waitForTimeout(100)
    assert.equal(await page.evaluate(() => window.probeCalls.filter(c => c.command === 'embedded_player_control').length), before)
    assert.equal(await page.evaluate(() => window.probeCalls.filter(c => c.command === 'embedded_player_layout').at(-1).bounds.visible), true)
  }
  await page.getByRole('button', { name: '倍速', exact: true }).click()
  await page.getByRole('button', { name: '1.5×', exact: true }).click()
  await page.waitForFunction(() => document.querySelector('.player-panel-list button[aria-pressed=true]')?.textContent === '1.5×')
  await page.getByRole('button', { name: '音轨', exact: true }).click()
  assert.equal(await page.locator('.player-panel-list button').count(), 3)
  await page.locator('.player-panel-list button').nth(2).click()
  await page.waitForFunction(() => document.querySelectorAll('.player-panel-list button')[2]?.getAttribute('aria-pressed') === 'true')
  await page.getByRole('button', { name: '字幕', exact: true }).click()
  assert.ok(await page.locator('.player-panel-list button').count() >= 3)
  await page.getByRole('button', { name: '关闭字幕', exact: true }).click()
  await page.getByRole('button', { name: '比例', exact: true }).click()
  await page.getByRole('button', { name: '4 : 3', exact: true }).click()
  await page.getByRole('button', { name: '选集', exact: true }).click()
  if(!native){
    await page.evaluate(()=>{window.probeSeekDelay=700;window.probeSeekDecodeDelay=700})
    await page.getByRole('slider',{name:'播放进度',exact:true}).click({position:{x:80,y:10}})
  }
  await page.locator('.player-panel-list button').nth(1).click()
  await page.waitForTimeout(native?600:1100)
  if(!native){
    assert.equal(+await page.getByRole('slider',{name:'播放进度',exact:true}).inputValue(),12,'replaced token ignores old seek replies')
    await page.evaluate(()=>{window.probeSeekDelay=0;window.probeSeekDecodeDelay=0})
  }
  assert.equal(await page.locator('.player-heading p').textContent(), '第 2集 演示.avi')
  assert.equal(await page.locator('dialog:modal').count(), 1)
  await page.getByRole('button', { name: '收起播放选项', exact: true }).click()
  assert.equal(await page.getByRole('button', { name: '下一集', exact: true }).isDisabled(), true)
  await page.getByRole('button', { name: '上一集', exact: true }).click()
  await page.waitForFunction(() => document.querySelector('.player-heading p')?.textContent === '第 1集 演示.avi')
  await page.getByRole('button', { name: '下一集', exact: true }).click()
  await page.waitForFunction(() => document.querySelector('.player-heading p')?.textContent === '第 2集 演示.avi')
  await page.getByRole('button', { name: '停止', exact: true }).click()
  await page.getByText('已停止', { exact: true }).waitFor()
  await page.getByRole('button', { name: '播放', exact: true }).click()
  await page.getByRole('button', { name: '暂停', exact: true }).waitFor()
  for (const width of [1280, 800, 390]) {
    if (!native) await page.setViewportSize({ width, height: 800 })
    await page.screenshot({ path: `${output}/${native ? 'native' : 'browser'}-${width}.png` })
    assert.equal(await page.locator('.player-shell').evaluate(e => e.scrollWidth <= e.clientWidth + 1), true)
  }
  await page.getByRole('button', { name: '倍速', exact: true }).click()
  await page.getByRole('slider', { name: '播放进度', exact: true }).focus()
  await page.evaluate(() => {
    window.probeExitPaused=false
    const watch=()=>{
      const animation=document.querySelector('.player-shell')?.getAnimations().find(a=>a.effect.getKeyframes().at(-1)?.transform==='translateX(100%)')
      if(animation){animation.pause();animation.currentTime=150;window.probeExitAnimation=animation;window.probeExitPaused=true}
      else requestAnimationFrame(watch)
    }
    requestAnimationFrame(watch)
  })
  if (native) {
    const hwnd=await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('probe_hwnd',{label:'main'}))
    execFileSync('powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File','tests/helpers/playerNativeInput.ps1','-WindowHandle',String(hwnd),'-Action','backspace'],{windowsHide:true})
  } else await page.keyboard.press('Backspace')
  await page.waitForFunction(()=>window.probeExitPaused)
  await page.waitForTimeout(50)
  if(native){
    const hwnd=await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('probe_hwnd',{label:'main'}))
    execFileSync('powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File','tests/helpers/capturePlayerWindow.ps1','-WindowHandle',String(hwnd),'-OutputPath',path.resolve(output,'native-exit-library.png')],{windowsHide:true})
  } else await page.screenshot({path:`${output}/browser-exit-library.png`})
  await page.evaluate(()=>window.probeExitAnimation.play())
  const closeStarted=Date.now()
  await page.getByRole('dialog').waitFor({ state: 'detached' })
  assert.ok(Date.now()-closeStarted<700,'return should not wait for engine disposal')
  if(!native)await page.waitForTimeout(1100)
  await page.getByText('当前筛选：已收藏；滚动位置保留。').waitFor()
  assert.equal(await page.evaluate(() => document.documentElement.classList.contains('player-active')), false)
  if (native) {
    await page.getByRole('button', { name: '使用独立窗口', exact: true }).click()
    await page.getByRole('button', { name: '播放第一集', exact: true }).click()
    let player
    for (let i = 0; i < 30; i++) { player = browser.contexts().flatMap(c => c.pages()).find(p => p.url().includes('player=1')); if (player) break; await page.waitForTimeout(200) }
    if (!player) {
      // Some WebView2 environments expose only the first controller via CDP.
      // The isolated probe can evaluate the actual detached WebView directly.
      const inPlayer = script => page.evaluate(script => window.__TAURI_INTERNALS__.invoke('probe_eval', { label: 'player', script }), script)
      assert.equal(await inPlayer("document.querySelectorAll('.player-shell[open]').length"), 1)
      assert.equal(await inPlayer("document.querySelectorAll('.player-back').length"),0)
      assert.equal(await inPlayer("document.querySelectorAll('.window-brand,.window-subtitle,.window-emblem').length"),0)
      assert.equal(await inPlayer("document.querySelectorAll('.player-window-titlebar .window-actions button').length"),3)
      assert.equal(await inPlayer("getComputedStyle(document.querySelector('.player-window-titlebar')).backgroundColor"),'rgba(0, 0, 0, 0)')
      await page.getByRole('button', { name: '播放第二集', exact: true }).click()
      await page.waitForTimeout(700)
      assert.equal(await inPlayer("document.querySelector('.player-heading p')?.textContent"), '第 2集 演示.avi')
      assert.equal((await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('probe_windows'))).filter(([label]) => label === 'player').length, 1)
      const hwnd = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('probe_hwnd', { label: 'player' }))
      execFileSync('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'tests/helpers/capturePlayerWindow.ps1', '-WindowHandle', String(hwnd), '-OutputPath', path.resolve(output, 'native-detached.png')], { windowsHide: true })
      await inPlayer("document.querySelector('[aria-label=最大化窗口]').click()")
      await page.waitForTimeout(250)
      assert.equal(await inPlayer("window.__TAURI_INTERNALS__.invoke('window_titlebar_action',{action:'state'})"),true)
      await inPlayer("document.querySelector('[aria-label=还原窗口]').click()")
      await page.waitForTimeout(250)
      assert.equal(await inPlayer("window.__TAURI_INTERNALS__.invoke('window_titlebar_action',{action:'state'})"),false)
      await inPlayer("document.querySelector('.window-close').click()")
      await page.waitForFunction(async () => (await window.__TAURI_INTERNALS__.invoke('embedded_player_state')) === null)
      await page.getByRole('button', { name: '播放第一集', exact: true }).click()
      await page.waitForTimeout(700)
      assert.equal(await inPlayer("document.querySelectorAll('.player-shell[open]').length"), 1)
      await inPlayer("document.querySelector('.window-close').click()")
      await page.waitForFunction(async () => (await window.__TAURI_INTERNALS__.invoke('embedded_player_state')) === null)
    } else {
    player.on('pageerror', error => errors.push(error.message))
    await player.getByRole('dialog').waitFor()
    await page.getByRole('button', { name: '播放第二集', exact: true }).click()
    await player.waitForTimeout(700)
    assert.equal(await player.locator('.player-heading p').textContent(), '第 2集 演示.avi')
    assert.equal(browser.contexts().flatMap(c => c.pages()).filter(p => p.url().includes('player=1')).length, 1)
    await player.screenshot({ path: `${output}/native-detached.png` })
    await player.getByRole('button', { name: '关闭窗口', exact: true }).click()
    await player.getByRole('dialog').waitFor({ state: 'detached' })
    await page.getByRole('button', { name: '播放第一集', exact: true }).click()
    await player.getByRole('dialog').waitFor()
    await player.getByRole('button', { name: '关闭窗口', exact: true }).click()
    await player.getByRole('dialog').waitFor({ state: 'detached' })
    }
    // Restart only this synthetic application. Its replacement process reuses
    // the test-only marker/environment; no installed application is launched.
    await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('restart_application')).catch(() => {})
    let pids=[]
    for(let i=0;i<100;i++) {pids=(await fs.readFile(`${output}/restart-pids.txt`,'utf8')).trim().split(/\s+/);if(pids.length>=2)break;await new Promise(resolve=>setTimeout(resolve,100))}
    assert.equal(pids.length,2,'one replacement process must start')
    assert.notEqual(pids[0],pids[1])
    await browser.close().catch(()=>{})
    browser=null
    for(let i=0;i<100;i++) {try {browser=await chromium.connectOverCDP('http://127.0.0.1:9444',{noDefaults:true});break}catch{await new Promise(resolve=>setTimeout(resolve,100))}}
    assert.ok(browser,'restarted synthetic application must open WebView2')
    const restarted=browser.contexts()[0].pages()[0]
    await restarted.getByRole('button',{name:'播放第一集',exact:true}).waitFor()
    await restarted.evaluate(()=>window.__TAURI_INTERNALS__.invoke('probe_exit')).catch(error=>{if(!String(error).includes('closed'))throw error})
  } else {
    await page.getByLabel('减少动态效果', { exact: true }).check()
    await page.getByRole('button', { name: '播放第一集', exact: true }).click()
    assert.equal(await page.locator('.player-shell').evaluate(e => e.getAnimations().length), 0)
    await page.getByRole('button',{name:'暂停',exact:true}).waitFor()
    await page.locator('.player-stage').focus()
    await page.keyboard.press('Escape')
    await page.getByRole('dialog').waitFor({ state: 'detached' })
    await page.evaluate(()=>{window.probeFailLoad=true})
    await page.getByRole('button',{name:'播放第一集',exact:true}).click()
    await page.waitForFunction(()=>document.querySelector('.player-notice')?.textContent.includes('内置播放引擎无法加载'),{},{timeout:5000})
    await page.keyboard.press('Backspace')
    await page.getByRole('dialog').waitFor({ state:'detached',timeout:3000 })
    assert.equal(await page.getByRole('group',{name:'窗口控制',exact:true}).count(),1)
  }
  assert.deepEqual(errors, [])
  await fs.writeFile(`${output}/${native ? 'native' : 'browser'}-result.json`, JSON.stringify({ status: 'passed', errors, scope: native ? 'isolated real Tauri/WebView2/libmpv, generated AVI with dual PCM + SRT' : 'actual React player with synthetic IPC' }, null, 2))
  console.log(`${native ? 'Native' : 'Browser'} player checks passed.`)
} finally { await browser?.close().catch(() => {}); if (processHandle && processHandle.exitCode === null) processHandle.kill(); await server.close() }
