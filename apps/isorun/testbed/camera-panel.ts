import { TILT_MAX, TILT_MIN, type CameraAngles } from '@isorun/render-iso'

const PERSPECTIVE_MAX = 60

const PRESETS: readonly { label: string; angles: CameraAngles }[] = [
  { label: 'Default', angles: { rotation: 45, tilt: 0, perspective: 0 } },
  { label: 'Isometric', angles: { rotation: 45, tilt: 1, perspective: 0 } },
  { label: 'High', angles: { rotation: 45, tilt: 20, perspective: 0 } },
  { label: 'Low', angles: { rotation: 30, tilt: -10, perspective: 0 } },
  { label: 'Top-down', angles: { rotation: 0, tilt: 50, perspective: 0 } },
  { label: 'Model', angles: { rotation: 45, tilt: 0, perspective: 25 } },
  { label: 'Film', angles: { rotation: 30, tilt: -8, perspective: 40 } },
]

const STORE_KEY = 'isorun.camera'

export const DEFAULT_ANGLES: CameraAngles = PRESETS[0].angles

/** The camera angles kept from the last visit, or the default. */
export function storedAngles(): CameraAngles {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(STORE_KEY) ?? 'null')
    if (typeof raw === 'object' && raw !== null && 'rotation' in raw && 'tilt' in raw && typeof raw.rotation === 'number' && typeof raw.tilt === 'number') {
      const perspective = 'perspective' in raw && typeof raw.perspective === 'number' ? raw.perspective : 0
      return { rotation: raw.rotation, tilt: raw.tilt, perspective }
    }
  } catch {
    return DEFAULT_ANGLES
  }
  return DEFAULT_ANGLES
}

/** The rotation in 0 to 359 degrees. */
export function wrapDegrees(degrees: number) {
  return ((Math.round(degrees) % 360) + 360) % 360
}

/**
 * A side panel section with sliders for the camera's rotation and tilt, and preset buttons.
 * `onChange` gets the angles the user picks; `show` puts angles that changed elsewhere (the keys) on the sliders.
 */
export function cameraPanel(container: HTMLElement, initial: CameraAngles, onChange: (angles: CameraAngles) => void) {
  let angles = { ...initial }
  const title = document.createElement('h2')
  title.textContent = 'Camera'
  const presetRow = document.createElement('div')
  presetRow.className = 'row'
  const sliders: { input: HTMLInputElement; value: HTMLSpanElement; get: () => number }[] = []

  const show = (next: CameraAngles) => {
    angles = { ...next }
    for (const { input, value, get } of sliders) {
      input.value = String(get())
      value.textContent = String(get())
    }
    for (const b of presetRow.querySelectorAll<HTMLButtonElement>('button')) {
      const preset = PRESETS.find((p) => p.label === b.textContent)
      b.classList.toggle('on', preset !== undefined && preset.angles.rotation === wrapDegrees(angles.rotation) && preset.angles.tilt === angles.tilt && preset.angles.perspective === angles.perspective)
    }
    localStorage.setItem(STORE_KEY, JSON.stringify(angles))
  }

  const pick = (next: CameraAngles) => {
    show(next)
    onChange(angles)
  }

  for (const preset of PRESETS) {
    const button = document.createElement('button')
    button.textContent = preset.label
    button.addEventListener('click', () => pick(preset.angles))
    presetRow.append(button)
  }

  const slider = (label: string, min: number, max: number, get: () => number, set: (v: number) => CameraAngles) => {
    const element = document.createElement('label')
    const input = document.createElement('input')
    const value = document.createElement('span')
    value.className = 'v'
    input.type = 'range'
    input.min = String(min)
    input.max = String(max)
    input.step = '1'
    input.addEventListener('input', () => pick(set(Number(input.value))))
    element.append(label, input, value)
    sliders.push({ input, value, get })
    return element
  }

  container.append(
    title,
    presetRow,
    slider('Rotation', 0, 359, () => wrapDegrees(angles.rotation), (v) => ({ ...angles, rotation: v })),
    slider('Tilt', TILT_MIN, TILT_MAX, () => angles.tilt, (v) => ({ ...angles, tilt: v })),
    slider('Perspective', 0, PERSPECTIVE_MAX, () => angles.perspective, (v) => ({ ...angles, perspective: v })),
  )
  show(angles)
  return { show }
}
