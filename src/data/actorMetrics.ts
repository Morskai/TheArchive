import type { Film } from './films'
import type { Actor } from './actors'

export const distinctFilms = (films: Film[]) => [...new Map(films.map(film => [film.id, film])).values()]
export const actorFilms = (actorId: string, films: Film[]) => distinctFilms(films.filter(film => film.actorIds?.includes(actorId)))

/** Build cast membership once per film snapshot, preserving per-actor deduplication. */
export function indexActorFilms(films: Film[]): Map<string, Film[]> {
  const memberships = new Map<string, Map<string, Film>>()
  for (const film of films) {
    for (const actorId of film.actorIds ?? []) {
      let related = memberships.get(actorId)
      if (!related) { related = new Map(); memberships.set(actorId, related) }
      related.set(film.id, film)
    }
  }
  return new Map([...memberships].map(([id, related]) => [id, [...related.values()]]))
}

export function averageActorRating(films: Film[]): number | null {
  const ratings = distinctFilms(films).flatMap(film => film.rating === null ? [] : [film.rating])
  return ratings.length ? ratings.reduce((sum, rating) => sum + rating, 0) / ratings.length : null
}

export function getDrawPool(actors: Actor[]) {
  return [...new Map(actors.map(actor => [actor.id, actor])).values()]
}
export function drawActors(actors: Actor[], random = Math.random) {
  const shuffled = (items: Actor[]) => {
    const result = [...items]
    for (let index = result.length - 1; index > 0; index--) {
      const target = Math.floor(random() * (index + 1))
      ;[result[index], result[target]] = [result[target], result[index]]
    }
    return result
  }
  const unique = [...new Map(actors.map(actor => [actor.id, actor])).values()]
  const hasPortrait = (actor: Actor) => actor.hasPortrait ?? actor.portrait > 0
  return [...shuffled(unique.filter(hasPortrait)), ...shuffled(unique.filter(actor => !hasPortrait(actor)))].slice(0, 8).map(actor => actor.id)
}
