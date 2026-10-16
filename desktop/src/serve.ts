import { readFile } from 'node:fs/promises'
import { extname, join, normalize, sep } from 'node:path'
import { GAMEDATA_PROXY_PREFIX, MORGUE_PROXY_PREFIX, serveGamedata, serveMorgue } from '../../apps/isorun/gamedata-proxy'
import { VISIT_PATH } from '../../apps/isorun/src/visit-beacon'

export const ORIGIN = 'https://isorun.app'

export interface GameFiles {
  web: string
  engine: string
}

const ENGINE_PREFIX = '/engine/'

const CONTENT_TYPES: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json',
  '.webmanifest': 'application/manifest+json',
  '.txt': 'text/plain; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.webp': 'image/webp',
  '.woff2': 'font/woff2',
  '.ogg': 'audio/ogg',
  '.glb': 'model/gltf-binary',
  '.wasm': 'application/wasm',
}

function notFound(): Response {
  return new Response('not found', { status: 404 })
}

async function fileResponse(root: string, relativePath: string): Promise<Response | null> {
  let decoded: string
  try {
    decoded = decodeURIComponent(relativePath)
  } catch {
    return null
  }
  const file = normalize(join(root, decoded))
  if (!file.startsWith(root + sep)) return null
  try {
    const body = await readFile(file)
    return new Response(body, { headers: { 'Content-Type': CONTENT_TYPES[extname(file)] ?? 'application/octet-stream' } })
  } catch {
    return null
  }
}

export async function serve(url: URL, files: GameFiles, fetchImpl: typeof fetch = fetch): Promise<Response> {
  const path = url.pathname
  if (path === VISIT_PATH) return new Response(null, { status: 204 })
  if (path.startsWith(GAMEDATA_PROXY_PREFIX)) return (await serveGamedata(path, fetchImpl)) ?? notFound()
  if (path.startsWith(MORGUE_PROXY_PREFIX)) return (await serveMorgue(path, fetchImpl)) ?? notFound()
  if (path.startsWith(ENGINE_PREFIX)) return (await fileResponse(files.engine, path.slice(ENGINE_PREFIX.length))) ?? notFound()
  const asset = await fileResponse(files.web, path === '/' ? 'index.html' : path.slice(1))
  if (asset) return asset
  if (extname(path)) return notFound()
  return (await fileResponse(files.web, 'index.html')) ?? notFound()
}
