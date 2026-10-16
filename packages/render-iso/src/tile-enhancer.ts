import { enhanceTile, type EnhancedTile, type TileJob } from './enhance.js'

/** At most this many workers enhance tiles, fewer on a machine with fewer cores. */
const MAX_WORKERS = 3

/**
 * Enhances tiles (enhance.ts `enhanceTile`) on workers, so a level full of new tiles costs the frames nothing: a
 * tile shows once its pixels are back. Where workers cannot run, the work is done at once.
 */
export class TileEnhancer {
  private readonly workers: Worker[] = []
  private readonly waiting = new Map<number, (tile: EnhancedTile) => void>()
  private next = 0

  constructor() {
    if (typeof Worker === 'undefined') return
    const count = Math.max(1, Math.min(MAX_WORKERS, (navigator.hardwareConcurrency || 2) - 1))
    for (let i = 0; i < count; i++) {
      const worker = new Worker(new URL('./enhance-worker.ts', import.meta.url), { type: 'module' })
      worker.addEventListener('message', (event: MessageEvent<{ id: number; tile: EnhancedTile }>) => {
        const done = this.waiting.get(event.data.id)
        this.waiting.delete(event.data.id)
        done?.(event.data.tile)
      })
      this.workers.push(worker)
    }
  }

  /** Enhance the tile; `done` gets its pixels, later when on a worker. */
  enhance(job: TileJob, done: (tile: EnhancedTile) => void) {
    if (!this.workers.length) {
      done(enhanceTile(job))
      return
    }
    const id = this.next++
    this.waiting.set(id, done)
    this.workers[id % this.workers.length].postMessage({ id, job }, { transfer: [job.pixels.buffer] })
  }

  /** Stop the workers; tiles still on them never come back. */
  dispose() {
    for (const worker of this.workers) worker.terminate()
    this.workers.length = 0
    this.waiting.clear()
  }
}
