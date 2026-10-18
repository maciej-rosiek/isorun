/**
 * The offline engine for the native app's web build: what `client::engine` (engine.rs, `web`)
 * calls through `globalThis.isorunEngine`. The engine is the WebAssembly crawl of engine/dist,
 * run in a Web Worker by ./engine/worker.ts (bundled beside this as engine-worker.js), with the
 * saves in IndexedDB: the save folders, notes and rc files the TypeScript client kept, so the saves
 * it made on this origin play on.
 *
 * The engine base is `/engine/` on the page's origin (the dev server and the Worker serve
 * engine/dist there), or the page's `?engine=<url>`.
 */
import { RC_PATH, channelOf, type EngineInfo } from './engine/channels.js'
import { browserSaveBook } from './engine/saves.js'
import type { FromWorker, ToWorker } from './engine/worker.js'

const base = new URL(new URLSearchParams(location.search).get('engine') ?? '/engine/', location.href).href
const saves = browserSaveBook()

/**
 * Whether this browser can run the engine: it waits for the player in a JSPI suspension
 * (`WebAssembly.Suspending`, Chrome and Edge 137+). It needs no SharedArrayBuffer (no threads), so
 * no cross-origin isolation.
 */
const jspi = typeof (WebAssembly as unknown as { Suspending?: unknown }).Suspending === 'function'

/** The stable build the page serves, as JSON for engine.rs; null when there is none, or the browser cannot run it. */
async function channel(): Promise<string | null> {
  if (!jspi) return null
  try {
    const res = await fetch(new URL('stable/engine.json', base), { cache: 'no-cache' })
    if (!res.ok || !(res.headers.get('content-type') ?? '').includes('json')) return null
    const info = (await res.json()) as EngineInfo
    return JSON.stringify({ ...channelOf(info), version: info.version, base })
  } catch {
    return null
  }
}

let build: string | null = null
const builds = channel().then((c) => {
  build = c ? (JSON.parse(c) as { build: string }).build : null
  return c
})

interface Handle {
  control(json: string): void
  keys(text: string): void
  terminate(): void
  /** Let a stop's save finish (the `*checkpoint` line), or a few seconds pass, then terminate. */
  retire(): void
}

/** How long a retired engine is given to save. */
const RETIRE_MS = 3000

function launch(saveDir: string, argsJson: string, rc: string, onOutput: (t: string) => void, onExit: (c: number) => void, onError: (m: string) => void): Handle {
  if (!build) throw new Error('no offline engine')
  const args = JSON.parse(argsJson) as string[]
  if (rc) args.push('-rc', RC_PATH)
  const worker = new Worker(new URL('./engine-worker.js', import.meta.url), { type: 'module' })
  const post = (m: ToWorker) => worker.postMessage(m)
  let retired = false
  let done = false
  const terminate = () => {
    done = true
    worker.terminate()
  }
  worker.onmessage = (e: MessageEvent<FromWorker>) => {
    const m = e.data
    if (m.type === 'output') {
      if (retired) {
        if (m.text.includes('*{"msg":"checkpoint"')) terminate()
      } else onOutput(m.text)
    } else if (m.type === 'exit') {
      terminate()
      if (!retired) onExit(m.code)
    } else {
      terminate()
      if (!retired) onError(m.message)
    }
  }
  worker.onerror = (e) => {
    terminate()
    if (!retired) onError(e.message || 'the offline engine failed to load')
  }
  post({ type: 'start', base: new URL(`builds/${build}/`, base).href, saveDir, args, files: rc ? { [RC_PATH]: rc } : undefined })
  return {
    control: (json) => post({ type: 'control', json }),
    keys: (text) => post({ type: 'keys', text }),
    terminate,
    retire: () => {
      if (done) return
      retired = true
      post({ type: 'control', json: JSON.stringify({ msg: 'checkpoint' }) })
      setTimeout(terminate, RETIRE_MS)
    },
  }
}

const isorunEngine = {
  channel: () => builds,
  saveInfo: (dir: string) => saves.info(dir).catch(() => null),
  noteSave: (dir: string, fields: string) => saves.note(dir, JSON.parse(fields) as Record<string, unknown>),
  launch,
}

;(globalThis as unknown as { isorunEngine: typeof isorunEngine }).isorunEngine = isorunEngine
