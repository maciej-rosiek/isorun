// @vitest-environment happy-dom
import { describe, it, expect, beforeEach } from 'vitest'
import { defaultSettings, getSavedAngles, getSettings, saveAngles, saveSettings } from '../src/servers'
import { settingsPanel } from '../src/settings-panel'
import { adjustSetting, ALL_SETTING_ROWS, groupAtDefaults, resetGroup, settingGroups, MINIMAP_CELLS, MINIMAP_TILES, rowHint, rowOff, SETTING_ROWS, settingValue } from '../src/settings-rows'

// happy-dom's localStorage has no working methods; give servers.ts a plain one
const store = new Map<string, string>()
Object.defineProperty(globalThis, 'localStorage', {
  configurable: true,
  value: {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, String(v)),
    removeItem: (k: string) => void store.delete(k),
  },
})

/** Any row, offered or not; `SETTING_ROWS` is what the player sees. */
const row = (label: string) => ALL_SETTING_ROWS.find((r) => r.label === label)!

describe('what is written down', () => {
  beforeEach(() => store.clear())

  it('is only what the player changed, so a later default reaches everyone who never touched the row', () => {
    saveSettings({ ...defaultSettings })
    expect(JSON.parse(store.get('isorun.settings')!)).toEqual({})
    saveSettings({ ...defaultSettings, uiScale: 1.3 })
    expect(JSON.parse(store.get('isorun.settings')!)).toEqual({ uiScale: 1.3 })
    // the row goes back to its default: the override is dropped, not pinned at today's value
    saveSettings({ ...defaultSettings })
    expect(JSON.parse(store.get('isorun.settings')!)).toEqual({})
    expect(getSettings().uiScale).toBe(defaultSettings.uiScale)
  })

  it('drops the old split hint fields once hints are saved once', () => {
    store.set('isorun.settings', JSON.stringify({ gamepadHints: 'contextual', keyHints: false }))
    const s = getSettings()
    expect(s.hints).toBe('contextual')
    saveSettings(s)
    expect(JSON.parse(store.get('isorun.settings')!)).toEqual({ hints: 'contextual' })
  })

  it('keeps the camera angles the game was left with, and nothing else', () => {
    expect(getSavedAngles()).toBeNull()
    saveAngles({ rotation: 135, tilt: 10, perspective: 25 })
    expect(JSON.parse(store.get('isorun.camera')!)).toEqual({ rotation: 135, tilt: 10, perspective: 25 })
    expect(getSavedAngles()).toEqual({ rotation: 135, tilt: 10, perspective: 25 })
  })
})

describe('Graphics', () => {
  beforeEach(() => saveSettings({ ...defaultSettings }))

  it('Quality is Medium by default and steps through High and Low', () => {
    const quality = row('Quality')
    expect(settingValue(quality)).toBe('Medium')
    expect(adjustSetting(quality, 1)).toBe('High')
    expect(getSettings().graphics).toBe('high')
    expect(adjustSetting(quality, -1)).toBe('Medium')
    expect(adjustSetting(quality, -1)).toBe('Low')
  })

  it('a saved quality or look the build does not know is read back as the default', () => {
    store.set('isorun.settings', JSON.stringify({ graphics: 'ultra', look: 'nope' }))
    expect(getSettings().graphics).toBe(defaultSettings.graphics)
    expect(getSettings().look).toBe(defaultSettings.look)
  })

  it('no row is ever off', () => {
    for (const r of ALL_SETTING_ROWS) expect(rowOff(r)).toBe(false)
  })
})

describe('removed settings', () => {
  it('are gone from the rows', () => {
    for (const label of ['Corners', 'Wall thickness', 'Purist look', 'Render scale', 'Auto-facing', 'Edge pips', 'Nearby', 'View', 'Camera height', 'Camera angle', 'Key hints', 'Gamepad hints', 'Menu room turn', 'Keyboard left/right', 'Gamepad left/right']) expect(ALL_SETTING_ROWS.find((r) => r.label === label)).toBeUndefined()
  })
})

describe('Hints', () => {
  beforeEach(() => saveSettings({ ...defaultSettings }))

  it('is one row for gamepad and keyboard, adaptive by default, then contextual, then off', () => {
    const hints = row('Hints')
    expect(defaultSettings.hints).toBe('adaptive')
    expect(SETTING_ROWS).toContain(hints)
    expect(hints.group).toBe('Controls')
    expect(settingValue(hints)).toBe('Adaptive')
    expect(rowOff(hints)).toBe(false)
    expect(adjustSetting(hints, 1)).toBe('Contextual only')
    expect(getSettings().hints).toBe('contextual')
    expect(adjustSetting(hints, 1)).toBe('Off')
    expect(getSettings().hints).toBe('off')
    expect(adjustSetting(hints, 1)).toBe('Adaptive')
  })
})

describe('Minimap size', () => {
  beforeEach(() => saveSettings({ ...defaultSettings }))

  it('is 19 tiles across by default (enough of the level to read at a glance) and reads as a tile count', () => {
    const size = row('Minimap size')
    expect(defaultSettings.minimapTiles).toBe(19)
    expect(MINIMAP_TILES).toContain(19)
    expect(settingValue(size)).toBe('19 tiles')
    expect(rowOff(size)).toBe(false)
    expect(rowHint(size)).toMatch(/tiles across/)
  })

  it('runs over every odd count from 9 to 61, two a step, so the player stands on the centre tile', () => {
    expect(MINIMAP_TILES[0]).toBe(9)
    expect(MINIMAP_TILES[MINIMAP_TILES.length - 1]).toBe(61)
    for (const n of MINIMAP_TILES) expect(n % 2).toBe(1)
    for (let i = 1; i < MINIMAP_TILES.length; i++) expect(MINIMAP_TILES[i] - MINIMAP_TILES[i - 1]).toBe(2)
  })

  it('right is two tiles more, left two fewer', () => {
    const size = row('Minimap size')
    expect(adjustSetting(size, 1)).toBe('21 tiles')
    expect(getSettings().minimapTiles).toBe(21)
    expect(adjustSetting(size, -1)).toBe('19 tiles')
    expect(adjustSetting(size, -1)).toBe('17 tiles')
    expect(getSettings().minimapTiles).toBe(17)
  })
})

describe('Minimap tile size', () => {
  beforeEach(() => saveSettings({ ...defaultSettings }))

  it('is the 20px cell the map follows the player at, and reads in px', () => {
    const cell = row('Minimap tile size')
    expect(defaultSettings.minimapCell).toBe(20)
    expect(MINIMAP_CELLS).toContain(20)
    expect(settingValue(cell)).toBe('20px')
    expect(rowOff(cell)).toBe(false)
  })

  it('runs from 8px to 32px, every stop bigger than the last', () => {
    expect(MINIMAP_CELLS[0]).toBe(8)
    expect(MINIMAP_CELLS[MINIMAP_CELLS.length - 1]).toBe(32)
    for (let i = 1; i < MINIMAP_CELLS.length; i++) expect(MINIMAP_CELLS[i]).toBeGreaterThan(MINIMAP_CELLS[i - 1])
  })

  it('right is the next cell up, left the one down, and the tile count is left alone', () => {
    const cell = row('Minimap tile size')
    expect(adjustSetting(cell, 1)).toBe('24px')
    expect(getSettings().minimapCell).toBe(24)
    expect(adjustSetting(cell, -1)).toBe('20px')
    expect(adjustSetting(cell, -1)).toBe('16px')
    expect(getSettings().minimapCell).toBe(16)
    expect(getSettings().minimapTiles).toBe(defaultSettings.minimapTiles)
  })
})

describe('the groups', () => {
  /**
   * One group to a page (settings-panel.ts): the settings screen and the pause
   * menu both list the groups as rows, so every offered row has to sit in one
   * of them, and every group has to have a line to print under its name.
   */
  it('lays every offered row out in one of the groups, each with its own line', () => {
    const groups = settingGroups()
    expect(groups.map((g) => g.group)).toEqual(['Graphics', 'Controls', 'Interface', 'Audio'])
    for (const g of groups) {
      expect(g.hint.length).toBeGreaterThan(0)
      expect(g.rows.length).toBeGreaterThan(0)
    }
    expect(groups.flatMap((g) => g.rows)).toEqual([...SETTING_ROWS])
  })
})

describe('settings: reset a page to its defaults', () => {
  beforeEach(() => store.clear())

  it("puts back only the page's own settings, and leaves the rest as they were", () => {
    saveSettings({ ...defaultSettings, graphics: 'high', look: 'sunny', uiScale: 1.3, minimapTiles: 31 })
    expect(groupAtDefaults('Graphics')).toBe(false)
    resetGroup('Graphics')
    const s = getSettings()
    expect(s.graphics).toBe(defaultSettings.graphics)
    expect(s.look).toBe(defaultSettings.look)
    expect(groupAtDefaults('Graphics')).toBe(true)
    expect(s.uiScale).toBe(1.3)
    expect(s.minimapTiles).toBe(31)
    resetGroup('Interface')
    expect(getSettings().uiScale).toBe(defaultSettings.uiScale)
    expect(document.documentElement.style.getPropertyValue('--ui-scale')).toBe(String(defaultSettings.uiScale))
  })

  it("is a row of each page, above Back, dark while there is nothing to put back", () => {
    const onchange = () => void changes++
    let changes = 0
    for (const { group } of settingGroups()) {
      const panel = settingsPanel(group, { onchange, back: () => {} })
      const ids = panel.rows.map((r) => r.dataset.focus)
      expect(ids.slice(-2)).toEqual(['settings-reset', 'settings-back'])
      const reset = panel.rows[ids.indexOf('settings-reset')]
      expect(reset.classList.contains('off')).toBe(true)
      reset.click()
      expect(changes).toBe(0)
    }
    saveSettings({ ...defaultSettings, hints: 'off' })
    const panel = settingsPanel('Controls', { onchange })
    const reset = panel.rows.find((r) => r.dataset.focus === 'settings-reset')!
    expect(reset.classList.contains('off')).toBe(false)
    reset.click()
    expect(getSettings().hints).toBe(defaultSettings.hints)
    expect(changes).toBe(1)
    expect(reset.classList.contains('off')).toBe(true)
  })
})
