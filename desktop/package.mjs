import { Arch, build, Platform } from 'electron-builder'
import { execFileSync } from 'node:child_process'
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const desktop = import.meta.dirname
const isorun = join(desktop, '..')
const webDist = join(isorun, 'apps/isorun/dist')
const engineDist = join(isorun, 'engine/dist')
const out = join(desktop, 'dist')
const stage = join(out, 'stage')
const appOutsideWorkspace = join(tmpdir(), 'isorun-desktop-app')
const release = join(out, 'release')
const publish = process.argv.includes('--publish')

const readJson = (file) => JSON.parse(readFileSync(file, 'utf8'))

if (!existsSync(join(engineDist, 'stable/engine.json'))) {
  console.error('No offline engine in engine/dist. Build it first: engine/build.sh stable (see ../CLAUDE.md).')
  process.exit(1)
}

const { VITE_OFFLINE: _hostedOnly, ...buildEnv } = process.env
execFileSync('npm', ['run', 'build'], { cwd: isorun, stdio: 'inherit', env: buildEnv })
execFileSync('npm', ['run', 'build'], { cwd: desktop, stdio: 'inherit' })

const { version, license, description } = readJson(join(isorun, 'apps/isorun/package.json'))
rmSync(stage, { recursive: true, force: true })
rmSync(appOutsideWorkspace, { recursive: true, force: true })
mkdirSync(stage, { recursive: true })
cpSync(join(out, 'app'), appOutsideWorkspace, { recursive: true })
writeFileSync(
  join(appOutsideWorkspace, 'package.json'),
  JSON.stringify(
    { name: 'isorun', productName: 'Isorun', description, author: 'Maciej Rosiek', homepage: 'https://github.com/maciej-rosiek/isorun', version, license, type: 'module', main: 'main.js' },
    null,
    2,
  ),
)
cpSync(webDist, join(stage, 'web'), { recursive: true, filter: (file) => !file.endsWith('.map') })
cpSync(engineDist, join(stage, 'engine'), { recursive: true })

if (publish && !process.env.GH_TOKEN) process.env.GH_TOKEN = execFileSync('gh', ['auth', 'token'], { encoding: 'utf8' }).trim()

const artifacts = await build({
  projectDir: appOutsideWorkspace,
  targets: Platform.LINUX.createTarget('AppImage', Arch.x64),
  publish: publish ? 'always' : 'never',
  config: {
    appId: 'io.github.maciej-rosiek.isorun',
    productName: 'Isorun',
    electronVersion: readJson(join(desktop, 'node_modules/electron/package.json')).version,
    directories: { app: appOutsideWorkspace, output: release },
    extraResources: [
      { from: join(stage, 'web'), to: 'web' },
      { from: join(stage, 'engine'), to: 'engine' },
    ],
    linux: {
      category: 'Game',
      icon: join(isorun, 'apps/isorun/public/icons/icon-512.png'),
      artifactName: 'Isorun-${arch}.AppImage',
    },
    publish: { provider: 'github', owner: 'maciej-rosiek', repo: 'isorun', releaseType: 'draft' },
  },
})

console.log(artifacts.join('\n'))
