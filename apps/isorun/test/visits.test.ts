import { DatabaseSync } from 'node:sqlite'
import { describe, expect, it } from 'vitest'
import { parseVisitBeacon, visitUrl, type VisitBeacon } from '../src/visit-beacon'
import { VisitLog } from '../worker/visit-log'

function sqliteState() {
  const db = new DatabaseSync(':memory:')
  const exec = (query: string, ...bindings: (string | number | null)[]) => {
    const rows = db.prepare(query).all(...bindings)
    return { toArray: () => rows }
  }
  return { storage: { sql: { exec } } }
}

const beacon = (url: string): VisitBeacon => {
  const parsed = parseVisitBeacon(new URL(url, 'https://isorun.test').searchParams)
  if (!parsed) throw new Error(`not a beacon: ${url}`)
  return parsed
}

describe('visit log', () => {
  it('counts a visit, its game and the play time of its beats', () => {
    // given
    const log = new VisitLog(sqliteState())
    const id = '0b6f8a52-3c1e-4d7a-9e2b-5f4c3d2a1b0c'
    const start = Date.UTC(2026, 9, 16, 12)

    // when
    log.record(beacon(visitUrl({ id, event: 'visit', mode: null, server: null, seconds: 0 })), 'PL', start)
    log.record(beacon(visitUrl({ id, event: 'game', mode: 'play', server: 'cdi', seconds: 0 })), 'PL', start + 5_000)
    log.record(beacon(visitUrl({ id, event: 'beat', mode: 'play', server: 'cdi', seconds: 30 })), 'PL', start + 35_000)
    log.record(beacon(visitUrl({ id, event: 'beat', mode: 'play', server: 'cdi', seconds: 30 })), 'PL', start + 65_000)
    const summary = log.summary(start + 70_000)

    // then
    expect(summary.live).toEqual({ onPage: 1, inGame: 1 })
    expect(summary.days).toEqual([{ day: '2026-10-16', visitors: 1, players: 1, games: 1, played: 60, watched: 0 }])
    expect(summary.sessions).toEqual([{ started: start, seen: start + 65_000, country: 'PL', server: 'cdi', games: 1, played: 60, watched: 0 }])
  })
})
