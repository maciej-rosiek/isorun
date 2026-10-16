const COMPACT_WIDTH = 900
const COMPACT_HEIGHT = 560

export function touchScreen(): boolean {
  return typeof window !== 'undefined' && typeof window.matchMedia === 'function' && window.matchMedia('(pointer: coarse)').matches
}

export function compactWindow(): boolean {
  return window.innerWidth < COMPACT_WIDTH || window.innerHeight < COMPACT_HEIGHT
}

export function landscapeWindow(): boolean {
  return window.innerWidth > window.innerHeight
}
