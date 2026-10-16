import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { expect, it } from 'vitest'
import { ORIGIN, serve } from '../src/serve'

it('serves the app, its routes and the offline engine from the files on the device', async () => {
  // given
  const root = mkdtempSync(join(tmpdir(), 'isorun-desktop-'))
  const files = { web: join(root, 'web'), engine: join(root, 'engine') }
  mkdirSync(files.web)
  mkdirSync(join(files.engine, 'stable'), { recursive: true })
  writeFileSync(join(files.web, 'index.html'), '<title>Isorun</title>')
  writeFileSync(join(files.engine, 'stable', 'engine.json'), '{"channel":"stable"}')

  // when
  const page = await serve(new URL(`${ORIGIN}/`), files)
  const route = await serve(new URL(`${ORIGIN}/watch/cue/someone`), files)
  const engine = await serve(new URL(`${ORIGIN}/engine/stable/engine.json`), files)
  const missing = await serve(new URL(`${ORIGIN}/assets/missing.js`), files)

  // then
  expect(page.headers.get('Content-Type')).toBe('text/html; charset=utf-8')
  expect(await route.text()).toBe('<title>Isorun</title>')
  expect(await engine.json()).toEqual({ channel: 'stable' })
  expect(missing.status).toBe(404)
})
