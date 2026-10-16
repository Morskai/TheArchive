import { invoke } from './diagnostics.ts'

export interface ApplicationOptions { closeWindowExits: boolean }
export const defaultOptions: ApplicationOptions = { closeWindowExits: false }
export const loadApplicationOptions = () => invoke<ApplicationOptions>('load_application_options')
export const saveApplicationOptions = (options: ApplicationOptions) => invoke<void>('save_application_options', { options })
