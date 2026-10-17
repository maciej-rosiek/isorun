#!/usr/bin/env node
/**
 * Plays the web build in headless Chromium, as native/tools/web-play.mjs does, and logs every
 * message the page sends to crawl (`SENT <json>`), to compare with the native example's log
 * (`cargo run -p isorun-input --example input` with ISORUN_INPUT_LOG).
 *
 *   node native/crates/input/tools/web-keys.mjs <out.log> --steps "key:k down:ArrowUp key:Space up:ArrowUp"
 *
 * Steps: `key:K` presses and releases, `down:K` / `up:K` hold and let go (Playwright key names).
 * The offline engine runs in a worker: the init script catches what is posted to it.
 */
import { chromium } from 'playwright-core'
import { writeFileSync } from 'node:fs'

const args = process.argv.slice(2)
const out = args[0] ?? 'web-sent.log'
const opt = (name, fallback) => {
  const i = args.indexOf(name)
  return i >= 0 ? args[i + 1] : fallback
}
const url = opt('--url', 'http://localhost:5173/')
const newKeys = opt('--new', 'b a c').split(' ').filter(Boolean)
const steps = opt('--steps', '').split(' ').filter(Boolean)
const wait = Number(opt('--wait', '2500'))

const browser = await chromium.launch({ executablePath: process.env.CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome' })
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } })
await page.addInitScript(() => {
  // the offline engine runs in a worker: what reaches crawl is posted to it, `keys` for an
  // `input` message (process_handler.py writes those to the pty), `control` for the rest
  window.__sent = []
  const post = Worker.prototype.postMessage
  Worker.prototype.postMessage = function (m, ...rest) {
    if (m && m.type === 'keys') window.__sent.push(JSON.stringify({ msg: 'input', text: m.text }))
    else if (m && m.type === 'control') window.__sent.push(m.json)
    return post.call(this, m, ...rest)
  }
})
page.on('pageerror', (e) => console.error('pageerror: ' + e.message))
await page.goto(url)
await page.waitForTimeout(6000)
await page.getByText(/^Play DCSS/).first().waitFor({ timeout: 120000 })
await page.evaluate(() => {
  const label = [...document.querySelectorAll('.label')].find((e) => /^Play DCSS/.test(e.textContent ?? ''))
  setTimeout(() => (label?.closest('button, a, [role=button]') ?? label)?.click(), 0)
})
// character creation: each key once its screen is up
await page.waitForSelector('.popup.newgame', { timeout: 180000 })
await page.waitForTimeout(3000)
for (const k of newKeys) {
  await page.keyboard.press(k)
  await page.waitForTimeout(5000)
}
// the game is up once the client has set its own pace for travel (game.ts paceTravel)
for (let i = 0; i < 180 && !(await page.evaluate(() => window.__sent.some((s) => s.includes('travel_delay')))); i++) await page.waitForTimeout(1000)
await page.waitForTimeout(5000)
await page.evaluate(() => (window.__sent.length = 0))
for (const s of steps) {
  const [op, key] = [s.slice(0, s.indexOf(':')), s.slice(s.indexOf(':') + 1)]
  if (op === 'down') await page.keyboard.down(key)
  else if (op === 'up') await page.keyboard.up(key)
  else await page.keyboard.press(key)
  await page.waitForTimeout(wait)
}
const sent = await page.evaluate(() => window.__sent)
writeFileSync(out, sent.map((s) => 'SENT ' + s).join('\n') + '\n')
await page.screenshot({ path: out.replace(/\.log$/, '.png') })
console.log(sent.map((s) => 'SENT ' + s).join('\n'))
await browser.close()
