#!/usr/bin/env node
/**
 * Explore benchmark: starts an offline game in a real Chromium (the dev
 * server and a built engine are needed), presses `o` again and again, and
 * prints the frame times of each explore burst. The CPU runs slower by a
 * factor (4 by default) to stand in for a weaker machine. With `--profile`
 * it records a CPU profile over three bursts and prints the functions that
 * take the most time.
 *
 *   CHROMIUM=/path/to/chromium node tools/perf/explore.mjs [url] [--throttle 4] [--bursts 10] [--profile]
 *
 * A burst line: frames drawn in 3 seconds (180 is 60 fps), the median, the
 * 95th percentile and the longest frame in ms, and the frames over 100 ms.
 */
import { chromium } from 'playwright-core'

const args = process.argv.slice(2)
const option = (name, fallback) => {
  const i = args.indexOf(name)
  return i >= 0 ? Number(args[i + 1]) : fallback
}
const url = args.find((a) => a.startsWith('http')) ?? 'http://localhost:5174/'
const throttle = option('--throttle', 4)
const bursts = option('--bursts', 10)
const profiling = args.includes('--profile')
const BURST_MS = 3000
const PROFILE_FROM = 4

const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM ?? 'chromium',
  args: ['--enable-gpu', '--ignore-gpu-blocklist', '--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan'],
})
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } })
page.on('pageerror', (e) => console.log('pageerror: ' + e.message))
await page.goto(url)
await page.waitForTimeout(6000)
await page.getByText(/^Play DCSS/).first().click()
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
const lastLines = async (n) => (await page.evaluate(() => document.body.innerText)).split('\n').slice(-n).join('\n')

for (let i = 0; i < bursts; i++) {
  if (/Done exploring/.test(await lastLines(3))) break
  if (/--more--/.test(await lastLines(8))) await page.keyboard.press('Escape')
  if (/nearby|comes into view|in view/.test(await lastLines(2)))
    for (let j = 0; j < 4; j++) {
      await page.keyboard.press('Tab')
      await page.waitForTimeout(700)
    }
  if (profiling && i === PROFILE_FROM) {
    await cdp.send('Profiler.enable')
    await cdp.send('Profiler.start')
  }
  await page.evaluate(() => (window.isorunFrames = []))
  await page.keyboard.press('o')
  await page.waitForTimeout(BURST_MS)
  const frames = await page.evaluate(() => window.isorunFrames)
  const sorted = [...frames].sort((a, b) => a - b)
  const at = (q) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))]?.toFixed(0)
  console.log(`burst ${i}: frames ${frames.length} p50 ${at(0.5)} p95 ${at(0.95)} max ${at(1)} over100 ${frames.filter((f) => f > 100).length}`)
  if (profiling && i === PROFILE_FROM + 2) {
    const { profile } = await cdp.send('Profiler.stop')
    const nodes = new Map(profile.nodes.map((n) => [n.id, n]))
    const self = new Map()
    profile.samples.forEach((id, k) => {
      const { functionName, url: source, lineNumber } = nodes.get(id).callFrame
      const name = `${functionName || '(anonymous)'} ${source.split('/').pop().split('?')[0]}:${lineNumber}`
      self.set(name, (self.get(name) ?? 0) + (profile.timeDeltas[k] ?? 0))
    })
    for (const [name, us] of [...self].sort((a, b) => b[1] - a[1]).slice(0, 30)) console.log(`${(us / 1000).toFixed(0).padStart(7)} ms  ${name}`)
  }
}
await browser.close()
