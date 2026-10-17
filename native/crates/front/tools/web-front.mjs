// Screenshots of the web build's front-end screens, by address, at a few sizes.
// node web-front.mjs <outdir> [size ...]
import { chromium } from 'playwright-core'
import { existsSync } from 'node:fs'
const out = process.argv[2]
const sizes = process.argv.slice(3).length ? process.argv.slice(3) : ['1600x1000']
const routes = (process.env.ROUTES ?? '/,/settings,/settings/graphics,/accounts,/accounts/add,/login/cdi,/watch,/whats-new,/about,/accounts/add/server,/settings/controls').split(',')
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome' })
for (const size of sizes) {
  const [width, height] = size.split('x').map(Number)
  const touch = width < 900
  const ctx = await browser.newContext({ viewport: { width, height }, isMobile: touch, hasTouch: touch })
  const page = await ctx.newPage()
  page.on('pageerror', (e) => console.error('pageerror: ' + e.message))
  for (const r of routes) {
    const name = (r === '/' ? 'home' : r.slice(1).replace(/\//g, '-')) + '-' + size
    if (process.env.SKIP && existsSync(`${out}/${name}.png`)) continue
    try {
      await page.goto('http://localhost:5173' + r, { timeout: 180000, waitUntil: 'commit' })
      await page.waitForTimeout(Number(process.env.WAIT ?? 7000))
      if (process.env.KEYS) for (const k of process.env.KEYS.split(' ')) { await page.keyboard.press(k); await page.waitForTimeout(800) }
      await page.screenshot({ path: `${out}/${name}.png`, timeout: 180000 })
      console.log(name)
    } catch (e) {
      console.log(name + ' failed: ' + e.message.split('\n')[0])
    }
  }
  await ctx.close()
}
await browser.close()
