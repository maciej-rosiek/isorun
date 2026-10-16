import { QUALITY, SURFACE_DETAILS, applySurfaceDetail, isQualityName, type IsoQuality, type SurfaceDetail } from '@isorun/render-iso'

const STORE_KEY = 'isorun.quality'
const SURFACE_KEY = 'isorun.surfaceDetail'

export const DEFAULT_QUALITY: IsoQuality = QUALITY.medium

/** The graphics settings kept from the last visit, or the default. */
export function storedQuality(): IsoQuality {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(STORE_KEY) ?? 'null')
    if (typeof raw === 'object' && raw !== null && 'pixelRatio' in raw && 'aoResolution' in raw && 'shadows' in raw && 'torchShadows' in raw) {
      const { pixelRatio, aoResolution, shadows, torchShadows } = raw
      const label = 'label' in raw && typeof raw.label === 'string' ? raw.label : ''
      const actorTorchShadows = 'actorTorchShadows' in raw && raw.actorTorchShadows === true
      const aoSamples = 'aoSamples' in raw && typeof raw.aoSamples === 'number' ? raw.aoSamples : DEFAULT_QUALITY.aoSamples
      const antialias = 'antialias' in raw && typeof raw.antialias === 'boolean' ? raw.antialias : DEFAULT_QUALITY.antialias
      if (typeof pixelRatio === 'number' && typeof aoResolution === 'number' && typeof shadows === 'boolean' && typeof torchShadows === 'number') return { label, pixelRatio, aoResolution, aoSamples, antialias, shadows, torchShadows, actorTorchShadows }
    }
  } catch {
    return DEFAULT_QUALITY
  }
  return DEFAULT_QUALITY
}

/**
 * A box with the graphics presets (Low, Medium, High) and a control for every graphics setting.
 * `onChange` gets the settings the user picks.
 */
export function qualityPanel(container: HTMLElement, initial: IsoQuality, onChange: (quality: IsoQuality) => void) {
  let quality = { ...initial }
  const title = document.createElement('h2')
  title.textContent = 'Graphics'
  const presetRow = document.createElement('div')
  presetRow.className = 'row'
  const controls: (() => void)[] = []

  const pick = (next: IsoQuality) => {
    quality = { ...next }
    for (const show of controls) show()
    for (const b of presetRow.querySelectorAll<HTMLButtonElement>('button')) {
      const name = b.dataset.preset ?? ''
      const preset = isQualityName(name) ? QUALITY[name] : undefined
      b.classList.toggle('on', preset !== undefined && preset.pixelRatio === quality.pixelRatio && preset.aoResolution === quality.aoResolution && preset.aoSamples === quality.aoSamples && preset.antialias === quality.antialias && preset.shadows === quality.shadows && preset.torchShadows === quality.torchShadows && preset.actorTorchShadows === quality.actorTorchShadows)
    }
    localStorage.setItem(STORE_KEY, JSON.stringify(quality))
    onChange(quality)
  }

  for (const [name, preset] of Object.entries(QUALITY)) {
    const button = document.createElement('button')
    button.textContent = preset.label
    button.dataset.preset = name
    button.addEventListener('click', () => pick(preset))
    presetRow.append(button)
  }

  const slider = (label: string, min: number, max: number, step: number, get: () => number, set: (v: number) => IsoQuality, format: (v: number) => string) => {
    const element = document.createElement('label')
    const input = document.createElement('input')
    const value = document.createElement('span')
    value.className = 'v'
    input.type = 'range'
    input.min = String(min)
    input.max = String(max)
    input.step = String(step)
    input.addEventListener('input', () => pick({ ...set(Number(input.value)), label: '' }))
    controls.push(() => {
      input.value = String(get())
      value.textContent = format(get())
    })
    element.append(label, input, value)
    return element
  }

  const toggle = (label: string, get: () => boolean, set: (v: boolean) => IsoQuality) => {
    const element = document.createElement('label')
    const input = document.createElement('input')
    input.type = 'checkbox'
    input.addEventListener('input', () => pick({ ...set(input.checked), label: '' }))
    controls.push(() => (input.checked = get()))
    element.append(label, input)
    return element
  }

  const surfaceRow = document.createElement('div')
  surfaceRow.className = 'row'
  const pickSurface = (level: SurfaceDetail) => {
    applySurfaceDetail(level)
    localStorage.setItem(SURFACE_KEY, level)
    for (const b of surfaceRow.querySelectorAll<HTMLButtonElement>('button')) b.classList.toggle('on', b.dataset.level === level)
  }
  for (const level of SURFACE_DETAILS) {
    const button = document.createElement('button')
    button.textContent = level[0].toUpperCase() + level.slice(1)
    button.dataset.level = level
    button.addEventListener('click', () => pickSurface(level))
    surfaceRow.append(button)
  }
  const surfaceTitle = document.createElement('label')
  surfaceTitle.textContent = 'Surface detail'

  container.append(
    title,
    presetRow,
    surfaceTitle,
    surfaceRow,
    slider('Pixel ratio', 0.5, 3, 0.25, () => quality.pixelRatio, (v) => ({ ...quality, pixelRatio: v }), (v) => `${v}x`),
    slider('AO resolution', 0, 1, 0.25, () => quality.aoResolution, (v) => ({ ...quality, aoResolution: v }), (v) => (v > 0 ? `${v * 100}%` : 'off')),
    slider('AO samples', 4, 16, 4, () => quality.aoSamples, (v) => ({ ...quality, aoSamples: v }), String),
    toggle('Antialiasing', () => quality.antialias, (v) => ({ ...quality, antialias: v })),
    toggle('Shadows', () => quality.shadows, (v) => ({ ...quality, shadows: v })),
    slider('Torch shadows', 0, 8, 1, () => quality.torchShadows, (v) => ({ ...quality, torchShadows: v }), String),
    toggle('Monster torch shadows', () => quality.actorTorchShadows, (v) => ({ ...quality, actorTorchShadows: v })),
  )
  pick(quality)
  pickSurface(SURFACE_DETAILS.find((level) => level === localStorage.getItem(SURFACE_KEY)) ?? 'medium')
}
