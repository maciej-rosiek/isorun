import { packager } from '@electron/packager'
import { execFileSync } from 'node:child_process'
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { basename, join } from 'node:path'

const desktop = import.meta.dirname
const isorun = join(desktop, '..')
const webDist = join(isorun, 'apps/isorun/dist')
const engineDist = join(isorun, 'engine/dist')
const out = join(desktop, 'dist')
const stage = join(out, 'stage')
const release = join(out, 'release')

const readJson = (file) => JSON.parse(readFileSync(file, 'utf8'))

if (!existsSync(join(engineDist, 'stable/engine.json'))) {
  console.error('No offline engine in engine/dist. Build it first: engine/build.sh stable (see ../CLAUDE.md).')
  process.exit(1)
}

const { VITE_OFFLINE: _hostedOnly, ...buildEnv } = process.env
execFileSync('npm', ['run', 'build'], { cwd: isorun, stdio: 'inherit', env: buildEnv })
execFileSync('npm', ['run', 'build'], { cwd: desktop, stdio: 'inherit' })

const { version, license } = readJson(join(isorun, 'apps/isorun/package.json'))
rmSync(stage, { recursive: true, force: true })
mkdirSync(stage, { recursive: true })
cpSync(join(out, 'app'), join(stage, 'app'), { recursive: true })
writeFileSync(
  join(stage, 'app/package.json'),
  JSON.stringify({ name: 'isorun', productName: 'Isorun', author: 'Maciej Rosiek', version, license, type: 'module', main: 'main.js' }, null, 2),
)
cpSync(webDist, join(stage, 'web'), { recursive: true, filter: (file) => !file.endsWith('.map') })
cpSync(engineDist, join(stage, 'engine'), { recursive: true })

const builds = await packager({
  dir: join(stage, 'app'),
  out: release,
  name: 'Isorun',
  appVersion: version,
  electronVersion: readJson(join(desktop, 'node_modules/electron/package.json')).version,
  platform: ['win32', 'linux'],
  arch: 'x64',
  asar: true,
  overwrite: true,
  extraResource: [join(stage, 'web'), join(stage, 'engine')],
})

for (const build of builds) {
  const name = basename(build)
  if (name.includes('win32')) {
    rmSync(join(release, `${name}.zip`), { force: true })
    execFileSync('zip', ['-qr', `${name}.zip`, name], { cwd: release, stdio: 'inherit' })
    console.log(join(release, `${name}.zip`))
  } else {
    execFileSync('tar', ['-czf', `${name}.tar.gz`, name], { cwd: release, stdio: 'inherit' })
    console.log(join(release, `${name}.tar.gz`))
  }
}
