// Checks the native app's web build in headless Chromium: the page starts, the test bed renders
// (?testbed), and the offline engine's bridge starts a game. Serves native/web/dist at /native/
// and engine/dist at /engine/ itself.
//
//   node native/web/test.mjs [out dir]        # screenshots there (default: /tmp/isorun-web-test)
//
// CHROMIUM picks the browser (default: Playwright's). Headless Chromium has no WebGPU, so the
// app runs on WebGL2 (SwiftShader): this is the WebGL2 check too.
import { createServer } from 'node:http'
import { createReadStream, existsSync, mkdirSync, statSync } from 'node:fs'
import { extname, join, normalize, sep } from 'node:path'
import { chromium } from 'playwright-core'

const ROOT = join(import.meta.dirname, '../..')
const OUT = process.argv[2] ?? '/tmp/isorun-web-test'
mkdirSync(OUT, { recursive: true })
const MOUNTS = [
  ['/native/', join(ROOT, 'native/web/dist')],
  ['/engine/', join(ROOT, 'engine/dist')],
  ['/', join(ROOT, 'apps/isorun/public')],
]
const TYPES = { '.js': 'text/javascript', '.wasm': 'application/wasm', '.json': 'application/json', '.png': 'image/png', '.html': 'text/html', '.data': 'application/octet-stream', '.bin': 'application/octet-stream' }

const server = createServer((req, res) => {
  let path = decodeURIComponent((req.url ?? '/').split('?')[0])
  if (path.endsWith('/')) path += 'index.html'
  for (const [prefix, dir] of MOUNTS) {
    if (!path.startsWith(prefix)) continue
    const file = normalize(join(dir, path.slice(prefix.length)))
    if (!file.startsWith(dir + sep) || !existsSync(file) || !statSync(file).isFile()) continue
    res.setHeader('Content-Type', TYPES[extname(file)] ?? 'application/octet-stream')
    res.setHeader('Content-Length', statSync(file).size)
    createReadStream(file).pipe(res)
    return
  }
  res.statusCode = 404
  res.end()
})
await new Promise((r) => server.listen(0, '127.0.0.1', r))
const base = `http://127.0.0.1:${server.address().port}`

const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome',
  args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'],
})
const failures = []
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`)
  if (!ok) failures.push(what)
}

async function open(query) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } })
  const log = []
  page.on('console', (m) => log.push(`[${m.type()}] ${m.text()}`))
  page.on('pageerror', (e) => log.push(`[pageerror] ${e.message}`))
  const started = Date.now()
  await page.goto(`${base}/native/${query}`)
  await page.waitForFunction(() => !document.getElementById('loading'), null, { timeout: 120_000 }).catch(() => {})
  const loaded = await page.evaluate(() => !document.getElementById('loading'))
  return { page, log, loaded, ms: Date.now() - started }
}

/** How much of the canvas is not the background: a rendered frame is. */
async function painted(page) {
  const shot = await page.screenshot()
  const { createHash } = await import('node:crypto')
  return { size: shot.length, hash: createHash('sha1').update(shot).digest('hex').slice(0, 8) }
}

try {
  // the home screen
  {
    const { page, log, loaded, ms } = await open('')
    check(loaded, `the page starts (${ms} ms)`)
    await page.waitForTimeout(3000)
    await page.screenshot({ path: join(OUT, 'home.png') })
    const engine = await page.evaluate(async () => (globalThis.isorunEngine ? await globalThis.isorunEngine.channel() : null))
    check(!!engine, `the engine is served (${engine ? JSON.parse(engine).label : 'none'})`)
    const panics = log.filter((l) => /panicked|RuntimeError|pageerror/.test(l))
    check(panics.length === 0, `no panics on the home screen ${panics.slice(0, 3).join(' | ')}`)
    console.log(log.slice(0, 20).join('\n'))
    await page.close()
  }
  // the test bed
  {
    const { page, log, loaded } = await open('?testbed')
    check(loaded, 'the test bed page starts')
    await page.waitForTimeout(20_000)
    await page.screenshot({ path: join(OUT, 'testbed.png') })
    const p = await painted(page)
    check(p.size > 50_000, `the test bed renders (${p.size} bytes of screenshot)`)
    const errors = log.filter((l) => /panicked|RuntimeError|pageerror|\[error\]/.test(l))
    check(errors.length === 0, `no errors in the test bed ${errors.slice(0, 5).join(' | ')}`)
    await page.close()
  }
  // an offline game through the bridge: the engine boots in a worker and draws its first map
  {
    const { page, loaded } = await open('')
    check(loaded, 'the page starts for the engine')
    const result = await page.evaluate(
      () =>
        new Promise((resolve) => {
          const out = []
          let done = false
          const finish = (r) => {
            if (!done) {
              done = true
              resolve(r)
            }
          }
          const args = JSON.stringify(['-headless', '-webtiles-socket', 'bridge', '-name', 'WebSmoke'])
          const started = performance.now()
          const h = globalThis.isorunEngine.launch(
            '/crawl-0.34~websmoke',
            args,
            '',
            (text) => {
              out.push(text)
              if (text.includes('"newgame-choice"')) h.keys('a')
              if (text.includes('"msg":"map"')) finish({ ok: true, ms: Math.round(performance.now() - started) })
            },
            (code) => finish({ ok: false, why: `exit ${code}` }),
            (m) => finish({ ok: false, why: m }),
          )
          h.control(JSON.stringify({ msg: 'attach', primary: true }))
          setTimeout(() => finish({ ok: false, why: 'timeout', tail: out.join('').slice(-500) }), 180_000)
        }),
    )
    check(result.ok, `an offline game starts in the worker (${result.ms ?? result.why} ms) ${result.tail ?? ''}`)
    await page.close()
  }
} finally {
  await browser.close()
  server.close()
}
if (failures.length) {
  console.error(`${failures.length} failed`)
  process.exit(1)
}
