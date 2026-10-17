// @vitest-environment happy-dom
import { describe, expect, it, vi } from 'vitest'
import { initialState } from '@isorun/webtiles'
import { GameScreen } from '../src/game'

/**
 * A held button that goes up outside the window is never heard as a pointerup,
 * so the drag has to be let go of some other way: the page losing focus, or
 * the next move arriving with nothing held.
 */
function harness(pointerType = 'mouse') {
  const canvas = document.createElement('canvas')
  Object.assign(canvas, {
    setPointerCapture: vi.fn(), releasePointerCapture: vi.fn(), hasPointerCapture: () => true,
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 600 }),
  })
  const dragView = vi.fn()
  const onPointer = vi.fn()
  const screen = Object.assign(Object.create(GameScreen.prototype), {
    canvas, levelMap: null, drag: null, hover: null, hoverByTouch: false, touchHoverTimer: 0, pointerLive: false, tooltipTimer: 0, touches: new Map(), twoFingers: null, longPressTimer: 0,
    view: { drag: dragView }, hud: { hideTooltip: vi.fn() }, session: { state: initialState() },
    inputFrom: vi.fn(), wake: vi.fn(), armCellTooltip: vi.fn(), onPointer,
  }) as { attachPointer(c: HTMLCanvasElement): void; cancelDrag(): void; drag: unknown; hover: unknown; pointerLive: boolean }
  screen.attachPointer(canvas)
  const at = (type: string, x: number, y: number, buttons: number) => {
    canvas.dispatchEvent(Object.assign(new Event(type), { pointerId: 1, pointerType, button: 0, buttons, clientX: x, clientY: y, pageX: x, pageY: y }))
  }
  return { screen, at, dragView, onPointer }
}

describe('a drag whose release the page never saw', () => {
  it('lets go on the first move with no button held, and orbits no further', () => {
    const h = harness()
    h.at('pointerdown', 100, 100, 1)
    h.at('pointermove', 200, 100, 1)
    expect(h.dragView).toHaveBeenCalledOnce()
    // the button came up off the window; the pointer comes back over the canvas
    h.at('pointermove', 300, 100, 0)
    expect(h.screen.drag).toBe(null)
    h.at('pointermove', 400, 100, 0)
    expect(h.dragView).toHaveBeenCalledOnce()
  })

  it('lets go when the window loses focus, and the press that follows is a fresh one', () => {
    const h = harness()
    h.at('pointerdown', 100, 100, 1)
    h.at('pointermove', 200, 100, 1)
    window.dispatchEvent(new Event('blur'))
    expect(h.screen.drag).toBe(null)
    h.at('pointermove', 300, 100, 1)
    expect(h.dragView).toHaveBeenCalledOnce()
  })

  it('a stale drag is dropped without acting on the cell it ended over', () => {
    const h = harness()
    h.at('pointerdown', 100, 100, 1)
    h.at('pointermove', 101, 100, 0)
    expect(h.screen.drag).toBe(null)
    expect(h.onPointer).not.toHaveBeenCalled()
  })

  it('a second canvas (a renderer swap) shares the one blur listener, so leaving the game leaves none behind', () => {
    const add = vi.spyOn(window, 'addEventListener')
    const h = harness()
    const again = document.createElement('canvas')
    Object.assign(again, { setPointerCapture: vi.fn(), releasePointerCapture: vi.fn(), hasPointerCapture: () => true })
    h.screen.attachPointer(again)
    const blurs = add.mock.calls.filter(([type]) => type === 'blur').map(([, fn]) => fn)
    expect(blurs.length).toBe(2)
    expect(new Set(blurs).size).toBe(1)
    add.mockRestore()
  })
})

describe('a finger on the view', () => {
  it('puts the cursor under it while held, and on the tapped cell a moment after it lifts', () => {
    vi.useFakeTimers()
    const h = harness('touch')
    h.at('pointerdown', 100, 120, 1)
    expect(h.screen.hover).toEqual({ x: 100, y: 120 })
    expect(h.screen.pointerLive).toBe(true)
    h.at('pointermove', 102, 121, 1)
    expect(h.screen.hover).toEqual({ x: 102, y: 121 })
    h.at('pointerup', 102, 121, 0)
    expect(h.onPointer).toHaveBeenCalledWith(102, 121, 0)
    expect(h.screen.hover).toEqual({ x: 102, y: 121 })
    vi.advanceTimersByTime(1000)
    expect(h.screen.hover).toBe(null)
    vi.useRealTimers()
  })

  it('takes the cursor away once it drags the map', () => {
    const h = harness('touch')
    h.at('pointerdown', 100, 100, 1)
    h.at('pointermove', 200, 100, 1)
    expect(h.screen.hover).toBe(null)
    h.at('pointerup', 200, 100, 0)
    expect(h.screen.hover).toBe(null)
    expect(h.onPointer).not.toHaveBeenCalled()
  })
})

describe('two fingers on the view', () => {
  it('turn as they move across, tilt as they move down, and the turn ends on a whole step', () => {
    const canvas = document.createElement('canvas')
    Object.assign(canvas, { setPointerCapture: vi.fn(), releasePointerCapture: vi.fn(), hasPointerCapture: () => true, getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 600 }) })
    const view = { drag: vi.fn(), zoomBy: vi.fn(), turn: vi.fn(), tiltBy: vi.fn(), snapTurn: vi.fn() }
    const screen = Object.assign(Object.create(GameScreen.prototype), {
      canvas, levelMap: null, drag: null, hover: null, hoverByTouch: false, touchHoverTimer: 0, pointerLive: false, tooltipTimer: 0, touches: new Map(), twoFingers: null, longPressTimer: 0,
      view, hud: { hideTooltip: vi.fn() }, session: { state: initialState() },
      inputFrom: vi.fn(), wake: vi.fn(), armCellTooltip: vi.fn(), onPointer: vi.fn(),
    }) as { attachPointer(c: HTMLCanvasElement): void }
    screen.attachPointer(canvas)
    const at = (type: string, id: number, x: number, y: number) =>
      canvas.dispatchEvent(Object.assign(new Event(type), { pointerId: id, pointerType: 'touch', button: 0, buttons: 1, clientX: x, clientY: y, pageX: x, pageY: y }))
    at('pointerdown', 1, 100, 300)
    at('pointerdown', 2, 300, 300)
    // both move 40 px right and 20 px down, as far apart and at the same angle as before
    at('pointermove', 1, 140, 320)
    at('pointermove', 2, 340, 320)
    const turned = view.turn.mock.calls.reduce((sum, [d]) => sum + d, 0)
    const tilted = view.tiltBy.mock.calls.reduce((sum, [d]) => sum + d, 0)
    expect(turned).toBeGreaterThan(0)
    expect(tilted).toBeGreaterThan(0)
    expect(view.drag).not.toHaveBeenCalled()
    expect(view.snapTurn).not.toHaveBeenCalled()
    at('pointerup', 2, 340, 320)
    expect(view.snapTurn).toHaveBeenCalledOnce()
  })
})
