// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from 'vitest'
import { canFullscreen } from '../src/fullscreen'

/** a browser that answers `display-mode` with `mode` and nothing else */
function displayMode(mode: string) {
  vi.stubGlobal('matchMedia', (q: string) => ({ matches: q === `(display-mode: ${mode})` }))
}

afterEach(() => {
  vi.unstubAllGlobals()
  Reflect.deleteProperty(document, 'fullscreenEnabled')
  Reflect.deleteProperty(document, 'fullscreenElement')
})

describe('full screen', () => {
  it('is offered in a tab, and not in a kiosk window that is full screen already', () => {
    // given
    Object.defineProperty(document, 'fullscreenEnabled', { value: true, configurable: true })
    Object.defineProperty(document, 'fullscreenElement', { value: null, configurable: true })

    // when
    displayMode('browser')
    const inTab = canFullscreen()
    displayMode('fullscreen')
    const inKiosk = canFullscreen()

    // then
    expect(inTab).toBe(true)
    expect(inKiosk).toBe(false)
  })
})
