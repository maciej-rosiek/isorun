import { actionPanelItems, cm, mapKey, MouseMode, MSGCH, UiState, formattedStringToText, keyMessage, topMenu, topPopup, type ClientMessage, type GameMessage, type GameState, type MenuState } from '@isorun/webtiles'
import { lightsFor, lightsPanel } from './lights-panel'
import { DIR8_DX, DIR8_DY, cellKey, dirFromDelta, getCell, type CellKey, type Dir8, type MapRenderer, type Scene, type SceneCursor } from '@isorun/scene'
import { audio } from './audio'
import { GameAudio } from './audio/director'
import { combatEvents, linesSince, monsterCell, type Fighter } from './combat'
import { DamageNumbers } from './damage-numbers'
import { cellDetails } from './cell-info'
import { DEFAULT_ANGLES, IsoAssets, applySurfaceDetail, IsoCamera, IsoRender3d, LIGHTS, STEP_SECONDS, TURN_DEGREES, stickToDir, type AimIntent } from '@isorun/render-iso'
import { qualityFor } from './graphics'
import { Render2d } from '@isorun/render-2d'
import { escapeHtml, h, onDprChange } from './dom'
import type { Session } from './session'
import { deriveContext, deriveMode, type Context } from './context'
import { HOLD_MS, LEVEL_MAP, STOP_EXPLORE, barLabels, buttonAction, contextualLabel, armsTapOrHold, holdAction, resolve, screenKey, type Action, type BindingLabel, type CommandCategory, type RelDir } from './bindings'
import type { CommandMenu } from './command-menu'
import { Runner, type LastStep } from './runner'
import { Hud, rcFont } from './hud'
import { GridHost } from './grid/host'
import { gameSplit, levelMapSplit } from './grid/console'
import { Chat } from './chat'
import { Overlays } from './overlays'
import { directionKey, isTextEntry, keydownMessage } from './keys'
import { isPadActivity, type Button, type GamepadInput, type PadEvent } from './gamepad'
import { gamepadHints, type PadHintEvidence } from './gamepad-hints'
import { isPack, openPackKeys } from './pack-tabs'
import { OKAY_THEN, SHOUT_KEY, SWAP_WEAPONS_KEY, actionNeighbour, actionTab, actionTabOf, type ActionTabId } from './action-tabs'
import { getSavedView, saveSettings, saveView, type Settings } from './servers'
import { type SettingGroup } from './settings-rows'
import { PerfOverlay, type LogContext } from './perf'
import { MapHold } from './map-hold'
import { Dock, type DockButtonId } from './dock'
import { dockActions } from './dock-actions'
import { abilitiesFromMenu, itemTiles, quickSlots, slotChoices, spellsFromMenu, type QuickSlot, type SlotRef, type SlotSources } from './quick-slots'
import { emptyDockMemory, loadDockMemory, saveDockMemory, type DockMemory } from './dock-memory'
import { equipSlotOf, itemActionKeys, itemTitle, packItems, wornFromMenu, wornNow, type DollPlace, type ItemActionId, type PackItem } from './gear'
import { characterScreen, inventoryScreen, weaponLine } from './screens'
import type { CommandEntry } from './command-menu'
import { compactWindow, landscapeWindow, touchScreen } from './touch'
import { TouchBar } from './touch-bar'
import { canFullscreen, toggleFullscreen } from './fullscreen'

/** at rest the frame loop sleeps, waking to run its body this often, so nothing that changed without waking it waits longer */
const IDLE_TICK_MS = 250
/** how long after the last thing that wanted a frame the loop keeps the display's rate before it parks (`awake`) */
const ACTIVE_MS = 500

/** css pixels a touch must travel before a press becomes a drag of the view */
const DRAG_SLOP = 6
const LONG_PRESS_MS = 450
/** how long the cursor stays on a tapped cell after the finger lifts */
const TAP_CURSOR_MS = 500
const RIGHT_BUTTON = 2
const TOUCH_CUT_AWAY_CELLS = 6
const COMPACT_MESSAGE_ROWS = { portrait: 4, landscape: 2 }
const MAX_CENTRE_LIFT = 0.15
const FULLSCREEN_ICON = '<svg viewBox="0 0 24 24"><path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"/></svg>'
const SETTINGS_ICON = '<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1 7 17M17 7l2.1-2.1"/></svg>'
/** How much the view zooms per css pixel of wheel scroll, and per second at full right stick. */
const ZOOM_PER_WHEEL_PIXEL = 0.0015
const ZOOM_PER_STICK_SECOND = 1.5
/** How much the view tilts for one press of Alt with Up or Down, in degrees. */
const TILT_PER_KEY = 5
/** level-map cells per unit of right-stick look while the map is open */
const MAP_PAN_RATE = 0.12
/** game.js `show_diameter`: the cells across a full field of view, which the view is fitted to */
const SHOW_DIAMETER = 17

/** The device the player touched last; the prompt strip is drawn for it. */
export type InputDevice = 'pad' | 'keyboard' | 'pointer'

export interface GameHooks {
  settings(): Settings
  /** one group's settings page, with what its Back does and what opens the Gamepad controls sheet (settings-panel.ts) */
  settingsPanel(group: SettingGroup, back: () => void, controls: () => void): { el: HTMLElement; rows: HTMLElement[] }
  onSystem(op: 'disconnect'): void
  gamepad: GamepadInput
  initialInput?: InputDevice
  /** the map view to draw with, in place of the isometric one (the e2e harness has no WebGL) */
  renderer?(): MapRenderer
}

/**
 * The game screen: canvas + renderer, HUD, overlays, input wiring.
 */
export class GameScreen {
  root: HTMLElement
  private canvas = h('canvas', { class: 'view' })
  /** The dungeon view: the isometric renderer, or the hook's stand-in. */
  private dungeon: MapRenderer
  /** The dungeon view when it is the isometric renderer. */
  private iso: IsoRender3d | null = null
  /** The level map (X): crawl's own 2D view, on a canvas of its own over the dungeon view while it is open. */
  private levelMap: { canvas: HTMLCanvasElement; renderer: Render2d } | null = null
  /** The view on screen. */
  private get renderer(): MapRenderer {
    return this.levelMap?.renderer ?? this.dungeon
  }
  /** the one console grid the screen is set on: the view, the sidebar and the messages are rectangles of its cells */
  private grid: GridHost
  /** what the last layout was made from; the split is redone only when it changes */
  private layoutKey = ''
  private lastMapView = false
  /** The level map is on screen: crawl's own, or held across a key that left it (map-hold.ts). Set once a frame, before the layout reads it. */
  private mapShown = false
  private mapHold = new MapHold()
  private session: Session
  private view = (() => {
    const saved = getSavedView()
    return new IsoCamera(saved?.angles, saved?.zoom)
  })()
  /** The angles and zoom last written down, so a view that comes to rest is saved once. */
  private savedView = ''
  private runner: Runner
  private hud: Hud
  private chat: Chat
  private chatPx = 0
  private chatResize = new ResizeObserver(() => {
    this.chatPx = this.chat.root.offsetHeight
    this.relayout()
  })
  private overlays: Overlays
  private hooks: GameHooks
  private ctx: Context
  /** frame timing readout (perf.ts), on with `?perf` */
  private perf: PerfOverlay
  private unsub: (() => void)[] = []
  private _needsRender = true
  /**
   * Something for the frame loop to look at: a server message, a key, a
   * pointer, a pad event, a resize, a settings change. Cleared as a frame's
   * body starts, so anything set during it earns another frame. With it
   * clear and nothing moving, the loop's callback returns at once (`awake`).
   */
  private dirty = true
  /** the pointer moved over the view since the last pick, so the hovered cell may have changed */
  private pickDirty = false
  /** when the frame body last ran, for the idle safety tick (IDLE_TICK_MS) */
  private lastBody = 0
  /** when something last wanted a frame, for the run-on before the loop parks (ACTIVE_MS) */
  private lastActive = 0
  /** the settings as last read; cleared by `applySettings`, so a frame reads storage at most once */
  private settingsCache: Settings | null = null
  private lookPanel: HTMLElement | null = null
  /** the context as last derived, and the key of what it was derived from */
  private ctxBase: Context | null = null
  private ctxKey = ''
  /** the revisions a frame was last drawn for: only a change to what is drawn (the map, the player, the UI stack) asks for a frame */
  private drawnRev = { map: -1, player: -1, ui: -1 }
  /** A game on this device has no one watching and no one to talk to: no chat box, no Chat row, F12 does nothing. */
  private get chatOn(): boolean {
    return !this.session.server.offline
  }
  /** A frame is needed; setting it also wakes the loop. */
  private get needsRender(): boolean {
    return this._needsRender
  }
  private set needsRender(v: boolean) {
    this._needsRender = v
    if (v) this.wake()
  }
  /** Something happened: the next frame runs its body, and a sleeping loop is started again at once. */
  private wake() {
    this.dirty = true
    if (!this.raf && !this.destroyed) {
      clearTimeout(this.idleTimer)
      this.idleTimer = 0
      // the clock starts again with the loop: an easing's first frame must not be handed the whole sleep as its dt
      this.lastFrame = performance.now()
      this.perf?.profile.resume()
      this.raf = requestAnimationFrame(this.loop)
    }
  }
  /** Scene callbacks flushed inside a frame share its timestamp with both animations. */
  private sceneTime: number | undefined
  private lastFrame = performance.now()
  private raf = 0
  /** the safety tick while the loop sleeps (IDLE_TICK_MS) */
  private idleTimer = 0
  private destroyed = false
  private lastPos = { x: NaN, y: NaN }
  /**
   * The player's position as of the last server message, and every hop it
   * made since the scene was last rebuilt. The scene is rebuilt once per
   * frame, so a fast explore or travel (a busy frame, or `travel_delay -1`)
   * arrives as several cells at once: the hops keep the path the feet took,
   * and the camera can face the way the last step went instead of mistaking
   * the batch for a teleport. `jumped` marks a hop that was not one cell.
   */
  private hopPos = { x: NaN, y: NaN }
  private hops: { dx: number; dy: number }[] = []
  private jumped = false
  /** place and depth the player was last seen on, so a level change is known even when the coordinates barely move */
  private lastLevel = ''
  /** LT held: this autoexplore runs with no delay and no hops to watch, whatever the Travel setting (`paceTravel`). */
  private exploringAtOnce = false
  /** LT held on the LT explore setting's Hold: the explore it started stops when LT is let go. */
  private exploringWhileHeld = false
  /** The `travel_delay` last sent to the game (`paceTravel`), null before the first. */
  private travelDelaySent: number | null = null
  /** The player's own blow, swung on the key before the server answered (`swing`). */
  private swung: { x: number; y: number; t: number } | null = null
  /** the newest message line the fight was read up to (`trackCombat`) */
  private lastCombatLine: GameMessage | undefined
  /** The sounds of this game (audio/director.ts). */
  private sound: GameAudio
  private damageNumbers: DamageNumbers
  private lastMode = ''
  private loading = h('div', { class: 'loading' })
  private pressTimes = new Map<string, number>()
  /** A tap-or-hold button down and short of HOLD_MS: its corner prompt fills its hold ring (hud.ts showHold). */
  private holding: { button: Button; fraction: number } | null = null
  /**
   * The pad button that opened the client overlay on screen. Every menu a
   * button opens is a toggle: the same button puts it away, from whatever
   * screen it was carried into (a nested settings page closes with the whole
   * menu, as `close` already does for Select). Null when the overlay was
   * opened by mouse or keyboard, which have Esc.
   */
  private overlayOpener: Button | null = null
  private lastCursor: SceneCursor | null = null
  private lastOptKey = ''
  private drag: { id: number; x: number; y: number; x0: number; y0: number; moved: boolean; button: number } | null = null
  private touchBar = new TouchBar((action) => {
    this.inputActed()
    this.runner.execute(action)
    this.needsRender = true
  })
  private fullscreenButton = h('button', { type: 'button', class: 'touch-fullscreen', 'aria-label': 'Full screen', html: FULLSCREEN_ICON, onclick: () => toggleFullscreen() })
  private touchCorner = h(
    'div',
    { class: 'touch-corner' },
    this.fullscreenButton,
    h('button', { type: 'button', class: 'touch-settings', 'aria-label': 'Settings', html: SETTINGS_ICON, onclick: () => this.openSystemMenu() }),
  )
  private touches = new Map<number, { x: number; y: number }>()
  private pinchSpan: number | null = null
  private longPressTimer = 0
  /** Lets go of a drag with no click behind it: the window lost focus, or the button was released out of sight. */
  private cancelDrag: () => void = () => {}
  /** One per screen, however many canvases it goes through: a renderer swap must not leave the old canvas's listener behind. */
  private onWindowBlur?: () => void
  /** where the pointer is aiming, in canvas css pixels: the hovering mouse, or the finger on a touch screen */
  private hover: { x: number; y: number } | null = null
  /** `hover` is a finger's: it goes when the finger lifts or drags (`dropTouchHover`) */
  private hoverByTouch = false
  private touchHoverTimer = 0
  /** dungeon_renderer.js tooltip_timeout: the cell tooltip waits half a second under a still pointer */
  private tooltipTimer = 0
  /** the pointer moved since the last target sent: keyboard cursor moves and turns never fight the mouse */
  private hoverMoved = false
  private lastTargetSent = -1
  /** the cursor shown is the pointer's own selection, not one the server placed: it stays off the minimap */
  private cursorIsOurs = false
  /**
   * the mouse has moved since the last keyboard or pad action: the pointer's
   * own selection shows only then. A key or button hides it until the mouse
   * moves again, so a player on the keys is not shown a cell the reticle
   * happens to rest on after a turn or a step.
   */
  private pointerLive = false
  /** the last pick; a frame that draws nothing new (no pointer, camera or level change) casts no ray */
  private pickMemo: CellKey | null | undefined
  /** the canvas's place on the screen, from the last relayout, in css px */
  private viewPx: { left: number; top: number; width: number; height: number } | null = null
  /**
   * What the player touched last: the prompt strip is drawn for it (pad
   * glyphs, key caps, nothing for the mouse). Merely plugging in a pad is
   * not input; the front end passes along the device used to launch play.
   */
  private lastInput: InputDevice = 'keyboard'
  private padHints = gamepadHints()
  /** the pack's page last up (pack-tabs.ts), which Y opens it on again */
  private packPage: string | null = null
  /** the item the cursor was last on in each of the pack's pages, by its letter: Y puts the cursor back on it */
  private packRow = new Map<string, number>()
  /** Y opened the pack: the page it is turning to, and whether the pack has been up since */
  private packReturn: { page: string; seen: boolean } | null = null
  /** X's actions as tabs (action-tabs.ts): the one up last, which X opens again */
  private actionTabUp: ActionTabId = 'spells'
  /**
   * A turn to one of X's tabs under way (`openActionTab`): the menu left
   * being put away (`closing`), a tab's key sent and its menu or crawl's
   * refusal awaited (`sent`), or, for Spells, `z`'s question answered with
   * `*` (`listing`). `target` is the tab the player is turning to, `sent`
   * the one whose key is out: presses quicker than crawl move the target
   * only, and the turn goes on from whatever lands. `last` is the log's last
   * line when the key went: crawl's answer is a line after it.
   */
  private actionTurn: { target: ActionTabId; sent: ActionTabId | null; stage: 'closing' | 'sent' | 'listing'; last?: GameMessage; t: number } | null = null
  private padStep: LastStep | null = null
  private padLooking = false
  /** the right stick's up and down, -1 to 1: the view zooms out while it is pushed up, in while down */
  private padZoom = 0
  /** right-stick travel accumulated toward the next level-map cursor step */
  private mapPan = { x: 0, y: 0 }
  /** the dock along the foot of the screen (dock.ts), with the HUD setting on Dock */
  private dock: Dock
  private dockOn = false
  /** the dock's height in css px, measured when it comes up (`relayout`) */
  private dockPx = 0
  /** what the dock was last drawn for, and what the dock knows of the character (dock-memory.ts) */
  private dockKey = ''
  private dockMemory: DockMemory = emptyDockMemory()
  private dockCharacter = ''
  private dockMemoryRev = 0
  /** the game was seen to start new (`loadMemory`), and the message revision last looked at for it */
  private dockFresh = false
  private dockFreshRev = -1
  private dockSlots: (QuickSlot | null)[] = []
  /** the `ui` revision the menus were last read at (`learnFromMenus`) */
  private menusReadRev = -1
  /** the pack opened behind the inventory screen to read what is worn (`probeGear`): waiting for it, then for it to close */
  private gearProbe: { stage: 'open' | 'closing'; t: number } | null = null
  /** a screen of the dock's put away for an action of crawl's, to come back once crawl is done (`trackResume`) */
  private resumeScreen: { id: 'inventory' | 'character'; t: number; left: boolean } | null = null
  /** what the open screen was last built for */
  private screenKey = ''

  constructor(host: HTMLElement, session: Session, hooks: GameHooks) {
    // first: anything below that asks for a frame (`wake`) schedules the loop
    this.loop = this.loop.bind(this)
    this.session = session
    this.hooks = hooks
    this.root = h('div', { class: 'screen game' })
    host.append(this.root)
    this.root.append(this.canvas, this.loading)
    this.damageNumbers = new DamageNumbers(this.root, (x, y) => this.canvasPointOf(x, y), () => this.settings().damageNumbers)
    const st = hooks.settings()
    this.perf = new PerfOverlay(this.root, () => this.perfContext())
    this.unsub.push(this.perf.watch(session))
    this.grid = new GridHost(this.root, this.textPx())
    this.grid.onfit = () => this.relayout(true)
    this.dungeon = this.makeDungeon()
    this.sound = new GameAudio(audio, {
      screenOf: (x, y) => this.iso?.screenOf(x, y) ?? { x: 0, y: 0 },
      width: () => this.viewPx?.width || this.canvas.clientWidth || 1,
      animationSpeed: () => this.settings().animationSpeed,
    })
    if (this.iso) {
      this.iso.onHop = (step) => this.sound.hop(step, this.session.scene)
      this.iso.onDoor = (x, y, open) => this.sound.door({ x, y }, open, this.session.scene)
    }
    window.addEventListener('pagehide', this.saveView)
    this.hud = new Hud(this.root, {
      onSelectMonster: () => {},
      onBarAction: (a) => this.runner.execute(a),
      onMinimapClick: () => this.runner.execute(LEVEL_MAP),
      // action_panel.js: items act only in command mode
      onPanelItem: (slot, describe) => {
        if (this.session.state.inputMode !== MouseMode.COMMAND) return
        this.runner.send(describe ? cm.invItemDescribe(slot) : cm.invItemAction(slot))
      },
      onPanelShow: () => this.setPanelShown(true),
      onPanelHide: () => this.setPanelShown(false),
      // action_panel.js: the game-menu button acts only in command mode
      onGameMenu: () => {
        if (this.session.state.inputMode === MouseMode.COMMAND) this.runner.send(cm.mainMenuAction())
      },
      // WebTiles' own settings menu here is the panel's rc options; Isorun's settings screen is where those live
      onPanelSettings: () => this.overlays.showSettings(),
    })
    this.dock = new Dock({
      onAction: (a) => this.dockCommand(() => this.runner.execute(a)),
      onSlot: (i) => this.useSlot(i),
      onSlotAssign: (i) => this.assignSlot(i),
      onButton: (id) => this.dockButton(id),
    })
    this.root.append(this.dock.root, this.touchCorner, this.touchBar.root)
    this.fullscreenButton.hidden = !canFullscreen()
    this.chat = new Chat(this.root, { send: (m) => this.runner.send(m), padKind: () => this.hooks.gamepad.kind })
    this.chatResize.observe(this.chat.root)
    this.overlays = new Overlays(this.root, {
      padKind: () => this.hooks.gamepad.kind,
      send: (m) => this.runner.send(m),
      gamedata: () => this.session.gamedata,
      watching: () => this.session.watching,
      onClientOverlayChange: () => {
        this.tapArmed.clear()
        this.holding = null
        this.needsRender = true
      },
      onSystemAction: (op) => this.systemAction(op),
      settingsPanel: (group, back, controls) => this.hooks.settingsPanel(group, back, controls),
      actionTab: (to) => this.openActionTab(to),
      hideMenu: (m) => isPack(m) && (!!this.gearProbe || this.overlays.screenOpen === 'inventory'),
      pinSlot: (kind, letter) => this.pinToFreeSlot({ kind, letter }),
      back: () => this.back(),
    })
    this.runner = new Runner(session, this.view, {
      context: () => this.ctx,
      ui: (op, arg, category, section) => this.uiOp(op, arg, category, section),
      osk: (op, dir) => this.overlays.oskOp(op, dir),
      menu: (op) => this.overlays.menuOp(this.session.state, op),
      focus: (op) => this.overlays.focusOp(this.session.state, this.ctx, op),
      status: (t) => this.hud.status(t),
      confirmDangerous: () => this.hooks.settings().confirmStairs,
      now: () => performance.now(),
      swing: (target) => this.swing(target),
      keepMap: () => {
        this.mapHold.begin(performance.now())
        // a frame after the wait, so a hold nothing came for is let go even on a still screen
        setTimeout(() => this.wake(), 1600)
      },
    })
    this.ctx = deriveContext(session.state, session.scene, this.runner.heading, 'micro')
    this.lastInput = hooks.initialInput ?? 'keyboard'
    this.padHints.cancel() // no pending server reply survives a screen/run change
    this.unsub.push(
      session.on((e) => {
        if (e.type === 'scene') {
          this.wake()
          this.onScene()
        } else if (e.type === 'state') {
          this.wake()
          this.trackHop()
          this.trackActions(performance.now(), false)
          this.trackCombat()
          this.trackSound()
          this.paceTravel()
          this.learnFromMenus()
          this.trackGearProbe(performance.now())
          // a new game on the same version reuses the loaded gamedata, so no
          // `gamedata` event follows: only wait when the session is fetching
          if (e.msg.msg === 'game_client' || e.msg.msg === 'go_lobby') {
            if (this.session.gamedata && !this.session.loading) this.hideLoading()
            else this.showLoading('Loading game data…')
          }
        } else if (e.type === 'sent') {
          this.sound.sent(e.msg, this.ctx.mode === 'command')
        } else if (e.type === 'gamedata') {
          if (e.status === 'ready') {
            this.hideLoading()
            if (this.session.gamedata) this.setTiles()
            this.needsRender = true
          } else if (e.status === 'error') this.showLoading('Could not load game data: ' + (e.detail || ''))
          else this.showLoading(`Loading game data… ${e.done ?? 0}/${e.total ?? 0}`)
        }
      }),
    )
    if (this.session.gamedata) this.hideLoading()
    else this.showLoading('Loading game data…')
    this.onKeyDown = this.onKeyDown.bind(this)
    window.addEventListener('keydown', this.onKeyDown, true)
    window.addEventListener('keyup', this.onKeyUp, true)
    this.onResize = this.onResize.bind(this)
    window.addEventListener('resize', this.onResize)
    // a move to a screen of another density need not resize the window, and every canvas is drawn for the old one
    this.unsub.push(onDprChange(() => {
      this.wake()
      this.relayout(true)
    }))
    this.onDocPointer = this.onDocPointer.bind(this)
    document.addEventListener('pointerdown', this.onDocPointer, true)
    this.onDocContextMenu = this.onDocContextMenu.bind(this)
    document.addEventListener('contextmenu', this.onDocContextMenu, true)
    this.attachPointer(this.canvas)
    this.canvas.addEventListener('wheel', this.onWheel, { passive: false })
    this.hud.setMinimapTiles(st.minimapTiles, st.minimapCell)
    this.relayout(true)
    this.wake()
  }

  destroy() {
    this.destroyed = true
    this.hooks.gamepad.rightStickTurns = false
    this.saveView()
    window.removeEventListener('pagehide', this.saveView)
    cancelAnimationFrame(this.raf)
    clearTimeout(this.idleTimer)
    window.removeEventListener('keydown', this.onKeyDown, true)
    window.removeEventListener('keyup', this.onKeyUp, true)
    window.removeEventListener('resize', this.onResize)
    this.chatResize.disconnect()
    document.removeEventListener('pointerdown', this.onDocPointer, true)
    if (this.onWindowBlur) window.removeEventListener('blur', this.onWindowBlur)
    document.removeEventListener('contextmenu', this.onDocContextMenu, true)
    for (const u of this.unsub) u()
    // a key sequence waiting on the server's prompt keeps its own session listener and timer past
    // these subscriptions: ended here, so a screen that is gone sends no follow-up key into the game
    this.runner.abortSequence()
    clearTimeout(this.tooltipTimer)
    clearTimeout(this.longPressTimer)
    clearTimeout(this.touchHoverTimer)
    this.sound.dispose()
    this.damageNumbers.dispose()
    this.hud.destroy()
    this.overlays.destroy()
    this.perf.destroy()
    this.grid.destroy()
    this.levelMap?.renderer.destroy()
    this.dungeon.destroy()
    this.root.remove()
  }

  /** The grid's text size: the console cell's font in px, from the UI scale setting. */
  private textPx(): number {
    return Math.round(16 * this.hooks.settings().uiScale)
  }

  /**
   * Set the screen on the grid: the canvas
   * fills the grid and the HUD's panes lie over it on the cells `gameSplit`
   * gives them, sized as game.js `layout` sizes them. While the level map is
   * open the canvas keeps to the clear part of the view and grows over the
   * sidebar and the messages exactly when the rc's `tile_level_map_hide_*`
   * say so (`levelMapSplit`). Cheap to call every frame; only acts on change.
   */
  private relayout(force = false) {
    if (!this.hud) return
    const st = this.session.state
    const o = st.options
    const g = this.grid.grid
    const mapView = this.mapShown
    const hideSidebar = mapView && o.tile_level_map_hide_sidebar === true
    const hideMessages = mapView && o.tile_level_map_hide_messages === true
    // no character yet: the official client's crt layer (game.js `set_ui_state`, `client.set_layer("crt")`) covers
    // the sidebar while the new-game chooser has the UI in CRT state (newgame.cc `choose_game`), until the redraw
    // that sends the first `player` (tileweb.cc `redraw`); a continued game has no sheet before that message either.
    const choosingCharacter = topPopup(st)?.type.startsWith('newgame') ?? false
    const dockOn = this.settings().hud === 'dock' && st.player.received && (st.phase === 'playing' || st.phase === 'watching') && !mapView && !choosingCharacter
    const hideStats = hideSidebar || st.uiState === UiState.CRT || !st.player.received || dockOn || (mapView && compactWindow())
    const chatRows = Math.ceil(this.chatPx / g.ch)
    const key = [g.cols, g.rows, g.cw, g.ch, g.ox, g.oy, st.messages.paneHeight, hideStats, hideSidebar, hideMessages, window.devicePixelRatio, dockOn, choosingCharacter, chatRows].join(',')
    if (!force && key === this.layoutKey) return
    this.layoutKey = key
    this.dockOn = dockOn
    this.root.classList.toggle('hud-dock', this.settings().hud === 'dock')
    this.hud.dockMode = this.settings().hud === 'dock'
    this.dock.setShown(dockOn)
    this.dockKey = ''
    if (dockOn) this.dockPx = this.dock.height
    const cells = gameSplit(g, this.messageRows(st.messages.paneHeight), undefined, dockOn ? Math.ceil(this.dockPx / g.ch) : 0)
    const view = mapView ? levelMapSplit(g, cells, { hideSidebar, hideMessages }) : cells.view
    this.grid.place(this.canvas, view)
    if (this.levelMap) this.grid.place(this.levelMap.canvas, view)
    const px = this.grid.px(view)
    this.viewPx = px
    this.renderer.resize(px.width, px.height, window.devicePixelRatio || 1)
    this.iso?.setCentreLift(compactWindow() && !mapView ? Math.min((px.height - this.grid.px(cells.clear).height) / 2, px.height * MAX_CENTRE_LIFT) : 0)
    // what rides the view's edges keeps clear of the panes
    const free = mapView ? view : cells.clear
    const columnAboveChat = { ...cells.sidebar, h: Math.max(0, cells.sidebar.h - chatRows) }
    const promptCorner = dockOn && cells.sidebar.w > 0 ? columnAboveChat : free
    this.hud.layout(this.grid, cells, free, { stats: hideStats, sidebar: hideSidebar, messages: hideMessages || choosingCharacter }, promptCorner)
    // the chat box (client.html #chat) sits at the foot of the right column, as wide as the sidebar
    const side = this.grid.px(cells.sidebar)
    const chat = this.chat.root.style
    chat.left = side.left + 'px'
    chat.width = side.width + 'px'
    chat.bottom = Math.max(0, this.root.clientHeight - side.top - side.height) + 'px'
    this.needsRender = true
  }

  private messageRows(paneHeight: number): number {
    if (!compactWindow()) return paneHeight
    return Math.min(paneHeight, landscapeWindow() ? COMPACT_MESSAGE_ROWS.landscape : COMPACT_MESSAGE_ROWS.portrait)
  }

  private openSystemMenu() {
    this.inputActed()
    if (this.overlays.hasClientOverlay) this.overlays.closeClientOverlay()
    this.overlays.showSystem({ spectating: this.session.watching, inGame: true, chat: this.chatOn, run: (a) => this.runner.execute(a) })
    this.needsRender = true
  }

  /** Remember the camera's angles and zoom so the next session starts with the same view. */
  private saveView = () => {
    const view = { angles: this.view.angles, zoom: this.view.zoomGoal }
    const key = JSON.stringify(view)
    if (key === this.savedView) return
    this.savedView = key
    saveView(view)
  }

  /** The dungeon view, on the screen's canvas, with what the session has so far. */
  private makeDungeon(): MapRenderer {
    const hooked = this.hooks.renderer?.()
    const r = hooked ?? new IsoRender3d()
    if (r instanceof IsoRender3d) {
      this.iso = r
      const st = this.hooks.settings()
      r.setQuality(qualityFor(st, window.devicePixelRatio || 1))
      applySurfaceDetail(st.surfaceDetail)
      r.setLights(lightsFor(st.look))
      r.setAnimationSpeed(st.animationSpeed)
      r.setCutAwayReach(touchScreen() ? TOUCH_CUT_AWAY_CELLS : 0)
      void IsoAssets.load('/iso/models').then((assets) => {
        if (this.destroyed) return
        r.setAssets(assets)
        this.needsRender = true
      })
    }
    r.mount(this.canvas)
    if (this.session.gamedata) r.setTiles(this.session.gamedata)
    r.setScene(this.session.scene)
    r.setCamera(this.view.camera)
    return r
  }

  /** The session's tiles, to every view that draws them. */
  private setTiles() {
    const tiles = this.session.gamedata
    if (!tiles) return
    this.dungeon.setTiles(tiles)
    this.levelMap?.renderer.setTiles(tiles)
  }

  /** Tune the look: the lights panel over the game (lights-panel.ts), every change shown at once; the Look setting turns Custom. */
  tuneLook() {
    if (this.lookPanel) return
    const look = this.settings().look
    const panel = h('div', { class: 'look-panel' }, h('button', { class: 'close', onclick: () => this.closeLookPanel() }, 'Close'))
    lightsPanel(panel, look === 'custom' ? '' : look, (lights) => {
      this.iso?.setLights(lights)
      if (this.settings().look === 'custom') return
      saveSettings({ ...this.settings(), look: 'custom' })
      this.settingsCache = null
    })
    this.root.append(panel)
    this.lookPanel = panel
  }

  private closeLookPanel() {
    this.lookPanel?.remove()
    this.lookPanel = null
  }

  applySettings() {
    this.settingsCache = null
    const st = this.settings()
    document.documentElement.style.setProperty('--ui-scale', String(st.uiScale))
    this.grid.setTextSize(this.textPx())
    this.iso?.setQuality(qualityFor(st, window.devicePixelRatio || 1))
    applySurfaceDetail(st.surfaceDetail)
    this.iso?.setLights(lightsFor(st.look))
    this.iso?.setAnimationSpeed(st.animationSpeed)
    this.paceTravel()
    // the minimap's size is the layout's to set
    this.hud.setMinimapTiles(st.minimapTiles, st.minimapCell)
    this.relayout(true)
    this.needsRender = true
  }

  /** The settings, read from storage once and kept until `applySettings` says they changed. */
  private settings(): Settings {
    return (this.settingsCache ??= this.hooks.settings())
  }

  /**
   * Crawl's `travel_delay` and `explore_delay` for the Travel setting, sent once a game takes commands and
   * again when the setting or the animation speed changes: paced, a travel waits as long as one hop takes,
   * so every cell shows; fast, the server sends the arrival alone.
   */
  private paceTravel() {
    const st = this.session.state
    if (this.session.watching || st.phase !== 'playing' || st.inputMode !== MouseMode.COMMAND) return
    const settings = this.settings()
    const delay = settings.travel === 'paced' && !this.exploringAtOnce ? Math.round((STEP_SECONDS * 1000) / settings.animationSpeed) : -1
    if (delay === this.travelDelaySent) return
    this.travelDelaySent = delay
    this.runner.send(cm.setOption('travel_delay', delay))
    this.runner.send(cm.setOption('explore_delay', delay))
  }

  /** A melee attack went out: the player strikes at the monster on `target` at once, before the server's answer. */
  private swing(target: { x: number; y: number }) {
    if (this.iso) this.iso.act('player', 'attack', target, () => this.sound.swing())
    else this.sound.swing()
    this.swung = { ...target, t: performance.now() }
    this.needsRender = true
  }

  /**
   * The fight in the messages since the last ones read (combat.ts), played
   * on the dungeon view: blows, hits and deaths. Read as the messages land,
   * before the scene they come with, so a monster that dies is still there
   * to fall. A blow the player's own key already swung is not swung again.
   */
  private trackCombat() {
    const st = this.session.state
    const lines = st.messages.lines
    const fresh = linesSince(lines, this.lastCombatLine, st.player.turn)
    this.lastCombatLine = lines.at(-1)
    const scene = this.session.scene
    this.damageNumbers.health(st.player.hp, st.player.hp_max, scene.player)
    if (!fresh.length) return
    const events = combatEvents(fresh, scene)
    this.sound.messages(fresh)
    this.overlays.levelUp.sync(st.player, st.messages.more)
    this.overlays.levelUp.messages(fresh)
    const iso = this.iso
    for (const e of events) {
      const sound = this.sound.combat(e, scene)
      if (!iso) sound()
      else if (e.kind !== 'attack') {
        const number = this.damageNumbers.landing(e, e.on === 'player' ? scene.player : monsterCell(scene, e.on))
        iso.act(e.on, e.kind, e.from, () => {
          sound()
          number()
        })
      }
      else if (!this.alreadySwung(e.by, e.at)) iso.act(e.by, 'attack', e.at, sound)
    }
    this.needsRender = true
  }

  /** Where a cell's middle at head height is on the game screen, in CSS pixels, while the dungeon view is up. */
  private canvasPointOf(x: number, y: number): { x: number; y: number } | null {
    const iso = this.iso
    const view = this.viewPx
    if (!iso || this.renderer !== iso || !view) return null
    const at = iso.screenOf(x, y)
    return { x: view.left + at.x, y: view.top + at.y }
  }

  /** Whether the player's own key already swung this blow at `at`. */
  private alreadySwung(by: Fighter, at: { x: number; y: number }): boolean {
    const swung = this.swung
    return by === 'player' && swung !== null && swung.x === at.x && swung.y === at.y && performance.now() - swung.t < OWN_SWING_MS
  }

  /** The player's health and the --more-- prompt, and the menus coming and going, as sounds. */
  private trackSound() {
    const st = this.session.state
    this.sound.status(st.player.hp, st.player.hp_max, st.messages.more, this.session.watching)
    const mode = deriveMode(st)
    if (mode === 'menu' && this.lastMode === 'command') audio.play('ui-open')
    else if (mode === 'command' && this.lastMode === 'menu') audio.play('ui-close')
    this.lastMode = mode
  }

  private showLoading(text: string) {
    this.loading.textContent = text
    this.loading.style.display = ''
  }
  private hideLoading() {
    this.loading.style.display = 'none'
  }

  private onResize() {
    this.wake()
    // the grid host watches the root too; a window resize is the sure signal on browsers without a ResizeObserver
    this.grid.fit()
  }

  // ------------------------------------------------------------- frame loop

  private loop(now: number) {
    this.raf = requestAnimationFrame(this.loop)
    const dt = Math.min(0.1, (now - this.lastFrame) / 1000)
    this.lastFrame = now
    this.sceneTime = now / 1000
    const perf = this.perf?.enabled ? this.perf.profile : null
    perf?.begin(now)
    try {
      if (this.session.flushScene()) {
        this.needsRender = true
        perf?.tag('scene')
      }
    } finally {
      this.sceneTime = undefined
    }
    perf?.mark('scene')
    if (this.advancePresentation(dt)) this.needsRender = true
    perf?.mark('present')
    if (!this.awake(now)) {
      // nothing to do and nothing moving: no frames until something happens (`wake`) or the safety tick
      cancelAnimationFrame(this.raf)
      this.raf = 0
      this.idleTimer = window.setTimeout(this.idleTick, IDLE_TICK_MS)
      perf?.resume()
      return
    }
    this.dirty = false
    this.lastBody = now
    // what is drawn changed: the map, the player, the UI stack (a menu's cursor, the level map opening)
    const rev = this.session.state.rev
    if (rev.map !== this.drawnRev.map || rev.player !== this.drawnRev.player || rev.ui !== this.drawnRev.ui) {
      this.drawnRev = { map: rev.map, player: rev.player, ui: rev.ui }
      this.needsRender = true
    }
    if (this.padLooking && this.lastInput === 'pad' && this.ctx.mode === 'command' && !this.overlays.hasClientOverlay && !this.session.watching) {
      this.view.zoomBy(Math.exp(-this.padZoom * ZOOM_PER_STICK_SECOND * dt))
      this.padHints.lookedBy(Math.abs(this.padZoom) * dt)
    }
    // the isometric view moves all the time: torches flicker, the fog drifts, the actors breathe
    if (this.iso && !this.levelMap) this.needsRender = true
    const st = this.session.state
    // overlays first: the popup's parsed actions and the focus cursor feed the context and the action bar
    // the device first: the overlays decide what to bring up with a server prompt by who spoke last
    this.overlays.setDevice(this.lastInput)
    this.overlays.update(st)
    // The gamedata fetch runs while the game is already talking: the launcher's
    // save-transfer question (a `show_dialog`) comes up with the veil still
    // over the screen. A veil that swallowed the click would strand the player,
    // so while anything of the server's is up it stands behind the overlays.
    this.loading.classList.toggle('behind', !!(st.dialog || st.menus.length || st.ui.length || st.textInput))
    this.ctx = this.deriveContext(st)
    if (this.aimDir !== null && this.ctx.mode !== 'command') this.clearAim()
    this.hooks.gamepad.rightStickTurns = this.ctx.mode === 'command' && !this.overlays.hasClientOverlay && !this.chat.capturing
    // the server reports targeting alone; the runner knows whether its `x` opened it
    if (this.runner.examining(this.ctx.mode)) this.ctx.examining = true
    if (this.runner.exploring(st.inputMode)) this.ctx.exploring = true
    if (this.settings().exploreTrigger === 'hold') this.ctx.exploreOnHold = true
    this.fireHolds(now)
    if (this.exploringAtOnce && !this.runner.exploreOpen) {
      this.exploringAtOnce = false
      this.paceTravel()
    }
    if (this.ctx.mode === 'popup') this.ctx.popupActions = this.overlays.popupActions()
    this.overlays.levelUp.sync(st.player, st.messages.more)
    this.overlays.updatePrompt(this.ctx.mode, this.ctx.prompt, this.lastInput)
    this.overlays.syncFocus(this.ctx)
    const fi = this.overlays.focusInfo(this.ctx)
    if (fi) this.ctx.focus = fi
    this.ctx.pageable = this.overlays.pageable(this.ctx)
    this.trackPack()
    this.trackActions(now, true)
    if (this.padHints.waiting) this.padHints.observe(this.padHintEvidence(), now)
    const cursor = this.cursorFor()
    const lc = this.lastCursor
    if ((cursor?.x !== lc?.x || cursor?.y !== lc?.y || cursor?.mode !== lc?.mode || cursor?.tile !== lc?.tile) && !(cursor === null && lc === null)) {
      this.lastCursor = cursor
      this.renderer.setCursor(cursor)
      // the pointer's own hover cell stays out of the minimap; only a cursor the server placed shows there
      this.hud.setCursor(this.cursorIsOurs ? null : cursor)
      this.needsRender = true
    }
    this.updateInspect()
    this.mapShown = this.mapHold.shown(st.uiState === UiState.VIEW_MAP, this.ctx.mode, now)
    this.relayout()
    this.syncMapRenderer()
    this.applyServerOptions()
    const settings = this.settings()
    // one of our own panels has its prompts from the panel (Overlays.padPrompts), set under it as crawl's menus' are
    const ours = this.dock.focused ? this.dockPrompts() : this.overlays.padPrompts
    const padLabels = this.chat.capturing ? [] : ours ?? this.padHints.prompts(this.ctx, settings.hints)
    // A held tap-or-hold button (B: Wait, hold for Rest) shows its prompt while it is down, even when the
    // situation did not put it in the corner, so the hold's progress has a place to show. It joins the
    // contextual ones, under the lessons.
    const held = this.holding
    if (held && !padLabels.some((l) => l.button === held.button)) {
      const l = barLabels(this.ctx).find((l) => l.button === held.button)
      if (l) {
        const at = padLabels.findIndex((l) => l.teaching)
        padLabels.splice(at < 0 ? padLabels.length : at, 0, l)
      }
    }
    // the minimap turns with the view, so up on the map is up on the screen
    this.hud.minimapUp = this.view.camera.yaw
    this.hud.minimapUpright = this.view.camera.yaw
    this.hud.update(st, this.session.scene, this.view.camera, this.ctx, this.hooks.gamepad.kind, this.session.gamedata, this.session.watching, this.lastInput, settings.hints !== 'off', padLabels, held, !this.overlays.hasClientOverlay && !this.chat.capturing && !this.dockOn, !!ours)
    this.updateDock(st)
    this.touchBar.update(this.ctx, touchScreen() && !this.session.watching && !this.overlays.hasClientOverlay && !this.chat.capturing && !this.overlays.levelUp.choosing)
    this.trackGearProbe(now)
    this.trackResume(now)
    this.refreshScreen(st)
    this.chat.update(st, this.chatOn && (st.phase === 'playing' || st.phase === 'watching'), !!st.lobby.username)
    this.syncTarget()
    perf?.mark('ui')
    if (this.needsRender) {
      this.needsRender = false
      // dungeon_renderer.js set_view_center: the level map sits on the server's view
      // centre (`vgrdc`), the map cursor while it is open, so the map scrolls under the cursor as WebTiles' does
      this.levelMap?.renderer.setOptions({ center: st.map.viewCenter, facing: null })
      this.renderer.setScene(this.session.scene, now / 1000)
      this.iso?.setView(this.view.view)
      const p = st.player
      this.iso?.setDanger(p.hp_max > 0 && !this.session.watching ? (DANGER_HP - p.hp / p.hp_max) / DANGER_HP : 0)
      this.renderer.setCamera(this.view.camera)
      this.renderer.render(now / 1000)
      perf?.mark('render')
      // a frame that drew: the key whose answer it put on the screen stops waiting (perf.ts `drew`)
      if (perf) this.perf.net.drew()
    }
    perf?.end()
  }

  /**
   * What was on the screen when a perf sample was taken (perf.ts
   * `LogContext`). A timing says what was slow; this says what it was slow
   * on, and the renderer's own counters say how much of it was work the
   * frame chose to do — a `level` of 28 ms with the chunk counter stepping
   * once is the chunk rebuild working, and the same 28 ms with it stepping
   * forty times is not.
   */
  private perfContext(): LogContext {
    // the log's header is written while this screen is still being built, before the first frame
    // derived either of them: nothing here may assume the loop has run
    const ctx = this.ctx as Context | undefined
    return {
      mode: ctx?.mode,
      place: this.session.state.player.place ? `${this.session.state.player.place}:${this.session.state.player.depth}` : undefined,
      cells: this.session.scene.cells.size,
      crowd: this.session.scene.billboards.length,
      loading: this.session.loading,
    }
  }

  /** The camera moves on toward the player; true while it still moves. */
  private advancePresentation(dt: number): boolean {
    const p = this.iso?.playerShown() ?? this.session.scene.player
    const moving = this.view.update(dt, p.x, p.y)
    // saved as soon as a turn or zoom ends, as an app that is killed never sees pagehide
    if (!moving) this.saveView()
    return moving
  }

  /**
   * The context of the frame (context.ts `deriveContext`): scans of the
   * scene's billboards and the message log, so it is derived once per change
   * of what it reads (every message bumps `rev.any`; a scene rebuild bumps
   * `scene.revision`; a turn changes the facing) and copied for the fields
   * the frame fills in afterwards (`examining`, `popupActions`, `focus`).
   */
  private deriveContext(st: GameState): Context {
    const key = `${st.rev.any}|${this.session.scene.revision}|${this.runner.heading}`
    if (!this.ctxBase || key !== this.ctxKey) {
      this.ctxKey = key
      this.ctxBase = deriveContext(st, this.session.scene, this.runner.heading, 'micro')
    }
    return { ...this.ctxBase }
  }

  /** The safety tick: a frame after IDLE_TICK_MS asleep, so anything that changed without waking the loop is seen. */
  private idleTick = () => {
    this.idleTimer = 0
    if (!this.raf && !this.destroyed) {
      this.lastFrame = performance.now()
      this.perf?.profile.resume()
      this.raf = requestAnimationFrame(this.loop)
    }
  }

  /**
   * Whether this frame has anything to do. At rest (no message, no input,
   * nothing easing or animating, no hold under way) the loop's body is
   * skipped whole, so an idle game costs the CPU nothing but the callback.
   * A safety tick every IDLE_TICK_MS still runs the body, so anything that
   * changes state without waking the loop (a runner's timeout, a tooltip)
   * is seen within a quarter second.
   *
   * Play is not one event but a stream of them: a step, the messages it
   * brings, the turn that follows. Parking between them would hand the
   * frames back to whatever woke the loop next, so they would arrive when a
   * packet did rather than when the display asked, which reads as judder
   * however many of them there are. So the loop holds the display's rate for
   * ACTIVE_MS past the last thing that wanted a frame, and only then parks.
   */
  private awake(now: number): boolean {
    if (this.busy()) {
      this.lastActive = now
      return true
    }
    if (now - this.lastActive < ACTIVE_MS) return true
    // the safety tick woke the loop: a frame's body, and then sleep again
    return now - this.lastBody >= IDLE_TICK_MS
  }

  /** Something wants this frame: a message, an input, an easing, an animation, a hold. */
  private busy(): boolean {
    if (this.dirty || this._needsRender || this.pickDirty || this.hoverMoved) return true
    if (this.padLooking || this.drag?.moved) return true
    if (this.iso && !this.levelMap) return true
    return this.pressTimes.size > 0 || this.padHints.waiting
  }

  /**
   * The level map (X) is the WebTiles 2D view fullscreen: while it is open
   * crawl's 2D view covers the dungeon view on a canvas of its own, which goes
   * when the map closes. The swap is a cut, in the frame the server's `ui_state`
   * lands: the map is there to be read, and nothing stands between the key and the reading.
   */
  private syncMapRenderer() {
    const mapView = this.mapShown
    if (mapView === this.lastMapView) return
    this.lastMapView = mapView
    if (mapView) {
      const canvas = h('canvas', { class: 'view' })
      this.canvas.after(canvas)
      this.canvas.style.visibility = 'hidden'
      this.attachPointer(canvas)
      const renderer = new Render2d({ cellSize: 32 })
      renderer.mount(canvas)
      if (this.session.gamedata) renderer.setTiles(this.session.gamedata)
      // the cursor the frame just placed went to the dungeon view: a level map opened by X or from the
      // ctrl-f results lands its `ui_state` and its map cursor (tileweb.cc `load_dungeon`) in one frame
      renderer.setCursor(this.lastCursor)
      this.levelMap = { canvas, renderer }
      this.hud.hideMinimap()
    } else {
      const map = this.levelMap
      if (map) {
        map.renderer.destroy()
        map.canvas.remove()
        this.levelMap = null
        this.canvas.style.visibility = ''
        this.dungeon.setCursor(this.lastCursor)
      }
      this.hud.showMinimap()
    }
    this.lastOptKey = ''
    this.relayout(true)
    this.needsRender = true
  }

  /**
   * Honour the rc options the server sends (the same `options` message the
   * official client reads): tile size and scales, display mode, minibars,
   * level-map pane hiding, fonts. Cheap to call every frame; only acts on change.
   */
  private applyServerOptions() {
    const st = this.session.state
    const o = st.options
    const p = st.player
    const mapView = this.mapShown
    const key = JSON.stringify([
      mapView,
      // the cell size is fitted to the view (`cellPixels`), so its size belongs to the key (`relayout` ran first in the frame)
      this.viewPx?.width,
      this.viewPx?.height,
      o.tile_cell_pixels,
      o.tile_viewport_scale,
      o.tile_map_scale,
      o.tile_display_mode,
      o.tile_filter_scaling,
      o.tile_show_minihealthbar,
      o.tile_show_minimagicbar,
      p.hp,
      p.hp_max,
      p.mp,
      p.mp_max,
      o.tile_level_map_hide_messages,
      o.tile_level_map_hide_sidebar,
      o.glyph_mode_font,
      o.tile_font_msg_family,
      o.tile_font_msg_size,
      o.tile_font_stat_family,
      o.tile_font_stat_size,
      o.tile_font_crt_family,
      o.tile_font_crt_size,
      o.tile_font_lbl_family,
      o.tile_font_lbl_size,
      o.custom_text_colours,
    ])
    if (key === this.lastOptKey) return
    this.lastOptKey = key
    // (the level map's pane hiding is the grid's: `relayout` and `levelMapSplit`)
    // fonts: a family or size from the rc overrides the theme for text off the grid (CRT screens, labels);
    // WebTiles' default "monospace" maps to ours. The grid's own cells keep the one grid font (console-grid.md).
    const rs = document.documentElement.style
    const font = (fam: unknown, size: unknown, v: string) => {
      if (typeof fam === 'string' && fam && fam !== 'monospace') rs.setProperty(`--font-${v}`, fam)
      else rs.removeProperty(`--font-${v}`)
      if (typeof size === 'number' && size > 0) rs.setProperty(`--font-${v}-size`, size + 'px')
      else rs.removeProperty(`--font-${v}-size`)
    }
    font(o.tile_font_msg_family, o.tile_font_msg_size, 'msg')
    font(o.tile_font_stat_family, o.tile_font_stat_size, 'stat')
    font(o.tile_font_crt_family, o.tile_font_crt_size, 'crt')
    font(o.tile_font_lbl_family, o.tile_font_lbl_size, 'lbl')
    // game.js init_custom_text_colours: the rc may replace any of the 16 terminal colours
    for (let i = 0; i < 16; i++) rs.removeProperty('--color-' + i)
    if (Array.isArray(o.custom_text_colours)) {
      for (const c of o.custom_text_colours as { index: number; r: number; g: number; b: number }[]) {
        if (typeof c?.index === 'number') rs.setProperty('--color-' + c.index, `rgb(${c.r}, ${c.g}, ${c.b})`)
      }
    }
    // cell_renderer.js draw_minibars: the player's bars, when tile_show_minihealthbar / tile_show_minimagicbar allow
    const minibars = { hp: p.hp, hpMax: p.hp_max, mp: p.mp, mpMax: p.mp_max, showHp: o.tile_show_minihealthbar !== false, showMp: o.tile_show_minimagicbar !== false }
    const map = this.levelMap?.renderer
    if (map) {
      const num = (v: unknown, d: number) => (typeof v === 'number' && v > 0 ? v : d)
      const px = num(o.tile_cell_pixels, 32)
      const scale = mapView ? num(o.tile_map_scale, 60) : num(o.tile_viewport_scale, 100)
      const mode = o.tile_display_mode === 'glyphs' ? 'glyphs' : o.tile_display_mode === 'hybrid' ? 'hybrid' : 'tiles'
      map.setOptions({
        cellSize: this.cellPixels(Math.max(8, Math.round((px * scale) / 100)), mode),
        mode,
        filterScaling: o.tile_filter_scaling === true,
        glyphFont: rcFont(o.glyph_mode_font),
        minibars,
      })
    }
    this.needsRender = true
  }

  /**
   * dungeon_renderer.js `fit_to`: the cells from the rc options, shrunk to fit
   * if a full field of view (game.js `show_diameter`) would not otherwise
   * stand in the view. Glyph modes size their cells from the font instead, so
   * they are left alone.
   */
  private cellPixels(cell: number, mode: string): number {
    if (mode === 'glyphs') return cell
    const w = this.canvas.clientWidth
    const h = this.canvas.clientHeight
    if (!w || !h) return cell
    const span = SHOW_DIAMETER * cell
    if (span <= w && span <= h) return cell
    return Math.max(1, Math.floor(cell * Math.min(w / span, h / span)))
  }

  /** The details card (cell-info.ts) about the cell the aim, the pointer or the server's cursor is on. */
  private updateInspect() {
    const cell = this.inspectedCell()
    if (!cell || !this.inspectShown()) return this.hud.showInspect([])
    const hints = this.aimDir === null && this.pointerLive ? this.clickHints(cell.x, cell.y) : []
    this.hud.showInspect(cellDetails(this.session.scene, cell.x, cell.y), hints)
  }

  /** The iso view is up, so the details card speaks for the cell under the pointer and the cell tooltip stays away. */
  private inspectShown(): boolean {
    return !!this.iso && this.renderer === this.iso
  }

  /**
   * mouse_control.js handle_cell_tooltip: what the clicks do on a cell with a monster (select target while
   * targeting, describe on right click). Nothing with `tile_web_mouse_control` off.
   */
  private clickHints(x: number, y: number): string[] {
    const st = this.session.state
    if (st.options.tile_web_mouse_control === false || !st.map.cells.get(mapKey(x, y))?.mon) return []
    const im = st.inputMode
    const canTarget = im === MouseMode.TARGET || im === MouseMode.TARGET_DIR || im === MouseMode.TARGET_PATH
    const canDescribe = canTarget || im === MouseMode.COMMAND || st.uiState === UiState.VIEW_MAP
    const visible = this.session.scene.cells.get(cellKey(x, y))?.visibility === 'visible'
    const hints: string[] = []
    if (canTarget && visible) hints.push('Left click: select target')
    if (canDescribe) hints.push('Right click: describe')
    return hints
  }

  private inspectedCell(): { x: number; y: number } | null {
    if (this.aimDir !== null) {
      const { x, y } = this.session.scene.player
      return { x: x + DIR8_DX[this.aimDir], y: y + DIR8_DY[this.aimDir] }
    }
    const cursor = this.lastCursor
    return cursor && cursor.mode !== 'map' ? { x: cursor.x, y: cursor.y } : null
  }

  private cursorFor(): SceneCursor | null {
    const st = this.session.state
    // cursor ids: 0 mouse/targeting, 1 tutorial, 2 level map
    const c = st.cursors[0] || st.cursors[2] || st.cursors[1]
    this.cursorIsOurs = false
    if (!c) {
      // dungeon_renderer.js handle_mouse: with nothing else up, the pointer's cell wears CURSOR_MOUSE
      const key = this.pointerCell()
      if (key === null) return null
      this.cursorIsOurs = true
      return { x: (key & 1023) - 512, y: (key >> 10) - 512, mode: 'examine', tile: this.session.gamedata?.icons.id('CURSOR') }
    }
    const mode: SceneCursor['mode'] = st.uiState === UiState.VIEW_MAP ? 'map' : st.inputMode === MouseMode.COMMAND ? 'examine' : 'target'
    // WebTiles paints icons.CURSOR for the mouse and map cursors, TUTORIAL_CURSOR for the tutorial one
    const tutorial = !st.cursors[0] && !st.cursors[2] && !!st.cursors[1]
    const tile = this.session.gamedata?.icons.id(tutorial ? 'TUTORIAL_CURSOR' : 'CURSOR')
    return { x: c.x, y: c.y, mode, tile }
  }

  /**
   * The cell the pointer selects in command mode, where a click acts and the
   * cursor icon lies. It is the plain WebTiles pointer: the cell it hovers,
   * whatever it is. Nothing for a spectator, with `tile_web_mouse_control`
   * off, or outside command mode, where the server owns the cursor; and
   * nothing until the mouse moves after a keyboard or pad action
   * (`pointerLive`).
   */
  private pointerCell(): CellKey | null {
    if (!this.hover || !this.pointerLive || this.session.watching || this.ctx.mode !== 'command') return null
    if (this.session.state.options.tile_web_mouse_control === false) return null
    // every camera move, resize and level change marks a render, and a pointer move marks the pick, so these are the only times it can change
    if (this.needsRender || this.pickDirty || this.pickMemo === undefined) {
      this.pickDirty = false
      this.pickMemo = this.renderer.pick(this.hover.x, this.hover.y)
    }
    return this.pickMemo
  }

  /**
   * Once a frame: while an aim is up the pointer steers its cursor
   * (`pointTarget`). Outside one, a mouse resting on the view does not aim
   * the `x` or `f` that opens next: the movement it made in command mode
   * is forgotten, so only movement while the aim is up moves its cursor.
   */
  private syncTarget() {
    if (this.ctx.mode === 'targeting') this.pointTarget()
    else {
      this.lastTargetSent = -1
      this.hoverMoved = false
    }
  }

  /**
   * Targeting with the pointer: the cell under the hovering mouse or the
   * dragging finger is where the server's cursor should be, as the
   * official client does with `target_cursor` on mouse move.
   */
  private pointTarget() {
    if (this.session.watching || !this.hover || !this.hoverMoved) return
    this.hoverMoved = false
    if (this.session.state.options.tile_web_mouse_control === false) return
    const key = this.renderer.pick(this.hover.x, this.hover.y)
    if (key === null || key === this.lastTargetSent) return
    this.lastTargetSent = key
    this.runner.send(cm.targetCursor((key & 1023) - 512, (key >> 10) - 512))
  }

  /** A server message came in: note the step the player made, if any (see `hops`). */
  private trackHop() {
    const pos = this.session.state.player.pos
    if (!pos) return
    const { x, y } = this.hopPos
    if (pos.x === x && pos.y === y) return
    const dx = pos.x - x
    const dy = pos.y - y
    if (Math.abs(dx) <= 1 && Math.abs(dy) <= 1) this.hops.push({ dx, dy })
    else this.jumped = true
    this.hopPos = { x: pos.x, y: pos.y }
  }

  private onScene() {
    const now = this.sceneTime ?? performance.now() / 1000
    const scene = this.session.scene
    const st = this.session.state
    const p = scene.player
    const hops = this.hops
    const jumped = this.jumped
    this.hops = []
    this.jumped = false
    if (scene.playerOnLevel) {
      const level = `${st.player.place}:${st.player.depth}`
      const newLevel = level !== this.lastLevel
      this.lastLevel = level
      const moved = p.x !== this.lastPos.x || p.y !== this.lastPos.y
      if (moved || newLevel) {
        const dx = p.x - this.lastPos.x
        const dy = p.y - this.lastPos.y
        const sameLevel = !newLevel && !Number.isNaN(this.lastPos.x) && !(dx === 0 && dy === 0)
        // a level change or a long jump (a teleport): the view is put on the player at once; a walk of a few
        // cells is hopped by the player's actor, and the view follows the hops
        if (!sameLevel || Math.max(Math.abs(dx), Math.abs(dy)) > SNAP_CELLS) this.view.snapTo(p.x, p.y)
        const ownStep = this.runner.stepForMove(dx, dy, performance.now(), jumped ? [] : hops)
        if (sameLevel && ownStep && this.padStep === ownStep && !this.session.watching) {
          this.padHints.moved(Math.max(1, hops.length))
          this.padStep = null
        }
        this.lastPos = { x: p.x, y: p.y }
        if (newLevel) {
          this.iso?.newLevel()
          this.sound.level(st.player.place, st.player.depth)
        }
        this.sound.surroundings(scene)
      }
    }
    if (this.aimDir !== null) this.refreshAim()
    this.renderer?.setScene(scene, now)
    this.needsRender = true
  }

  // ------------------------------------------------------------------ input

  /** Buttons whose hold action already ran this press; their release is swallowed. */
  private holdFired = new Set<string>()

  /**
   * Buttons whose press opened a tap-or-hold decision (bindings.ts
   * `armsTapOrHold`). A press that acted at once (space under a --more--, a
   * menu select) is not here, so its release cannot become a tap in whatever
   * mode the server moved to meanwhile: A on a --more-- while standing on
   * stairs must not also climb them.
   */
  private tapArmed = new Set<string>()

  /** Fire the hold half of any tap-or-hold button that has been down for HOLD_MS, without waiting for release. */
  private fireHolds(now: number) {
    this.holding = null
    if (this.chat.capturing || this.overlays.hasClientOverlay) {
      this.tapArmed.clear()
      return
    }
    for (const [b, t0] of this.pressTimes) {
      if (!this.tapArmed.has(b)) continue
      // the server may have moved the mode under the press: a button the new context has no tap-or-hold for
      // disarms, so neither half can fire where it would mean something else (LB pressed in a menu must not rest)
      if (!armsTapOrHold(b as Button, this.ctx)) { this.tapArmed.delete(b); continue }
      if (this.holdFired.has(b) || !this.hooks.gamepad.isHeld(b as Button)) continue
      const a = holdAction(b as Button, this.ctx)
      if (!a) continue
      if (now - t0 < HOLD_MS) {
        // the button's own prompt shows how far along the hold is; the same for every tap-or-hold
        this.holding = { button: b as Button, fraction: (now - t0) / HOLD_MS }
        continue
      }
      this.holdFired.add(b)
      this.executePadAction(a)
      this.needsRender = true
    }
  }

  pad(ev: PadEvent) {
    this.wake()
    // a press, a stick or the d-pad is the pad speaking; a stick settling back to centre is not
    if (isPadActivity(ev)) this.inputFrom('pad')
    if (this.chat.capturing) {
      this.chat.pad(ev)
      return
    }
    if (this.dock.focused && this.dockPad(ev)) return
    if (this.overlays.hasClientOverlay) {
      if (ev.type === 'press') {
        // the button that opened this screen puts it away whole, before anything else it would do here
        if (ev.button === this.overlayOpener && !(this.overlays.screenOpen && ev.button === 'Y')) this.overlays.clientOverlayInput('close')
        // an empty tab of X's actions keeps the two that are no list: Y swaps weapons, LT shouts
        else if (this.overlays.openMenu === 'battle' && (ev.button === 'Y' || ev.button === 'LT')) this.actionKey(ev.button === 'Y' ? SWAP_WEAPONS_KEY : SHOUT_KEY)
        else if (ev.button === 'A') this.overlays.clientOverlayInput('select')
        else if (ev.button === 'START') this.overlays.clientOverlayInput('submit')
        else if (ev.button === 'B') this.overlays.clientOverlayInput('cancel')
        // Select is the put-away button even for a screen it did not open
        else if (ev.button === 'SELECT') this.overlays.clientOverlayInput('close')
        else if (ev.button === 'X') this.overlays.clientOverlayInput('keyboard')
        else if (ev.button === 'Y') this.overlays.clientOverlayInput('alt')
        else if (ev.button === 'LB') this.overlays.clientOverlayInput('bumperPrev')
        else if (ev.button === 'RB') this.overlays.clientOverlayInput('bumperNext')
        // put away by any route (the opener, Select, B on the first screen): the next opener owns the next screen
        if (!this.overlays.hasClientOverlay) this.overlayOpener = null
      } else if (ev.type === 'dir' || ev.type === 'dirRepeat') {
        if (ev.dir === 0) this.overlays.clientOverlayInput('prev')
        else if (ev.dir === 4) this.overlays.clientOverlayInput('next')
        else if (ev.dir === 6) this.overlays.clientOverlayInput('left')
        else if (ev.dir === 2) this.overlays.clientOverlayInput('right')
      }
      return
    }
    if (this.stickAim(ev)) return
    if (ev.type === 'look') {
      this.padLooking = ev.dx !== 0 || ev.dy !== 0
      if (this.ctx.mode === 'levelmap') {
        // the level map is north-up: the right stick pans it instead of the camera
        this.panMap(ev.dx, ev.dy)
        return
      }
      this.mapPan = { x: 0, y: 0 }
      // the stick's left and right turn the view (`rstick` directions); up and down zoom it, in the frame loop
      this.padZoom = ev.dy
      this.needsRender = true
      return
    }
    if (ev.type === 'press') {
      this.pressTimes.set(ev.button, ev.t)
      this.holdFired.delete(ev.button)
      if (this.ctx.mode !== 'spectating' && armsTapOrHold(ev.button, this.ctx)) this.tapArmed.add(ev.button)
      else this.tapArmed.delete(ev.button)
    }
    if (ev.type === 'release') {
      if (ev.button === 'LT' && this.exploringWhileHeld) {
        this.exploringWhileHeld = false
        if (this.runner.exploreOpen) this.runner.execute(STOP_EXPLORE)
      }
      this.pressTimes.delete(ev.button)
      this.holding = null
      const armed = this.tapArmed.delete(ev.button)
      if (this.holdFired.delete(ev.button) || !armed) return
    }
    if (this.ctx.mode === 'spectating') {
      if (ev.type === 'press' && (ev.button === 'B' || ev.button === 'START')) {
        this.overlays.showSystem({ spectating: true, inGame: true })
        this.overlayOpener = ev.button
      }
      // a spectator's chat is one button away, as F12 is in the official client
      if (ev.type === 'press' && ev.button === 'X') this.chat.padFocus()
      if ((ev.type === 'dir' || ev.type === 'dirRepeat') && ev.dir !== null) this.hud.scrollLog(ev.dir === 0 ? -1 : ev.dir === 4 ? 1 : 0)
      return
    }
    const a = resolve(ev, this.ctx)
    if (!a) return
    this.inputActed()
    const overlayWasUp = this.overlays.hasClientOverlay
    this.executePadAction(a)
    // whichever button just opened a client overlay owns closing it; a screen
    // reached from inside one (A on a settings row) keeps the opener it came with
    if (ev.type === 'press') {
      if (!this.overlays.hasClientOverlay) this.overlayOpener = null
      // (the frame a turn of X's tabs stands in is no screen of the bumper's: the bumper turns it again)
      else if (!overlayWasUp && this.overlays.openMenu !== 'battle') this.overlayOpener = ev.button
    }
    this.needsRender = true
  }

  private padHintEvidence(): PadHintEvidence {
    const st = this.session.state
    const cursor = st.cursors[0] ?? st.cursors[2]
    return {
      mode: deriveMode(st), turn: st.player.turn, x: st.player.pos?.x ?? NaN, y: st.player.pos?.y ?? NaN,
      cursor: cursor ? `${cursor.x},${cursor.y}` : '',
      focus: this.overlays.focusInfo(this.ctx)?.index ?? st.menus.at(-1)?.last_hovered ?? -1,
      clientOverlay: this.overlays.hasClientOverlay,
    }
  }

  private executePadAction(a: Action) {
    const beforeStep = this.runner.lastStep
    this.padHints.attempt(a, this.ctx, this.padHintEvidence(), performance.now())
    this.runner.execute(a)
    if (a.kind === 'step' && this.ctx.mode === 'command' && this.runner.lastStep !== beforeStep) this.padStep = this.runner.lastStep
    else this.padStep = null
    this.padHints.observe(this.padHintEvidence(), performance.now())
  }

  /**
   * A keyboard or pad action fired: the pointer's own selection (and the
   * tooltip hanging off it) goes away until the mouse moves again. The
   * server's cursors are untouched.
   */
  /** Which device spoke last: the prompt strip is drawn in its terms (hud.ts renderBar). */
  private inputFrom(device: InputDevice) {
    if (device !== 'pad') {
      this.padHints.cancel()
      this.padStep = null
      this.padLooking = false
    }
    if (device === this.lastInput) return
    this.lastInput = device
    this.needsRender = true
  }

  private inputActed() {
    this.tapArmed.clear()
    this.holding = null
    if (!this.pointerLive) return
    this.pointerLive = false
    clearTimeout(this.tooltipTimer)
    this.hud.hideTooltip()
    this.needsRender = true
  }

  back() {
    this.wake()
    this.inputActed()
    this.needsRender = true
    if (this.lookPanel) return this.closeLookPanel()
    if (this.overlays.levelUp.choosing) return
    if (this.chat.capturing) return this.chat.close()
    if (this.dock.focused) return this.dock.focusOff()
    if (this.overlays.hasClientOverlay) return void this.overlays.clientOverlayInput('cancel')
    if (this.session.watching) return this.overlays.showSystem({ spectating: true, inGame: true })
    if (this.aimDir !== null) return this.clearAim()
    if (this.ctx.mode === 'command') return this.menuKey('system')
    if (this.ctx.mode === 'text') return this.runner.send(cm.key(27))
    const cancel = buttonAction('B', this.ctx)
    if (cancel) this.runner.execute(cancel)
  }

  /** One of the menu keys (MENU_KEYS): it opens that menu, and closes it again as the button does. */
  private menuKey(menu: CommandMenu | 'levelmap' | 'equipment' | 'system') {
    const open = this.overlays.openMenu
    if (this.session.watching) {
      // a spectator has the Isorun menu only, as the pad's Start
      if (menu !== 'system') return
      if (open === 'system') this.overlays.clientOverlayInput('close')
      else if (!this.overlays.hasClientOverlay) this.uiOp('system')
      return
    }
    // Select's level map is crawl's own: F2 again puts it away, as Escape would
    if (menu === 'levelmap' && this.ctx.mode === 'levelmap') this.runner.execute({ kind: 'keys', label: 'Cancel', seq: [{ key: 27 }] })
    else if (open === menu) this.overlays.clientOverlayInput('close')
    // the pack is crawl's own menu: F4 again puts it away, as Escape would
    else if (menu === 'equipment' && this.ctx.mode === 'menu' && this.ctx.menu && isPack(this.ctx.menu.menu)) this.runner.execute({ kind: 'keys', label: 'Cancel', seq: [{ key: 27 }] })
    // and so are X's actions
    else if (menu === 'battle' && this.ctx.mode === 'menu' && this.ctx.menu?.actions) this.runner.execute({ kind: 'keys', label: 'Cancel', seq: [{ key: 27 }] })
    // from the map, or from another of the four, the key goes straight to its own menu
    else if (this.ctx.mode === 'command' && (!this.overlays.hasClientOverlay || open)) {
      // the level map is crawl's, under ours: the menu up goes first
      if (menu === 'levelmap' && open) this.overlays.clientOverlayInput('close')
      this.uiOp(menu === 'battle' ? 'commands' : menu)
    }
    else if (this.overlays.hasClientOverlay) this.overlays.clientOverlayInput('close')
    // elsewhere the Isorun menu opens where Start opens it, and nowhere Start means something else
    else if (menu === 'system') { const a = buttonAction('START', this.ctx); if (a?.kind === 'ui' && a.op === 'system') this.uiOp('system') }
    // off the map F2 is the Select button there: the palette for the screen
    else if (menu === 'levelmap') this.overlays.showPalette(this.ctx.mode === 'targeting' || this.ctx.mode === 'menu' ? this.ctx.mode : 'command')
  }

  private onKeyDown(ev: KeyboardEvent) {
    if (this.lookPanel && ev.key === 'Escape') {
      ev.preventDefault()
      ev.stopPropagation()
      return this.closeLookPanel()
    }
    if (this.lookPanel && ev.target instanceof Node && this.lookPanel.contains(ev.target)) return
    this.wake()
    const target = ev.target as HTMLElement | null
    // a key typed into a field is still the keyboard speaking (a server prompt's on-screen keyboard goes away for it)
    this.inputFrom('keyboard')
    if (target && isTextEntry(target)) return
    // F2–F5 are Isorun's own: the pad's four menus, so a player can rebind a button (or anything) to them. A bare
    // F1 goes on to Crawl (keys.ts CODES), which binds it to CMD_GAME_MENU alongside `~`; crawl binds none of these.
    const menu = !ev.shiftKey && !ev.ctrlKey && !ev.altKey && !ev.metaKey ? MENU_KEYS[ev.key] : undefined
    if (menu) {
      ev.preventDefault()
      if (!ev.repeat) this.menuKey(menu)
      return
    }
    if (this.dock.focused && this.dockKeyDown(ev)) return
    if (ev.altKey && !ev.ctrlKey && !ev.metaKey && !this.levelMap && this.viewKey(ev.key)) {
      ev.preventDefault()
      return
    }
    if (this.overlays.hasClientOverlay) {
      if (ev.key === 'Escape') this.overlays.clientOverlayInput('cancel')
      else if (ev.key === 'ArrowDown') this.overlays.clientOverlayInput('next')
      else if (ev.key === 'ArrowUp') this.overlays.clientOverlayInput('prev')
      else if (ev.key === 'ArrowLeft') this.overlays.clientOverlayInput('left')
      else if (ev.key === 'ArrowRight') this.overlays.clientOverlayInput('right')
      else if (ev.key === 'PageDown') this.overlays.clientOverlayInput('pageNext')
      else if (ev.key === 'PageUp') this.overlays.clientOverlayInput('pagePrev')
      else if (ev.key === 'Home') this.overlays.clientOverlayInput('first')
      else if (ev.key === 'End') this.overlays.clientOverlayInput('last')
      // Enter and space both fire the row the cursor is on, as they do in a server menu
      else if (ev.key === 'Enter' || ev.key === ' ') this.overlays.clientOverlayInput('select')
      // an empty tab of X's actions: crawl's own keys for the two that are no list
      else if (this.overlays.openMenu === 'battle' && (ev.key === SWAP_WEAPONS_KEY || ev.key === SHOUT_KEY)) this.actionKey(ev.key)
      else if (!ev.altKey && !ev.metaKey) {
        if (ev.key === 'Tab') this.overlays.clientOverlayHotkey('Tab')
        else if (ev.key.length === 1) this.overlays.clientOverlayHotkey(ev.ctrlKey ? 'Ctrl-' + ev.key.toUpperCase() : ev.key)
      }
      ev.preventDefault()
      return
    }
    if (ev.key === 'F12' || ev.code === 'F12') {
      // client.js: F12 focuses chat
      if (this.chatOn) this.chat.focus()
      ev.preventDefault()
      return
    }
    if (this.session.watching) {
      if (ev.key === 'Escape') this.overlays.showSystem({ spectating: true, inGame: true })
      return
    }
    if (this.aimKey(ev)) return
    // Esc opens the Isorun menu where crawl takes commands, and is crawl's own everywhere else; Isorun's settings are on that menu.
    const mode = this.ctx.mode
    if (ev.key === 'Escape' && mode === 'command' && !ev.ctrlKey && !ev.altKey && !ev.metaKey && !ev.shiftKey) {
      ev.preventDefault()
      this.inputActed()
      if (!ev.repeat) this.menuKey('system')
      return
    }
    const screen = mode === 'command' && this.dockOn && !ev.ctrlKey && !ev.altKey && !ev.metaKey ? DOCK_SCREEN_KEYS[ev.key] : undefined
    if (screen) {
      ev.preventDefault()
      this.inputActed()
      if (!ev.repeat) this.dockButton(screen)
      return
    }
    // on a screen the keyboard is a pad: Enter, Escape, Space and the arrows are its buttons and its
    // d-pad, and do exactly what those do (bindings.ts screenKey), down the same path
    if (!ev.ctrlKey && !ev.altKey && !ev.metaKey && !ev.shiftKey) {
      const sk = screenKey(ev.key, this.ctx, this.overlays.popupScrolls)
      if (sk) {
        ev.preventDefault()
        // a pad's buttons never repeat (gamepad.ts): a held Enter confirms once, not on every screen it
        // opens; the arrows repeat as the d-pad does
        if (ev.repeat && 'button' in sk) return
        this.inputActed()
        const a = 'button' in sk ? buttonAction(sk.button, this.ctx) : resolve({ type: 'dir', dir: sk.dir, source: 'dpad' }, this.ctx)
        if (a) this.runner.execute(a)
        this.needsRender = true
        return
      }
    }
    // a server menu: paging and the paging characters move its hover and page on
    // the client, as menu.js does before any key reaches the server
    if (mode === 'menu' && this.ctx.menu?.menu.type !== 'crt' && this.overlays.menuKey(this.session.state, ev)) {
      ev.preventDefault()
      this.inputActed()
      this.needsRender = true
      return
    }
    // a popup that scrolls on the client (describe screens, help, the game-over
    // text): arrows, paging and the paging characters scroll it, as ui-layouts.js does
    if ((mode === 'popup' || mode === 'ended') && this.overlays.popupKey(this.session.state, ev)) {
      ev.preventDefault()
      this.inputActed()
      this.needsRender = true
      return
    }
    // direction keys are rewritten in command and targeting modes only; macro capture and everything else is raw
    if (mode === 'command' || mode === 'targeting') {
      const d = directionKey(ev)
      if (d) {
        ev.preventDefault()
        // the key's compass meaning is taken as relative to facing; the runner rotates it back
        // (plain: step / cursor move, Shift: run / fire that way, Ctrl: attack).
        // Left and right turn or strafe as this key set's setting says
        this.inputActed()
        this.runner.step(d.abs as RelDir, { run: d.mod === 'shift', attack: d.mod === 'ctrl' })
        this.needsRender = true
        return
      }
    }
    const msg: ClientMessage | null = keydownMessage(ev)
    if (!msg) return
    ev.preventDefault()
    this.inputActed()
    this.runner.send(msg)
  }

  /**
   * Mouse and touch alike: the pointer stays visible and free (the browser
   * never takes it). Press and release without moving acts on the cell under
   * the pointer, right click describes it; dragging past a small slop orbits
   * the camera instead, and no click follows the drag. Pointer capture keeps
   * the drag alive off the canvas. A hovering mouse still selects the cell it
   * rests on, and while targeting it moves the server's cursor. The gamepad's
   * right stick turns the camera whenever it is pushed, drag or no drag.
   */
  private attachPointer(c: HTMLCanvasElement) {
    c.addEventListener('contextmenu', (ev) => ev.preventDefault())
    c.addEventListener('pointerdown', (ev) => {
      this.inputFrom('pointer')
      // mouse_control.js: any mousedown hides the cell tooltip
      clearTimeout(this.tooltipTimer)
      this.hud.hideTooltip()
      // a click is the mouse speaking: its selection is live again, so a click right after a key still acts on the hovered cell
      if (ev.pointerType === 'mouse') this.pointerLive = true
      if (ev.pointerType === 'touch') this.touches.set(ev.pointerId, { x: ev.clientX, y: ev.clientY })
      if (this.touches.size === 2) return this.startPinch()
      if (this.drag) return
      this.drag = { id: ev.pointerId, x: ev.clientX, y: ev.clientY, x0: ev.clientX, y0: ev.clientY, moved: false, button: ev.button }
      c.setPointerCapture(ev.pointerId)
      if (ev.pointerType === 'touch') {
        this.armLongPress(ev.clientX, ev.clientY)
        this.touchHover(ev.clientX, ev.clientY)
      }
    })
    c.addEventListener('pointermove', (ev) => {
      if (ev.pointerType === 'mouse') {
        const rect = this.canvas.getBoundingClientRect()
        this.hover = { x: ev.clientX - rect.left, y: ev.clientY - rect.top }
        this.hoverByTouch = false
        clearTimeout(this.touchHoverTimer)
        this.hoverMoved = true
        this.pointerLive = true
        // the hovered cell is picked again next frame; only a change of cell draws one
        this.pickDirty = true
        this.wake()
        this.armCellTooltip(ev.pageX, ev.pageY)
      }
      if (this.touches.has(ev.pointerId)) {
        this.touches.set(ev.pointerId, { x: ev.clientX, y: ev.clientY })
        if (this.pinchSpan !== null) return this.pinch()
      }
      const d = this.drag
      if (!d || ev.pointerId !== d.id) return
      // the button came up somewhere the page could not see it (off the window
      // entirely, so no pointerup was ever delivered): this move, with nothing
      // held, is that release. Drop the drag instead of orbiting a held button
      // that is not held.
      if (ev.pointerType === 'mouse' && ev.buttons === 0) return this.cancelDrag()
      const dx = ev.clientX - d.x
      const dy = ev.clientY - d.y
      d.x = ev.clientX
      d.y = ev.clientY
      if (!d.moved && Math.hypot(ev.clientX - d.x0, ev.clientY - d.y0) < DRAG_SLOP) {
        // the cursor follows the finger within the slop, onto the cell its tap would act on
        if (ev.pointerType === 'touch') this.touchHover(ev.clientX, ev.clientY)
        return
      }
      if (!d.moved) this.dropTouchHover()
      d.moved = true
      clearTimeout(this.longPressTimer)
      // the drag grabs the map: it follows the pointer until the player moves
      if (!this.levelMap) this.view.drag(dx, dy)
      this.needsRender = true
    })
    const end = (ev: PointerEvent) => {
      this.touches.delete(ev.pointerId)
      if (this.touches.size < 2) this.pinchSpan = null
      const d = this.drag
      if (!d || ev.pointerId !== d.id) return
      clearTimeout(this.longPressTimer)
      this.drag = null
      if (c.hasPointerCapture(ev.pointerId)) c.releasePointerCapture(ev.pointerId)
      if (d.moved) this.needsRender = true
      else if (ev.type === 'pointerup') {
        const rect = this.canvas.getBoundingClientRect()
        this.onPointer(ev.clientX - rect.left, ev.clientY - rect.top, d.button)
      }
      // a tap leaves the cursor on its cell a moment, so the player sees where it sent them
      if (this.hoverByTouch && !d.moved && ev.type === 'pointerup') {
        this.pointerLive = true
        clearTimeout(this.touchHoverTimer)
        this.touchHoverTimer = window.setTimeout(() => this.dropTouchHover(), TAP_CURSOR_MS)
      } else this.dropTouchHover()
    }
    // a drag does not survive leaving the page: the button released out there
    // sends no pointerup, and coming back should not still be a press
    // (the one listener reads `cancelDrag` when it fires, so it always lets go of the canvas in play; adding it again is a no-op)
    this.onWindowBlur ??= () => this.cancelDrag()
    window.addEventListener('blur', this.onWindowBlur)
    this.cancelDrag = () => {
      const d = this.drag
      if (!d) return
      this.drag = null
      if (c.hasPointerCapture(d.id)) c.releasePointerCapture(d.id)
      if (d.moved) this.needsRender = true
      this.dropTouchHover()
    }
    c.addEventListener('pointerup', end)
    c.addEventListener('pointercancel', end)
    c.addEventListener('pointerleave', (ev) => {
      if (ev.pointerType === 'mouse') this.hover = null
      clearTimeout(this.tooltipTimer)
      this.hud.hideTooltip()
    })
  }

  /** A finger on the view puts the cursor on the cell under it, as a hovering mouse does, so the player sees where a tap goes. */
  private touchHover(clientX: number, clientY: number) {
    const rect = this.canvas.getBoundingClientRect()
    clearTimeout(this.touchHoverTimer)
    this.hover = { x: clientX - rect.left, y: clientY - rect.top }
    this.hoverByTouch = true
    this.pointerLive = true
    this.pickDirty = true
    this.wake()
  }

  private dropTouchHover() {
    clearTimeout(this.touchHoverTimer)
    if (!this.hoverByTouch) return
    this.hoverByTouch = false
    this.hover = null
    this.pickDirty = true
    this.wake()
  }

  private armLongPress(x: number, y: number) {
    clearTimeout(this.longPressTimer)
    this.longPressTimer = window.setTimeout(() => {
      const d = this.drag
      if (!d || d.moved) return
      d.moved = true
      const rect = this.canvas.getBoundingClientRect()
      this.onPointer(x - rect.left, y - rect.top, RIGHT_BUTTON)
    }, LONG_PRESS_MS)
  }

  private startPinch() {
    clearTimeout(this.longPressTimer)
    this.dropTouchHover()
    if (this.drag) this.drag.moved = true
    this.pinchSpan = this.touchSpan()
  }

  private pinch() {
    const span = this.touchSpan()
    if (this.pinchSpan && span && !this.levelMap) this.view.zoomBy(span / this.pinchSpan)
    this.pinchSpan = span
    this.wake()
  }

  private touchSpan(): number {
    const [a, b] = [...this.touches.values()]
    return a && b ? Math.hypot(a.x - b.x, a.y - b.y) : 0
  }

  /**
   * dungeon_renderer.js handle_mouse + mouse_control.js handle_cell_tooltip:
   * with `tile_web_mouse_control` on, a pointer resting on a cell with a
   * monster for half a second shows its name, and what a click would do
   * (select target while targeting, describe on right click). The cell is
   * the pointer's selection (`pointerCell`); the tip is drawn at page
   * coordinates (pageX, pageY), where the pointer is.
   */
  private armCellTooltip(pageX: number, pageY: number) {
    clearTimeout(this.tooltipTimer)
    this.hud.hideTooltip()
    const st = this.session.state
    if (st.options.tile_web_mouse_control === false || this.inspectShown()) return
    this.tooltipTimer = window.setTimeout(() => {
      if (this.drag) return
      const key = this.ctx.mode === 'command' ? this.pointerCell() : this.hover ? this.renderer.pick(this.hover.x, this.hover.y) : null
      if (key === null) return
      const x = (key & 1023) - 512
      const y = (key >> 10) - 512
      const cell = st.map.cells.get(mapKey(x, y))
      if (!cell?.mon) return
      const hints = this.clickHints(x, y)
      const text = hints.length ? '<br><br>' + hints.join('<br>') : ''
      this.hud.showTooltip(escapeHtml(cell.mon.name || '') + text, pageX + 10, pageY + 10)
    }, 500)
  }

  /**
   * action_panel.js show_panel / hide_panel: the panel's X folds it away and
   * the placeholder unfolds it again, the rc's `action_panel_show` either
   * way, and the change goes back to the rc, so the official client and
   * Isorun agree on the next game.
   */
  private setPanelShown(shown: boolean) {
    const st = this.session.state
    if (st.options.action_panel_disabled === true) return
    st.options.action_panel_show = shown
    st.rev.player++
    if (!this.session.watching) this.runner.send(cm.setOption('action_panel_show', shown))
  }

  /** A server popup or dialog is up, and the pointer may close it: ui.js registers its handlers on the same condition. */
  private popupUp(): boolean {
    if (this.session.watching || this.overlays.hasClientOverlay) return false
    if (this.ctx.mode !== 'popup' && this.ctx.mode !== 'dialog') return false
    return this.session.state.options.tile_web_mouse_control !== false
  }

  /**
   * ui.js popup_clickoutside_handler: while a popup is up (with
   * `tile_web_mouse_control` on) a mousedown outside it sends Escape, "since
   * for crawl we really need to send a 'close popup' message to the server
   * rather than do it in the client". The chat is outside the game
   * (`target_outside_game`) and keeps its clicks. Isorun adds: a right click
   * closes wherever it lands, inside the popup too, the mirror of the right
   * click that opened a describe, so a look is over as fast as it began.
   * Isorun's own overlays are not server popups and handle their own
   * pointer.
   */
  private onDocPointer(ev: PointerEvent) {
    this.wake()
    this.dock.focusOff()
    const t = ev.target instanceof Element ? ev.target : null
    if (this.tapAway(ev, t)) return
    if (!this.popupUp()) return
    if (t && this.chat.root.contains(t)) return
    // the perf pane takes its own taps (it saves the log); a tap on it is not a tap outside the popup
    if (t?.closest('.perf')) return
    const inside = !!t?.closest('.popup')
    if (inside && ev.button !== 2) return
    ev.preventDefault()
    ev.stopPropagation()
    clearTimeout(this.tooltipTimer)
    this.hud.hideTooltip()
    this.runner.send(cm.key(27))
  }

  private tapAway(ev: PointerEvent, target: Element | null): boolean {
    if (ev.pointerType !== 'touch' || this.session.watching) return false
    if (!this.overlays.hasClientOverlay && this.ctx.mode !== 'menu') return false
    if (target?.closest('.popup, .osk, .dock, .touch-bar, .touch-corner, .perf') || (target && this.chat.root.contains(target))) return false
    ev.preventDefault()
    ev.stopPropagation()
    this.back()
    return true
  }

  /** ui.js context_disable: no browser context menu while a popup is up, except on a text input. */
  private onDocContextMenu(ev: MouseEvent) {
    this.wake()
    if (!this.popupUp()) return
    const t = ev.target instanceof Element ? ev.target : null
    if (t && (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement)) return
    ev.preventDefault()
  }

  /**
   * A click at canvas coordinates (px, py): the official client's
   * `click_cell` on the cell the pointer selects, with the button numbered as
   * `ev.which` (1 left, 2 middle, 3 right), and the server decides what it
   * means (see Runner.clickCell). The camera is never turned: the player's
   * view is where the drag left it.
   */
  private onPointer(px: number, py: number, button: number) {
    if (this.session.watching) return
    // tile_web_mouse_control = false turns off clicking on the dungeon view
    if (this.session.state.options.tile_web_mouse_control === false) return
    const key = this.renderer.pick(px, py)
    if (key === null) return
    this.runner.clickCell((key & 1023) - 512, (key >> 10) - 512, button + 1)
  }

  /**
   * Right stick over the level map: accumulate deflection and step the map
   * cursor (north-up, absolute) each time a whole cell's worth builds up.
   */
  private panMap(dx: number, dy: number) {
    if (dx === 0 && dy === 0) {
      this.mapPan = { x: 0, y: 0 }
      return
    }
    const p = this.mapPan
    p.x += dx * MAP_PAN_RATE
    p.y += dy * MAP_PAN_RATE
    const sx = Math.abs(p.x) >= 1 ? Math.sign(p.x) : 0
    const sy = Math.abs(p.y) >= 1 ? Math.sign(p.y) : 0
    if (!sx && !sy) return
    p.x -= sx
    p.y -= sy
    const d = dirFromDelta(sx, sy)
    if (d !== null) this.runner.execute({ kind: 'cursor', dir: d })
  }

  /**
   * The arrow keys aim, as on the test bed (the Arrow keys setting on Aim): an arrow on the ground
   * points the way the held arrows point on the screen, two of them a diagonal, and Space or Enter
   * steps that way, again and again while it is held. Whether the key was one of these.
   */
  private aimKey(ev: KeyboardEvent): boolean {
    if (ev.ctrlKey || ev.altKey || ev.metaKey || ev.shiftKey) return false
    const vector = ARROW_VECTORS[ev.key]
    if (vector && this.settings().arrowKeys === 'aim' && this.ctx.mode === 'command' && !this.levelMap) {
      ev.preventDefault()
      this.inputActed()
      this.arrowsDown.add(ev.key)
      this.updateAim()
      return true
    }
    if ((ev.key === ' ' || ev.key === 'Enter') && this.aimDir !== null && this.ctx.mode === 'command') {
      ev.preventDefault()
      this.inputActed()
      this.runner.walk(this.aimDir)
      this.needsRender = true
      return true
    }
    if (ev.key === 'Escape' && this.aimDir !== null) {
      ev.preventDefault()
      this.clearAim()
      return true
    }
    return false
  }

  private onKeyUp = (ev: KeyboardEvent) => {
    if (!this.arrowsDown.delete(ev.key)) return
    this.updateAim()
  }

  /** The way the held arrows point, as a compass direction on the ground, shown as the aim arrow. */
  private updateAim() {
    let x = 0
    let y = 0
    for (const key of this.arrowsDown) {
      const v = ARROW_VECTORS[key]
      if (!v) continue
      x += v[0]
      y += v[1]
    }
    if (x === 0 && y === 0) return this.clearAim()
    this.setAim(stickToDir(x, y, this.view.yawGoal))
  }

  /** The aim arrow again for a new scene: from the held arrows, or from where the left stick leans on the Left stick setting's aim. */
  private refreshAim() {
    if (this.stickLean === null || this.settings().leftStick !== 'aim') this.updateAim()
    else this.setAim(stickToDir(DIR8_DX[this.stickLean], DIR8_DY[this.stickLean], this.view.yawGoal))
  }

  /**
   * The Left stick setting on Aim: the stick points the aim arrow on the ground the way it leans on the screen,
   * and A steps that way, again on each of its repeats while held; the stick back at centre puts the arrow away.
   * Whether the event was one of these.
   */
  private stickAim(ev: PadEvent): boolean {
    if (ev.type === 'release' && ev.button === 'A' && this.stickAimStepped) {
      this.stickAimStepped = false
      return true
    }
    if (ev.type === 'dir' && ev.source === 'lstick') this.stickLean = ev.dir
    if (this.settings().leftStick !== 'aim' || this.ctx.mode !== 'command' || this.levelMap || this.session.watching) return false
    if ((ev.type === 'dir' || ev.type === 'dirRepeat') && ev.source === 'lstick') {
      if (ev.type === 'dir') {
        this.inputActed()
        if (ev.dir === null) this.clearAim()
        else this.refreshAim()
      }
      return true
    }
    if ((ev.type === 'press' || ev.type === 'repeat') && ev.button === 'A' && this.stickLean !== null) {
      this.inputActed()
      this.refreshAim()
      if (this.aimDir === null) return true
      this.stickAimStepped = true
      this.runner.walk(this.aimDir)
      this.needsRender = true
      return true
    }
    return false
  }

  private setAim(dir: Dir8) {
    this.aimDir = dir
    this.iso?.setAim({ dir, intent: aimIntent(this.session.scene, dir) })
    this.needsRender = true
  }

  private clearAim() {
    this.arrowsDown.clear()
    this.aimDir = null
    this.iso?.setAim(null)
    this.needsRender = true
  }

  /** The wheel zooms the dungeon view; Shift with the wheel turns it. */
  private onWheel = (ev: WheelEvent) => {
    if (this.levelMap) return
    ev.preventDefault()
    if (ev.shiftKey) {
      this.wheelTurn += ev.deltaY || ev.deltaX
      if (Math.abs(this.wheelTurn) < WHEEL_TURN_PIXELS) return
      this.view.turn(Math.sign(this.wheelTurn) * TURN_DEGREES)
      this.wheelTurn = 0
    } else this.view.zoomBy(Math.exp(-ev.deltaY * ZOOM_PER_WHEEL_PIXEL))
    this.wake()
  }

  /** The arrow keys held down while they aim (`aimKey`), and the way they point. */
  private arrowsDown = new Set<string>()
  private aimDir: Dir8 | null = null
  /** A pressed on the Left stick setting's aim stepped: its release is that step's, not a tap of its own (`stickAim`) */
  private stickAimStepped = false
  /** Where the left stick leans on the screen, kept while a menu is up so the aim comes back after it; null at centre */
  private stickLean: Dir8 | null = null

  /** Wheel scroll with Shift since the last turn, in css pixels; a trackpad arrives in crumbs. */
  private wheelTurn = 0

  /** Alt with the arrows: left and right turn the view a step, up and down tilt it. Whether the key was one of them. */
  private viewKey(key: string): boolean {
    if (key === 'ArrowLeft' || key === 'ArrowRight') this.view.turn(key === 'ArrowLeft' ? -TURN_DEGREES : TURN_DEGREES)
    else if (key === 'ArrowUp' || key === 'ArrowDown') this.view.tiltBy(key === 'ArrowUp' ? TILT_PER_KEY : -TILT_PER_KEY)
    else return false
    this.needsRender = true
    return true
  }

  /** Put the camera back to its default angles. */
  private resetView() {
    this.view.setAngles(DEFAULT_ANGLES)
    this.needsRender = true
  }

  private uiOp(op: string, arg?: number, category?: CommandCategory, section?: string) {
    switch (op) {
      case 'commands':
        this.openActionTab(this.actionTabUp)
        break
      case 'actionTab':
        // from the tab being turned to, when presses run ahead of crawl
        this.openActionTab(actionNeighbour(this.actionTurn?.target ?? this.ctx.menu?.actions?.current ?? this.actionTabUp, arg ?? 1))
        break
      case 'dock':
        if (this.dockOn && !this.session.watching) this.dock.focusOn()
        break
      case 'exploreWhileHeld':
        this.exploringWhileHeld = true
        this.runner.execute({ kind: 'keys', seq: [{ text: 'o' }], label: 'Explore while held' })
        break
      case 'instantExplore':
        this.exploringAtOnce = true
        this.paceTravel()
        this.runner.execute({ kind: 'keys', seq: [{ text: 'o' }], label: 'Explore at once' })
        break
      case 'equipment': {
        if (this.dockOn) {
          this.showInventory()
          break
        }
        // crawl's own pack, at once, on the page and row it was left on: its pages are the tabs (pack-tabs.ts)
        const seq = openPackKeys(this.session.state, this.packPage)
        this.runner.execute({ kind: 'keys', label: 'Inventory', seq })
        // no turns: the page left is gone (or was the first), so the pack stays on its first, Gear
        this.packReturn = { page: seq.length > 1 && this.packPage ? this.packPage : 'gear', seen: false }
        break
      }
      case 'interact':
        this.overlays.showChoices('Interact', [
          { label: contextualLabel(this.ctx), run: () => this.runner.contextual(false) },
          { label: 'Pick up items', run: () => this.runner.contextual(true) },
        ])
        break
      case 'palette':
        this.overlays.showPalette(category || 'command', section)
        break
      case 'popupAction':
        this.overlays.triggerPopupAction(arg ?? 0)
        break
      case 'system':
        this.overlays.showSystem({ spectating: this.session.watching, inGame: true, chat: this.chatOn, run: (a) => this.runner.execute(a) })
        break
      case 'bindings':
        this.overlays.showBindings(this.hooks.gamepad.kind)
        break
      case 'resetView':
        this.resetView()
        break
      case 'levelmap':
        this.runner.execute(LEVEL_MAP)
        break
      case 'scrollLog':
        if (arg) this.hud.scrollLog(arg)
        break
      case 'keyboard':
        this.overlays.oskOp('shift')
        break
    }
  }

  /**
   * One of X's actions as tabs (action-tabs.ts), by its key. The frame
   * lights the tab at once; crawl is asked for one menu at a time. From one
   * of its menus that one goes away first, and the key follows once it has
   * (`trackActions`): a key sent behind the Escape in one go could not wait
   * for `z`'s question, and a refused one would leak its `*` into the
   * dungeon. A press while a turn is under way only moves its target, so a
   * quick player never has a key land in the menu that was opening.
   */
  private openActionTab(id: ActionTabId) {
    this.actionTabUp = id
    this.overlays.showActionTabs(id, null)
    if (this.actionTurn) {
      this.actionTurn.target = id
      return
    }
    if (deriveMode(this.session.state) === 'menu') this.closeForTurn(id)
    else this.sendActionKey(id)
  }

  private closeForTurn(target: ActionTabId) {
    this.actionTurn = { target, sent: null, stage: 'closing', t: performance.now() }
    this.runner.execute({ kind: 'keys', label: 'Cancel', seq: [{ key: 27 }] })
  }

  private sendActionKey(target: ActionTabId) {
    const tab = actionTab(target)
    this.actionTurn = { target, sent: target, stage: 'sent', last: this.session.state.messages.lines.at(-1), t: performance.now() }
    this.runner.execute({ kind: 'keys', label: tab.label, seq: [{ text: tab.key }] })
  }

  /** Y or LT on an empty tab: the frame goes, and crawl's key for the action goes alone (no menu to put away). */
  private actionKey(key: string) {
    // a menu still opening would take the key as a row's letter: the frame waits for it
    if (this.actionTurn) return
    this.overlays.closeClientOverlay()
    this.runner.execute({ kind: 'keys', label: key === SHOUT_KEY ? 'Shout' : 'Swap weapons', seq: [{ text: key }] })
  }

  /**
   * A turn of X's tabs, seen through: the tab's menu up (the frame goes,
   * and X will open on it again), or crawl's line refusing it ("You don't
   * know any spells."), which the frame shows where the menu would be.
   * `z` asks before it lists (spl-cast.cc `cast_a_spell`, a prompt-channel
   * line), and `*` answers. Anything else crawl puts up, or nothing for a
   * while, ends the turn and the frame with it. The keys go as crawl's
   * answers land; the frame goes only from a drawn frame (`drawn`), once
   * what crawl put up stands in its place, or the screen would be bare
   * for a frame between the two.
   */
  private trackActions(now: number, drawn: boolean) {
    const turn = this.actionTurn
    const st = this.session.state
    // read off the state, not the frame's context: crawl's answer to one key may call for the next before a frame is drawn
    const mode = deriveMode(st)
    const menu = mode === 'menu' ? topMenu(st) : undefined
    const up = menu ? actionTabOf(menu) : null
    if (!turn) {
      if (up) this.actionTabUp = up
      return
    }
    const end = () => {
      if (!drawn) return
      this.actionTurn = null
      if (this.overlays.openMenu === 'battle') this.overlays.closeClientOverlay()
    }
    // the frame put away (B, F3) while crawl was still answering: what lands goes too
    const abandoned = this.overlays.openMenu !== 'battle'
    // a menu going away passes through `prompt` (its last line was one); anything else crawl puts up ends the turn
    const elsewhere = mode !== 'command' && mode !== 'menu' && mode !== 'prompt'
    if (elsewhere || now - turn.t > ACTION_TURN_MS) return end()
    if (turn.stage === 'closing') {
      // the menu left is still up until the Escape lands, whichever tab it is
      if (mode !== 'command') return
      if (abandoned) return end()
      return this.sendActionKey(turn.target)
    }
    if (up) {
      if (abandoned) {
        this.actionTurn = null
        this.runner.execute({ kind: 'keys', label: 'Cancel', seq: [{ key: 27 }] })
        return
      }
      // the player went on past this one while it opened
      if (up !== turn.target) return this.closeForTurn(turn.target)
      this.actionTabUp = up
      return end()
    }
    const last = st.messages.lines.at(-1)
    if (!last || last === turn.last) return
    const text = formattedStringToText(last.text).trim()
    if (text === OKAY_THEN) return
    // `z` asks before it lists, and waits for the answer; a refusal (some on the same channel) waits for nothing
    if (mode !== 'command') {
      if (turn.sent === 'spells' && turn.stage === 'sent' && last.channel === MSGCH.PROMPT) {
        turn.stage = 'listing'
        turn.last = last
        this.runner.execute({ kind: 'keys', label: 'List spells', seq: [{ text: '*' }] })
      }
      return
    }
    // refused: the tab is empty, and crawl's line says so; or the player has gone on, and the next key goes
    if (abandoned) return end()
    if (turn.sent !== turn.target) return this.sendActionKey(turn.target)
    this.actionTurn = null
    this.overlays.showActionTabs(turn.target, text)
  }

  /**
   * The pack's page and row, kept as the player leaves them, so Y brings the
   * pack back where it was: the same scroll under the cursor for reading it
   * again and again. Once Y's pack is on its page the cursor goes back to the
   * item, by its letter (a stack keeps its letter as it shrinks); an item
   * gone leaves crawl's own first row.
   */
  private trackPack() {
    const m = this.ctx.mode === 'menu' ? this.ctx.menu : undefined
    const page = m?.pack?.current
    const back = this.packReturn
    if (!m || !page) {
      // the pack put away (or never opened: crawl refused it) after Y's turn at it
      if (back?.seen && this.ctx.mode !== 'menu') this.packReturn = null
      return
    }
    if (back) {
      back.seen = true
      if (page !== back.page) return
      this.packReturn = null
      const key = this.packRow.get(page)
      if (key !== undefined) this.overlays.hoverHotkey(this.session.state, key)
    }
    this.packPage = page
    const hovered = m.menu.items[m.menu.last_hovered]?.hotkeys?.[0]
    if (hovered !== undefined) this.packRow.set(page, hovered)
  }

  /** The dock's parts for this frame, rebuilt only when what they show changed: the player, the context, what the dock knows. */
  private updateDock(st: GameState) {
    if (!this.dockOn) return
    const watching = this.session.watching
    if (!watching) this.loadMemory(st)
    const key = `${st.rev.player}|${this.dockMemoryRev}|${this.ctxKey}|${this.ctx.mode}|${this.lastInput}|${this.hooks.gamepad.kind}|${!!this.session.gamedata}`
    if (key === this.dockKey) return
    this.dockKey = key
    const p = st.player
    this.dock.setTiles(this.session.gamedata)
    this.dockSlots = watching ? [] : quickSlots(this.dockMemory.pins, this.slotSources(st))
    this.dock.update({
      hp: p.hp,
      hpMax: p.hp_max,
      poisonSurvival: p.poison_survival,
      mp: p.mp,
      mpMax: p.mp_max,
      xl: p.xl,
      progress: p.progress,
      gold: p.gold,
      place: p.depth ? `${p.place}:${p.depth}` : p.place,
      stats: { ac: p.ac, ev: p.ev, sh: p.sh, acMod: p.ac_mod, evMod: p.ev_mod, shMod: p.sh_mod, str: p.str, int: p.int, dex: p.dex, strMax: p.str_max, intMax: p.int_max, dexMax: p.dex_max },
      statuses: p.status,
      actions: this.ctx.mode === 'command' && !watching ? dockActions(this.ctx, this.session.scene) : [],
      slots: this.dockSlots,
      spectating: watching,
      device: this.lastInput,
      padKind: this.hooks.gamepad.kind,
    })
  }

  /** What the quick slots can hold now. */
  private slotSources(st: GameState): SlotSources {
    const p = st.player
    const items = packItems(p)
    const quiver = p.quiver_available ? formattedStringToText(p.quiver_desc) : ''
    return {
      spells: this.dockMemory.spells,
      abilities: this.dockMemory.abilities,
      consumables: actionPanelItems(st).flatMap((c) => items.filter((i) => i.slot === c.slot)),
      items,
      quiver,
      quiverItem: items.find((i) => i.slot === p.quiver_item),
    }
  }

  /** The character's dock memory, loaded once its name is known and again for another character; a new game (crawl's "Welcome," not "Welcome back,") starts empty. */
  private loadMemory(st: GameState) {
    const p = st.player
    if (!p.received || !p.name) return
    const id = `${p.name}|${p.species}`
    if (id !== this.dockCharacter) {
      this.dockCharacter = id
      this.dockMemory = loadDockMemory(id, p.turn)
      this.dockMemoryRev++
      this.dockFresh = false
    }
    if (this.dockFresh || p.turn > 0 || st.rev.messages === this.dockFreshRev) return
    this.dockFreshRev = st.rev.messages
    if (!st.messages.lines.some((l) => formattedStringToText(l.text).startsWith(`Welcome, ${p.name} `))) return
    this.dockFresh = true
    this.dockMemory = emptyDockMemory()
    this.rememberDock()
  }

  private rememberDock() {
    this.dockMemory.turn = this.session.state.player.turn
    this.dockMemoryRev++
    if (this.dockCharacter) saveDockMemory(this.dockCharacter, this.dockMemory)
  }

  /**
   * Crawl's menus as they come up: the spell and ability lists name what the
   * quick slots can hold, and the pack's Gear page says what is worn.
   */
  private learnFromMenus() {
    const st = this.session.state
    if (this.session.watching || st.rev.ui === this.menusReadRev) return
    this.menusReadRev = st.rev.ui
    this.loadMemory(st)
    const menu = topMenu(st)
    if (!menu || !this.dockCharacter) return
    const mem = this.dockMemory
    const spells = spellsFromMenu(menu)
    const abilities = abilitiesFromMenu(menu)
    let changed = false
    if (spells && JSON.stringify(spells) !== JSON.stringify(mem.spells)) {
      mem.spells = spells
      changed = true
    }
    if (abilities && JSON.stringify(abilities) !== JSON.stringify(mem.abilities)) {
      mem.abilities = abilities
      changed = true
    }
    if (isGearPage(menu)) {
      const worn = wornFromMenu(menu.items)
      if (JSON.stringify([...worn]) !== JSON.stringify([...mem.worn])) {
        mem.worn = worn
        changed = true
      }
    }
    if (changed) this.rememberDock()
  }

  /**
   * Read what is worn off crawl's own pack: `i`, read the Gear page when it
   * is up (`learnFromMenus`), Escape. The pack is not drawn meanwhile
   * (`hideMenu`); the inventory screen shows what was known until then.
   */
  private probeGear() {
    if (this.gearProbe || this.session.watching || deriveMode(this.session.state) !== 'command') return
    this.gearProbe = { stage: 'open', t: performance.now() }
    this.runner.send(cm.input('i'))
  }

  private trackGearProbe(now: number) {
    const st = this.session.state
    const menu = topMenu(st)
    if (!this.gearProbe && menu && isPack(menu) && this.overlays.screenOpen === 'inventory') this.gearProbe = { stage: 'open', t: now }
    const probe = this.gearProbe
    if (!probe) return
    const mode = deriveMode(st)
    if (now - probe.t > GEAR_PROBE_MS) this.gearProbe = null
    else if (probe.stage === 'open') {
      if (menu && isPack(menu)) {
        this.learnFromMenus()
        probe.stage = 'closing'
        this.runner.send(cm.key(27))
      } else if (mode !== 'command') this.gearProbe = null
    } else if (mode === 'command' && !st.menus.length) this.gearProbe = null
    if (!this.gearProbe) this.screenKey = ''
  }

  /** A dock action: only where crawl takes commands, with no screen of ours over the dungeon. */
  private dockCommand(run: () => void) {
    this.dock.focusOff()
    if (this.session.watching || this.ctx.mode !== 'command') return
    if (this.overlays.hasClientOverlay) this.overlays.closeClientOverlay()
    run()
  }

  private useSlot(i: number) {
    const slot = this.dockSlots[i]
    if (!slot) return this.assignSlot(i)
    const st = this.session.state
    this.dockCommand(() => {
      const ref = slot.ref
      if (ref.kind === 'fire') this.runner.execute({ kind: 'fire' })
      else if (ref.kind === 'item') this.runner.send(cm.invItemAction(ref.slot))
      else this.runner.execute({ kind: 'keys', label: ref.kind === 'spell' ? 'Cast' : 'Ability', seq: [{ text: ref.kind === 'spell' ? 'z' : 'a' }, { text: ref.letter, await: 'prompt' }] })
    })
  }

  /** A spell or an ability onto the first quick slot the player has not pinned, from its row in crawl's list. */
  private pinToFreeSlot(ref: SlotRef, itemName?: string) {
    const mem = this.dockMemory
    const same = mem.pins.findIndex((p) => !!p && JSON.stringify(p) === JSON.stringify(ref))
    const at = same >= 0 ? same : mem.pins.findIndex((p) => !p)
    if (at < 0) return this.hud.status('Every quick slot is taken: right click one to change it')
    mem.pins[at] = ref
    this.rememberDock()
    const known = ref.kind === 'spell' ? mem.spells : ref.kind === 'ability' ? mem.abilities : []
    const letter = ref.kind === 'spell' || ref.kind === 'ability' ? ref.letter : ''
    const name = itemName ?? known.find((k) => k.letter === letter)?.name ?? 'It'
    this.hud.status(`${name} is on quick slot ${(at + 1) % 10}`)
  }

  /** Put something else on a quick slot: what it can hold, as a list, with "fill by itself" first. */
  private assignSlot(i: number) {
    this.dock.focusOff()
    if (this.session.watching) return
    const st = this.session.state
    const mem = this.dockMemory
    const pin = (ref: SlotRef | null) => {
      if (ref) mem.pins = mem.pins.map((p) => (p && JSON.stringify(p) === JSON.stringify(ref) ? null : p))
      mem.pins[i] = ref
      this.rememberDock()
    }
    const choices = slotChoices(this.slotSources(st)).map((c) => ({ label: formattedStringToText(c.label), sub: c.group, tiles: c.tiles, run: () => pin(c.ref) }))
    const more = [
      { label: 'Fill by itself', sub: 'the quiver, then your spells, abilities and consumables, in order', run: () => pin(null) },
      ...choices,
      { label: 'Open the spell list', sub: 'the slots learn your spells from it', tile: 'CMD_CAST_SPELL', sep: true, run: () => this.dockButton('spells') },
      { label: 'Open the abilities', sub: 'the slots learn your abilities from it', tile: 'CMD_USE_ABILITY', run: () => this.dockButton('abilities') },
    ]
    this.overlays.showChoices(`Quick slot ${(i + 1) % 10}`, more, undefined, 'quick-slot')
  }

  private dockButton(id: DockButtonId) {
    this.dock.focusOff()
    if (this.session.watching) return
    if (id === 'inventory' || id === 'character') {
      if (this.ctx.mode === 'menu' || this.ctx.mode === 'popup') {
        this.resumeScreen = { id, t: performance.now(), left: true }
        return this.runner.execute({ kind: 'keys', label: 'Cancel', seq: [{ key: 27 }] })
      }
      return id === 'inventory' ? this.showInventory() : this.showCharacter()
    }
    if (id === 'menu') {
      if (this.overlays.hasClientOverlay) this.overlays.closeClientOverlay()
      return this.uiOp('system')
    }
    const mode = this.ctx.mode
    if (mode !== 'command' && mode !== 'menu' && mode !== 'popup') return
    if (this.overlays.hasClientOverlay) this.overlays.closeClientOverlay()
    if (id === 'spells' || id === 'abilities') return this.openActionTab(id)
    const away = mode === 'command' ? [] : [{ key: 27 }]
    if (id === 'skills') this.runner.execute({ kind: 'keys', label: 'Skills', seq: [...away, { text: 'm' }] })
    else this.runner.execute({ kind: 'keys', label: 'Level map', seq: [...away, { text: 'X' }] })
  }

  /** The player's sprite on the level, for the screens' figure. */
  private playerBillboard() {
    const scene = this.session.scene
    return scene.playerOnLevel ? scene.billboards.find((b) => b.kind === 'player' && b.x === scene.player.x && b.y === scene.player.y) : undefined
  }

  /** The inventory screen (screens.ts), up or rebuilt in place; opening it reads what is worn again (`probeGear`). */
  private showInventory(rebuild = false) {
    if (this.session.watching) return
    const st = this.session.state
    const p = st.player
    const items = packItems(p)
    const worn = wornNow(p, items, this.dockMemory.worn)
    const screen = inventoryScreen(
      { player: p, items, worn, me: this.playerBillboard(), gd: this.session.gamedata, checking: !!this.gearProbe, padKind: this.hooks.gamepad.kind },
      { act: (item, a) => this.itemAct(item, a), fill: (place, label) => this.fillPlace(place, label), pin: (item) => this.pinToFreeSlot({ kind: 'item', slot: item.slot }, itemTitle(item)) },
    )
    this.overlays.showScreen('inventory', 'Inventory', screen)
    this.screenKey = this.screenFor(st)
    if (!rebuild) this.probeGear()
  }

  private showCharacter() {
    if (this.session.watching) return
    const st = this.session.state
    const p = st.player
    const screen = characterScreen({ player: p, me: this.playerBillboard(), gd: this.session.gamedata, weapon: weaponLine(p), quiver: p.quiver_desc }, { run: (c) => this.characterCommand(c) })
    this.overlays.showScreen('character', 'Character', screen)
    this.screenKey = this.screenFor(st)
  }

  private characterCommand(c: CommandEntry) {
    if (this.ctx.mode !== 'command') return
    this.overlays.closeClientOverlay()
    this.resumeScreen = { id: 'character', t: performance.now(), left: false }
    this.runner.execute(c.action)
  }

  /** An empty place of the doll: what in the pack goes there, as a list to put one on; the quiver is crawl's own Quiver tab. */
  private fillPlace(place: DollPlace, label: string) {
    if (place === 'quiver') {
      this.overlays.closeClientOverlay()
      return this.openActionTab('quiver')
    }
    const p = this.session.state.player
    const items = packItems(p)
    const worn = wornNow(p, items, this.dockMemory.worn)
    const slot = place === 'ring1' || place === 'ring2' ? 'ring' : place
    const fits = items.filter((i) => equipSlotOf(i) === slot && !worn.has(i.letter))
    if (!fits.length) return this.hud.status(`Nothing in the pack goes on: ${label}`)
    this.overlays.showChoices(
      `${label}: put on`,
      fits.map((i) => ({ label: `${i.letter} - ${itemTitle(i)}`, tiles: itemTiles(i), run: () => this.itemAct(i, 'equip') })),
      () => this.showInventory(true),
      'fill-place',
    )
  }


  /** An item's action from the inventory: the screen goes while crawl takes it, and comes back after (`trackResume`). */
  private itemAct(item: PackItem, id: ItemActionId) {
    if (this.session.watching || this.gearProbe || deriveMode(this.session.state) !== 'command') return
    this.overlays.closeClientOverlay()
    this.resumeScreen = { id: 'inventory', t: performance.now(), left: false }
    if (id === 'use') return this.runner.send(cm.invItemAction(item.slot))
    if (id === 'describe') return this.runner.send(cm.invItemDescribe(item.slot))
    const keys = itemActionKeys(id, item)
    if (!keys) return
    if (keys.describeFirst) this.runner.sendThen(cm.invItemDescribe(item.slot), keys.seq)
    else this.runner.execute({ kind: 'keys', label: id, seq: keys.seq })
  }

  /** What an open screen shows: rebuilt when this changes (`refreshScreen`). */
  private screenFor(st: GameState): string {
    return `${st.rev.player}|${this.dockMemoryRev}|${!!this.gearProbe}|${!!this.session.gamedata}`
  }

  private refreshScreen(st: GameState) {
    const open = this.overlays.screenOpen
    if (!open || this.screenFor(st) === this.screenKey) return
    if (open === 'inventory') this.showInventory(true)
    else if (open === 'character') this.showCharacter()
  }

  /**
   * A screen put away for crawl to take an action comes back once crawl is
   * done with it: back in command mode after whatever it opened (a menu, a
   * prompt, a description, an aim), or at once for an action that opened
   * nothing.
   */
  private trackResume(now: number) {
    const r = this.resumeScreen
    if (!r) return
    if (now - r.t > RESUME_MS || this.overlays.hasClientOverlay) {
      this.resumeScreen = null
      return
    }
    if (deriveMode(this.session.state) !== 'command') {
      r.left = true
      return
    }
    if (!r.left && now - r.t < RESUME_QUIET_MS) return
    this.resumeScreen = null
    if (r.id === 'inventory') this.showInventory()
    else this.showCharacter()
  }

  /** The pad while its cursor is on the dock: the d-pad and the stick walk it, A uses, Y puts something else on a slot, B and L3 leave. */
  private dockPad(ev: PadEvent): boolean {
    if (ev.type === 'dir' || ev.type === 'dirRepeat') {
      if (ev.dir === 0) this.dock.nav('up')
      else if (ev.dir === 4) this.dock.nav('down')
      else if (ev.dir === 6) this.dock.nav('left')
      else if (ev.dir === 2) this.dock.nav('right')
      this.needsRender = true
      return true
    }
    if (ev.type !== 'press') return ev.type === 'release' || ev.type === 'repeat'
    if (ev.button === 'DU' || ev.button === 'DD' || ev.button === 'DL' || ev.button === 'DR') return true
    if (ev.button === 'A') this.dock.nav('select')
    else if (ev.button === 'Y') this.dock.nav('assign')
    else if (ev.button === 'B' || ev.button === 'L3') this.dock.nav('cancel')
    else {
      this.dock.focusOff()
      return false
    }
    this.dock.focusOff()
    this.needsRender = true
    return true
  }

  /** The arrows, Enter and Escape on the dock while the pad's cursor is there. */
  private dockKeyDown(ev: KeyboardEvent): boolean {
    const op = DOCK_KEYS[ev.key]
    if (!op) {
      this.dock.focusOff()
      return false
    }
    ev.preventDefault()
    this.dock.nav(op)
    if (op === 'select') this.dock.focusOff()
    return true
  }

  /** The prompts under the dock while the pad's cursor is on it. */
  private dockPrompts(): BindingLabel[] {
    const l = this.dock.focusLabels()
    if (!l) return []
    const out: BindingLabel[] = [{ button: 'B', label: 'Back', action: { kind: 'focus', op: 'cancel' }, contextual: true }]
    if (l.assign) out.push({ button: 'Y', label: 'Change slot', action: { kind: 'focus', op: 'altSelect' }, contextual: true })
    out.push({ button: 'A', label: l.select, action: { kind: 'focus', op: 'select' }, contextual: true })
    return out
  }

  private systemAction(op: string) {
    if (op === 'chat' && this.chatOn) this.chat.padFocus()
    else if (op === 'disconnect') this.hooks.onSystem('disconnect')
  }
}

/** How long reading the pack behind the inventory waits for crawl (`probeGear`). */
const GEAR_PROBE_MS = 3000
/** How long a screen put away for an action waits to come back, and how long an action that opened nothing is given (`trackResume`). */
const RESUME_MS = 20000
const RESUME_QUIET_MS = 600
const DOCK_SCREEN_KEYS: Readonly<Record<string, DockButtonId>> = { i: 'inventory', '@': 'character' }

/** The keys that walk the dock while the pad's cursor is on it. */
const DOCK_KEYS: Record<string, 'up' | 'down' | 'left' | 'right' | 'select' | 'cancel'> = { ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right', Enter: 'select', ' ': 'select', Escape: 'cancel' }

/** The pack on its Gear page, the one that says what is worn (pack-tabs.ts `PAGES`). */
function isGearPage(menu: MenuState): boolean {
  return isPack(menu) && formattedStringToText(menu.title?.text ?? '').trimStart().startsWith('Gear')
}

/** How long a turn of X's tabs waits on crawl before it gives up (`trackActions`), as the runner's `awaitPrompt` waits on a prompt. */
const ACTION_TURN_MS = 1500

/** The keys for the pad's four menus: F2 Select's level map, F3 X's actions, F4 Y's gear, F5 Start's Isorun menu. */
const MENU_KEYS: Record<string, CommandMenu | 'levelmap' | 'equipment' | 'system'> = { F2: 'levelmap', F3: 'battle', F4: 'equipment', F5: 'system' }

/** A move of more cells than this in one scene is a jump the view cuts to (the actors' MAX_WALK). */
const SNAP_CELLS = 8
/** How long after the player's own key swung a blow the server's line about it is that same blow. */
const OWN_SWING_MS = 1000
/** Below this share of their hit points the screen's edge turns red, deeper as they fall. */
const DANGER_HP = 0.5

/** The screen way each arrow key points: x right, y down. */
const ARROW_VECTORS: Record<string, [number, number]> = { ArrowUp: [0, -1], ArrowDown: [0, 1], ArrowLeft: [-1, 0], ArrowRight: [1, 0] }

/** What a step that way would do: attack a monster, walk, or be stopped by a wall. */
function aimIntent(scene: Scene, dir: Dir8): AimIntent {
  const x = scene.player.x + DIR8_DX[dir]
  const y = scene.player.y + DIR8_DY[dir]
  if (scene.billboards.some((b) => b.kind === 'monster' && b.x === x && b.y === y && b.attitude !== 'friendly')) return 'attack'
  const cell = getCell(scene, x, y)
  if (!cell || (cell.occluder && cell.kind !== 'door')) return 'blocked'
  return 'move'
}

/** Shift-wheel scroll that turns the view by one step. */
const WHEEL_TURN_PIXELS = 60

export { keyMessage }
