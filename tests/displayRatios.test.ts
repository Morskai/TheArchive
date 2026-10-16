import test from 'node:test'
import assert from 'node:assert/strict'
import { actorRatios, filmRatios, nextRatio, ratioLabel, ratioCss, filmRatioForSources, currentFilmRatio } from '../src/data/displayRatios.ts'
const prefs={defaultFilmRatio:'original' as const,actorRatio:'original' as const,libraryRatios:{a:'5:7' as const,b:'16:9' as const}}
test('ratio cycles use only the offered values and preserve display labels',()=>{
 assert.deepEqual(filmRatios.map(v=>ratioLabel(v)),['800:543','16:9','5:7'])
 assert.deepEqual(actorRatios.map(v=>ratioLabel(v,true)),['5:7','3:4','2:3'])
 assert.equal(nextRatio(filmRatios,'5:7'),'original'); assert.equal(nextRatio(actorRatios,'2:3'),'original')
 assert.equal(ratioCss('original'),'800 / 543')
})
test('source scopes switch independently and all view keeps mixed ratios',()=>{
 assert.equal(currentFilmRatio(['a','b'],'a',prefs),'5:7')
 assert.equal(currentFilmRatio(['a','b'],'b',prefs),'16:9')
 assert.equal(currentFilmRatio(['a','b'],'all',prefs),'mixed')
 assert.equal(filmRatioForSources(['a'],'all',prefs),'5:7')
 assert.equal(filmRatioForSources(['b'],'all',prefs),'16:9')
 assert.equal(filmRatioForSources(['b','a'],'b',prefs),'16:9')
 assert.equal(filmRatioForSources(['b','a'],'all',prefs),'5:7','multi-source card resolves consistently by stable source id')
 assert.equal(filmRatioForSources(['new'],'all',prefs),'original')
})

test('temporary overview ratio overrides all cards without changing saved library ratios',()=>{
 assert.equal(currentFilmRatio(['a','b'],'all',prefs,'16:9'),'16:9')
 assert.equal(filmRatioForSources(['a'],'all',prefs,'16:9'),'16:9')
 assert.equal(filmRatioForSources(['b'],'all',prefs,'16:9'),'16:9')
 assert.equal(currentFilmRatio(['a','b'],'a',prefs,'16:9'),'5:7')
 assert.equal(filmRatioForSources(['a'],'a',prefs,'16:9'),'5:7')
 assert.deepEqual(prefs.libraryRatios,{a:'5:7',b:'16:9'})
 assert.equal(currentFilmRatio(['a','b'],'all',prefs),'mixed')
})
