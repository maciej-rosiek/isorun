import { BEAT_MS, visitUrl, type GameMode, type VisitBeacon } from './visit-beacon'

export interface GameOnScreen {
  mode: GameMode
  server: string
}

export class Visits {
  private readonly id = crypto.randomUUID()
  private countedSince = performance.now()

  constructor(private readonly gameOnScreen: () => GameOnScreen | null) {}

  start(): void {
    this.send({ event: 'visit', mode: null, server: null, seconds: 0 })
    setInterval(() => this.beat(!document.hidden), BEAT_MS)
    document.addEventListener('visibilitychange', () => this.beat(document.hidden))
    window.addEventListener('pagehide', () => this.beat(!document.hidden))
  }

  gameStarted(game: GameOnScreen): void {
    this.countedSince = performance.now()
    this.send({ event: 'game', mode: game.mode, server: game.server, seconds: 0 })
  }

  private beat(wasVisible: boolean): void {
    const now = performance.now()
    const seconds = Math.round((now - this.countedSince) / 1000)
    this.countedSince = now
    if (!wasVisible) return
    const game = this.gameOnScreen()
    this.send({ event: 'beat', mode: game?.mode ?? null, server: game?.server ?? null, seconds: game ? seconds : 0 })
  }

  private send(beacon: Omit<VisitBeacon, 'id'>): void {
    if (import.meta.env.DEV) return
    navigator.sendBeacon(visitUrl({ id: this.id, ...beacon }))
  }
}
