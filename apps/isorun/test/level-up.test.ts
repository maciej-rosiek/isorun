// @vitest-environment happy-dom
import { describe, it, expect } from 'vitest'
import { initialState } from '@isorun/webtiles'
import { LevelUp } from '../src/level-up'

describe('the level-up banner', () => {
  it('names the new level and what it brought', () => {
    // given
    const root = document.createElement('div')
    const levelUp = new LevelUp(root, { icon: () => null, continueMore: () => {} })
    const player = { ...initialState().player, xl: 1, hp_max: 19, mp_max: 0, str: 20, int: 5, dex: 9 }
    levelUp.sync(player, false)

    // when
    levelUp.messages([{ text: 'You have reached level 2!', channel: 11, turn: 7 }])
    levelUp.sync({ ...player, xl: 2, hp_max: 26, mp_max: 1 }, false)

    // then
    expect(root.querySelector('.level-up .level')?.textContent).toBe('Level 2')
    expect(Array.from(root.querySelectorAll('.level-up-gains .gain')).map((g) => g.textContent)).toEqual(['+7 Max HP', '+1 Max MP'])
    levelUp.destroy()
  })
})
