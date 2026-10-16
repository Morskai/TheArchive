import assert from 'node:assert/strict'
import test from 'node:test'
import { actorFilms, averageActorRating, drawActors, getDrawPool, indexActorFilms } from '../src/data/actorMetrics.ts'
import type { Actor } from '../src/data/actors'
import type { Film } from '../src/data/films'

test('independent film IDs count once and unrated entries do not dilute actor average', () => {
  const films = [{ id: 'one', rating: 4 }, { id: 'one', rating: 4 }, { id: 'two', rating: 2 }, { id: 'three', rating: null }] as Film[]
  assert.equal(averageActorRating(films), 3)
  assert.equal(averageActorRating([{ id: 'none', rating: null }] as Film[]), null)
})

test('same names never share cast, but all saved actor IDs can be drawn without film links', () => {
  const actors = [{ id: 'a', name: '同名', portrait: 0 }, { id: 'b', name: '同名', portrait: 0 }, { id: 'c', name: '其他', drawable: false }] as Actor[]
  const films = [{ id: 'one', actorIds: ['a', 'c'], actor: '同名', rating: 4 }, { id: 'one', actorIds: ['a', 'c'], rating: 4 }] as Film[]
  assert.deepEqual(actorFilms('a', films).map(film => film.id), ['one'])
  assert.deepEqual(actorFilms('b', films), [])
  assert.deepEqual(getDrawPool(actors).map(actor => actor.id), ['a','b','c'])
  assert.deepEqual(getDrawPool([...actors,actors[0]]).map(actor=>actor.id), ['a','b','c'])
})

test('draws at most eight distinct actor IDs with portrait priority and no demo padding', () => {
  const actors = Array.from({ length: 11 }, (_, index) => ({ id: `a${index}`, portrait: 0, hasPortrait: index < 3 })) as Actor[]
  const draw = drawActors([...actors, actors[0]], () => 0.5)
  assert.equal(draw.length, 8)
  assert.equal(new Set(draw).size, 8)
  assert.deepEqual(new Set(draw.slice(0, 3)), new Set(['a0', 'a1', 'a2']))
  assert.equal(drawActors(actors.slice(0, 2)).length, 2)
  assert.deepEqual(drawActors([]), [])
})

test('actor index keeps independent cast memberships and the latest duplicate film values', () => {
  const first = { id: 'one', actorIds: ['a', 'b', 'a'], rating: 4 } as Film
  const second = { id: 'two', actorIds: ['a'], rating: null } as Film
  const latest = { id: 'one', actorIds: ['a'], rating: 2 } as Film
  const noCast = { id: 'three', rating: 5 } as Film
  const index = indexActorFilms([first, second, latest, noCast])
  assert.deepEqual([...index.keys()], ['a', 'b'])
  assert.deepEqual(index.get('a'), [latest, second])
  assert.deepEqual(index.get('b'), [first])
  assert.equal(averageActorRating(index.get('a')!), 2)
  assert.equal(index.has('missing'), false)
  assert.equal(indexActorFilms([]).size, 0)
})

test('rebuilt actor index reflects ratings and changed links without mutating the prior snapshot', () => {
  const film = { id: 'one', actorIds: ['a'], rating: null } as Film
  const original = indexActorFilms([film])
  const saved = { ...film, actorIds: ['b'], rating: 5 } as Film
  const updated = indexActorFilms([saved])
  assert.equal(updated.has('a'), false)
  assert.equal(averageActorRating(updated.get('b')!), 5)
  assert.deepEqual(original.get('a'), [film])
  assert.equal(averageActorRating(original.get('a')!), null)
})


test('small actor libraries draw every distinct actor once from one through seven', () => {
 for(let count=1;count<8;count++){
  const actors=Array.from({length:count},(_,i)=>({id:'actor-'+i,hasPortrait:false,filmIds:[]})) as Actor[]
  const draw=drawActors(getDrawPool(actors),()=>.5)
  assert.equal(draw.length,count);assert.deepEqual(new Set(draw),new Set(actors.map(a=>a.id)))
 }
 assert.deepEqual(getDrawPool([]),[])
})
