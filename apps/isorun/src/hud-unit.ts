const REFERENCE_HEIGHT = 1080
const REFERENCE_WIDTH = 1700
const MIN_UNIT = 0.75
const MAX_UNIT = 1.6

export function hudUnit(): number {
  const uiScale = Number(document.documentElement.style.getPropertyValue('--ui-scale')) || 1
  const fit = Math.min(window.innerHeight / REFERENCE_HEIGHT, window.innerWidth / REFERENCE_WIDTH)
  return uiScale * Math.max(MIN_UNIT, Math.min(MAX_UNIT, fit))
}

export function hudPx(size: number): number {
  return Math.round(size * hudUnit())
}
