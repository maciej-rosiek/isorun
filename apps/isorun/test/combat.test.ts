import { describe, it, expect } from 'vitest'
import { emptyScene, type Billboard, type Scene } from '@isorun/scene'
import type { GameMessage } from '@isorun/webtiles'
import { combatEvents } from '../src/combat'

const line = (text: string): GameMessage => ({ text, turn: 100 })

function sceneWithGoblin(): Scene {
  const scene = emptyScene()
  scene.player = { x: 5, y: 5 }
  const goblin: Billboard = { x: 6, y: 5, tile: 0, kind: 'monster', height: 1, attitude: 'hostile', ref: { id: 7, name: 'goblin' } }
  scene.billboards.push(goblin)
  return scene
}

describe('the fight in the message lines', () => {
  it('plays the player’s blows, the monster’s blows and the kill', () => {
    // given
    const scene = sceneWithGoblin()
    const lines = [line('<lightgrey>You slash the goblin.<lightgrey>'), line('The goblin hits you.'), line('You kill the goblin!')]

    // when
    const events = combatEvents(lines, scene)

    // then
    expect(events).toEqual([
      { kind: 'attack', by: 'player', at: { x: 6, y: 5 } },
      { kind: 'hit', on: 7, from: { x: 5, y: 5 }, tier: 0 },
      { kind: 'attack', by: 7, at: { x: 5, y: 5 } },
      { kind: 'hit', on: 'player', from: { x: 6, y: 5 }, tier: 0 },
      { kind: 'attack', by: 'player', at: { x: 6, y: 5 } },
      { kind: 'death', on: 7, from: { x: 5, y: 5 } },
    ])
  })

  it('plays a miss as a swing and a dodge', () => {
    // given
    const scene = sceneWithGoblin()
    const lines = [line('You barely miss the goblin.'), line('The goblin closely misses you.')]

    // when
    const events = combatEvents(lines, scene)

    // then
    expect(events).toEqual([
      { kind: 'attack', by: 'player', at: { x: 6, y: 5 } },
      { kind: 'miss', on: 7, from: { x: 5, y: 5 } },
      { kind: 'attack', by: 7, at: { x: 5, y: 5 } },
      { kind: 'miss', on: 'player', from: { x: 6, y: 5 } },
    ])
  })

  it('plays a monster’s ranged attack and the missile that hits the player', () => {
    // given
    const scene = sceneWithGoblin()
    const lines = [line('The goblin breathes fire at you.'), line('The searing blast hits you!')]

    // when
    const events = combatEvents(lines, scene)

    // then
    expect(events).toEqual([
      { kind: 'attack', by: 7, at: { x: 5, y: 5 } },
      { kind: 'hit', on: 'player', from: undefined, tier: 1 },
    ])
  })

  it('reads how hard a blow struck and the damage it did', () => {
    // given
    const scene = sceneWithGoblin()
    const lines = [line('You slash the goblin for 12!!'), line('The goblin hits you for 3 with a club.')]

    // when
    const events = combatEvents(lines, scene)

    // then
    expect(events).toEqual([
      { kind: 'attack', by: 'player', at: { x: 6, y: 5 } },
      { kind: 'hit', on: 7, from: { x: 5, y: 5 }, tier: 2, amount: 12 },
      { kind: 'attack', by: 7, at: { x: 5, y: 5 } },
      { kind: 'hit', on: 'player', from: { x: 6, y: 5 }, tier: 0, amount: 3 },
    ])
  })
})
