import { createHash } from 'node:crypto'
import { closeSync, existsSync, mkdirSync, openSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { verifyPlayer } from './prepare-player.mjs'

const repository = fileURLToPath(new URL('..', import.meta.url))
const productPaths = ['src', 'src-tauri', 'package.json', 'pnpm-lock.yaml', 'pnpm-workspace.yaml', 'scripts/build-installer.mjs', 'scripts/prepare-player.mjs']

export function validateArguments(args) {
  if (args.length === 0 || (args.length === 2 && args[0] === '--bundles' && args[1] === 'nsis')) return
  throw new Error('This entry builds the standard release NSIS installer; use pnpm desktop:build --bundles nsis.')
}

export function buildEnvironment(root, inherited = process.env) {
  const environment = { ...inherited, CARGO_TARGET_DIR: path.resolve(root, 'src-tauri/target'),
    CARGO_BUILD_JOBS: '1', CARGO_NET_OFFLINE: 'true', pnpm_config_verify_deps_before_run: 'warn' }
  delete environment.CARGO_BUILD_TARGET
  const pathKey = Object.keys(environment).find(key => key.toLowerCase() === 'path') ?? 'Path'
  const nasm = path.resolve(root, '.local-tools/nasm-3.01')
  if (existsSync(path.join(nasm, 'nasm.exe'))) environment[pathKey] = nasm + path.delimiter + (environment[pathKey] ?? '')
  return environment
}

export function verifyInstaller(file, started) {
  const information = statSync(file)
  if (!information.isFile() || information.size === 0) throw new Error('Installer is missing or empty.')
  if (information.mtimeMs < started) throw new Error('Installer predates this build.')
  return { path: file, bytes: information.size, sha256: createHash('sha256').update(readFileSync(file)).digest('hex').toUpperCase(),
    writtenAt: information.mtime.toISOString() }
}

function git(args) {
  const result = spawnSync('git', args, { cwd: repository, encoding: 'utf8' })
  if (result.error || result.status !== 0) throw new Error('Git source verification failed; preserve and commit the intended product changes before packaging.')
  return result.stdout.trim()
}

function verifySource() {
  if (git(['status', '--porcelain', '--untracked-files=all', '--', ...productPaths])) {
    throw new Error('Product source has uncommitted files; preserve and commit the intended changes before packaging.')
  }
}

function main(args) {
  validateArguments(args)
  if (process.platform !== 'win32' || process.arch !== 'x64') throw new Error('This installer entry requires Windows x64.')
  const settings = JSON.parse(readFileSync(path.join(repository, 'src-tauri/tauri.conf.json'), 'utf8'))
  const manifest = JSON.parse(readFileSync(path.join(repository, 'package.json'), 'utf8'))
  if (settings.productName !== 'TheArchive' || settings.version !== manifest.version || !/^\d+\.\d+\.\d+$/.test(settings.version)) {
    throw new Error('Unexpected product identity or version; check the existing project configuration.')
  }
  const environment = buildEnvironment(repository)
  const nasm = spawnSync('nasm', ['--version'], { cwd: repository, env: environment, encoding: 'utf8' })
  if (nasm.error || nasm.status !== 0) throw new Error('NASM is unavailable; place it in .local-tools/nasm-3.01 or configure the system PATH.')
  verifySource()
  const sourceRevision = git(['rev-parse', 'HEAD'])
  const runtime = JSON.parse(readFileSync(path.join(repository, 'src-tauri/player-runtime.json'), 'utf8'))
  verifyPlayer(path.join(repository, '.local-tools/mpv/libmpv-2.dll'), runtime.dllSha256)
  const directory = path.join(environment.CARGO_TARGET_DIR, 'release/bundle/nsis')
  const installer = path.join(directory, `TheArchive_${settings.version}_x64-setup.exe`)
  const receiptFile = path.join(directory, 'installer-receipt.json')
  const logFile = path.join(environment.CARGO_TARGET_DIR, 'build-installer.log')
  mkdirSync(directory, { recursive: true })
  const started = Date.now()
  const receipt = { status: 'building', sourceRevision, version: settings.version, startedAt: new Date(started).toISOString(), path: installer,
    playerRuntime: { build: runtime.build, dllSha256: runtime.dllSha256 } }
  const save = () => writeFileSync(receiptFile, JSON.stringify(receipt, null, 2) + '\n')
  save()
  console.log(`Building the release NSIS installer at ${installer}`)
  const log = openSync(logFile, 'w')
  try {
    const result = spawnSync(process.execPath, [path.join(repository, 'node_modules/@tauri-apps/cli/tauri.js'), 'build', '--bundles', 'nsis'],
      { cwd: repository, env: environment, stdio: ['ignore', log, log] })
    receipt.exitCode = result.status
    if (result.error || result.status !== 0) throw new Error(`NSIS build failed; see ${logFile}`)
    if (git(['rev-parse', 'HEAD']) !== sourceRevision) throw new Error('Source commit changed during packaging.')
    verifySource()
    verifyPlayer(path.join(repository, '.local-tools/mpv/libmpv-2.dll'), runtime.dllSha256)
    Object.assign(receipt, verifyInstaller(installer, started), { status: 'complete', endedAt: new Date().toISOString() })
    save()
    console.log(JSON.stringify(receipt, null, 2))
  } catch (error) {
    Object.assign(receipt, { status: 'failed', endedAt: new Date().toISOString() })
    save()
    throw error
  } finally {
    closeSync(log)
    console.log(readFileSync(logFile, 'utf8').split(/\r?\n/).slice(-18).join('\n'))
  }
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  try { main(process.argv.slice(2)) } catch (error) { console.error(error.message); process.exitCode = 1 }
}
