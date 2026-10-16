import { describe, it, expect } from 'vitest'
import { ambientLevels, cuesForCombat, heartbeatDelay, hearing, moodFor, soundForLine, soundForUse, useKindOfKey } from '../src/audio/cues'

describe('the sounds of the game', () => {
  it('names the sound of a message line', () => {
    // given
    const lines = ['You now have 59 gold pieces.', 'f - a +0 dagger.', 'Welcome to level 5.', 'You die...', 'You see here a rat.']

    // when
    const sounds = lines.map(soundForLine)

    // then
    expect(sounds).toEqual(['gold', 'pickup', 'levelup', 'gameover', undefined])
  })

  it('sounds a use of an item by the key that started it, and not a refusal', () => {
    // given
    const key = useKindOfKey('q')

    // when
    const drunk = key && soundForUse(key, ['You feel better.'])
    const refused = key && soundForUse(key, ['You have no potions to drink.'])
    const fire = soundForUse('cast', ['The fireball explodes.'])

    // then
    expect(drunk).toBe('potion')
    expect(refused).toBeUndefined()
    expect(fire).toBe('cast-fire')
  })

  it('plays a swing for an attack and a hit and a death for a kill', () => {
    // given
    const monster = { x: 6, y: 5 }

    // when
    const swing = cuesForCombat({ kind: 'attack', by: 'player', at: monster }, () => monster)
    const kill = cuesForCombat({ kind: 'death', on: 7, from: { x: 5, y: 5 } }, () => monster)

    // then
    expect(swing).toEqual([{ sound: 'swing' }])
    expect(kill.map((c) => c.sound)).toEqual(['hit-heavy', 'death'])
    expect(kill.every((c) => c.at === monster)).toBe(true)
  })

  it('pans a sound by the screen and fades it with the distance', () => {
    // given
    const player = { x: 10, y: 10 }

    // when
    const near = hearing(player, { x: 11, y: 10 }, 1)
    const far = hearing(player, { x: 40, y: 10 }, 0)

    // then
    expect(near?.pan).toBeGreaterThan(0)
    expect(near?.gain).toBeGreaterThan(0.9)
    expect(far).toBeUndefined()
  })

  it('picks the music, the bed and the heartbeat by place and health', () => {
    // given
    const place = 'Crypt'

    // when
    const mood = moodFor(place, 2)
    const beds = ambientLevels('Lair', 20, 0)

    // then
    expect(mood).toBe('dark')
    expect(moodFor('Dungeon', 3)).toBe('calm')
    expect(beds).toEqual({ dungeon: 0, cavern: 1, water: 1, lava: 0 })
    expect(heartbeatDelay(0.2)).toBeLessThan(heartbeatDelay(0.3) ?? 0)
    expect(heartbeatDelay(0.9)).toBeUndefined()
  })
})
