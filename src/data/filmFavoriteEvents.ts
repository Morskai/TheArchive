import { createFavoriteReadGuard } from './filmFavoriteState'

export const favoriteReads = createFavoriteReadGuard()
export const favoriteSaved = (filmId: string, favorite: boolean) => {
  favoriteReads.commit(filmId, favorite)
  window.dispatchEvent(new CustomEvent('film-favorite-saved', { detail: { filmId, favorite } }))
}
export function listenFavoriteSaved(listener: (filmId: string, favorite: boolean) => void) {
  const receive = (event: Event) => {
    const { filmId, favorite } = (event as CustomEvent<{ filmId: string; favorite: boolean }>).detail
    listener(filmId, favorite)
  }
  window.addEventListener('film-favorite-saved', receive)
  return () => window.removeEventListener('film-favorite-saved', receive)
}
