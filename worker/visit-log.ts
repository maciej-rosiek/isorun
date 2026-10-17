import { parseVisitBeacon, type VisitBeacon } from './visit-beacon'

type SqlValue = string | number | null
type SqlRow = Record<string, unknown>

export interface SqlStorage {
  exec(query: string, ...bindings: SqlValue[]): { toArray(): SqlRow[] }
}

export interface VisitLogState {
  storage: { sql: SqlStorage }
}

export interface LiveCounts {
  onPage: number
  inGame: number
}

export interface DayCounts {
  day: string
  visitors: number
  players: number
  games: number
  played: number
  watched: number
}

export interface SessionRow {
  started: number
  seen: number
  country: string | null
  server: string | null
  games: number
  played: number
  watched: number
}

export interface VisitSummary {
  live: LiveCounts
  days: DayCounts[]
  sessions: SessionRow[]
}

const LIVE_MS = 2 * 60_000
const DAY_MS = 24 * 60 * 60_000
const REPORT_DAYS = 30
const KEEP_DAYS = 365
const RECENT_SESSIONS = 50
const COUNTRY = /^[A-Z]{2}$/

const textOrNull = (value: unknown) => (typeof value === 'string' ? value : null)

const toLive = (row: SqlRow): LiveCounts => ({ onPage: Number(row.onPage), inGame: Number(row.inGame) })

const toDay = (row: SqlRow): DayCounts => ({
  day: String(row.day),
  visitors: Number(row.visitors),
  players: Number(row.players),
  games: Number(row.games),
  played: Number(row.played),
  watched: Number(row.watched),
})

const toSession = (row: SqlRow): SessionRow => ({
  started: Number(row.started),
  seen: Number(row.seen),
  country: textOrNull(row.country),
  server: textOrNull(row.server),
  games: Number(row.games),
  played: Number(row.played),
  watched: Number(row.watched),
})

export class VisitLog {
  private readonly sql: SqlStorage

  constructor(state: VisitLogState) {
    this.sql = state.storage.sql
    this.sql.exec(`CREATE TABLE IF NOT EXISTS sessions (
      id TEXT PRIMARY KEY,
      started INTEGER NOT NULL,
      seen INTEGER NOT NULL,
      in_game INTEGER NOT NULL DEFAULT 0,
      country TEXT,
      server TEXT,
      games INTEGER NOT NULL DEFAULT 0,
      played INTEGER NOT NULL DEFAULT 0,
      watched INTEGER NOT NULL DEFAULT 0
    )`)
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url)
    if (url.pathname === '/record') {
      const beacon = parseVisitBeacon(url.searchParams)
      const country = url.searchParams.get('country') ?? ''
      if (beacon) this.record(beacon, COUNTRY.test(country) ? country : null, Date.now())
      return new Response(null, { status: 204 })
    }
    if (url.pathname === '/report') {
      return new Response(renderReport(this.summary(Date.now())), {
        headers: { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' },
      })
    }
    return new Response('not found', { status: 404 })
  }

  record(beacon: VisitBeacon, country: string | null, now: number): void {
    this.sql.exec(
      'INSERT INTO sessions (id, started, seen, country) VALUES (?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET seen = excluded.seen',
      beacon.id,
      now,
      now,
      country,
    )
    if (beacon.event === 'game') {
      this.sql.exec('UPDATE sessions SET games = games + 1, in_game = ?, server = ? WHERE id = ?', now, beacon.server, beacon.id)
    } else if (beacon.event === 'beat' && beacon.mode) {
      const column = beacon.mode === 'watch' ? 'watched' : 'played'
      this.sql.exec(`UPDATE sessions SET ${column} = ${column} + ?, in_game = ?, server = ? WHERE id = ?`, beacon.seconds, now, beacon.server, beacon.id)
    }
  }

  summary(now: number): VisitSummary {
    this.sql.exec('DELETE FROM sessions WHERE started < ?', now - KEEP_DAYS * DAY_MS)
    const [live] = this.sql
      .exec('SELECT count(*) AS onPage, coalesce(sum(in_game > ?), 0) AS inGame FROM sessions WHERE seen > ?', now - LIVE_MS, now - LIVE_MS)
      .toArray()
      .map(toLive)
    const days = this.sql
      .exec(
        `SELECT date(started / 1000, 'unixepoch') AS day, count(*) AS visitors, sum(games > 0) AS players,
          sum(games) AS games, sum(played) AS played, sum(watched) AS watched
        FROM sessions WHERE started > ? GROUP BY day ORDER BY day DESC`,
        now - REPORT_DAYS * DAY_MS,
      )
      .toArray()
      .map(toDay)
    const sessions = this.sql
      .exec('SELECT started, seen, country, server, games, played, watched FROM sessions ORDER BY started DESC LIMIT ?', RECENT_SESSIONS)
      .toArray()
      .map(toSession)
    return { live, days, sessions }
  }
}

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds} s`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes} min`
  return `${Math.floor(minutes / 60)} h ${String(minutes % 60).padStart(2, '0')} min`
}

const formatTime = (ms: number) => new Date(ms).toISOString().slice(0, 16).replace('T', ' ')

const escapeHtml = (text: string) => text.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`)

const cells = (values: (string | number)[]) => `<tr>${values.map((v) => `<td>${escapeHtml(String(v))}</td>`).join('')}</tr>`

function renderReport({ live, days, sessions }: VisitSummary): string {
  const dayRows = days.map((d) =>
    cells([d.day, d.visitors, d.players, d.games, formatDuration(d.played), d.players ? formatDuration(Math.round(d.played / d.players)) : '-', formatDuration(d.watched)]),
  )
  const sessionRows = sessions.map((s) =>
    cells([formatTime(s.started), formatDuration(Math.round((s.seen - s.started) / 1000)), s.country ?? '-', s.server ?? '-', s.games, formatDuration(s.played), formatDuration(s.watched)]),
  )
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Isorun visits</title>
<style>
  body { font: 15px system-ui, sans-serif; background: #15131a; color: #e6e1d8; margin: 2rem; }
  h1, h2 { color: #d9b56a; font-weight: 600; }
  table { border-collapse: collapse; margin-bottom: 2rem; }
  th, td { padding: 0.3rem 0.8rem; text-align: right; border-bottom: 1px solid #2c2833; }
  th { color: #a59f96; font-weight: 500; }
  td:first-child, th:first-child { text-align: left; }
  .live { font-size: 1.4rem; }
</style>
</head>
<body>
<h1>Isorun visits</h1>
<p class="live">On the page now: <b>${live.onPage}</b> &middot; in a game now: <b>${live.inGame}</b></p>
<h2>Last ${REPORT_DAYS} days (UTC)</h2>
<table>
<tr><th>Day</th><th>Visits</th><th>Players</th><th>Games</th><th>Play time</th><th>Per player</th><th>Watch time</th></tr>
${dayRows.join('\n')}
</table>
<h2>Last ${RECENT_SESSIONS} visits</h2>
<table>
<tr><th>Started (UTC)</th><th>On the page</th><th>Country</th><th>Server</th><th>Games</th><th>Play time</th><th>Watch time</th></tr>
${sessionRows.join('\n')}
</table>
</body>
</html>`
}
