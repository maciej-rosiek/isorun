import { describe, expect, it } from 'vitest'
import { MouseMode, initialState, reduce, type GameState } from '@isorun/webtiles'
import { cellKey, emptyScene, type Scene, type SceneCell } from '@isorun/scene'
import { deriveContext } from '../src/context'
import { cellMenuRows } from '../src/cell-menu'

function playing(): GameState {
  const st = initialState()
  st.phase = 'playing'
  st.inputMode = MouseMode.COMMAND
  reduce(st, { msg: 'player', pos: { x: 4, y: 4 }, turn: 1 })
  return st
}

function cell(x: number, y: number, extra: Partial<SceneCell> = {}): SceneCell {
  return { x, y, kind: 'floor', occluder: false, visibility: 'visible', floorTile: 1, label: 'floor', flags: { water: false, lava: false, excluded: false, travelTrail: false, newStair: false, cursor: false, outOfRange: false, magicMapped: false }, ...extra }
}

function level(): Scene {
  const scene = emptyScene()
  scene.player = { x: 4, y: 4 }
  scene.playerOnLevel = true
  for (let x = 0; x < 9; x++) for (let y = 0; y < 9; y++) scene.cells.set(cellKey(x, y), cell(x, y))
  return scene
}

const rowsAt = (scene: Scene, x: number, y: number) => cellMenuRows(deriveContext(playing(), scene, 0, 'micro'), scene, x, y).map((r) => [r.label, r.run])

describe('the menu a long press opens on a cell', () => {
  it('attacks a hostile next to the player, and describes it', () => {
    // given
    const scene = level()
    scene.billboards.push({ x: 5, y: 4, tile: 0, kind: 'monster', height: 1, name: 'goblin', attitude: 'hostile', threat: 'easy' })

    // when / then
    expect(rowsAt(scene, 5, 4)).toEqual([
      ['Attack', { kind: 'click', button: 1 }],
      ['Describe', { kind: 'click', button: 3 }],
    ])
  })

  it('only describes a hostile further away', () => {
    // given
    const scene = level()
    scene.billboards.push({ x: 7, y: 4, tile: 0, kind: 'monster', height: 1, name: 'goblin', attitude: 'hostile', threat: 'easy' })

    // when / then
    expect(rowsAt(scene, 7, 4)).toEqual([['Describe', { kind: 'click', button: 3 }]])
  })

  it('goes to a cell with items, and describes them', () => {
    // given
    const scene = level()
    scene.billboards.push({ x: 2, y: 2, tile: 0, kind: 'item', height: 0.5, name: 'dagger' })

    // when / then
    expect(rowsAt(scene, 2, 2).map(([label]) => label)).toEqual(['Go here', 'Describe'])
  })

  it('offers what the player’s own square offers', () => {
    // given
    const scene = level()
    scene.cells.set(cellKey(4, 4), cell(4, 4, { kind: 'feature', feature: { type: 'stairs', dir: 'down' }, featureTile: 7, label: 'stone staircase leading down' }))

    // when / then
    expect(rowsAt(scene, 4, 4).map(([label]) => label)).toEqual(['Go down', 'Describe'])
  })

  it('has nothing for a cell never seen', () => {
    // given
    const scene = level()
    scene.cells.set(cellKey(1, 1), cell(1, 1, { visibility: 'unseen' }))

    // when / then
    expect(rowsAt(scene, 1, 1)).toEqual([])
  })
})
