// Screenshots a page (the test bed) in headless Chromium: node native/tools/shot.mjs <url> <out.png> [w] [h] [wait ms] [css to inject]
import { chromium } from 'playwright-core'
const [url, out, w = 1280, h = 800, wait = 6000, css = ''] = process.argv.slice(2)
const browser = await chromium.launch({ executablePath: '/opt/pw-browsers/chromium-1194/chrome-linux/chrome', args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'] })
const page = await browser.newPage({ viewport: { width: +w, height: +h } })
page.on('console', (m) => { if (m.type() === 'error') console.error('console:', m.text()) })
if (css) await page.addInitScript((css) => { addEventListener('DOMContentLoaded', () => { const s = document.createElement('style'); s.textContent = css; document.head.append(s) }) }, css)
await page.goto(url)
await page.waitForTimeout(+wait)
await page.screenshot({ path: out })
await browser.close()
