import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import { networkPaths, shellFiles, withShell } from '../shell'

const here = path.dirname(fileURLToPath(import.meta.url))
const sw = fs.readFileSync(path.join(here, '..', 'public', 'sw.js'), 'utf8')

/** a build's files, as dist has them */
const BUILT = [
  'index.html',
  'assets/index-AbC123.js',
  'assets/index-AbC123.js.map',
  'assets/index-DeF456.css',
  'assets/worker-GhI789.js',
  'fonts/dejavu-sans-mono-400.woff2',
  'fonts/DEJAVU-LICENSE.txt',
  'room/atlas.json',
  'room/atlas.png',
  'room/poster.jpg',
  'room/card.jpg',
  'room/places/lair.json',
  'room/places/lair.png',
  'room/README.md',
  'icons/icon-192.png',
  'favicon.svg',
  'orb.png',
  'manifest.webmanifest',
  'sw.js',
]

/** Isorun opens with no connection: the service worker keeps what the app needs to start, and nothing else. */
describe('the app kept on the device', () => {
  it('keeps the page, the bundles, the fonts, the places the front screen stands in and the icons', () => {
    expect(shellFiles(BUILT)).toEqual([
      '/',
      '/assets/index-AbC123.js',
      '/assets/index-DeF456.css',
      '/assets/worker-GhI789.js',
      '/favicon.svg',
      '/fonts/dejavu-sans-mono-400.woff2',
      '/icons/icon-192.png',
      '/manifest.webmanifest',
      '/orb.png',
      '/room/places/lair.json',
      '/room/places/lair.png',
    ])
  })

  it('sends a page load of the build’s other files to the network, not to the app', () => {
    const network = networkPaths(BUILT)
    expect(network).toContain('/room/poster.jpg')
    expect(network).not.toContain('/')
    expect(network).not.toContain('/assets/index-AbC123.js.map')
    for (const f of shellFiles(BUILT)) expect(network).not.toContain(f)
  })

  it('writes the list into sw.js, which keeps nothing of the app as written', () => {
    expect(sw).toContain('const SHELL = null')
    const out = withShell(sw, { version: 'v1', files: ['/'], network: ['/room/poster.jpg'] })
    expect(out).toContain('const SHELL = {"version":"v1","files":["/"],"network":["/room/poster.jpg"]}')
    expect(() => withShell('self.addEventListener()', { version: 'v1', files: [], network: [] })).toThrow()
  })
})

/** A device's CacheStorage, shared by every version of sw.js run on it; `addAll` fails while `failing` holds. */
function device() {
  const stores = new Map<string, Map<string, unknown>>()
  const dev = {
    failing: false,
    caches: {
      open: async (name: string) => {
        if (!stores.has(name)) stores.set(name, new Map())
        const m = stores.get(name)!
        return {
          match: async (url: string) => m.get(url),
          put: async (url: string, res: unknown) => void m.set(url, res),
          addAll: async (rs: { url: string }[]) => {
            if (dev.failing) throw new Error('a file would not come')
            for (const r of rs) m.set(r.url, `${name} page`)
          },
        }
      },
      keys: async () => [...stores.keys()],
      delete: async (name: string) => stores.delete(name),
      match: async () => undefined,
    },
  }
  return dev
}

/** sw.js of app version `version`, on `dev`: its install, and a page load of `/` once all it set off has settled */
function serviceWorker(dev: ReturnType<typeof device>, version: string) {
  const on: Record<string, (e: unknown) => void> = {}
  const self = {
    location: { href: 'https://isorun.app/sw.js', origin: 'https://isorun.app' },
    addEventListener: (type: string, f: (e: unknown) => void) => (on[type] = f),
    skipWaiting: async () => {},
    clients: { claim: async () => {} },
  }
  class Request {
    constructor(readonly url: string) {}
  }
  new Function('self', 'caches', 'Request', 'fetch', withShell(sw, { version, files: ['/'], network: [] }))(self, dev.caches, Request, async () => 'network page')
  const settle = async (type: string, event: object) => {
    const waits: Promise<unknown>[] = []
    let answer: Promise<unknown> = Promise.resolve()
    on[type]({ ...event, waitUntil: (p: Promise<unknown>) => waits.push(p), respondWith: (p: Promise<unknown>) => (answer = p) })
    const out = await answer
    await Promise.all(waits)
    return out
  }
  return {
    install: () => settle('install', {}),
    load: () => settle('fetch', { request: { method: 'GET', url: 'https://isorun.app/', mode: 'navigate' } }),
  }
}

describe('a new version of the app', () => {
  it('is kept on a later page load when keeping it failed at install, rather than the older one standing in for good', async () => {
    const dev = device()
    await serviceWorker(dev, 'v1').install()
    const v2 = serviceWorker(dev, 'v2')
    dev.failing = true
    await v2.install()
    // the older version stands in, so the app still opens; and this load tries again
    expect(await v2.load()).toBe('isorun-app-v1 page')
    dev.failing = false
    expect(await v2.load()).toBe('isorun-app-v1 page')
    expect(await v2.load()).toBe('isorun-app-v2 page')
  })
})
