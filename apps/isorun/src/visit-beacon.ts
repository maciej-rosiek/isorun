export const VISIT_PATH = '/api/visit'
export const VISIT_EVENTS = ['visit', 'game', 'beat'] as const
export type VisitEvent = (typeof VISIT_EVENTS)[number]
export const GAME_MODES = ['play', 'watch', 'offline'] as const
export type GameMode = (typeof GAME_MODES)[number]
export const BEAT_MS = 30_000
export const MAX_BEAT_SECONDS = 120

export interface VisitBeacon {
  id: string
  event: VisitEvent
  mode: GameMode | null
  server: string | null
  seconds: number
}

const SESSION_ID = /^[0-9a-f-]{36}$/
const SERVER_ID = /^[a-z0-9-]{1,24}$/

export function visitUrl(beacon: VisitBeacon): string {
  const params = new URLSearchParams({ id: beacon.id, e: beacon.event })
  if (beacon.mode) params.set('m', beacon.mode)
  if (beacon.server) params.set('s', beacon.server)
  if (beacon.seconds > 0) params.set('t', String(beacon.seconds))
  return `${VISIT_PATH}?${params}`
}

export function parseVisitBeacon(params: URLSearchParams): VisitBeacon | null {
  const id = params.get('id') ?? ''
  const event = VISIT_EVENTS.find((e) => e === params.get('e'))
  if (!SESSION_ID.test(id) || !event) return null
  const server = params.get('s') ?? ''
  const seconds = Math.round(Number(params.get('t')) || 0)
  return {
    id,
    event,
    mode: GAME_MODES.find((m) => m === params.get('m')) ?? null,
    server: SERVER_ID.test(server) ? server : null,
    seconds: Math.min(MAX_BEAT_SECONDS, Math.max(0, seconds)),
  }
}
