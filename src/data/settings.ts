import { invoke } from './diagnostics.ts'

export interface PreviewSettings {
  playerMode: 'internal' | 'custom'
  internalDisplay: 'overlay' | 'window'
  playerPath: string
  launchArguments: string
  reduceMotion: boolean
  thinScrollbar: boolean
  filmCoverRatio: 'original' | '16:9' | '3:2' | '5:7'
  actorPhotoRatio: 'original' | '2:3' | '3:4'
  filmCoverFit: 'cover' | 'contain'
  actorPhotoFit: 'cover' | 'contain'
}

export type InterfacePreferences = Omit<PreviewSettings, 'playerMode' | 'playerPath' | 'launchArguments' | 'internalDisplay'>
export const loadInterfacePreferences = () => invoke<InterfacePreferences>('load_interface_preferences')
export const saveInterfacePreferences = (preferences: InterfacePreferences) => invoke<void>('save_interface_preferences', { preferences, preserveRatios: true })

export const initialSettings: PreviewSettings = {
  playerMode: 'internal',
  internalDisplay: 'overlay',
  playerPath: '',
  launchArguments: '',
  reduceMotion: false,
  thinScrollbar: true,
  filmCoverRatio: 'original',
  actorPhotoRatio: 'original',
  filmCoverFit: 'cover',
  actorPhotoFit: 'cover',
}
