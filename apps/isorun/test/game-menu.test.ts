// @vitest-environment happy-dom
import { describe, it, expect, beforeEach } from 'vitest'
import { initialState, reduce, MouseMode, MenuFlag, type ClientMessage, type GameState } from '@isorun/webtiles'
import { Overlays } from '../src/overlays'
import { deriveContext, type Context } from '../src/context'

/**
 * Crawl's game menu (main.cc `GameMenu`, F1) as it is drawn: the server's rows
 * as they come, and the cursor back on the row it was left on.
 */

const scene = { player: { x: 0, y: 0 }, cells: new Map(), billboards: [], playerOnLevel: false } as never
const cam = { facing: 0 } as never

function setup() {
  const sent: ClientMessage[] = []
  const host = document.createElement('div')
  document.body.append(host)
  const panel = document.createElement('div')
  panel.className = 'settings-panel'
  const ov = new Overlays(host, {
    send: (m) => sent.push(m),
    gamedata: () => null,
    watching: () => false,
    onClientOverlayChange: () => {},
    onSystemAction: () => {},
    settingsPanel: () => ({ el: panel, rows: [] }),
  })
  const st = initialState()
  st.phase = 'playing' as GameState['phase']
  st.inputMode = MouseMode.COMMAND
  const frame = (): Context => {
    ov.update(st)
    const ctx = deriveContext(st, scene, cam, 'micro')
    ov.updatePrompt(ctx.mode, ctx.prompt)
    ov.syncFocus(ctx)
    return ctx
  }
  const rows = () => Array.from(host.querySelectorAll('li')).map((li) => li.textContent?.trim() ?? '')
  const hovered = () => {
    const li = host.querySelector('li.hovered')
    return li ? li.textContent?.trim() ?? '' : null
  }
  return { ov, st, sent, host, frame, rows, hovered }
}

/**
 * The game menu as crawl sends it (main.cc `GameMenu::fill_entries`, with
 * `MenuEntry::get_text`'s " <key> - " preface): a blank subtitle, the options,
 * a second blank subtitle, then Quit. MF_ARROWS_SELECT, MF_WRAP and
 * MF_INIT_HOVER, its hover seated on the first row.
 */
function openGameMenu(st: GameState, hovered = 1) {
  const row = (key: string, code: number, label: string) => ({ text: ` ${key} - ${label}`, hotkeys: [code], level: 2 })
  reduce(st, {
    msg: 'menu',
    tag: 'game_menu',
    flags: MenuFlag.SINGLESELECT | MenuFlag.ALLOW_FORMATTING | MenuFlag.ARROWS_SELECT | MenuFlag.WRAP | MenuFlag.INIT_HOVER,
    last_hovered: hovered,
    title: { text: '<w>Dungeon Crawl Stone Soup 0.35-a0' },
    more: '',
    alt_more: '',
    items: [
      { text: '', level: 1 },
      row('Esc', 27, 'Return to game'),
      row('S', 83, 'Save and exit'),
      row('#', 35, 'Generate and view character dump'),
      row('~', 126, 'Edit macros'),
      row('?', 63, 'Help and manual'),
      row('/', 47, 'Lookup info'),
      { text: '', level: 1 },
      row('Q', 81, 'Quit and <lightred>abandon character</lightred>'),
    ],
  } as never)
}

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('the game menu', () => {
  it('draws the server rows as they come', () => {
    // given
    const { st, frame, rows } = setup()
    openGameMenu(st)

    // when
    frame()

    // then
    expect(rows()).toEqual([
      '',
      'Esc - Return to game',
      'S - Save and exit',
      '# - Generate and view character dump',
      '~ - Edit macros',
      '? - Help and manual',
      '/ - Lookup info',
      '',
      'Q - Quit and abandon character',
    ])
  })

  it('opens again on the row it was left on', () => {
    // given
    const { ov, st, frame, hovered } = setup()
    openGameMenu(st)
    frame()
    for (let i = 0; i < 2; i++) ov.menuOp(st, 'next')
    reduce(st, { msg: 'close_menu' } as never)
    frame()

    // when
    openGameMenu(st)
    frame()

    // then
    expect(hovered()).toBe('# - Generate and view character dump')
  })
})
