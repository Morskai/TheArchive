import assert from 'node:assert/strict'
import test from 'node:test'
import { visibleRealFilms, type RealFilmFilters } from '../src/data/realFilmFilters.ts'
import { titleSearchText } from '../src/data/titleTranslation.ts'
const defaults: RealFilmFilters = { query: '', source: 'all', rating: 'all', favoritesOnly: false, selectedTags: [], sort: 'title', direction: 'asc' }
test('Chinese and original titles remain searchable together with code and cast', () => {
  const film = { id: 'demo', title: '虚构中文片名', original_title: 'Fictional Original', automatic_title: '虚构中文片名', manual_code: 'DEMO-1', actors: [{ id:'demo-actor', name:'Fictional Actor', system:true }], needs_organization:false, rating:null, favorite:false, tags:[], videos:[{id:"demo-video",source_id:"source",source_name:"演示片库",observed_path:"[DEMO-1] Fictional Original/video.mp4"}] }
  for (const query of ['中文', 'original', 'DEMO-1', 'fictional actor']) assert.equal(visibleRealFilms([film], { ...defaults, query }).length, 1)
  assert.equal(visibleRealFilms([film], { ...defaults, query: 'unknown' }).length, 0)
  assert.match(titleSearchText(film), /Fictional Original/)
})
