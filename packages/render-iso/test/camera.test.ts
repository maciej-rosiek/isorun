import { describe, expect, it } from 'vitest'
import { IsoCamera } from '../src/camera'

describe('the camera', () => {
  it('ends a free turn on the nearest whole step', () => {
    // given
    const cam = new IsoCamera({ rotation: 45, tilt: 0, perspective: 0 })
    cam.turn(30)

    // when
    cam.snapTurn()

    // then
    expect(cam.angles.rotation).toBeCloseTo(90)
    expect(cam.facing).toBe(new IsoCamera({ rotation: 90, tilt: 0, perspective: 0 }).facing)
  })

  it('starts at the zoom it is given, within its limits', () => {
    expect(new IsoCamera(undefined, 90).zoomGoal).toBe(90)
    expect(new IsoCamera(undefined, 10_000).zoomGoal).toBe(160)
  })
})
