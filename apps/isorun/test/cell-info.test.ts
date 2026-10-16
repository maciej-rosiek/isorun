import { describe, it, expect } from 'vitest'
import { cellKey, emptyScene, type SceneCell } from '@isorun/scene'
import { cellDetails } from '../src/cell-info'

function floor(x: number, y: number): SceneCell {
  return { x, y, kind: 'floor', occluder: false, visibility: 'visible', floorTile: 1, label: 'floor', flags: { water: false, lava: false, excluded: false, travelTrail: false, newStair: false, cursor: false, outOfRange: false, magicMapped: false } }
}

describe('the details of a cell', () => {
  it('names the monster with its threat and wounds, and the items under it', () => {
    // given
    const scene = emptyScene()
    scene.cells.set(cellKey(3, 4), floor(3, 4))
    scene.billboards.push({ x: 3, y: 4, tile: 0, kind: 'monster', height: 1, name: 'goblin', attitude: 'hostile', threat: 'easy', damage: 'heavily_damaged' })
    scene.billboards.push({ x: 3, y: 4, tile: 0, kind: 'item', height: 0.5, name: 'dagger' })

    // when
    const details = cellDetails(scene, 3, 4)

    // then
    expect(details).toEqual([
      { text: 'Goblin', tone: 'easy', wound: 'heavily_damaged' },
      { text: 'easy threat, heavily wounded', tone: 'note' },
      { text: 'Dagger' },
    ])
  })

  it('says when more items lie under the top item of a pile', () => {
    // given
    const scene = emptyScene()
    scene.cells.set(cellKey(3, 4), floor(3, 4))
    scene.billboards.push({ x: 3, y: 4, tile: 0, kind: 'item', height: 0.5, name: 'club', statusIcons: [{ tile: 1, ox: 0, oy: 0, square: true }] })

    // when
    const details = cellDetails(scene, 3, 4)

    // then
    expect(details).toEqual([{ text: 'Club and more items' }])
  })

  it('says nothing about a wall', () => {
    // given
    const scene = emptyScene()
    scene.cells.set(cellKey(3, 4), { ...floor(3, 4), kind: 'wall', occluder: true, wallTile: 2, label: 'brick dark 1' })

    // when
    const details = cellDetails(scene, 3, 4)

    // then
    expect(details).toEqual([])
  })
})
