import { describe, expect, it } from 'vitest'
import { liquidLabel, liquidOf } from '../src/index'

describe('the liquid of a cell', () => {
  it('tells every liquid and wet floor apart by the tile, and deep from shallow', () => {
    // given
    const tiles = ['DNGN_LAVA_3', 'DNGN_SHALLOW_WATER_1', 'DNGN_DEEP_WATER_MURKY', 'DNGN_OPEN_SEA', 'DNGN_TOXIC_BOG_2', 'DNGN_STARWATER_4', 'LIQUEFACTION_1', 'FLOOR_SLIME_2', 'FLOOR_SLIME_ACIDIC', 'FLOOR_GREY_DIRT']

    // when
    const liquids = tiles.map(liquidOf)

    // then
    expect(liquids).toEqual([
      { kind: 'lava', deep: true },
      { kind: 'water', deep: false },
      { kind: 'murky', deep: true },
      { kind: 'water', deep: true },
      { kind: 'bog', deep: false },
      { kind: 'starwater', deep: true },
      { kind: 'mud', deep: false },
      { kind: 'slime', deep: false },
      { kind: 'acid', deep: false },
      undefined,
    ])
    expect(liquids.flatMap((l) => (l ? [liquidLabel(l)] : []))).toEqual(['lava', 'shallow water', 'deep water', 'deep water', 'toxic bog', 'deep water', 'mud', 'floor', 'corroding ground'])
  })
})
