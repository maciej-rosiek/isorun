import { IsoAssets, IsoRender3d, applySurfaceDetail, LIGHTS, type IsoView } from '@isorun/render-iso'
import { qualityFor } from '../graphics'
import { lightsFor } from '../lights-panel'
import type { Camera } from '@isorun/scene'
import { getSettings } from '../servers'
import { loadPlace, type LoadedPlace } from './places'

/**
 * The room's renderer: the game's isometric renderer, in the chunk the game
 * already loads it in. `RoomView` (view.ts) imports this lazily, so the front
 * end's first paint never waits on it. It draws one place at a time
 * (places.ts), so a place looks as it did in the game it was taken from.
 */
export class RoomIso {
  private r = new IsoRender3d()
  /** the place on show, once one is */
  place: LoadedPlace | null = null

  private constructor(canvas: HTMLCanvasElement) {
    this.r.mount(canvas)
    this.applySettings()
    void IsoAssets.load('/iso/models').then((assets) => this.r.setAssets(assets))
  }

  /** The renderer on `canvas`, with nothing to draw until a place is shown. */
  static create(canvas: HTMLCanvasElement): RoomIso {
    return new RoomIso(canvas)
  }

  /** The place `id`, fetched (once) and handed to the renderer; the next render draws it. */
  async show(id: string): Promise<LoadedPlace> {
    const p = await loadPlace(id)
    if (this.place !== p) {
      this.place = p
      this.r.setTiles(p.tiles)
      this.r.setScene(p.scene)
    }
    return p
  }

  resize(width: number, height: number, dpr: number) {
    this.r.resize(width, height, dpr)
  }

  /** The Graphics settings (quality and look), as the game draws them: the settings screen shows its changes on the room behind it. */
  applySettings() {
    const st = getSettings()
    this.r.setQuality(qualityFor(st, window.devicePixelRatio || 1))
    applySurfaceDetail(st.surfaceDetail)
    this.r.setLights(lightsFor(st.look))
  }

  render(cam: Camera, view: IsoView) {
    this.r.setView(view)
    this.r.setCamera(cam)
    this.r.render()
  }

  destroy() {
    this.r.destroy()
  }
}
