import { describe, expect, it } from 'vitest'
import { MouseMode, initialState, reduce, type GameState, type MenuState } from '@isorun/webtiles'
import { cellKey, emptyScene, type SceneCell } from '@isorun/scene'
import { packItems, paperDoll, wornFromMenu, wornNow } from '../src/gear'
import { quickSlots, spellsFromMenu, type SlotSources } from '../src/quick-slots'
import { dockActions } from '../src/dock-actions'
import { deriveContext } from '../src/context'

function playing(): GameState {
  const st = initialState()
  st.phase = 'playing'
  st.inputMode = MouseMode.COMMAND
  return st
}

function cell(x: number, y: number, extra: Partial<SceneCell> = {}): SceneCell {
  return {
    x,
    y,
    kind: 'floor',
    visibility: 'visible',
    occluder: false,
    floorTile: 1,
    flags: { water: false, lava: false, excluded: false, travelTrail: false, newStair: false, cursor: false, outOfRange: false, magicMapped: false },
    ...extra,
  }
}

describe('the dock', () => {
  it('hangs what crawl says is worn on the paper doll', () => {
    // given
    const st = playing()
    reduce(st, {
      msg: 'player',
      weapon_index: 0,
      inv: {
        0: { base_type: 0, sub_type: 10, quantity: 1, letter: 97, name: 'a +0 war axe' },
        1: { base_type: 2, sub_type: 3, quantity: 1, letter: 98, name: 'a +0 scale mail' },
        2: { base_type: 2, sub_type: 12, quantity: 1, letter: 99, name: 'a +0 buckler' },
      },
    })
    const gear = { tag: 'inventory', items: [{ text: 'b - a +0 scale mail (worn)', hotkeys: [98] }, { text: 'c - a +0 buckler (worn)', hotkeys: [99] }] }

    // when
    const items = packItems(st.player)
    const doll = paperDoll(items, wornNow(st.player, items, wornFromMenu(gear.items)))

    // then
    expect([doll.weapon?.letter, doll.body?.letter, doll.offhand?.letter]).toEqual(['a', 'b', 'c'])
  })

  it('fills the quick slots with the spells crawl listed, then the consumables, around a pinned one', () => {
    // given
    const menu: Pick<MenuState, 'tag' | 'items' | 'title'> = { tag: 'spell', title: { text: 'Your spells (cast)' }, items: [{ text: ' a + Magic Dart                      Conjuration               1%       1', hotkeys: [97], tiles: [{ t: 5, tex: 5 }] }] }
    const spells = spellsFromMenu(menu) ?? []
    const potion = { slot: 52, letter: 'm', base_type: 7, quantity: 2, name: '2 potions of might', action_verb: 'Quaff' }
    const src: SlotSources = { spells, abilities: [], consumables: [potion], items: [potion], quiver: '' }

    // when
    const slots = quickSlots([null, null, { kind: 'item', slot: 52 }], src)

    // then
    expect(slots.slice(0, 3).map((s) => s && [s.label, s.pinned])).toEqual([['Magic Dart', false], null, ['2 potions of might', true]])
  })

  it('offers the stairs underfoot first, then exploring', () => {
    // given
    const st = playing()
    reduce(st, { msg: 'player', pos: { x: 4, y: 4 }, turn: 1 })
    const scene = emptyScene()
    scene.player = { x: 4, y: 4 }
    scene.playerOnLevel = true
    scene.cells.set(cellKey(4, 4), cell(4, 4, { feature: { type: 'stairs', dir: 'down' }, featureTile: 7, label: 'stone staircase leading down' }))

    // when
    const actions = dockActions(deriveContext(st, scene, 0, 'micro'), scene)

    // then
    expect(actions.slice(0, 2).map((a) => [a.label, a.here])).toEqual([['Go down', true], ['Explore', false]])
  })
})
