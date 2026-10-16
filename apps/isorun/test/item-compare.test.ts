import { describe, expect, it } from 'vitest'
import { compareStats } from '../src/item-compare'

describe('item comparison', () => {
  it('compares body armour with what is worn: more armour is better, more encumbrance is worse', () => {
    // given
    const chainMail = { base_type: 2, sub_type: 4, name: 'a +2 chain mail' }
    const scaleMail = { base_type: 2, sub_type: 3, name: 'a +0 scale mail' }

    // when
    const lines = compareStats(chainMail, scaleMail)

    // then
    expect(lines).toEqual([
      { label: 'Armour', value: '10 (+4)', tone: 'better' },
      { label: 'Encumbrance', value: '14 (+4)', tone: 'worse' },
    ])
  })
})
