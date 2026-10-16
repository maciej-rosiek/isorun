import { LIGHTS, UPSCALES, isLightsName, type IsoLights, type LightsName } from '@isorun/render-iso'

interface ValueField {
  label: string
  get: (l: IsoLights) => number
  set: (l: IsoLights, value: number) => void
  kind: 'number' | 'colour'
  min?: number
  max?: number
  step?: number
  /** Names shown for the values 0, 1, 2... in place of the numbers. */
  names?: readonly string[]
}

interface ToggleField {
  label: string
  get: (l: IsoLights) => boolean
  set: (l: IsoLights, value: boolean) => void
  kind: 'toggle'
}

type Field = ValueField | ToggleField

function toggle(label: string, of: (l: IsoLights) => { enabled: boolean }): ToggleField {
  return { label, kind: 'toggle', get: (l) => of(l).enabled, set: (l, v) => (of(l).enabled = v) }
}

const SECTIONS: { title: string; fields: Field[] }[] = [
  {
    title: 'Overall',
    fields: [
      { label: 'Exposure', kind: 'number', min: 0.5, max: 6, step: 0.1, get: (l) => l.exposure, set: (l, v) => (l.exposure = v) },
      { label: 'Background', kind: 'colour', get: (l) => l.background, set: (l, v) => (l.background = v) },
    ],
  },
  {
    title: 'Fill light',
    fields: [
      { label: 'Strength', kind: 'number', min: 0, max: 6, step: 0.05, get: (l) => l.ambient.intensity, set: (l, v) => (l.ambient.intensity = v) },
      { label: 'From above', kind: 'colour', get: (l) => l.ambient.sky, set: (l, v) => (l.ambient.sky = v) },
      { label: 'From below', kind: 'colour', get: (l) => l.ambient.ground, set: (l, v) => (l.ambient.ground = v) },
    ],
  },
  {
    title: 'Sun',
    fields: [
      { label: 'Colour', kind: 'colour', get: (l) => l.sun.colour, set: (l, v) => (l.sun.colour = v) },
      { label: 'Strength', kind: 'number', min: 0, max: 8, step: 0.05, get: (l) => l.sun.intensity, set: (l, v) => (l.sun.intensity = v) },
    ],
  },
  {
    title: 'Wall torches',
    fields: [
      { label: 'Colour', kind: 'colour', get: (l) => l.torch.colour, set: (l, v) => (l.torch.colour = v) },
      { label: 'Strength', kind: 'number', min: 0, max: 30, step: 0.25, get: (l) => l.torch.intensity, set: (l, v) => (l.torch.intensity = v) },
      { label: 'Reach', kind: 'number', min: 1, max: 16, step: 0.5, get: (l) => l.torch.distance, set: (l, v) => (l.torch.distance = v) },
      { label: 'Falloff', kind: 'number', min: 0, max: 2, step: 0.1, get: (l) => l.torch.decay, set: (l, v) => (l.torch.decay = v) },
    ],
  },
  {
    title: 'Shadows',
    fields: [{ label: 'Softness', kind: 'number', min: 0, max: 12, step: 0.5, get: (l) => l.shadowSoftness, set: (l, v) => (l.shadowSoftness = v) }],
  },
  {
    title: 'Shading',
    fields: [
      { label: 'Wrap (clay)', kind: 'number', min: 0, max: 1, step: 0.05, get: (l) => l.shading.wrap, set: (l, v) => (l.shading.wrap = v) },
      { label: 'Flat tones', kind: 'number', min: 0, max: 8, step: 1, get: (l) => l.shading.bands, set: (l, v) => (l.shading.bands = v) },
      { label: 'Relief', kind: 'number', min: 0, max: 2, step: 0.05, get: (l) => l.shading.relief, set: (l, v) => (l.shading.relief = v) },
      { label: 'Sprites', kind: 'number', min: 0.2, max: 1.5, step: 0.05, get: (l) => l.shading.sprites, set: (l, v) => (l.shading.sprites = v) },
    ],
  },
  {
    title: 'Painted tiles',
    fields: [
      { label: 'Upscale', kind: 'number', min: 0, max: UPSCALES.length - 1, step: 1, names: UPSCALES, get: (l) => UPSCALES.indexOf(l.paint.upscale), set: (l, v) => (l.paint.upscale = UPSCALES[v] ?? 'scale2x') },
      { label: 'Brush', kind: 'number', min: 0, max: 8, step: 1, get: (l) => l.paint.radius, set: (l, v) => (l.paint.radius = v) },
      { label: 'Lift', kind: 'number', min: 0.5, max: 3, step: 0.1, get: (l) => l.paint.lift, set: (l, v) => (l.paint.lift = v) },
      { label: 'Tones', kind: 'number', min: 0, max: 16, step: 1, get: (l) => l.paint.levels, set: (l, v) => (l.paint.levels = v) },
    ],
  },
  {
    title: 'Walls',
    fields: [
      { label: 'Bevel', kind: 'number', min: 0, max: 0.3, step: 0.01, get: (l) => l.walls.bevel, set: (l, v) => (l.walls.bevel = v) },
      { label: 'Moss', kind: 'number', min: 0, max: 1, step: 0.05, get: (l) => l.walls.moss, set: (l, v) => (l.walls.moss = v) },
      { label: 'Moss colour', kind: 'colour', get: (l) => l.walls.mossColour, set: (l, v) => (l.walls.mossColour = v) },
    ],
  },
  {
    title: 'Ambient occlusion (GTAO)',
    fields: [
      toggle('On', (l) => l.ao),
      { label: 'Strength', kind: 'number', min: 0, max: 1, step: 0.05, get: (l) => l.ao.intensity, set: (l, v) => (l.ao.intensity = v) },
      { label: 'Radius', kind: 'number', min: 0.1, max: 4, step: 0.05, get: (l) => l.ao.radius, set: (l, v) => (l.ao.radius = v) },
    ],
  },
  {
    title: 'Outlines',
    fields: [
      toggle('On', (l) => l.outline),
      { label: 'Thickness', kind: 'number', min: 0.5, max: 4, step: 0.25, get: (l) => l.outline.thickness, set: (l, v) => (l.outline.thickness = v) },
      { label: 'Strength', kind: 'number', min: 0, max: 1, step: 0.05, get: (l) => l.outline.strength, set: (l, v) => (l.outline.strength = v) },
      { label: 'Colour', kind: 'colour', get: (l) => l.outline.colour, set: (l, v) => (l.outline.colour = v) },
    ],
  },
  {
    title: 'Painted screen',
    fields: [
      toggle('On', (l) => l.painted),
      { label: 'Brush', kind: 'number', min: 0.5, max: 4, step: 0.25, get: (l) => l.painted.radius, set: (l, v) => (l.painted.radius = v) },
    ],
  },
  {
    title: 'Pixel look',
    fields: [
      toggle('On', (l) => l.pixelate),
      { label: 'Size', kind: 'number', min: 2, max: 8, step: 0.5, get: (l) => l.pixelate.size, set: (l, v) => (l.pixelate.size = v) },
    ],
  },
  {
    title: 'Bloom',
    fields: [
      toggle('On', (l) => l.bloom),
      { label: 'Strength', kind: 'number', min: 0, max: 3, step: 0.05, get: (l) => l.bloom.strength, set: (l, v) => (l.bloom.strength = v) },
      { label: 'Radius', kind: 'number', min: 0, max: 1, step: 0.05, get: (l) => l.bloom.radius, set: (l, v) => (l.bloom.radius = v) },
      { label: 'Threshold', kind: 'number', min: 0, max: 2, step: 0.05, get: (l) => l.bloom.threshold, set: (l, v) => (l.bloom.threshold = v) },
    ],
  },
  {
    title: 'Tilt-shift blur',
    fields: [
      toggle('On', (l) => l.tiltShift),
      { label: 'Blur', kind: 'number', min: 0, max: 6, step: 0.1, get: (l) => l.tiltShift.blur, set: (l, v) => (l.tiltShift.blur = v) },
      { label: 'Sharp band', kind: 'number', min: 0, max: 1, step: 0.05, get: (l) => l.tiltShift.focus, set: (l, v) => (l.tiltShift.focus = v) },
      { label: 'Falloff', kind: 'number', min: 0.05, max: 1, step: 0.05, get: (l) => l.tiltShift.falloff, set: (l, v) => (l.tiltShift.falloff = v) },
    ],
  },
  {
    title: 'Colour grade',
    fields: [
      { label: 'Saturation', kind: 'number', min: 0, max: 2, step: 0.05, get: (l) => l.grade.saturation, set: (l, v) => (l.grade.saturation = v) },
      { label: 'Dark tint', kind: 'colour', get: (l) => l.grade.shadows, set: (l, v) => (l.grade.shadows = v) },
      { label: 'Bright tint', kind: 'colour', get: (l) => l.grade.highlights, set: (l, v) => (l.grade.highlights = v) },
      { label: 'Vignette', kind: 'number', min: 0, max: 1.5, step: 0.05, get: (l) => l.grade.vignette, set: (l, v) => (l.grade.vignette = v) },
    ],
  },
  {
    title: 'Fog',
    fields: [
      toggle('On', (l) => l.fog),
      { label: 'Colour', kind: 'colour', get: (l) => l.fog.colour, set: (l, v) => (l.fog.colour = v) },
      { label: 'Density', kind: 'number', min: 0, max: 1, step: 0.05, get: (l) => l.fog.density, set: (l, v) => (l.fog.density = v) },
      { label: 'Height', kind: 'number', min: 0.1, max: 2, step: 0.05, get: (l) => l.fog.height, set: (l, v) => (l.fog.height = v) },
      { label: 'Reach', kind: 'number', min: 1, max: 10, step: 0.5, get: (l) => l.fog.reach, set: (l, v) => (l.fog.reach = v) },
      { label: 'Softness', kind: 'number', min: 0, max: 1, step: 0.05, get: (l) => l.fog.softness, set: (l, v) => (l.fog.softness = v) },
    ],
  },
  {
    title: 'Water',
    fields: [
      toggle('On', (l) => l.water),
      { label: 'Deep', kind: 'colour', get: (l) => l.water.deep, set: (l, v) => (l.water.deep = v) },
      { label: 'Shallow', kind: 'colour', get: (l) => l.water.shallow, set: (l, v) => (l.water.shallow = v) },
      { label: 'Light', kind: 'colour', get: (l) => l.water.light, set: (l, v) => (l.water.light = v) },
      { label: 'Strength', kind: 'number', min: 0, max: 1, step: 0.05, get: (l) => l.water.strength, set: (l, v) => (l.water.strength = v) },
      { label: 'Speed', kind: 'number', min: 0, max: 3, step: 0.1, get: (l) => l.water.speed, set: (l, v) => (l.water.speed = v) },
    ],
  },
  {
    title: 'Lava',
    fields: [
      toggle('On', (l) => l.lava),
      { label: 'Crust', kind: 'colour', get: (l) => l.lava.crust, set: (l, v) => (l.lava.crust = v) },
      { label: 'Molten', kind: 'colour', get: (l) => l.lava.molten, set: (l, v) => (l.lava.molten = v) },
      { label: 'Hot', kind: 'colour', get: (l) => l.lava.hot, set: (l, v) => (l.lava.hot = v) },
      { label: 'Glow', kind: 'number', min: 0, max: 3, step: 0.1, get: (l) => l.lava.glow, set: (l, v) => (l.lava.glow = v) },
      { label: 'Speed', kind: 'number', min: 0, max: 3, step: 0.1, get: (l) => l.lava.speed, set: (l, v) => (l.lava.speed = v) },
    ],
  },
  {
    title: 'Combat',
    fields: [
      { label: 'Blood', kind: 'colour', get: (l) => l.combat.blood, set: (l, v) => (l.combat.blood = v) },
      { label: 'Gore', kind: 'number', min: 0, max: 3, step: 0.1, get: (l) => l.combat.gore, set: (l, v) => (l.combat.gore = v) },
      { label: 'Shake', kind: 'number', min: 0, max: 3, step: 0.1, get: (l) => l.combat.shake, set: (l, v) => (l.combat.shake = v) },
      { label: 'Hit stop', kind: 'number', min: 0, max: 0.3, step: 0.01, get: (l) => l.combat.hitStop, set: (l, v) => (l.combat.hitStop = v) },
      { label: 'Flash', kind: 'number', min: 0, max: 3, step: 0.1, get: (l) => l.combat.flash, set: (l, v) => (l.combat.flash = v) },
    ],
  },
  {
    title: 'Steps',
    fields: [
      { label: 'Colour', kind: 'colour', get: (l) => l.steps.colour, set: (l, v) => (l.steps.colour = v) },
      { label: 'Dust', kind: 'number', min: 0, max: 3, step: 0.1, get: (l) => l.steps.dust, set: (l, v) => (l.steps.dust = v) },
      { label: 'Footprints', kind: 'number', min: 0, max: 30, step: 1, get: (l) => l.steps.prints, set: (l, v) => (l.steps.prints = v) },
    ],
  },
  {
    title: 'Pointer',
    fields: [
      { label: 'Colour', kind: 'colour', get: (l) => l.aim.colour, set: (l, v) => (l.aim.colour = v) },
      { label: 'Brightness', kind: 'number', min: 0.2, max: 4, step: 0.1, get: (l) => l.aim.brightness, set: (l, v) => (l.aim.brightness = v) },
    ],
  },
]

const FIELDS = SECTIONS.flatMap((s) => s.fields.map((field) => ({ key: `${s.title}/${field.label}`, field })))
const CURRENT_KEY = 'isorun.lights.current'
const SAVED_KEY = 'isorun.lights.saved'
const BASE: LightsName = 'diorama'

type Values = Record<string, number | boolean>

interface Saved {
  name: string
  values: Values
}

function readJson(key: string): unknown {
  try {
    return JSON.parse(localStorage.getItem(key) ?? 'null')
  } catch {
    return null
  }
}

function parseValues(raw: unknown): Values {
  const out: Values = {}
  if (typeof raw !== 'object' || raw === null) return out
  for (const [k, v] of Object.entries(raw)) if (typeof v === 'number' || typeof v === 'boolean') out[k] = v
  return out
}

function parseSaved(raw: unknown): Saved[] {
  if (!Array.isArray(raw)) return []
  return raw.flatMap((e: unknown) => (typeof e === 'object' && e !== null && 'name' in e && typeof e.name === 'string' && 'values' in e ? [{ name: e.name, values: parseValues(e.values) }] : []))
}

function valuesOf(lights: IsoLights): Values {
  const out: Values = {}
  for (const { key, field } of FIELDS) out[key] = field.get(lights)
  return out
}

function withValues(values: Values): IsoLights {
  const lights: IsoLights = structuredClone(LIGHTS[BASE])
  for (const { key, field } of FIELDS) {
    const v = values[key]
    if (field.kind === 'toggle' && typeof v === 'boolean') field.set(lights, v)
    else if (field.kind !== 'toggle' && typeof v === 'number') field.set(lights, v)
  }
  return lights
}

/** The lights tuned in the panel last (the Look setting's Custom), or nothing when none were. */
export function customLights(): IsoLights | null {
  const stored = readJson(CURRENT_KEY)
  return stored === null ? null : withValues(parseValues(stored))
}

/** The lights a Look setting stands for. */
export function lightsFor(look: LightsName | 'custom'): IsoLights {
  return look === 'custom' ? (customLights() ?? LIGHTS[BASE]) : LIGHTS[look]
}

const hex = (colour: number) => `#${colour.toString(16).padStart(6, '0')}`

/**
 * A side panel to pick a light variant and tune every value of it. The values
 * are kept in the browser (restored on reload), can be saved under a name, and
 * show as text to copy back into lights.ts.
 */
export function lightsPanel(container: HTMLElement, requested: string, onChange: (lights: IsoLights) => void) {
  const stored = readJson(CURRENT_KEY)
  let lights: IsoLights = isLightsName(requested) ? structuredClone(LIGHTS[requested]) : stored !== null ? withValues(parseValues(stored)) : structuredClone(LIGHTS[BASE])
  let active: string | null = isLightsName(requested) ? `built-in/${requested}` : stored !== null ? null : `built-in/${BASE}`
  let saved = parseSaved(readJson(SAVED_KEY))
  const presetRow = document.createElement('div')
  presetRow.className = 'row'
  const inputs: { field: Field; input: HTMLInputElement; value: HTMLSpanElement }[] = []
  const values = document.createElement('pre')

  const sync = () => {
    for (const { field, input, value } of inputs) {
      if (field.kind === 'toggle') {
        input.checked = field.get(lights)
        continue
      }
      const v = field.get(lights)
      input.value = field.kind === 'colour' ? hex(v) : String(v)
      value.textContent = field.kind === 'colour' ? '' : field.names?.[v] ?? String(v)
    }
    for (const b of presetRow.querySelectorAll<HTMLButtonElement>('button[data-preset]')) b.classList.toggle('on', b.dataset.preset === active)
    values.textContent = JSON.stringify(lights, (_, v: unknown) => (typeof v === 'number' && v > 255 ? hex(v) : v), 1)
      .replace(/\n\s*/g, ' ')
      .replace(/ (?="(ambient|torch|exposure|background|sun|shadowSoftness|shading|paint|walls|ao|outline|painted|pixelate|bloom|tiltShift|grade|aim|fog|water|lava|combat|steps))/g, '\n  ')
    localStorage.setItem(CURRENT_KEY, JSON.stringify(valuesOf(lights)))
    onChange(lights)
  }

  const presetButton = (id: string, label: string, pick: () => IsoLights) => {
    const button = document.createElement('button')
    button.textContent = label
    button.dataset.preset = id
    button.addEventListener('click', () => {
      lights = pick()
      active = id
      sync()
    })
    return button
  }

  const renderPresets = () => {
    presetRow.replaceChildren()
    for (const [name, spec] of Object.entries(LIGHTS)) {
      if (isLightsName(name)) presetRow.append(presetButton(`built-in/${name}`, spec.label, () => structuredClone(LIGHTS[name])))
    }
    for (const entry of saved) {
      const group = document.createElement('span')
      group.className = 'saved'
      const remove = document.createElement('button')
      remove.textContent = 'x'
      remove.title = `Delete ${entry.name}`
      remove.addEventListener('click', () => {
        saved = saved.filter((s) => s !== entry)
        localStorage.setItem(SAVED_KEY, JSON.stringify(saved))
        renderPresets()
        sync()
      })
      group.append(presetButton(`saved/${entry.name}`, entry.name, () => withValues(entry.values)), remove)
      presetRow.append(group)
    }
    const save = document.createElement('button')
    save.textContent = 'Save as...'
    save.addEventListener('click', () => {
      const name = prompt('Name for these lights', active?.startsWith('saved/') ? active.slice('saved/'.length) : '')?.trim()
      if (!name) return
      saved = [...saved.filter((s) => s.name !== name), { name, values: valuesOf(lights) }]
      localStorage.setItem(SAVED_KEY, JSON.stringify(saved))
      active = `saved/${name}`
      renderPresets()
      sync()
    })
    presetRow.append(save)
  }

  renderPresets()
  const presetTitle = document.createElement('h2')
  presetTitle.textContent = 'Lights'
  container.append(presetTitle, presetRow)

  for (const section of SECTIONS) {
    const title = document.createElement('h2')
    title.textContent = section.title
    container.append(title)
    for (const field of section.fields) {
      const label = document.createElement('label')
      const input = document.createElement('input')
      const value = document.createElement('span')
      value.className = 'v'
      if (field.kind === 'toggle') input.type = 'checkbox'
      else if (field.kind === 'colour') input.type = 'color'
      else {
        input.type = 'range'
        input.min = String(field.min)
        input.max = String(field.max)
        input.step = String(field.step)
      }
      input.addEventListener('input', () => {
        if (field.kind === 'toggle') field.set(lights, input.checked)
        else field.set(lights, field.kind === 'colour' ? parseInt(input.value.slice(1), 16) : Number(input.value))
        active = null
        sync()
      })
      label.append(field.label, input, value)
      container.append(label)
      inputs.push({ field, input, value })
    }
  }
  const valuesTitle = document.createElement('h2')
  valuesTitle.textContent = 'Values'
  container.append(valuesTitle, values)
  sync()
}
