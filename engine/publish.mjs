// Publishes engine/dist to isorun-engine (wrangler.jsonc), keeping what is
// live for any channel not built here: a deploy replaces every file, and CI
// builds one channel a run, so the other has to come along. It is staged
// from the live Worker into engine/.publish beside this build, and Wrangler
// uploads only files whose content it does not have, so what comes along is
// never uploaded again.
//
//   node engine/publish.mjs                # stage and deploy
//   node engine/publish.mjs --dry-run      # stage, and check without deploying
//
// ENGINE_LIVE is the live engine base (default: the Worker's own address).
// Every file must be under Cloudflare's 25 MiB, and a version holds at most
// 20,000 files: both are checked before anything is uploaded.

import { execFileSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'

const ENGINE = import.meta.dirname
const DIST = join(ENGINE, 'dist')
const STAGE = join(ENGINE, '.publish')
const LIVE = (process.env.ENGINE_LIVE ?? 'https://isorun-engine.maciej-rosiek.workers.dev/engine').replace(/\/$/, '')
const CHANNELS = ['stable', 'trunk']
const dryRun = process.argv.includes('--dry-run')

rmSync(STAGE, { recursive: true, force: true })

async function fetchTo(url, path, size) {
  for (let attempt = 1; ; attempt++) {
    try {
      const res = await fetch(url)
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const body = Buffer.from(await res.arrayBuffer())
      if (size !== undefined && body.length !== size) throw new Error(`${body.length} bytes, expected ${size}`)
      mkdirSync(dirname(path), { recursive: true })
      writeFileSync(path, body)
      return
    } catch (e) {
      if (attempt === 3) throw new Error(`${url}: ${e.message}`)
    }
  }
}

for (const channel of CHANNELS) {
  const local = join(DIST, channel, 'engine.json')
  let pointer
  if (existsSync(local)) {
    pointer = JSON.parse(readFileSync(local, 'utf8'))
    for (const [file, size] of pointer.files) {
      if (statSync(join(DIST, file)).size !== size) throw new Error(`dist/${file} is not the size engine.json says`)
      mkdirSync(dirname(join(STAGE, file)), { recursive: true })
      copyFileSync(join(DIST, file), join(STAGE, file))
    }
    console.log(`publish: ${channel} ${pointer.version}, built here`)
  } else {
    const res = await fetch(`${LIVE}/${channel}/engine.json`, { cache: 'no-store' })
    if (res.status === 404) {
      console.log(`publish: ${channel} neither built here nor live, left out`)
      continue
    }
    if (!res.ok) throw new Error(`${channel}/engine.json: HTTP ${res.status}`)
    pointer = await res.json()
    // a few at a time: every file must come, or the deploy would drop them
    const queue = [...pointer.files]
    await Promise.all(
      Array.from({ length: 6 }, async () => {
        for (let f; (f = queue.shift()); ) await fetchTo(`${LIVE}/${f[0]}`, join(STAGE, f[0]), f[1])
      }),
    )
    console.log(`publish: ${channel} ${pointer.version}, kept from live`)
  }
  mkdirSync(join(STAGE, channel), { recursive: true })
  writeFileSync(join(STAGE, channel, 'engine.json'), JSON.stringify(pointer) + '\n')
}

// Cloudflare's static asset limits: 25 MiB a file, 20,000 files a version
const MAX_FILE = 25 * 1024 * 1024
const MAX_FILES = 20000
if (!existsSync(STAGE)) throw new Error('publish: no channel to publish')
const files = readdirSync(STAGE, { recursive: true, withFileTypes: true })
  .filter((e) => e.isFile())
  .map((e) => [join(e.parentPath, e.name), statSync(join(e.parentPath, e.name)).size])
  .sort((a, b) => b[1] - a[1])
if (files.length > MAX_FILES) throw new Error(`publish: ${files.length} files, more than Cloudflare's ${MAX_FILES}`)
const over = files.filter(([, size]) => size >= MAX_FILE)
if (over.length) throw new Error(`publish: over 25 MiB: ${over.map(([f, s]) => `${f} (${s})`).join(', ')}`)
console.log(`publish: ${files.length} files, the largest ${files[0][0].slice(STAGE.length + 1)} (${(files[0][1] / 1048576).toFixed(1)} MiB)`)

execFileSync(
  'npx',
  ['--yes', 'wrangler@4', 'deploy', '-c', join(ENGINE, 'wrangler.jsonc'), ...(dryRun ? ['--dry-run'] : [])],
  { stdio: 'inherit' },
)
