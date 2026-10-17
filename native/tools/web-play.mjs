#!/usr/bin/env node
/**
 * Plays the web build in headless Chromium and screenshots it: the reference the native port is
 * compared with. Needs the dev server (`npx vite --port 5173` in apps/isorun) and a built engine.
 *
 *   node native/tools/web-play.mjs <out.png> [--url URL] [--size 1600x1000] [--new "b a c"]
 *        [--keys "o . i"] [--wait 2500] [--shots] [--home]
 *
 * --home       screenshot the title screen and stop (no game)
 * --new        the keys that make a character (species, background, weapon), default "b a c"
 * --keys       keys pressed once the game is up, one at a time, `--wait` ms apart; Playwright key
 *              names (Escape, Enter, Shift+O, Control+f, ArrowUp)
 * --shots      also writes <out>-<n>.png after each key
 * --eval JS    runs JS in the page before the last screenshot
 *
 * Headless Chromium has no GPU: the look renders at about 2 frames a second, so the waits are long.
 */
import { chromium } from 'playwright-core'

const args = process.argv.slice(2)
const out = args[0] ?? 'web.png'
const opt = (name, fallback) => {
  const i = args.indexOf(name)
  return i >= 0 ? args[i + 1] : fallback
}
const url = opt('--url', 'http://localhost:5173/')
const [width, height] = opt('--size', '1600x1000').split('x').map(Number)
const newKeys = opt('--new', 'b a c').split(' ').filter(Boolean)
const keys = opt('--keys', '').split(' ').filter(Boolean)
const wait = Number(opt('--wait', '2500'))
const shots = args.includes('--shots')
const js = opt('--eval', null)

const browser = await chromium.launch({ executablePath: process.env.CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome' })
const page = await browser.newPage({ viewport: { width, height } })
page.on('pageerror', (e) => console.error('pageerror: ' + e.message))
await page.goto(url)
await page.waitForTimeout(6000)
if (args.includes('--home')) {
  await page.screenshot({ path: out })
  await browser.close()
  process.exit(0)
}
await page.getByText(/^Play DCSS/).first().waitFor()
// a DOM click: the page is busy loading the engine and its tiles, and Playwright's own click waits
await page.evaluate(() => {
  const label = [...document.querySelectorAll('.label')].find((e) => /^Play DCSS/.test(e.textContent ?? ''))
  setTimeout(() => (label?.closest('button, a, [role=button]') ?? label)?.click(), 0)
})
await page.waitForTimeout(12000)
for (const k of newKeys) {
  await page.keyboard.press(k)
  await page.waitForTimeout(2500)
}
await page.waitForTimeout(5000)
let n = 0
for (const k of keys) {
  await page.keyboard.press(k)
  await page.waitForTimeout(wait)
  if (shots) await page.screenshot({ path: out.replace(/\.png$/, `-${++n}.png`) })
}
if (js) console.log(await page.evaluate(js))
await page.screenshot({ path: out })
await browser.close()
