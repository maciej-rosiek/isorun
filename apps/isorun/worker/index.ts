import { GAMEDATA_PROXY_PREFIX, MORGUE_PROXY_PREFIX, serveGamedata, serveMorgue } from '../gamedata-proxy'
import { VISIT_PATH } from '../src/visit-beacon'

export { VisitLog } from './visit-log'

/**
 * isorun.app: the static build, plus the gamedata proxy the browser cannot do
 * without and the morgue proxy the exit screen reads through (see
 * ../gamedata-proxy.ts for why and for the routes), and the offline engine's
 * files, handed to the isorun-engine Worker as they are.
 *
 * Everything that is not a proxy path is handed to the static assets, so
 * `public/_headers` keeps working. `run_worker_first` in wrangler.jsonc is what stops the asset
 * layer from answering /gamedata-proxy/* with index.html: that fallback is
 * exactly what used to reach the loader as `Unexpected token '<'`.
 * */
interface Env {
  ASSETS: { fetch(request: Request): Promise<Response> }
  /** isorun-engine (engine/wrangler.jsonc): the offline engine's files, at /engine/* */
  ENGINE?: { fetch(request: Request): Promise<Response> }
  VISITS: { idFromName(name: string): VisitLogId; get(id: VisitLogId): { fetch(request: Request | string): Promise<Response> } }
  STATS_KEY?: string
}

interface VisitLogId {
  toString(): string
}

interface ExecutionContext {
  waitUntil(promise: Promise<unknown>): void
}

type IncomingRequest = Request & { cf?: { country?: string } }

const STATS_PATH = '/stats'

const visitLog = (env: Env) => env.VISITS.get(env.VISITS.idFromName('all'))

function recordVisit(request: IncomingRequest, url: URL, env: Env, ctx: ExecutionContext): Response {
  if (request.method === 'POST') {
    const params = new URLSearchParams(url.search)
    params.set('country', request.cf?.country ?? '')
    ctx.waitUntil(visitLog(env).fetch(`https://visits/record?${params}`))
  }
  return new Response(null, { status: 204, headers: { 'Cache-Control': 'no-store' } })
}

function showStats(url: URL, env: Env): Promise<Response> | Response {
  if (!env.STATS_KEY || url.searchParams.get('key') !== env.STATS_KEY) return new Response('not found', { status: 404 })
  return visitLog(env).fetch('https://visits/report')
}

export default {
  async fetch(request: IncomingRequest, env: Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url)
    if (url.pathname === VISIT_PATH) return recordVisit(request, url, env, ctx)
    if (url.pathname === STATS_PATH) return showStats(url, env)
    if (url.pathname.startsWith('/engine/')) return env.ENGINE?.fetch(request) ?? new Response('not found', { status: 404 })
    const morgue = url.pathname.startsWith(MORGUE_PROXY_PREFIX)
    if (!morgue && !url.pathname.startsWith(GAMEDATA_PROXY_PREFIX)) return env.ASSETS.fetch(request)

    if (request.method === 'OPTIONS') {
      return new Response(null, {
        status: 204,
        headers: {
          'Access-Control-Allow-Origin': '*',
          'Access-Control-Allow-Methods': 'GET, HEAD, OPTIONS',
          'Access-Control-Max-Age': '86400',
        },
      })
    }
    if (request.method !== 'GET' && request.method !== 'HEAD') {
      return new Response('method not allowed', { status: 405, headers: { Allow: 'GET, HEAD, OPTIONS' } })
    }

    // a morgue is read as it is: a character dump is rewritten on every save
    if (morgue) return (await serveMorgue(url.pathname)) ?? new Response('not found', { status: 404 })

    // versioned paths are immutable, so the edge cache is the whole story:
    // one fetch per file per colo, however many players are on that version
    // `caches.default` is the Workers runtime's own cache; the DOM's
    // CacheStorage type has no such member, hence the cast
    const cache = (caches as unknown as { default: Cache }).default
    const key = new Request(url.toString(), { method: 'GET' })
    const cached = await cache.match(key)
    if (cached) return cached

    const res = await serveGamedata(url.pathname)
    if (!res) return new Response('not found', { status: 404 })
    if (res.ok) await cache.put(key, res.clone())
    return res
  },
}
