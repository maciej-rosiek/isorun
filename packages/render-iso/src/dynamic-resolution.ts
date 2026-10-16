/** The share of the quality's pixel ratio the view may go down to. */
const LOWEST = 0.7
const STEP = 0.1
/** A frame later than this many of the screen's frame times is late. */
const LATE = 1.4
/** Lower the resolution when more frames than this were late in a second; raise it again only after seconds with fewer than `CALM`. */
const TOO_MANY_LATE = 0.08
const CALM = 0.02
const CALM_SECONDS = 4
/** A step up that brings late frames back within this many seconds was too much: the next try waits `BACKOFF_SECONDS` longer. */
const TRIAL_SECONDS = 2
const BACKOFF_SECONDS = 20
/** A frame further apart than this, in ms, is a pause (a hidden tab, a load), not a frame. */
const PAUSE_MS = 100
const INTERVALS_KEPT = 240

/**
 * Lowers the share of the pixels the view draws while the frames come later than the screen asks for them (the GPU cannot
 * keep up), and raises it again once they have been on time for a while, so the frame rate holds on a level that fills
 * the screen. It goes by the time between frames: the screen's frame time is the shortest the frames come at.
 */
export class DynamicResolution {
  /** The share of the quality's pixel ratio to draw at, `LOWEST` to 1. */
  scale = 1
  private readonly intervals: number[] = []
  /** The screen's frame time, found again each second. */
  private screen = Infinity
  private second = { frames: 0, late: 0, ms: 0 }
  private calmSeconds = 0
  private trial = 0
  private wait = 0

  /** A frame came `ms` after the one before; returns whether the scale changed. */
  frame(ms: number): boolean {
    if (ms <= 0 || ms > PAUSE_MS) return false
    this.intervals.push(ms)
    if (this.intervals.length > INTERVALS_KEPT) this.intervals.shift()
    this.second.frames++
    this.second.ms += ms
    if (ms > this.screen * LATE) this.second.late++
    if (this.second.ms < 1000) return false
    this.screen = this.screenFrame()
    const late = this.second.late / this.second.frames
    this.second = { frames: 0, late: 0, ms: 0 }
    this.wait = Math.max(0, this.wait - 1)
    this.trial = Math.max(0, this.trial - 1)
    if (late > TOO_MANY_LATE) {
      this.calmSeconds = 0
      if (this.trial > 0) this.wait = BACKOFF_SECONDS
      this.trial = 0
      return this.set(this.scale - STEP)
    }
    this.calmSeconds = late < CALM ? this.calmSeconds + 1 : 0
    if (this.calmSeconds < CALM_SECONDS || this.wait > 0 || this.scale >= 1) return false
    this.calmSeconds = 0
    this.trial = TRIAL_SECONDS
    return this.set(this.scale + STEP)
  }

  /** Start again at the full resolution: the quality or the view's size changed. */
  reset() {
    this.scale = 1
    this.intervals.length = 0
    this.screen = Infinity
    this.second = { frames: 0, late: 0, ms: 0 }
    this.calmSeconds = 0
    this.trial = 0
    this.wait = 0
  }

  /** The screen's frame time: the tenth shortest of the recent frames, so one early frame does not count. */
  private screenFrame(): number {
    const sorted = [...this.intervals].sort((a, b) => a - b)
    return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length / 10))]
  }

  private set(scale: number): boolean {
    const next = Math.round(Math.min(1, Math.max(LOWEST, scale)) * 10) / 10
    if (next === this.scale) return false
    this.scale = next
    this.second = { frames: 0, late: 0, ms: 0 }
    return true
  }
}
