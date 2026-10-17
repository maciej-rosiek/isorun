import { describe, expect, it } from 'vitest'
import worker from '../index'

/** The Worker's bindings, with the asset layer holding only `files`. */
function env(files: Record<string, string>) {
  const recorded: string[] = []
  return {
    recorded,
    env: {
      ASSETS: {
        fetch: async (request: Request) => {
          const path = new URL(request.url).pathname
          const body = files[path === '/' ? '/index.html' : path]
          return body === undefined ? new Response('not found', { status: 404 }) : new Response(body, { status: 200 })
        },
      },
      VISITS: {
        idFromName: () => ({ toString: () => 'all' }),
        get: () => ({
          fetch: async (r: Request | string) => {
            recorded.push(typeof r === 'string' ? r : r.url)
            return new Response('report')
          },
        }),
      },
      STATS_KEY: 'secret',
    },
  }
}

const ctx = { waitUntil: (p: Promise<unknown>) => void p }
const page = { 'Sec-Fetch-Mode': 'navigate', Accept: 'text/html' }

async function get(path: string, headers: Record<string, string> = {}, method = 'GET') {
  const e = env({ '/index.html': '<!doctype html>', '/pkg-abc/isorun_app.js': 'js' })
  const res = await worker.fetch(new Request(`https://isorun.test${path}`, { method, headers }), e.env, ctx)
  return { res, recorded: e.recorded }
}

describe('the hosted page', () => {
  it('serves the build at the root', async () => {
    const { res } = await get('/', page)
    expect(res.status).toBe(200)
    expect(await res.text()).toBe('<!doctype html>')
  })

  it('sends the web build’s old place, /native/, home with its query', async () => {
    for (const path of ['/native', '/native/', '/native/index.html?testbed']) {
      const { res } = await get(path)
      expect(res.status).toBe(301)
      expect(res.headers.get('Location')).toBe(path.endsWith('?testbed') ? 'https://isorun.test/?testbed' : 'https://isorun.test/')
    }
  })

  it('sends a page load of any other address home, but a missing file stays a 404', async () => {
    const watch = await get('/watch/cue/someone', page)
    expect(watch.res.status).toBe(301)
    expect(watch.res.headers.get('Location')).toBe('https://isorun.test/')
    expect((await get('/pkg-old/isorun_app.js', { 'Sec-Fetch-Mode': 'cors' })).res.status).toBe(404)
  })

  it('has no offline engine without its Worker', async () => {
    expect((await get('/engine/stable/engine.json')).res.status).toBe(404)
  })

  it('records a visit beacon only when it is posted, and shows the stats only with the key', async () => {
    const posted = await get('/api/visit?id=00000000-0000-4000-8000-000000000000&e=visit', {}, 'POST')
    expect(posted.res.status).toBe(204)
    expect(posted.recorded).toHaveLength(1)
    expect(posted.recorded[0]).toContain('https://visits/record?id=00000000-0000-4000-8000-000000000000&e=visit')
    const got = await get('/api/visit?id=00000000-0000-4000-8000-000000000000&e=visit')
    expect(got.res.status).toBe(204)
    expect(got.recorded).toHaveLength(0)
    expect((await get('/stats')).res.status).toBe(404)
    expect((await get('/stats?key=secret')).res.status).toBe(200)
  })
})
