import { describe, it, expect, vi } from 'vitest'
import { cellKey, emptyScene } from '@isorun/scene'
import { IsoCamera } from '@isorun/render-iso'
import { OWN_STEP_WINDOW_MS, Runner, stepIsOurs, type LastStep, type RunnerHooks } from '../src/runner'
import type { Session } from '../src/session'

const step = (over: Partial<LastStep> = {}): LastStep => ({ dir: 2, t: 1000, ...over })

describe('stepIsOurs', () => {
  it('claims a one-cell move in the direction the step was sent', () => {
    expect(stepIsOurs(step(), 1, 0, 1200)).toBe(true)
  })

  it('does not claim a move that went another way: a one-cell explore after a keyed step', () => {
    // stepped east, then `o` walked north-east and a monster came into view
    expect(stepIsOurs(step(), 1, -1, 1200)).toBe(false)
    // a pad step has the same rule
    expect(stepIsOurs(step({ dir: 0 }), 1, -1, 1200)).toBe(false)
  })

  it('forgets a step once the window has passed', () => {
    expect(stepIsOurs(step(), 1, 0, 1000 + OWN_STEP_WINDOW_MS)).toBe(false)
    expect(stepIsOurs(step(), 1, 0, 1000 + OWN_STEP_WINDOW_MS - 1)).toBe(true)
  })

  it('claims several echoed hops that all went the way the step was sent: a held stick, batched in one frame', () => {
    // strafing east on the stick, two echoes landed in the same scene update
    expect(stepIsOurs(step(), 2, 0, 1200, [{ dx: 1, dy: 0 }, { dx: 1, dy: 0 }])).toBe(true)
    // one of the hops went another way: a walk, not ours
    expect(stepIsOurs(step(), 2, -1, 1200, [{ dx: 1, dy: 0 }, { dx: 1, dy: -1 }])).toBe(false)
    // a multi-cell displacement without hops is still a jump
    expect(stepIsOurs(step(), 2, 0, 1200)).toBe(false)
  })

  it('never claims a jump or no step at all', () => {
    expect(stepIsOurs(step(), 5, -6, 1200)).toBe(false)
    expect(stepIsOurs(null, 1, 0, 1200)).toBe(false)
  })
})

describe('explore and the last step', () => {
  function runner() {
    const sent: unknown[] = []
    const session = { watching: false, scene: emptyScene(), send: (m: unknown) => sent.push(m) } as unknown as Session
    const hooks = { context: () => ({ mode: 'command' }), now: () => 1000 } as unknown as RunnerHooks
    return { r: new Runner(session, new IsoCamera(), hooks), sent }
  }

  it('sending `o` forgets the last step, so the explore’s first cell is path-driven', () => {
    const { r, sent } = runner()
    r.lastStep = step()
    r.send({ msg: 'key', keycode: 'o'.charCodeAt(0) })
    expect(r.lastStep).toBeNull()
    expect(sent).toHaveLength(1)
  })

  it('other keys keep the last step', () => {
    const { r } = runner()
    r.lastStep = step()
    r.send({ msg: 'key', keycode: 'i'.charCodeAt(0) })
    expect(r.lastStep).not.toBeNull()
  })
})

describe('autofight', () => {
  function runner(hostileX: number) {
    const sent: unknown[] = []
    const swings: { x: number; y: number }[] = []
    const scene = emptyScene()
    scene.player = { ...scene.player, x: 5, y: 5 }
    scene.cells.set(cellKey(hostileX, 5), { x: hostileX, y: 5, kind: 'floor', visibility: 'visible', occluder: false, floorTile: 1, label: 'floor' } as never)
    scene.billboards.push({ kind: 'monster', x: hostileX, y: 5, attitude: 'hostile', tile: 0 } as never)
    const session = { watching: false, scene, send: (m: unknown) => sent.push(m) } as unknown as Session
    const hooks = { context: () => ({ mode: 'command' }), now: () => 1000, swing: (target: { x: number; y: number }) => swings.push(target) } as unknown as RunnerHooks
    return { r: new Runner(session, new IsoCamera(), hooks), sent, swings }
  }

  it('Tab only sends the key: the blow shows when the server says it landed, so a refused autofight swings nothing', () => {
    // given
    const { r, sent, swings } = runner(6)

    // when
    r.send({ msg: 'key', keycode: 9 })

    // then
    expect(swings).toEqual([])
    expect(sent).toHaveLength(1)
  })
})

describe('left and right in command mode', () => {
  function runner(watching = false) {
    const sent: { text: string }[] = []
    const swings: { x: number; y: number }[] = []
    const scene = emptyScene()
    scene.player = { ...scene.player, x: 5, y: 5 }
    const session = { watching, scene, send: (m: unknown) => sent.push(m as { text: string }) } as unknown as Session
    const view = new IsoCamera()
    const hooks = { context: () => ({ mode: 'command' }), now: () => 1000, swing: (target: { x: number; y: number }) => swings.push(target) } as unknown as RunnerHooks
    return { r: new Runner(session, view, hooks), sent, view, swings, scene }
  }

  it('an input set to strafe steps sideways: the view keeps its facing', () => {
    const { r, sent, view } = runner()
    r.step(2)
    r.step(6)
    expect(view.facing).toBe(1)
    // screen-up is north-east: right is south-east (n), left is north-west (y)
    expect(sent.map((m) => m.text)).toEqual(['n', 'y'])
    expect(r.lastStep).toMatchObject({ dir: 7 })
  })

  it('a step back into a friendly is a swap, not a swing; into a hostile it is a swing', () => {
    const { r, sent, swings, scene } = runner()
    // screen-down is south-west of 5,5: 4,6
    scene.billboards.push({ kind: 'monster', x: 4, y: 6, attitude: 'friendly', tile: 0 } as never)
    r.step(4)
    expect(sent.map((m) => m.text)).toEqual(['b'])
    expect(swings).toEqual([])
    scene.billboards[0] = { kind: 'monster', x: 4, y: 6, attitude: 'hostile', tile: 0 } as never
    r.step(4)
    expect(swings).toEqual([{ x: 4, y: 6 }])
    expect(r.heading).toBe(5)
  })

  it('an input set to turn turns the view: nothing is sent and no time passes', () => {
    const { r, sent, view } = runner()
    r.step(2, { turns: true })
    expect(view.facing).toBe(2)
    r.step(6, { turns: true })
    r.step(6, { turns: true })
    expect(view.facing).toBe(0)
    expect(sent).toEqual([])
  })

  it('only left and right answer to the setting: forward, back and the diagonals always step', () => {
    const { r, sent, view } = runner()
    for (const dir of [0, 1, 3, 4, 5, 7] as const) r.step(dir, { turns: true })
    expect(view.facing).toBe(1)
    expect(sent).toHaveLength(6)
  })

  it('spectating: a turn still turns the view, and no step is ever ours', () => {
    const { r, sent, view } = runner(true)
    r.step(2, { turns: true })
    expect(view.facing).toBe(2)
    r.step(0, { turns: true })
    r.step(4, { turns: true })
    expect(sent).toEqual([])
    expect(r.lastStep).toBe(null)
  })

  it('Shift and Ctrl with left / right step even on an input that turns', () => {
    const { r, sent, view } = runner()
    r.step(2, { turns: true, run: true })
    r.step(6, { turns: true, attack: true })
    expect(view.facing).toBe(1)
    expect(sent).toHaveLength(2)
  })
})
