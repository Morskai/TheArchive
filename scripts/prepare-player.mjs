import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))
export function verifyPlayer(file, expected) {
  if (!existsSync(file) || createHash('sha256').update(readFileSync(file)).digest('hex').toUpperCase() !== expected) throw new Error('Pinned libmpv runtime checksum mismatch; run node scripts/prepare-player.mjs to prepare the approved runtime.')
}
export async function preparePlayer() {
  const manifest = JSON.parse(readFileSync(path.join(root, 'src-tauri/player-runtime.json'), 'utf8'))
  const destination = path.join(root, '.local-tools/mpv')
  const dll = path.join(destination, 'libmpv-2.dll')
  if (existsSync(dll)) { verifyPlayer(dll, manifest.dllSha256); return }
  const archive = path.join(root, 'src-tauri/target/embedded-player/libmpv.7z')
  mkdirSync(path.dirname(archive), { recursive: true })
  if (!existsSync(archive)) {
    const response = await fetch(manifest.url)
    if (!response.ok) throw new Error(`libmpv download failed (${response.status}).`)
    writeFileSync(archive, new Uint8Array(await response.arrayBuffer()))
  }
  verifyPlayer(archive, manifest.archiveSha256)
  mkdirSync(destination, { recursive: true })
  execFileSync('tar', ['-xf', archive, '-C', destination, 'libmpv-2.dll'], { stdio: 'inherit' })
  verifyPlayer(dll, manifest.dllSha256)
}
if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) await preparePlayer()
