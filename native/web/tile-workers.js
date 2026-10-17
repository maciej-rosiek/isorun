// The tile workers (the web build's tile-enhancer.ts): Web Workers that run the app's own wasm
// module for `enhance_job`, so the page's thread keeps its frames while the renderer upscales the
// tiles it meets. The app finds them as `globalThis.isorunEnhance(job, done)` (lib.rs
// `use_tile_workers`); `done` gets the tile's bytes, or nothing when a worker failed, and the app
// then enhances that tile itself.

export function startTileWorkers(module) {
  if (typeof Worker === 'undefined') return
  const count = Math.max(1, Math.min(4, (navigator.hardwareConcurrency || 2) - 1))
  const pending = new Map()
  let next = 0
  const workers = []
  for (let i = 0; i < count; i++) {
    let worker
    try {
      worker = new Worker(new URL('./tile-worker.js', import.meta.url), { type: 'module' })
    } catch {
      break
    }
    const entry = { worker, jobs: new Set(), alive: true }
    worker.onmessage = (e) => {
      const { id, tile } = e.data
      const done = pending.get(id)
      pending.delete(id)
      entry.jobs.delete(id)
      done?.(tile)
    }
    worker.onerror = () => {
      // a worker that broke: its jobs come back empty, and the others take the rest
      entry.alive = false
      for (const id of entry.jobs) {
        const done = pending.get(id)
        pending.delete(id)
        done?.(new Uint8Array(0))
      }
      entry.jobs.clear()
      worker.terminate()
    }
    worker.postMessage({ module })
    workers.push(entry)
  }
  if (!workers.length) return
  globalThis.isorunEnhance = (job, done) => {
    const alive = workers.filter((w) => w.alive)
    if (!alive.length) return done(new Uint8Array(0))
    // the least busy worker
    const entry = alive.reduce((a, b) => (b.jobs.size < a.jobs.size ? b : a))
    const id = next++
    pending.set(id, done)
    entry.jobs.add(id)
    entry.worker.postMessage({ id, job }, [job.buffer])
  }
}
