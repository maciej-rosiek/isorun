// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest'
import { initialState, reduce, UiState } from '@isorun/webtiles'
import { deriveContext, deriveMode } from '../src/context'

describe('the level map opened from the ctrl-f results', () => {
  /** stash.cc `on_single_selection`: `show_map` runs with the results menu still on crawl's stack, hidden by its `ui::cutoff_point` */
  function openFromSearch() {
    const st = initialState()
    st.phase = 'playing'
    reduce(st, { msg: 'menu', tag: 'stash', flags: 0, title: { text: 'Search results' }, items: [{ text: 'a - a scroll', hotkeys: [97], level: 2 }] })
    reduce(st, { msg: 'ui_cutoff', cutoff: 0 })
    reduce(st, { msg: 'ui_state', state: UiState.VIEW_MAP })
    reduce(st, { msg: 'cursor', id: 2, loc: { x: 12, y: 7 } })
    return st
  }

  it('is the level map, not the hidden menu (menu.js takes no keys for a hidden popup)', () => {
    const st = openFromSearch()
    expect(st.menus[0]?.hidden).toBe(true)
    expect(deriveMode(st)).toBe('levelmap')
    expect(st.cursors[2]).toEqual({ x: 12, y: 7 })
  })

  it('gives the menu back when the map closes and the cutoff lifts', () => {
    const st = openFromSearch()
    reduce(st, { msg: 'cursor', id: 2 })
    reduce(st, { msg: 'ui_state', state: 0 })
    reduce(st, { msg: 'ui_cutoff', cutoff: -1 })
    expect(deriveMode(st)).toBe('menu')
    expect(st.cursors[2]).toBeNull()
  })
})

describe('the level map cursor away from the player', () => {
  it('is what the pad travels to (CURSOR_MAP is id 2)', () => {
    const st = initialState()
    st.phase = 'playing'
    reduce(st, { msg: 'ui_state', state: UiState.VIEW_MAP })
    const scene = { player: { x: 0, y: 0 }, cells: new Map(), billboards: [], playerOnLevel: true } as never
    reduce(st, { msg: 'cursor', id: 2, loc: { x: 0, y: 0 } })
    expect(deriveContext(st, scene, 0, 'micro').mapCursorHome).toBe(true)
    reduce(st, { msg: 'cursor', id: 2, loc: { x: 2, y: 1 } })
    expect(deriveContext(st, scene, 0, 'micro').mapCursorHome).toBe(false)
  })
})
