import { describe, expect, it } from 'vitest'
import { DynamicResolution } from '../src/dynamic-resolution'

const SCREEN_MS = 1000 / 120

function play(resolution: DynamicResolution, seconds: number, frameMs: (i: number) => number) {
  for (let i = 0, ms = 0; ms < seconds * 1000; i++) {
    const frame = frameMs(i)
    resolution.frame(frame)
    ms += frame
  }
}

describe('dynamic resolution', () => {
  it('draws fewer pixels while frames come late, and more again once they are on time', () => {
    // given
    const resolution = new DynamicResolution()
    play(resolution, 2, () => SCREEN_MS)

    // when
    play(resolution, 3, (i) => (i % 3 === 0 ? SCREEN_MS * 2 : SCREEN_MS))
    const lowered = resolution.scale
    play(resolution, 60, () => SCREEN_MS)

    // then
    expect(lowered).toBeLessThan(1)
    expect(resolution.scale).toBe(1)
  })
})
