import { describe, expect, it } from 'vitest'
import site from '../index'
import engine from '../../engine/worker/index'

const BUILD = `builds/${'a'.repeat(40)}-${'b'.repeat(16)}`
const FILES: Record<string, string> = {
  '/stable/engine.json': '{"channel":"stable"}',
  [`/${BUILD}/crawl.wasm`]: 'wasm',
  [`/${BUILD}/crawl.data`]: 'data',
}

/** The engine Worker's bindings, with the asset layer holding FILES. */
const engineEnv = {
  ASSETS: {
    fetch: async (request: Request) => {
      const body = FILES[new URL(request.url).pathname]
      return body === undefined
        ? new Response('not found', { status: 404 })
        : new Response(body, { headers: { 'Content-Type': request.url.endsWith('.json') ? 'application/json' : 'application/octet-stream' } })
    },
  },
}

describe('the engine Worker', () => {
  it('serves engine/dist under /engine/ and at its own root', async () => {
    for (const base of ['https://isorun.test/engine', 'https://isorun-engine.test']) {
      const res = await engine.fetch(new Request(`${base}/stable/engine.json`), engineEnv)
      expect(res.status).toBe(200)
      expect(res.headers.get('Content-Type')).toBe('application/json')
      expect(res.headers.get('Cache-Control')).toBe('no-cache')
    }
  })

  it('keeps a build for good, as types Cloudflare compresses', async () => {
    const wasm = await engine.fetch(new Request(`https://isorun.test/engine/${BUILD}/crawl.wasm`), engineEnv)
    expect(wasm.headers.get('Content-Type')).toBe('application/wasm')
    expect(wasm.headers.get('Cache-Control')).toBe('public, max-age=31536000, immutable')
    const data = await engine.fetch(new Request(`https://isorun.test/engine/${BUILD}/crawl.data`), engineEnv)
    expect(data.headers.get('Content-Type')).toBe('application/x-protobuf')
  })

  it('has no channel that was not published, and takes only reads', async () => {
    expect((await engine.fetch(new Request('https://isorun.test/engine/trunk/engine.json'), engineEnv)).status).toBe(404)
    expect((await engine.fetch(new Request('https://isorun.test/engine/stable/engine.json', { method: 'POST' }), engineEnv)).status).toBe(405)
  })

  it('is reached from the hosted page through the ENGINE binding', async () => {
    const env = {
      ASSETS: { fetch: async () => new Response('not found', { status: 404 }) },
      ENGINE: { fetch: (r: Request) => engine.fetch(r, engineEnv) },
      STATS_KEY: 'secret',
    }
    const ctx = { waitUntil: (p: Promise<unknown>) => void p }
    const res = await site.fetch(new Request('https://isorun.test/engine/stable/engine.json'), env as never, ctx)
    expect(res.status).toBe(200)
    expect(await res.text()).toBe('{"channel":"stable"}')
  })
})
