#!/usr/bin/env node
/**
 * Play benchmark: starts an offline game in a real Chromium (the dev server
 * and a built engine are needed), first stands still for 10 seconds, then
 * plays as a player does: explore with `o`, three presses of Tab to fight
 * what explore stopped at, and travel down when the level is explored. It
 * prints the idle frame times, the slow frames per second, the frame times
 * of the play, and a CPU profile of the slow frames: the time of each
 * function inside them, callers included.
 *
 *   CHROMIUM=/path/to/chromium node tools/perf/play.mjs [--seconds 60] [--throttle 1]
 *     [--width 1440 --height 900 --dpr 2] [--unlimited] [--slow 25]
 *
 * `URL` sets the page (http://localhost:5174/ by default) and `SETTINGS` a
 * JSON object for `isorun.settings`. `--unlimited` turns off vsync and the
 * frame rate limit, so a frame takes as long as its work: with `--slow 8.4`
 * the slow frames are the ones a 120 Hz screen drops.
 */
import { chromium } from 'playwright-core'

const args = process.argv.slice(2)
const option = (name, fallback) => {
  const i = args.indexOf(name)
  return i >= 0 ? Number(args[i + 1]) : fallback
}
const throttle = option('--throttle', 4)
const seconds = option('--seconds', 60)
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM ?? 'chromium',
  args: [...(args.includes('--unlimited') ? ['--disable-gpu-vsync', '--disable-frame-rate-limit'] : []), '--enable-gpu', '--ignore-gpu-blocklist', '--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan'],
})
const context = await browser.newContext({ viewport: { width: option('--width', 1440), height: option('--height', 900) }, deviceScaleFactor: option('--dpr', 2) })
if (process.env.SETTINGS) await context.addInitScript((v) => localStorage.setItem('isorun.settings', v), process.env.SETTINGS)
const page = await context.newPage()
page.on('pageerror', (e) => console.log('pageerror: ' + e.message))
await page.goto(process.env.URL ?? 'http://localhost:5174/')
await page.waitForTimeout(6000)
await page.getByText(/^Play DCSS|^Continue DCSS/).first().click()
await page.waitForTimeout(12000)
for (const key of ['b', 'a', 'c']) {
  await page.keyboard.press(key)
  await page.waitForTimeout(2500)
}
await page.waitForTimeout(5000)
const cdp = await page.context().newCDPSession(page)
await cdp.send('Emulation.setCPUThrottlingRate', { rate: throttle })
await page.evaluate(() => {
  window.isorunFrames = []
  let last = performance.now()
  const loop = (t) => {
    window.isorunFrames.push(t - last)
    last = t
    requestAnimationFrame(loop)
  }
  requestAnimationFrame(loop)
})
await page.waitForTimeout(1000)
await page.evaluate(() => (window.isorunFrames = []))
await page.waitForTimeout(10000)
{
  const idle = await page.evaluate(() => window.isorunFrames)
  const s = [...idle].sort((a, b) => a - b)
  console.log(`idle frames ${idle.length} median ${s[Math.floor(s.length / 2)].toFixed(2)} p95 ${s[Math.floor(s.length * 0.95)].toFixed(2)} over8.3 ${idle.filter((f) => f > 8.4).length}`)
}
await page.evaluate(() => (window.isorunFrames = []))
await cdp.send('Profiler.enable')
await cdp.send('Profiler.setSamplingInterval', { interval: 500 })
await cdp.send('Profiler.start')
const lastLines = async (n) => (await page.evaluate(() => document.body.innerText)).split('\n').slice(-n).join('\n')
const counts = { o: 0, tab: 0, down: 0 }
const end = Date.now() + seconds * 1000
while (Date.now() < end) {
  const text = await lastLines(6)
  if (/--more--/.test(text)) await page.keyboard.press('Escape')
  if (/You die|rest in peace/i.test(text)) break
  if (/nearby|comes into view|in view|attacks|hits you|misses you/.test(text)) {
    counts.tab++
    await page.keyboard.press('Tab')
    await page.waitForTimeout(400)
  } else if (/Done exploring|explored/.test(text)) {
    counts.down++
    await page.keyboard.press('G')
    await page.waitForTimeout(300)
    await page.keyboard.press('>')
    await page.waitForTimeout(300)
    await page.keyboard.press('Enter')
    await page.waitForTimeout(2500)
    await page.keyboard.press('>')
    await page.waitForTimeout(1500)
  } else {
    counts.o++
    await page.keyboard.press('o')
    await page.waitForTimeout(1500)
    for (let j = 0; j < 3; j++) {
      counts.tab++
      await page.keyboard.press('Tab')
      await page.waitForTimeout(500)
    }
  }
}
const { profile } = await cdp.send('Profiler.stop')
const frames = await page.evaluate(() => window.isorunFrames)
const sorted = [...frames].sort((a, b) => a - b)
{
  const perSecond = []
  let t = 0
  for (const f of frames) {
    t += f
    const i = Math.floor(t / 1000)
    perSecond[i] = (perSecond[i] ?? 0) + (f > 8.4 ? 1 : 0)
  }
  console.log('slow per second: ' + Array.from(perSecond, (n) => n ?? 0).join(' '))
}
const at = (q) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))]?.toFixed(1)
console.log(`keys ${JSON.stringify(counts)}`)
console.log(`frames ${frames.length} in ${seconds}s p50 ${at(0.5)} p95 ${at(0.95)} p99 ${at(0.99)} max ${at(1)} over8.3 ${frames.filter((f) => f > 8.4).length} over12 ${frames.filter((f) => f > 12).length} over16.7 ${frames.filter((f) => f > 17).length} over25 ${frames.filter((f) => f > 25).length} over50 ${frames.filter((f) => f > 50).length} over100 ${frames.filter((f) => f > 100).length}`)
const nodes = new Map(profile.nodes.map((n) => [n.id, n]))
const parent = new Map()
for (const n of profile.nodes) for (const c of n.children ?? []) parent.set(c, n.id)
const label = (id) => {
  const { functionName, url: source, lineNumber } = nodes.get(id).callFrame
  return `${functionName || '(anonymous)'} ${source.split('/').pop().split('?')[0]}:${lineNumber}`
}
const inSlow = new Map()
let slowMs = 0
let run = []
const flush = () => {
  const ms = run.reduce((a, [, d]) => a + d, 0) / 1000
  if (ms > option('--slow', 25) * throttle) {
    slowMs += ms
    for (const [id, d] of run) {
      const seen = new Set()
      for (let x = id; x !== undefined; x = parent.get(x)) {
        const l = label(x)
        if (!seen.has(l)) {
          seen.add(l)
          inSlow.set(l, (inSlow.get(l) ?? 0) + d)
        }
      }
    }
  }
  run = []
}
profile.samples.forEach((id, k) => {
  if (label(id).startsWith('(idle)')) flush()
  else run.push([id, profile.timeDeltas[k] ?? 0])
})
flush()
console.log(`--- inclusive ms inside slow frames (${slowMs.toFixed(0)} ms in total)`)
for (const [name, us] of [...inSlow].filter(([n]) => !/^\((root)/.test(n)).sort((a, b) => b[1] - a[1]).slice(0, 70)) console.log(`${(us / 1000).toFixed(0).padStart(7)} ms  ${name}`)
await page.screenshot({ path: '/tmp/play.png' })
await browser.close()
