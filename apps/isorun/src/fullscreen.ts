/**
 * Full screen on request, for the players in an ordinary tab: the page asks
 * the browser to put it full screen (the Fullscreen API), and F11 or Escape,
 * or the same row again, is the way back.
 */

import { androidApp } from './servers'

/** whether the page is full screen now, by its own request */
export function isFullscreen(): boolean {
  return typeof document !== 'undefined' && document.fullscreenElement !== null
}

/**
 * Whether to offer full screen: the browser allows it (an iPhone does not),
 * and the window is not already full screen without the page asking, as a
 * kiosk browser is (quit.ts), where there is nothing to leave. The Android app
 * is always full screen: nothing to offer there either.
 */
export function canFullscreen(): boolean {
  if (androidApp()) return false
  if (typeof document === 'undefined' || !document.fullscreenEnabled) return false
  if (isFullscreen()) return true
  return typeof window.matchMedia !== 'function' || !window.matchMedia('(display-mode: fullscreen)').matches
}

/**
 * Go full screen, or leave it. A browser asks for a click or a key to allow
 * it, and a press on a gamepad does not count, so a refusal is let pass: the
 * row stays as it was.
 */
export function toggleFullscreen(): void {
  const asked = isFullscreen() ? document.exitFullscreen() : document.documentElement.requestFullscreen({ navigationUI: 'hide' })
  asked.catch(() => {})
}

/** the words for the row that goes full screen or leaves it */
export function fullscreenLabel(): string {
  return isFullscreen() ? 'Leave full screen' : 'Full screen'
}
