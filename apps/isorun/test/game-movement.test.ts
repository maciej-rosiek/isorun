import { describe, expect, it, vi } from 'vitest'
import { emptyScene } from '@isorun/scene'
import { IsoCamera } from '@isorun/render-iso'
import { initialState, MouseMode } from '@isorun/webtiles'
import { GameScreen } from '../src/game'
import { Runner, type RunnerHooks } from '../src/runner'
import type { Session } from '../src/session'

/** Real input, hop tracking and scene routing; no socket, DOM or renderer. */
function harness() {
  const state = initialState()
  state.inputMode = MouseMode.COMMAND
  const scene = emptyScene()
  scene.playerOnLevel = true
  const send = vi.fn()
  const session = { watching: false, state, scene, send } as unknown as Session
  const cam = new IsoCamera()
  const swing = vi.fn()
  const runner = new Runner(session, cam, {
    context: () => ({ mode: 'command' }), now: () => performance.now(), swing,
  } as unknown as RunnerHooks)
  const screen = Object.assign(Object.create(GameScreen.prototype), {
    session, cam, runner, wake: vi.fn(), lastPos: { x: 0, y: 0 }, hopPos: { x: 0, y: 0 },
    hops: [], jumped: false, lastLevel: `${state.player.place}:${state.player.depth}`,
    padStep: null, aimDir: null, arrowsDown: new Set(), view: cam,
  }) as { trackHop(): void; onScene(): void }
  const position = (x: number, y: number) => {
    state.player.pos = { x, y }
    scene.player = { ...scene.player, x, y }
    screen.trackHop()
  }
  const echo = (x: number, y: number) => {
    position(x, y)
    screen.onScene()
  }
  return { cam, runner, state, scene, send, swing, position, echo, flush: () => screen.onScene() }
}

describe('forward input and confirmed movement', () => {
  it('steps screen-up, which is north-east at the default rotation, and sends the key at once', () => {
    const h = harness()
    h.runner.step(0)
    expect(h.send).toHaveBeenCalledExactlyOnceWith({ msg: 'input', text: 'u' })
    expect(h.runner.heading).toBe(1)
  })

  it('claims only the reply that matches its step, and forgets old steps on explore', () => {
    const h = harness()
    h.runner.step(0)
    h.runner.step(2, { turns: false })
    const now = h.runner.lastStep!.t
    expect(h.runner.stepForMove(1, 1, now)).not.toBeNull()
    expect(h.runner.stepForMove(-1, -1, now)).toBeNull()
    expect(h.runner.stepForMove(2, 2, now)).toBeNull()
    expect(h.runner.stepForMove(1, 1, now + 2000)).toBeNull()
    h.runner.send({ msg: 'input', text: 'o' })
    expect(h.runner.stepForMove(1, 1, now)).toBeNull()
  })

  it('swings at a hostile ahead, as the step is sent', () => {
    const h = harness()
    h.scene.billboards.push({ kind: 'monster', x: 1, y: -1, attitude: 'hostile', tile: 0 })
    h.runner.step(0)
    h.flush()
    expect(h.swing).toHaveBeenCalledOnce()
  })

  it('sends held steps without waiting for replies', () => {
    const h = harness()
    for (let i = 0; i < 3; i++) h.runner.execute({ kind: 'step', dir: 0, held: true })
    expect(h.send).toHaveBeenCalledTimes(3)
  })
})
