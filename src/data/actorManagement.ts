import { invoke } from './diagnostics.ts'
import { invalidateChangedCovers, refreshMissingCovers } from './covers'
import { notifyRealLibraryChanged } from './playback'
import type { ActorExportPreview, ActorImportPreview, ManagedActorInput, ActorScanResolution, ActorScanResult } from './actorManagementState'
import { actorManagedSaved } from './realActors'

const pendingSaves = new Map<string, Promise<string>>()
export function saveManagedActor(input: ManagedActorInput) {
  const key = input.actorId ?? '__new_actor__'
  if (pendingSaves.has(key)) return Promise.reject('actor_save_pending')
  const promise = invoke<string>('save_managed_actor', { input }).then(id => {
    actorManagedSaved(id, input)
    if (input.photoPath || input.removePhoto) invalidateChangedCovers([`actor:${id}`])
    notifyRealLibraryChanged()
    return id
  }).finally(() => pendingSaves.delete(key))
  pendingSaves.set(key, promise)
  return promise
}
export const chooseActorPhoto = () => invoke<string | null>('choose_actor_photo')
export const chooseActorImportRoot = () => invoke<string | null>('choose_actor_import_root')
export const previewActorImport = (root: string) => invoke<ActorImportPreview>('preview_actor_import', { root })
export async function confirmActorScanAdd(preview: ActorImportPreview, resolutions: ActorScanResolution[] = []) {
  const result = await invoke<ActorScanResult>('confirm_actor_scan_add', { root: preview.root, token: preview.token, resolutions })
  invalidateChangedCovers(result.artwork_keys ?? [])
  refreshMissingCovers(result.successful_artwork_keys ?? [])
  notifyRealLibraryChanged()
  return result
}
export const chooseActorExportDestination = () => invoke<string | null>('choose_actor_export_destination')
export const previewActorExport = (actorIds: string[], destination: string) => invoke<ActorExportPreview>('preview_actor_export', { actorIds, destination })
export const confirmActorExport = (actorIds: string[], destination: string, token: string, overwrite: boolean) => invoke<{ exported: number; directory: string }>('confirm_actor_export', { actorIds, destination, token, overwrite })
