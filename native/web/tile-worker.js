// One tile worker (tile-workers.js): the app's wasm module, given by the page, for `enhance_job`.
import init, { enhance_job } from './pkg/isorun_app.js'

let ready = null
onmessage = async (e) => {
  const m = e.data
  if (m.module) {
    ready = init({ module_or_path: m.module })
    return
  }
  let tile
  try {
    await ready
    tile = enhance_job(m.job)
  } catch (err) {
    console.warn('a tile worker failed a job', err)
    tile = new Uint8Array(0)
  }
  postMessage({ id: m.id, tile }, [tile.buffer])
}
