import { touchScreen } from './touch'

type BackEntry = 'game' | 'back'

function backEntryOf(state: unknown): BackEntry | null {
  if (typeof state !== 'object' || state === null || !('isorun' in state)) return null
  return state.isorun === 'game' || state.isorun === 'back' ? state.isorun : null
}

function entry(isorun: BackEntry) {
  return { isorun }
}

export type BackOutcome = 'taken' | 'stale' | null

export class BackButton {
  private armed = false

  arm() {
    if (this.armed || !touchScreen()) return
    this.armed = true
    if (backEntryOf(history.state) === 'back') return
    history.replaceState(entry('game'), '')
    history.pushState(entry('back'), '')
  }

  disarm() {
    this.armed = false
  }

  take(state: unknown, onBack: () => void): BackOutcome {
    if (backEntryOf(state) !== 'game') return null
    if (!this.armed) return 'stale'
    onBack()
    history.pushState(entry('back'), '')
    return 'taken'
  }
}
