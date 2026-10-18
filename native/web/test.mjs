// Checks the native app's web build in headless Chromium: the page starts, the test bed renders
// (?testbed), and the offline engine's bridge starts a game. Serves native/web/dist at / and
// engine/dist at /engine/ itself, as the hosted page lays them out.
//
//   node native/web/test.mjs [out dir]        # screenshots there (default: /tmp/isorun-web-test)
//   BASE=http://127.0.0.1:8787 node native/web/test.mjs   # a page served elsewhere: the Workers
//                                                          # under `wrangler dev`, or the live site
//
// CHROMIUM picks the browser (default: Playwright's). Headless Chromium has no WebGPU, so the
// app runs on WebGL2 (SwiftShader): this is the WebGL2 check too. A second browser with
// SwiftShader's WebGPU (Dawn on its Vulkan; `--use-angle=swiftshader` too, or the canvas cannot be
// presented and the GPU process dies) checks that the browsers' shader compiler (Tint) takes the
// shaders, which naga's checks do not show.
import { createServer } from 'node:http'
import { createReadStream, existsSync, mkdirSync, statSync } from 'node:fs'
import { extname, join, normalize, sep } from 'node:path'
import { chromium } from 'playwright-core'

const ROOT = join(import.meta.dirname, '../..')
const OUT = process.argv[2] ?? '/tmp/isorun-web-test'
mkdirSync(OUT, { recursive: true })
const MOUNTS = [
  ['/engine/', join(ROOT, 'engine/dist')],
  ['/', process.env.DIST ?? join(ROOT, 'native/web/dist')],
]
const TYPES = { '.js': 'text/javascript', '.wasm': 'application/wasm', '.json': 'application/json', '.png': 'image/png', '.html': 'text/html', '.data': 'application/octet-stream', '.bin': 'application/octet-stream' }

// the visit counts' beacons (client::visits), as the hosted page's Worker takes them
const visits = []
const server = createServer((req, res) => {
  let path = decodeURIComponent((req.url ?? '/').split('?')[0])
  if (path === '/api/visit') {
    if (req.method === 'POST') visits.push(req.url)
    res.statusCode = 204
    res.end()
    return
  }
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
// with BASE, the page and its engine are served there, and the visit beacons go to its Worker
const external = process.env.BASE?.replace(/\/$/, '')
if (!external) await new Promise((r) => server.listen(0, '127.0.0.1', r))
const base = external ?? `http://127.0.0.1:${server.address().port}`

const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome',
  args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'],
})
const webgpu = await chromium.launch({
  executablePath: process.env.CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome',
  args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-vulkan=swiftshader', '--use-webgpu-adapter=swiftshader', '--use-angle=swiftshader', '--ignore-gpu-blocklist'],
})
const failures = []
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`)
  if (!ok) failures.push(what)
}

async function open(query, on = browser) {
  const page = await on.newPage({ viewport: { width: 1280, height: 800 } })
  const log = []
  page.on('console', (m) => log.push(`[${m.type()}] ${m.text()}`))
  page.on('pageerror', (e) => log.push(`[pageerror] ${e.message}`))
  page.on('response', (r) => { if (r.status() >= 400) log.push(`[http ${r.status()}] ${r.url()}`) })
  const started = Date.now()
  await page.goto(`${base}/${query}`)
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
    // a dev build sends none (client::visits)
    if (process.env.PROFILE !== 'dev' && !external) check(visits.some((v) => v.includes('e=visit')), `the visit beacon is posted (${visits.length} so far)`)
    console.log(log.slice(0, 20).join('\n'))
    await page.close()
  }
  // the test bed
  {
    const { page, log, loaded } = await open('?testbed')
    check(loaded, 'the test bed page starts')
    await page.waitForTimeout(Number(process.env.TESTBED_WAIT ?? 20_000))
    await page.screenshot({ path: join(OUT, 'testbed.png') })
    const p = await painted(page)
    check(p.size > 50_000, `the test bed renders (${p.size} bytes of screenshot)`)
    const errors = log.filter((l) => /panicked|RuntimeError|pageerror|\[error\]/.test(l))
    check(errors.length === 0, `no errors in the test bed ${errors.slice(0, 5).join(' | ')}`)
    if (process.env.VERBOSE) console.log(log.slice(0, 60).join('\n'))
    await page.close()
  }
  // the test bed on WebGPU: every shader compiles and the frame renders
  {
    const { page, log, loaded } = await open('?testbed', webgpu)
    check(loaded, 'the test bed page starts on WebGPU')
    await page.waitForTimeout(Number(process.env.TESTBED_WAIT ?? 20_000))
    const backend = log.find((l) => /backend: (BrowserWebGpu|Gl)/.test(l))?.match(/backend: (\w+)/)?.[1] ?? 'unknown'
    check(backend === 'BrowserWebGpu', `the test bed runs on WebGPU (${backend})`)
    await page.screenshot({ path: join(OUT, 'testbed-webgpu.png'), timeout: 120_000 })
    const errors = log.filter((l) => /panicked|RuntimeError|pageerror|\[error\]|Error while parsing WGSL|is invalid/.test(l))
    check(errors.length === 0, `no errors in the test bed on WebGPU ${errors.slice(0, 3).join(' | ')}`)
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
  // a phone's browser (touch events, as Chrome on Android hands them to eframe): an offline game
  // is started, a tap on the dungeon must walk (crawl is sent a click_cell), and the Settings over
  // the game must take taps on their rows and arrows (the HUD row's > writes the stored setting)
  // and a tap beside them. The places tapped are the layout's at 412x915.
  // Needs the offline engine (Play is the first row with it).
  if (!process.env.SKIP_PHONE) {
    const ctx = await browser.newContext({ viewport: { width: 412, height: 915 }, hasTouch: true, isMobile: true })
    const page = await ctx.newPage()
    const log = []
    page.on('console', (m) => log.push(`[${m.type()}] ${m.text()}`))
    page.on('pageerror', (e) => log.push(`[pageerror] ${e.message}`))
    await page.goto(`${base}/`)
    await page.waitForFunction(() => !document.getElementById('loading') && globalThis.isorunEngine, null, { timeout: 120_000 })
    const engine = await page.evaluate(async () => globalThis.isorunEngine.channel())
    check(!!engine, 'a phone: the engine is served')
    // what the app sends crawl, and what crawl says
    await page.evaluate(() => {
      const e = globalThis.isorunEngine
      const launch = e.launch
      globalThis.__sent = []
      globalThis.__heard = ''
      e.launch = (dir, args, rc, out, exit, err) => {
        const h = launch(dir, args, rc, (t) => { globalThis.__heard += t; out(t) }, exit, err)
        const { control, keys } = h
        h.control = (json) => { globalThis.__sent.push(json); control(json) }
        h.keys = (text) => { globalThis.__sent.push(text); keys(text) }
        return h
      }
    })
    const heard = (s, ms) => page.waitForFunction((s) => globalThis.__heard.includes(s), s, { timeout: ms }).then(() => true, () => false)
    const tap = async (x, y, wait = 2500) => { await page.touchscreen.tap(x, y); await page.waitForTimeout(wait) }
    await page.waitForTimeout(3000)
    await tap(140, 521) // Play
    check(await heard('"newgame-choice"', 180_000), 'a phone: a tap on Play starts an offline game')
    for (const k of ['a', 'a', 'a']) {
      await page.keyboard.press(k)
      await page.waitForTimeout(3000)
    }
    check(await heard('"msg":"map"', 120_000), 'a phone: the game draws its map')
    await page.waitForTimeout(8000)
    // a cell near the player: above, or left or right below where a wall or the fog leaves no floor
    await page.evaluate(() => { globalThis.__sent = [] })
    let sent = []
    for (const [x, y] of [[206, 300], [150, 420], [260, 420]]) {
      await tap(x, y, 4000)
      sent = await page.evaluate(() => globalThis.__sent)
      if (sent.some((m) => m.includes('click_cell'))) break
    }
    check(sent.some((m) => m.includes('click_cell')), `a phone: a tap on the dungeon walks (${sent.slice(0, 3).join(' ') || 'nothing sent'})`)
    await page.screenshot({ path: join(OUT, 'phone-game.png') })
    // crawl's text prompt (Ctrl-F's "Search for what?"): the phone's keyboard comes with it (eframe's
    // text agent, the hidden <input> it focuses, has the focus), comes back on a tap on the field
    // after it was put away, takes what is typed, and the popup's OK sends it
    {
      await page.evaluate(() => { globalThis.__sent = [] })
      await page.keyboard.press('Control+f')
      check(await heard('stash_search', 20_000), 'a phone: Ctrl-F asks crawl what to search for')
      await page.waitForTimeout(3000)
      const agent = () => page.evaluate(() => document.activeElement?.tagName === 'INPUT')
      check(await agent(), 'a phone: the text prompt brings the keyboard (the text agent has the focus)')
      await page.evaluate(() => document.activeElement?.blur())
      await tap(200, 107) // the field, high on a phone's screen
      check(await agent(), 'a phone: a tap on the text field brings the keyboard back')
      await page.keyboard.type('stair', { delay: 150 })
      await page.waitForTimeout(2000)
      await page.screenshot({ path: join(OUT, 'phone-text-prompt.png') })
      await tap(300, 150) // OK
      sent = await page.evaluate(() => globalThis.__sent)
      check(sent.some((m) => m.includes('stair')), `a phone: the prompt's OK sends what was typed (${sent.slice(-3).join(' ') || 'nothing sent'})`)
      await page.keyboard.press('Escape') // crawl's results
      await page.waitForTimeout(2000)
    }
    await tap(76, 28) // the gear: the Isorun menu
    await tap(210, 75) // its System tab
    await tap(90, 265) // Settings
    await tap(100, 144) // Interface
    await tap(380, 99) // the HUD row's >
    const stored = await page.evaluate(() => localStorage.getItem('isorun.settings') ?? '')
    check(stored.includes('classic'), `a phone: the Settings take taps on a row and its arrow (${stored || 'nothing stored'})`)
    await page.screenshot({ path: join(OUT, 'phone-settings.png') })
    await tap(380, 99) // back to the dock
    await tap(200, 700) // beside the panel: back to the groups
    await tap(200, 700) // and again: back to the Isorun menu
    await page.screenshot({ path: join(OUT, 'phone-tap-away.png') })
    const errors = log.filter((l) => /panicked|RuntimeError|pageerror/.test(l))
    check(errors.length === 0, `a phone: no panics ${errors.slice(0, 3).join(' | ')}`)
    await ctx.close()
  }
} finally {
  await browser.close()
  await webgpu.close()
  if (!external) server.close()
}
if (failures.length) {
  console.error(`${failures.length} failed`)
  process.exit(1)
}
