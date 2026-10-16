import { IsoCamera } from '@isorun/render-iso'
import { h } from '../dom'
import { placeById } from './places'
import type { RoomIso } from './room-iso'

/**
 * The room behind the front end: a place of a real game (places.ts), one a
 * visit, drawn by the game's own renderer around the place's eye. The camera
 * turns slowly round it; the right stick and a drag on the scenery turn it faster.
 *
 * Stands where the level backdrop stood: a canvas under the screen's frame,
 * publishing the cell size the console-face menus are laid out on (`--cw`,
 * `--ch`, `--fs`, `--fit` on the host) and `onfit` when it changes.
 *
 * The renderer, three.js with it, is a lazy chunk: the menus draw and work
 * before it lands, on the dark. The live room fades in from the dark facing
 * a random way, so each visit opens on a different view; nothing is
 * remembered between visits. No WebGPU or WebGL leaves the menus on the dark.
 */
/** how fast the camera turns round the place by itself, and at full right stick, in degrees a second */
const ORBIT_DEGREES_PER_SECOND = 3
const STICK_DEGREES_PER_SECOND = 90
/** how far a drag across the scenery turns the camera, in degrees per css pixel */
const DRAG_DEGREES_PER_PIXEL = 0.3
/** a press on the scenery is a look once it has moved this far, in css px (game.ts DRAG_SLOP) */
const DRAG_SLOP = 6
/** the console grid the menus stand on: the cell the level backdrop settled on for its usual floor, kept so nothing about the menus' scale changes */
const GRID_COLS = 59
const GRID_ROWS = 32

export class RoomView {
  readonly el: HTMLCanvasElement
  onfit: (() => void) | null = null
  private reducedMotion = false
  private host: HTMLElement
  private ro: ResizeObserver | null = null
  private grid = { cw: 16, ch: 23 }
  private room: RoomIso | null = null
  private cam = new IsoCamera({ rotation: Math.random() * 360, tilt: 0, perspective: 0 })
  /** the right stick's left and right, -1 to 1 */
  private stick = 0
  private raf = 0
  private last = 0
  private needsRender = false
  private destroyed = false
  private drag: { id: number; x: number; y: number; x0: number; y0: number; moved: boolean } | null = null
  /** Lets go of a look drag: the window lost focus, or the button was released out of sight. */
  private cancelDrag: () => void = () => {}
  private onWindowBlur: () => void = () => {}
  private size = { w: 1, h: 1 }
  /** the veil over the room that keeps the menus readable, tinted for the place (styles.css `.room-veil`) */
  readonly veil: HTMLElement
  /** the place this visit stands in */
  private readonly place: string
  /** a still stands over the room (menu.ts `stand`): nothing drawn, nothing to take hold of */
  private covered = false

  get cellWidth(): number {
    return this.grid.cw
  }

  constructor(host: HTMLElement, place: string) {
    this.host = host
    this.place = place
    this.el = h('canvas', { class: 'room', 'aria-hidden': 'true' })
    this.veil = h('div', { class: 'room-veil', 'aria-hidden': 'true' })
    this.veil.dataset.tint = placeById(place)?.tint ?? ''
    host.prepend(this.veil)
    host.prepend(this.el)
    this.reducedMotion = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false
    this.fit = this.fit.bind(this)
    this.tick = this.tick.bind(this)
    this.onVisibility = this.onVisibility.bind(this)
    if (typeof ResizeObserver !== 'undefined') {
      this.ro = new ResizeObserver(this.fit)
      this.ro.observe(this.el)
    }
    this.fit()
    document.addEventListener('visibilitychange', this.onVisibility)
    this.attachPointer()
    void this.start()
  }

  /** A screen stands in a still over the room, or no longer does: the room keeps its eye and draws again once uncovered. */
  cover(on: boolean) {
    if (on === this.covered) return
    this.covered = on
    if (on) this.cancelDrag()
    else this.invalidate()
  }

  destroy() {
    this.destroyed = true
    cancelAnimationFrame(this.raf)
    this.ro?.disconnect()
    document.removeEventListener('visibilitychange', this.onVisibility)
    window.removeEventListener('blur', this.onWindowBlur)
    this.room?.destroy()
    this.room = null
    this.el.remove()
    this.veil.remove()
  }

  /** Right stick: dx, dy in [-1, 1]; zero releases it. */
  look(dx: number, _dy: number) {
    if (this.covered) return
    this.stick = dx
    this.invalidate()
  }

  private async start() {
    if (!canDraw()) return
    let room: RoomIso
    try {
      const mod = await import('./room-iso')
      if (this.destroyed) return
      room = mod.RoomIso.create(this.el)
    } catch (e) {
      console.warn('the front room could not be drawn; the menus stand on the dark', e)
      return
    }
    let camera
    try {
      camera = (await room.show(this.place)).camera
    } catch (e) {
      console.warn(`the place ${this.place} could not be drawn; the menus stand on the dark`, e)
      return room.destroy()
    }
    if (this.destroyed) return room.destroy()
    this.room = room
    this.cam.snapTo(camera.x, camera.y)
    this.room.resize(this.size.w, this.size.h, window.devicePixelRatio || 1)
    this.invalidate()
  }

  /** The Graphics settings changed (the settings screen stands over the room): they show at once, so a change is seen before it is played. */
  applySettings() {
    this.room?.applySettings()
    this.invalidate()
  }

  /** The screen's grid: the cell size the menus are laid out on, as the level backdrop chose it for its usual floor. */
  private fit() {
    const w = this.el.clientWidth || window.innerWidth
    const hgt = this.el.clientHeight || window.innerHeight
    const cw = Math.min(18, Math.max(11, Math.ceil(Math.max(w / GRID_COLS, hgt / (GRID_ROWS * 1.45)))))
    const ch = Math.round(cw * 1.45)
    const fs = Math.round(cw * 1.55)
    this.grid = { cw, ch }
    this.host.style.setProperty('--cw', cw + 'px')
    this.host.style.setProperty('--ch', ch + 'px')
    this.host.style.setProperty('--fs', fs + 'px')
    this.host.style.setProperty('--fit', (cw - fs * 0.602).toFixed(2) + 'px')
    this.size = { w, h: hgt }
    this.room?.resize(w, hgt, window.devicePixelRatio || 1)
    this.invalidate()
    this.onfit?.()
  }

  private invalidate() {
    this.needsRender = true
    if (!this.raf && !document.hidden && !this.covered) {
      // frames had stopped: the clock starts again, so a look's first frame eases from zero
      this.last = performance.now()
      this.raf = requestAnimationFrame(this.tick)
    }
  }


  /** A frame for as long as the room shows: the camera turns, the torches flicker and the fog drifts. */
  private tick(now: number) {
    this.raf = 0
    // covered since the frame was asked for: the render waits for the room to show again (cover)
    if (this.covered) return
    const dt = Math.min(0.1, (now - this.last) / 1000 || 0)
    this.last = now
    const room = this.room
    // with no room to draw (none yet, or none at all) no frames follow: the room's arrival invalidates, and that wakes the loop
    if (!room) return
    const orbit = this.reducedMotion ? 0 : ORBIT_DEGREES_PER_SECOND
    this.cam.turn((orbit + this.stick * STICK_DEGREES_PER_SECOND) * dt)
    const eye = this.cam.camera
    this.cam.update(dt, eye.x, eye.y)
    room.render(this.cam.camera, this.cam.view)
    this.el.classList.add('in')
    this.raf = requestAnimationFrame(this.tick)
  }

  private onVisibility() {
    if (document.hidden) {
      cancelAnimationFrame(this.raf)
      this.raf = 0
      this.last = 0
    } else this.invalidate()
  }

  /**
   * A drag on the scenery looks around, as in the game (game.ts attachPointer);
   * a press that never moves does nothing. The scenery is the whole screen but
   * the menus themselves: the room is seen around and behind the words, so a
   * press anywhere it shows takes hold of it (isScenery below), not only where
   * the canvas is left uncovered.
   */
  private attachPointer() {
    const surface = this.host
    surface.addEventListener('pointerdown', (ev) => {
      if (this.drag || !this.room || this.covered) return
      if (!isScenery(ev.target)) return
      // the press is the room's: no caret dropped in the words it went through, no row taking focus
      ev.preventDefault()
      this.drag = { id: ev.pointerId, x: ev.clientX, y: ev.clientY, x0: ev.clientX, y0: ev.clientY, moved: false }
      surface.setPointerCapture(ev.pointerId)
    })
    surface.addEventListener('pointermove', (ev) => {
      const d = this.drag
      if (!d || ev.pointerId !== d.id) return
      // the button came up out of the window's sight, so no pointerup arrived:
      // this move, with nothing held, is that release (game.ts attachPointer)
      if (ev.pointerType === 'mouse' && ev.buttons === 0) return this.cancelDrag()
      const dx = ev.clientX - d.x
      const dy = ev.clientY - d.y
      d.x = ev.clientX
      d.y = ev.clientY
      if (!d.moved && Math.hypot(ev.clientX - d.x0, ev.clientY - d.y0) < DRAG_SLOP) return
      d.moved = true
      surface.classList.add('looking')
      this.cam.turn(-dx * DRAG_DEGREES_PER_PIXEL)
      this.invalidate()
    })
    const end = (ev: PointerEvent) => {
      const d = this.drag
      if (!d || ev.pointerId !== d.id) return
      this.drag = null
      surface.classList.remove('looking')
      if (surface.hasPointerCapture(ev.pointerId)) surface.releasePointerCapture(ev.pointerId)
      if (d.moved) this.invalidate()
    }
    // leaving the page lets go: the button released out there sends no pointerup
    this.onWindowBlur = () => this.cancelDrag()
    window.addEventListener('blur', this.onWindowBlur)
    this.cancelDrag = () => {
      const d = this.drag
      if (!d) return
      this.drag = null
      surface.classList.remove('looking')
      if (surface.hasPointerCapture(d.id)) surface.releasePointerCapture(d.id)
      if (d.moved) this.invalidate()
    }
    surface.addEventListener('pointerup', end)
    surface.addEventListener('pointercancel', end)
  }

}

/**
 * What the menus keep for themselves: the rows the cursor walks, the fields
 * and forms, the on-screen keyboard, and anything that scrolls (a document,
 * the roster, the settings). Everything else on the screen — the canvas, the
 * title, the splash line, the message line, the button legend, the empty
 * space between them — is scenery, and a press on it takes hold of the room.
 */
const MENU_SELECTOR = [
  'a',
  'button',
  'input',
  'textarea',
  'select',
  'label',
  'table',
  '[data-focus]',
  '[role="button"]',
  '.item',
  '.osk',
  '.doc-scroll',
  '.roster-scroll',
  '.settings-scroll',
  '.bindings-sheet',
].join(',')

export function isScenery(target: EventTarget | null): boolean {
  return target instanceof Element ? !target.closest(MENU_SELECTOR) : false
}

/** Whether this browser can draw the room at all: WebGPU, or WebGL 2 to fall back on (a test's DOM has neither). */
function canDraw(): boolean {
  return (typeof navigator !== 'undefined' && 'gpu' in navigator) || typeof WebGL2RenderingContext !== 'undefined'
}
