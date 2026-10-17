// Puts the Android app's files in dist/www, for `cap sync` to copy into the APK:
// the game built for the app (VITE_PLATFORM=android: no service worker, no
// Cache Storage, the proxies on the hosted page) and the stable channel of the
// offline engine (engine/dist, built by engine/build.sh stable) under /engine/.
//
//   node mobile/stage.mjs

import { execFileSync } from 'node:child_process'
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'

const mobile = import.meta.dirname
const isorun = join(mobile, '..')
const webDist = join(isorun, 'apps/isorun/dist')
const engineDist = join(isorun, 'engine/dist')
const www = join(mobile, 'dist/www')
const CHANNEL = 'stable'

const pointer = join(engineDist, CHANNEL, 'engine.json')
if (!existsSync(pointer)) {
  console.error(`No offline engine in engine/dist. Build it first: engine/build.sh ${CHANNEL} (see ../CLAUDE.md).`)
  process.exit(1)
}

const { VITE_OFFLINE: _hostedOnly, ...buildEnv } = process.env
execFileSync('npm', ['run', 'build'], { cwd: isorun, stdio: 'inherit', env: { ...buildEnv, VITE_PLATFORM: 'android' } })

rmSync(www, { recursive: true, force: true })
cpSync(webDist, www, { recursive: true, filter: (file) => !file.endsWith('.map') })

// only the channel's pointer and the files it lists: the APK carries one build, and trunk is a 404
const info = JSON.parse(readFileSync(pointer, 'utf8'))
const engine = join(www, 'engine')
mkdirSync(join(engine, CHANNEL), { recursive: true })
writeFileSync(join(engine, CHANNEL, 'engine.json'), readFileSync(pointer))
let bytes = 0
for (const [path, size] of info.files) {
  mkdirSync(dirname(join(engine, path)), { recursive: true })
  cpSync(join(engineDist, path), join(engine, path))
  bytes += size
}
console.log(`staged ${www}: crawl ${info.version}, ${info.files.length} engine files, ${(bytes / 1e6).toFixed(1)} MB`)
