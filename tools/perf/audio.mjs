#!/usr/bin/env node
/**
 * Sound check: starts an offline game in a real Chromium (the dev server
 * and a built engine are needed) with `?audio-trace`, explores and fights,
 * and prints how often each sound was played and the errors of the console.
 *
 *   CHROMIUM=/path/to/chromium node tools/perf/audio.mjs [url] [--bursts 10]
 */
import { chromium } from 'playwright-core'

const args = process.argv.slice(2)
const i = args.indexOf('--bursts')
const bursts = i >= 0 ? Number(args[i + 1]) : 10
const base = args.find((a) => a.startsWith('http')) ?? 'http://localhost:5174/'

const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM ?? 'chromium',
  args: ['--enable-gpu', '--ignore-gpu-blocklist', '--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--autoplay-policy=no-user-gesture-required'],
})
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } })
const problems = []
page.on('pageerror', (e) => problems.push('pageerror: ' + e.message))
page.on('response', (r) => {
  if (r.status() >= 400) problems.push('http ' + r.status() + ' ' + r.url())
})
page.on('console', (m) => {
  if (m.type() === 'error') problems.push('console: ' + m.text())
})
await page.goto(base + '?audio-trace')
await page.waitForTimeout(6000)
await page.getByText(/^Play DCSS/).first().click()
await page.waitForTimeout(12000)
for (const key of ['b', 'a', 'c']) {
  await page.keyboard.press(key)
  await page.waitForTimeout(2500)
}
await page.waitForTimeout(5000)
const lastLines = async (n) => (await page.evaluate(() => document.body.innerText)).split('\n').slice(-n).join('\n')
for (let burst = 0; burst < bursts; burst++) {
  if (/--more--/.test(await lastLines(8))) await page.keyboard.press('Escape')
  if (/nearby|comes into view|in view/.test(await lastLines(2)))
    for (let j = 0; j < 6; j++) {
      await page.keyboard.press('Tab')
      await page.waitForTimeout(700)
    }
  await page.keyboard.press('o')
  await page.waitForTimeout(3000)
}
const trace = await page.evaluate(() => Reflect.get(window, 'isorunAudioTrace') ?? [])
const counts = {}
for (const name of trace) counts[name] = (counts[name] ?? 0) + 1
console.log(counts)
console.log(problems.length ? problems.join('\n') : 'no errors')
await browser.close()
