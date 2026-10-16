import { enhanceTile, type TileJob } from './enhance.js'

/** Enhances tiles off the main thread (materials.ts `TileEnhancer`): a job in, the tile's pixels back, transferred. */
addEventListener('message', (event: MessageEvent<{ id: number; job: TileJob }>) => {
  const tile = enhanceTile(event.data.job)
  postMessage({ id: event.data.id, tile }, { transfer: [tile.colour.buffer, tile.normal.buffer, tile.surface.buffer] })
})
