import { invoke } from '@tauri-apps/api/core'
export interface PlayerTrack { id: string; kind: 'audio' | 'sub'; title: string; selected: boolean }
export interface PlayerState {
  token: number; window: string; filmId: string; videoId: string; title: string
  videos: { id: string; name: string }[]; tracks: PlayerTrack[]
  position: number; duration: number; seeking: boolean; paused: boolean; stopped: boolean; loading: boolean; volume: number; speed: number; fullscreen: boolean; aspect: string; error: string | null
}
export const readPlayer = () => invoke<PlayerState | null>('embedded_player_state')
export const playerControl = (token: number, action: string, value?: string) => invoke<void>('embedded_player_control', { token, action, value })
export const closePlayer = (token: number) => invoke<void>('embedded_player_close', { token })
export const choosePlayerSubtitle = (token: number) => invoke<void>('embedded_player_choose_subtitle', { token })
export function layoutPlayer(token: number, element: HTMLElement, visible: boolean, popup?: HTMLElement | null) {
  const { x, y, width, height } = element.getBoundingClientRect()
  const rect = popup?.getBoundingClientRect()
  const cutout = rect ? { x: rect.x - x, y: rect.y - y, width: rect.width, height: rect.height } : null
  return invoke<void>('embedded_player_layout', { token, bounds: { x, y, width, height, visible, cutout } })
}
