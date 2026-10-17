import type { HintMode } from './gamepad-hints'
import type { GameLink } from '@isorun/webtiles'
import bundled from '../data/servers.json'
import { canQuit } from './quit'
import { DEFAULT_PROFILE } from '@isorun/offline'
import { isLightsName, isQualityName, type CameraAngles, type LightsName, type QualityName, type SurfaceDetail } from '@isorun/render-iso'
import { AMBIENT_OCCLUSION, RESOLUTIONS, TORCH_SHADOWS, defaultSurfaceDetail, surfaceDetails, type AmbientOcclusion, type Resolution, type TorchShadows } from './graphics'

export interface Page {
  /** the tab's title; the name and the rest are set apart by a hyphen, as a game's tab is (gameTitle) */
  title: string
  /** the app's name, which a window of its own is titled with */
  name: string
}

export const HOME: Page = {
  name: 'Isorun',
  title: 'Isorun - Dungeon Crawl Stone Soup',
}

/** the front end's screens that belong to no server; a settings group is its name in lower case (settings-rows.ts) */
export const MENU_PATH = /^(settings(\/[a-z]+)?|settings\/controls\/gamepad|accounts(\/add(\/server)?)?|watch(\/add)?|whats-new|about|app(\/steam)?)$/

/** an account's Other versions, under Accounts: `/accounts/versions/cdi` */
export const VERSIONS_PATH = 'accounts/versions'

/** an account's rc file, under Accounts: `/accounts/rc/cdi/dcss-0.34`, and a player on this device's `/accounts/rc/offline` */
export const RC_PATH = 'accounts/rc'

export interface ServerInfo {
  id: string
  name: string
  region?: string
  ws: string
  http: string
  host: string
  custom?: boolean
  /** Games on this device (@isorun/offline): no socket, no proxies, gamedata from ENGINE_BASE. */
  offline?: boolean
}

/** Where the offline engine's files are served (engine/dist; the dev server's /engine route). */
export const ENGINE_BASE = '/engine'

/** The hosted page, whose Worker serves the proxies and counts visits for a shell that has neither (the Android app). */
const HOSTED_ORIGIN = 'https://isorun.maciej-rosiek.workers.dev'

/**
 * Whether this build is the Android app (mobile/, `VITE_PLATFORM=android`):
 * its files, the engine among them, are in the APK, so nothing is kept in
 * Cache Storage and no service worker runs; and it has no proxies of its
 * own, so they are the hosted page's.
 */
export function androidApp(): boolean {
  return import.meta.env.VITE_PLATFORM === 'android'
}

/** What the same-origin paths of the proxies and the visit beacon are prefixed with: the hosted page in the Android app. */
export function hostedBase(): string {
  return androidApp() ? HOSTED_ORIGIN : ''
}

/**
 * The server that is this device: the offline engine. Its accounts are
 * profiles, each with saves of its own (@isorun/offline `profileSaveDir`),
 * made with a name and no password; the server list offers it first when an
 * account is being added, and never for watching.
 */
export const OFFLINE_SERVER: ServerInfo = { id: 'offline', name: 'Offline', ws: '', http: '', host: 'this device', offline: true }

/**
 * Whether Offline is offered: everywhere but under test, where no engine is
 * served (the dev server serves a local engine/build.sh through
 * engine-files.ts, a deploy the published one through the ENGINE binding).
 * Read on each call, so a test can stub the environment.
 */
export function offlineOffered(): boolean {
  return import.meta.env.MODE !== 'test' && import.meta.env.VITE_OFFLINE !== 'off'
}

export function appDownloadsOffered(): boolean {
  return import.meta.env.VITE_OFFLINE === 'off'
}

/**
 * Whether this browser can run the offline engine: it needs JSPI, which the
 * engine blocks in while it waits for the player (Firefox 151 has none, and a
 * game there crashes at start). Without it Offline is still offered, and asks
 * for a newer browser where a game would start.
 */
export function offlineRuns(): boolean {
  return typeof (WebAssembly as { Suspending?: unknown }).Suspending === 'function'
}

const KEY = 'isorun.servers'
const TOKEN_KEY = 'isorun.tokens'
const LAST_KEY = 'isorun.last'
const GAMES_KEY = 'isorun.games'

function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw ? (JSON.parse(raw) as T) : fallback
  } catch {
    return fallback
  }
}

function save(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    /* ignore */
  }
}

export function listServers(): ServerInfo[] {
  const custom = load<ServerInfo[]>(KEY, [])
  return [...(bundled as ServerInfo[]), ...custom]
}

/**
 * Add a server by its host or the address of its page (any path is dropped:
 * the socket is always `/socket` at its root). A host already on the list
 * gives back the server listed there, bundled or not, rather than a second
 * row for it. Throws, in words for the player, on anything that is not an address.
 */
export function addServer(url: string): ServerInfo {
  const text = url.trim()
  if (!text) throw new Error('Enter a host or an address.')
  let u: URL
  try {
    u = new URL(text.includes('://') ? text : 'https://' + text)
  } catch {
    throw new Error(`${text} is not an address.`)
  }
  if (u.protocol !== 'http:' && u.protocol !== 'https:') throw new Error('A server is reached over http or https.')
  const host = u.host.toLowerCase()
  if (!host || /\s/.test(text) || !/[a-z0-9]/.test(host)) throw new Error(`${text} is not an address.`)
  const had = listServers().find((s) => !s.offline && s.host.toLowerCase() === host)
  if (had) return had
  const ws = (u.protocol === 'http:' ? 'ws://' : 'wss://') + host + '/socket'
  const info: ServerInfo = { id: host, name: host, ws, http: u.origin, host, custom: true }
  const custom = load<ServerInfo[]>(KEY, []).filter((s) => s.id !== info.id)
  custom.push(info)
  save(KEY, custom)
  return info
}

/** Take a server the player added off the list; the bundled ones stay. */
export function removeServer(id: string) {
  save(
    KEY,
    load<ServerInfo[]>(KEY, []).filter((s) => s.id !== id),
  )
}

// ------------------------------------------------------------------ accounts

/**
 * An account: a name on a server. The home screen lists them, one gateway
 * each; the player picks one to reach that server's lobby, or adds another
 * (a server from the list, then the lobby page's login or register). What
 * is kept for each is the server's login token (never the password), as
 * the official client keeps its login cookie, so the token login goes out
 * on the next connection (session.ts).
 *
 * The token's life follows the official client's (client.js `start_login`,
 * `set_login_cookie`): the server burns a token the moment it is used
 * (ws_handler.py `token_login` calls `forget_login_cookie` first), so it is
 * dropped here as it goes out and the fresh one `login_cookie` answers with
 * takes its place. The server also forgets tokens older than its
 * `login_token_lifetime` (7 days by default), which `login_cookie` reports
 * in `expires`; a token past that is dropped here without being tried.
 * Unlike the official client, which holds one server's token, this device
 * holds one per account, so nothing is kept a moment longer than it works.
 */
export interface Account {
  serverId: string
  username: string
}

const ACCOUNTS_KEY = 'isorun.accounts'
const ACCOUNT_KEY = 'isorun.account'
/** when each account was last picked (ms since the epoch), by tokenKey */
const USED_KEY = 'isorun.accountUsed'
/** older builds: one token per server, and the server chosen on its own */
const OLD_SERVER_KEY = 'isorun.server'

/** The key an account's token is filed under: the server, then the name (the server takes it in any case). */
function tokenKey(serverId: string, username: string): string {
  return serverId + '/' + username.toLowerCase()
}

export function sameAccount(a: Account | null | undefined, b: Account | null | undefined): boolean {
  return !!a && !!b && a.serverId === b.serverId && a.username.toLowerCase() === b.username.toLowerCase()
}

/**
 * Older builds kept one token per server (`isorun.tokens[serverId]`) and the
 * chosen server on its own; the name was in `isorun.last`. Those make one
 * account, chosen, and the token is filed under it. Runs once.
 */
function migrateAccounts() {
  if (localStorageHas(ACCOUNTS_KEY)) return
  const tokens = readTokens()
  const last = load<LastPlayed | null>(LAST_KEY, null)
  const accounts: Account[] = []
  for (const [serverId, token] of Object.entries(tokens)) {
    if (serverId.includes('/') || !findServer(serverId)) continue
    const username = last && last.serverId === serverId && last.username ? last.username : null
    delete tokens[serverId]
    if (!username) continue
    tokens[tokenKey(serverId, username)] = token
    accounts.push({ serverId, username })
  }
  save(TOKEN_KEY, tokens)
  save(ACCOUNTS_KEY, accounts)
  const chosenServer = load<string | null>(OLD_SERVER_KEY, null)
  const chosen = accounts.find((a) => a.serverId === chosenServer) ?? accounts[0] ?? null
  if (chosen) save(ACCOUNT_KEY, chosen)
}

function localStorageHas(key: string): boolean {
  try {
    return localStorage.getItem(key) !== null
  } catch {
    return false
  }
}

/**
 * The accounts on this device, the one picked last first, then the ones
 * never picked in the order they were added; offline profiles only where
 * Offline is offered.
 */
export function listAccounts(): Account[] {
  const stored = storedAccounts()
  const all = offlineOffered() ? stored : stored.filter((a) => a.serverId !== OFFLINE_SERVER.id)
  const used = load<Record<string, number>>(USED_KEY, {})
  const at = (a: Account) => used[tokenKey(a.serverId, a.username)] ?? 0
  return all.sort((a, b) => at(b) - at(a))
}

function storedAccounts(): Account[] {
  migrateAccounts()
  return load<Account[]>(ACCOUNTS_KEY, []).filter((a) => a && a.serverId && a.username)
}

/** Add `a` (or spell its name as the server does, if it is already there). */
export function addAccount(a: Account) {
  const all = storedAccounts().filter((x) => !sameAccount(x, a))
  all.push({ serverId: a.serverId, username: a.username })
  save(ACCOUNTS_KEY, all)
}

/**
 * A device with no account yet gets a player of its own, chosen, so the
 * first Play starts a game with nothing to set up. Not where the engine
 * cannot run: there Play still asks for an account on a server.
 */
export function ensureDeviceAccount() {
  if (!offlineOffered() || !offlineRuns() || storedAccounts().length) return
  const player: Account = { serverId: OFFLINE_SERVER.id, username: DEFAULT_PROFILE }
  addAccount(player)
  setChosenAccount(player)
}

/**
 * Forget `a`: the account and its token. If it was the chosen one, the one
 * used most recently of those left is chosen in its place (without counting
 * as a use), so there is never an account list with none of it chosen.
 */
export function removeAccount(a: Account) {
  save(
    ACCOUNTS_KEY,
    storedAccounts().filter((x) => !sameAccount(x, a)),
  )
  setToken(a.serverId, a.username, null)
  const used = load<Record<string, number>>(USED_KEY, {})
  delete used[tokenKey(a.serverId, a.username)]
  save(USED_KEY, used)
  if (sameAccount(getChosenAccount(), a)) save(ACCOUNT_KEY, listAccounts()[0] ?? null)
}

/** The account last picked on the home screen: the one the `#lobby` (Watch screen), play and watch routes mean. */
export function getChosenAccount(): Account | null {
  migrateAccounts()
  const a = load<Account | null>(ACCOUNT_KEY, null)
  return a && a.serverId && a.username && findServer(a.serverId) ? a : null
}

/** Choose `a`, and mark it used now: listAccounts puts it first. */
export function setChosenAccount(a: Account | null, now = Date.now()) {
  save(ACCOUNT_KEY, a ? { serverId: a.serverId, username: a.username } : null)
  if (!a) return
  const used = load<Record<string, number>>(USED_KEY, {})
  used[tokenKey(a.serverId, a.username)] = now
  save(USED_KEY, used)
}

/**
 * A stored token: the cookie string and when the server will have forgotten
 * it (ms since the epoch), or a bare string from a build that kept no expiry
 * (2026-09-18), read as a token that has not expired.
 */
type StoredToken = string | { token: string; until?: number }

function readTokens(): Record<string, StoredToken> {
  const t = load<unknown>(TOKEN_KEY, {})
  return t && typeof t === 'object' && !Array.isArray(t) ? (t as Record<string, StoredToken>) : {}
}

const DAY_MS = 24 * 60 * 60 * 1000

/** The account's token, or null when there is none or the server will have forgotten it (it is dropped then). */
export function getToken(serverId: string, username: string, now = Date.now()): string | null {
  const all = readTokens()
  const key = tokenKey(serverId, username)
  const v = all[key]
  if (typeof v === 'string') return v || null
  if (!v || typeof v !== 'object' || typeof v.token !== 'string' || !v.token) return null
  if (typeof v.until === 'number' && now >= v.until) {
    delete all[key]
    save(TOKEN_KEY, all)
    return null
  }
  return v.token
}

/**
 * File `token` under the account, or forget it (`null`). `expiresDays` is the
 * `login_cookie` message's `expires`: how many days the server keeps the token
 * (ws_handler.py `set_login_cookie` sends `login_token_lifetime`).
 */
export function setToken(serverId: string, username: string, token: string | null, expiresDays?: number, now = Date.now()) {
  const all = readTokens()
  const key = tokenKey(serverId, username)
  if (token) {
    const until = typeof expiresDays === 'number' && isFinite(expiresDays) && expiresDays > 0 ? now + expiresDays * DAY_MS : undefined
    all[key] = until === undefined ? { token } : { token, until }
  } else delete all[key]
  save(TOKEN_KEY, all)
}

/**
 * Where an account stands in its server's lobby: `in` (the lobby said who),
 * `pending` (a token is stored and its login not yet answered; a refused one
 * is dropped, so this cannot last), or `out` (nobody, and nothing to try).
 * The server sends its "Play now" lines only once logged in (ws_handler.py
 * `send_lobby_html`: `if not self.username: return`), so `out` is when the
 * lobby screen offers Log in instead of Play.
 */
export type LoginState = 'in' | 'pending' | 'out'

export function loginState(account: Account | null, loggedIn: string | null | undefined, loggingIn = false): LoginState {
  if (loggedIn) return 'in'
  // its token already sent (and so forgotten), its answer still to come
  if (loggingIn) return 'pending'
  // an offline profile's token is its name (session.ts), never stored and never refused
  if (account && (account.serverId === OFFLINE_SERVER.id || getToken(account.serverId, account.username))) return 'pending'
  return 'out'
}

/**
 * A character as a roster line or a lobby's save info describes them: the
 * stats screen's first lines (grid/stats.ts), as the home screen's Continue
 * and the browser tab speak them.
 */
export interface LastCharacter {
  name: string
  /** "the Severer", or ", the ..." when the title carries its own comma (stats.ts) */
  title: string
  species: string
  /** "Vehumet"; '' for a character without a god (the `player` message's `god`, the roster's) */
  god?: string
  xl: number
  /** "Dungeon", "Snake Pit"; `depth` 0 for a branch without one */
  place: string
  depth: number
}

export interface LastPlayed {
  serverId: string
  gameId?: string
  username?: string
  /**
   * The gamedata version (a sha1) the last game on this server ran on, from its `game_client`: what the
   * next visit warms the cache with before Play is pressed (main.ts `warmGamedata`). Only ever learned
   * in play, so a version can lag behind a server upgrade by one game; the warm-up then fetches a set
   * that is still served, and the new one loads as it always has.
   */
  gamedataVersion?: string
}

/**
 * The character a lobby roster line describes (`lobby_entry`: the game a
 * player has open on the server), as the home screen's Continue speaks one:
 * the title without its "the", the species and background as the roster's
 * four letters ("VSIE"), the place as the HUD abbreviates it ("D:2"). The
 * account's own line is a game of theirs still open on the server, left
 * through a dropped socket or in another tab.
 *
 * Everything here comes from the line itself, nothing from what this device
 * saw last: the same account can be played from anywhere, so a remembered
 * character may already be dead and would contradict the live line.
 */
export function characterOf(e: { username: string; char?: string; xl?: string; place?: string; title?: string; god?: string }): LastCharacter {
  const colon = e.place?.lastIndexOf(':') ?? -1
  const depth = colon > 0 ? Number(e.place!.slice(colon + 1)) : 0
  const place = colon > 0 && depth ? e.place!.slice(0, colon) : e.place ?? ''
  return {
    name: e.username,
    title: e.title ? 'the ' + e.title : '',
    species: e.char ?? '',
    god: e.god || '',
    xl: Number(e.xl) || 0,
    place,
    depth: depth || 0,
  }
}

/**
 * "Dwarfsong the Severer, Mountain Dwarf XL12": the character in a line, as
 * the lobby roster speaks one. Where they stand is a line of its own
 * (describePlace): the two together run past a handheld's width.
 */
export function describeCharacter(c: LastCharacter): string {
  const who = c.name + (c.title ? ((c.title[0] === ',' ? '' : ' ') + c.title) : '')
  const what = [c.species, c.xl ? `XL${c.xl}` : ''].filter(Boolean).join(' ')
  return [who, what].filter(Boolean).join(', ')
}

/** "Snake Pit:2", "Zot": where the character stands, as the HUD's place line says it; '' before the game has said. */
export function describePlace(c: LastCharacter): string {
  return c.place ? c.place + (c.depth ? ':' + c.depth : '') : ''
}

/**
 * "isorun the Chiller | Vine Stalker of Vehumet - Isorun": the browser tab's
 * title while in a game, so a tab (or a window in a taskbar) says whose game
 * it is. The species and god read as the HUD's second line does, and the site
 * follows a hyphen, as the home page's title has it (HOME).
 * `base` is the site's name.
 */
export function gameTitle(c: LastCharacter, base: string): string {
  const who = c.name + (c.title ? ((c.title[0] === ',' ? '' : ' ') + c.title) : '')
  const what = c.species + (c.god ? (c.species ? ' of ' : 'of ') + c.god : '')
  const game = [who, what].filter(Boolean).join(' | ')
  return [game, base].filter(Boolean).join(' - ')
}

const MORGUE_KEY = 'isorun.morgue'

/**
 * Where a server keeps an account's morgue directory ("/crawl/morgue/orbrun/"), once something has said: a
 * `game_ended` `dump` names the real one, and before that the home screen tries the layouts
 * dgamelaunch-config ships with (whereis.ts `morgueDirGuesses`) and keeps whichever answered. '' is a
 * server that answered none of them, kept so the tries are not made again on every redraw.
 */
export function getMorgueDir(serverId: string, username: string): string | null {
  const all = load<Record<string, string>>(MORGUE_KEY, {})
  return all[tokenKey(serverId, username)] ?? null
}

export function setMorgueDir(serverId: string, username: string, dir: string) {
  const all = load<Record<string, string>>(MORGUE_KEY, {})
  all[tokenKey(serverId, username)] = dir
  save(MORGUE_KEY, all)
}

export function getLast(): LastPlayed | null {
  return load<LastPlayed | null>(LAST_KEY, null)
}

export function setLast(l: LastPlayed) {
  save(LAST_KEY, l)
}

/** A game the server's lobby offers, as `set_game_links` lists it, with the save it reported waiting there. */
export type CachedGame = GameLink

/**
 * The game list a server's lobby sent last time, kept per server so the
 * home screen can offer "Play 0.34" and "Play trunk" before the connection
 * is up. The live list replaces it as soon as the lobby sends one.
 */
export function getGames(serverId: string): CachedGame[] | null {
  return load<Record<string, CachedGame[]>>(GAMES_KEY, {})[serverId] ?? null
}

export function setGames(serverId: string, games: CachedGame[]) {
  const all = load<Record<string, CachedGame[]>>(GAMES_KEY, {})
  all[serverId] = games.map((g) => {
    const c: CachedGame = { id: g.id, label: g.label }
    if (g.save !== undefined) c.save = g.save
    if (g.disabled) c.disabled = true
    if (g.rc) c.rc = g.rc
    return c
  })
  save(GAMES_KEY, all)
}

/** Where gamedata is fetched from for a server, in this shell. */
export function gamedataBaseFor(server: ServerInfo): string {
  if (server.offline) return ENGINE_BASE
  return `${hostedBase()}/gamedata-proxy/${server.host}`
}

/**
 * Where a game's morgue file or character dump is read from, in this shell:
 * `game_ended` hands the file's URL less its `.txt`, absolute or as a path
 * on the server, and the server publishes it without CORS headers, so it is
 * read through the same-origin proxy. Null when the URL is not one the
 * proxy can carry (another scheme, a host the server picker would refuse).
 */
export function morgueUrlFor(server: ServerInfo, dump: string): string | null {
  if (server.offline) return null
  let u: URL
  try {
    u = new URL(dump + '.txt', server.http)
  } catch {
    return null
  }
  if (u.protocol !== 'https:' && u.protocol !== 'http:') return null
  return `${hostedBase()}/morgue-proxy/${u.host}${u.pathname}`
}

/**
 * Where a direction came from: the three keyboard sets keys.ts `directionKey`
 * tells apart, and the pad's two direction sources (gamepad.ts `PadEvent`).
 */
export type DirSource = 'arrows' | 'vim' | 'numpad' | 'dpad' | 'lstick'

export interface Settings {
  uiScale: number
  confirmStairs: boolean
  /** How many tiles the minimap shows across (hud.md "Minimap"); 19 tiles reads at a glance and reaches a little past the reference layout's sidebar column. */
  minimapTiles: number
  /** How big a minimap tile is drawn, in css px (hud.md "Minimap"); 20 is the cell the map follows the player at. */
  minimapCell: number
  /** The gamepad prompts in the corner of the view (gamepad-hints.ts `HintMode`). */
  hints: HintMode
  /** How much work the dungeon view does a frame (render-iso `QUALITY`). */
  graphics: QualityName
  /** The graphics settings that take the Quality preset's value while Auto (graphics.ts `qualityFor`). */
  resolution: Resolution
  ambientOcclusion: AmbientOcclusion
  antialias: 'auto' | 'off' | 'on'
  /** The tiles' surface effects (render-iso `applySurfaceDetail`); a phone starts at Off and stops at Medium (graphics.ts `surfaceDetails`). */
  surfaceDetail: SurfaceDetail
  torchShadows: TorchShadows
  /** The dungeon's lights, colours and screen effects (render-iso `LIGHTS`). */
  look: LightsName | 'custom'
  /** How fast the player and the monsters walk and fight on the dungeon view, 1 for the base speed. */
  animationSpeed: number
  /** The arrow keys: aim with an arrow on the ground and step with Space or Enter, or step at once. */
  arrowKeys: 'aim' | 'step'
  /** The pad's left stick: step at once, or aim with an arrow on the ground and step with A. */
  leftStick: 'aim' | 'step'
  /** LT: a tap explores with every hop shown and a hold explores at once, or a hold explores with every hop shown until let go and a tap explores at once. */
  exploreTrigger: 'tap' | 'hold'
  /** Travel and explore: paced so the player hops every cell on the screen, or as fast as the server goes. */
  travel: 'paced' | 'fast'
  /** The loudness of everything, the sound effects, the ambience of the dungeon and the music, each 0 to 100 (audio/). */
  masterVolume: number
  effectsVolume: number
  ambientVolume: number
  musicVolume: number
  /** Silences everything and keeps the volumes. */
  muted: boolean
  /** The HUD: the dock along the foot of the screen (dock.ts), or the WebTiles panes as the official client lays them out. */
  hud: 'dock' | 'classic'
  /** Numbers that pop over the actors as blows land (damage-numbers.ts). */
  damageNumbers: boolean
}

const HINT_MODES: readonly string[] = ['adaptive', 'contextual', 'off']

/**
 * What a saved settings object says about hints, in the one field or in the
 * two it used to be split into: `gamepadHints` (adaptive/contextual/off) and
 * `keyHints` (a boolean), merged 2026-09-09. An old pad mode carries over; a
 * keyboard opt-out alone was the older global opt-out and still means off.
 */
function hintsFrom(saved: Partial<Settings> & { gamepadHints?: unknown; keyHints?: unknown }): HintMode {
  for (const v of [saved.hints, saved.gamepadHints]) if (typeof v === 'string' && HINT_MODES.includes(v)) return v as HintMode
  if (saved.keyHints === false) return 'off'
  return 'adaptive'
}

const SETTINGS_KEY = 'isorun.settings'
export const defaultSettings: Settings = {
  uiScale: 1,
  confirmStairs: false,
  minimapTiles: 19,
  minimapCell: 20,
  hints: 'adaptive',
  graphics: 'medium',
  surfaceDetail: 'medium',
  resolution: 'auto',
  ambientOcclusion: 'auto',
  antialias: 'auto',
  torchShadows: 'auto',
  look: 'diorama',
  animationSpeed: 1.5,
  arrowKeys: 'aim',
  leftStick: 'aim',
  exploreTrigger: 'hold',
  travel: 'paced',
  masterVolume: 70,
  effectsVolume: 100,
  ambientVolume: 70,
  musicVolume: 50,
  muted: false,
  hud: 'dock',
  damageNumbers: true,
}

export function getSettings(): Settings {
  const saved = load<Partial<Settings>>(SETTINGS_KEY, {})
  const s = { ...defaultSettings, ...saved }
  s.hints = hintsFrom(saved)
  if (!isQualityName(s.graphics)) s.graphics = defaultSettings.graphics
  if (!RESOLUTIONS.includes(s.resolution)) s.resolution = defaultSettings.resolution
  if (s.ambientOcclusion !== 'auto' && !(s.ambientOcclusion in AMBIENT_OCCLUSION)) s.ambientOcclusion = defaultSettings.ambientOcclusion
  if (!['auto', 'off', 'on'].includes(s.antialias)) s.antialias = defaultSettings.antialias
  if (saved.surfaceDetail === undefined) s.surfaceDetail = defaultSurfaceDetail()
  if (!surfaceDetails().includes(s.surfaceDetail)) s.surfaceDetail = defaultSettings.surfaceDetail
  if (!TORCH_SHADOWS.includes(s.torchShadows)) s.torchShadows = defaultSettings.torchShadows
  if (s.look !== 'custom' && !isLightsName(s.look)) s.look = defaultSettings.look
  if (s.hud !== 'dock' && s.hud !== 'classic') s.hud = defaultSettings.hud
  return s
}

/**
 * Only what the player changed is written down: a setting still at its
 * default is left out of storage, so a default we move later moves for
 * everyone who never touched that row.
 */
export function saveSettings(s: Settings) {
  const out: Partial<Settings> = {}
  for (const key of Object.keys(defaultSettings) as (keyof Settings)[]) {
    if (s[key] !== defaultSettings[key]) (out as Record<string, unknown>)[key] = s[key]
  }
  save(SETTINGS_KEY, out)
}

const CAMERA_KEY = 'isorun.camera'

/** How the camera looked when the game was last left: its angles and its zoom. */
export interface SavedView {
  angles: CameraAngles
  /** Missing in what an older version saved. */
  zoom?: number
}

/** The camera's angles and zoom when the game was last left. */
export function getSavedView(): SavedView | null {
  const v = load<(Partial<CameraAngles> & { zoom?: unknown }) | null>(CAMERA_KEY, null)
  if (!v || typeof v.rotation !== 'number' || typeof v.tilt !== 'number' || !isFinite(v.rotation) || !isFinite(v.tilt)) return null
  const angles = { rotation: v.rotation, tilt: v.tilt, perspective: typeof v.perspective === 'number' ? v.perspective : 0 }
  return typeof v.zoom === 'number' && isFinite(v.zoom) && v.zoom > 0 ? { angles, zoom: v.zoom } : { angles }
}

export function saveView(view: SavedView) {
  save(CAMERA_KEY, { ...view.angles, zoom: view.zoom })
}

// ------------------------------------------------------------------- servers

/** A server by id or host, from the bundled list or the custom ones, or this device's. */
export function findServer(idOrHost: string): ServerInfo | null {
  if (offlineOffered() && idOrHost === OFFLINE_SERVER.id) return OFFLINE_SERVER
  return listServers().find((s) => s.id === idOrHost || s.host === idOrHost) ?? null
}

// --------------------------------------------------------------------- route

/**
 * What the URL says we should be doing. The path names it, the verb first,
 * then the server, then the game: `/play/cdi/dcss-0.34`, `/watch/cdi` (the
 * Watch screen: the server's lobby roster, the official client's `#lobby`),
 * `/login/cdi`, so a link opens the same thing on another device. Who is
 * after the `#`: `/play/cdi/dcss-0.34#isorun`, `/watch/cdi#bob`,
 * `/login/cdi#isorun`. The fragment never leaves the browser, so nothing
 * that reads the address names anybody, and a copied link still does. Watching needs no account, so its address names none: it is the
 * chosen account's when that is on the same server. Games on this device are
 * named as CDI names its own (`/play/offline/dcss-0.34#Marc`, `dcss-git` for
 * trunk), so a game reads the same played here or there. The front end's own
 * screens have addresses too (`/settings/camera`, `/accounts/add`); the bare
 * root is the home screen.
 * (`watch`'s `username` is the player being watched, not the account's own.)
 */
export type Route =
  | { kind: 'home' }
  | MenuRoute
  | { kind: 'lobby'; serverId: string; account: Account | null }
  | { kind: 'play'; account: Account; gameId: string }
  | { kind: 'watch'; serverId: string; account: Account | null; username: string }

/**
 * A front-end screen: `path` is one of MENU_PATH's, or `login`/`register`
 * on `serverId` (as `username`, when the address names one), or
 * `accounts/versions`, an account's Other versions (`username` always named),
 * or `accounts/rc`, an account's rc file: the one `gameId` reads on a server,
 * the one every game reads on this device (no `gameId`).
 */
export interface MenuRoute {
  kind: 'menu'
  path: string
  serverId?: string
  username?: string
  gameId?: string
}

/** what every game id on this device starts with (@isorun/offline channelOf), which its address says as CDI would */
const DEVICE_GAME = 'offline-'

/** a game on this device in an address: `offline-0.34` is `dcss-0.34`, `offline-trunk` CDI's `dcss-git` */
function deviceGameAddress(gameId: string): string {
  const slot = gameId.startsWith(DEVICE_GAME) ? gameId.slice(DEVICE_GAME.length) : gameId
  return 'dcss-' + (slot === 'trunk' ? 'git' : slot)
}

/** the game on this device an address names; the bare slot (`0.34`, `trunk`) is how older addresses said it */
function deviceGameAt(game: string): string {
  const slot = game.toLowerCase().replace(/^dcss-/, '')
  return DEVICE_GAME + (slot === 'git' ? 'trunk' : slot)
}

function decode(s: string): string {
  try {
    return decodeURIComponent(s)
  } catch {
    return s
  }
}

/** `name` among the accounts on this device on `server`, spelled as it was added */
function knownAccount(serverId: string, name: string): Account | null {
  return listAccounts().find((a) => sameAccount(a, { serverId, username: name })) ?? null
}

/** the chosen account, when it is on `serverId`: who a watch address, which names nobody, watches as */
function chosenOn(serverId: string): Account | null {
  const chosen = getChosenAccount()
  return chosen?.serverId === serverId ? chosen : null
}

/** the only account on `serverId`, when there is just the one: who a play address that names nobody is for */
function onlyAccountOn(serverId: string): Account | null {
  const on = listAccounts().filter((a) => a.serverId === serverId)
  return on.length === 1 ? on[0] : null
}

export function parseRoute(href: string = window.location.href): Route {
  const { pathname, hash } = new URL(href)
  // split before decoding, so a `/` in a name (`%2F`) stays in it; an empty segment (`//`, a trailing `/`) says nothing
  const parts = pathname.split('/').filter(Boolean).map(decode)
  if (!parts.length) return { kind: 'home' }
  const menu = parts.join('/').toLowerCase()
  if (MENU_PATH.test(menu)) return { kind: 'menu', path: menu }
  // an account's Other versions (`/accounts/versions/cdi#isorun`): who, as a play address says it; there are no
  // versions to pick on this device
  if (parts.length === 3 && parts.slice(0, 2).join('/').toLowerCase() === VERSIONS_PATH) {
    const server = findServer(parts[2].toLowerCase())
    if (!server || server.offline) return { kind: 'home' }
    const named = hash.length > 1 ? decode(hash.slice(1)) : null
    const account = named ? knownAccount(server.id, named) : (chosenOn(server.id) ?? onlyAccountOn(server.id))
    if (!account) return { kind: 'menu', path: 'login', serverId: server.id, ...(named ? { username: named } : {}) }
    return { kind: 'menu', path: VERSIONS_PATH, serverId: server.id, username: account.username }
  }
  // an account's rc file (`/accounts/rc/cdi/dcss-0.34#isorun`): the game names the file on a server; a player on
  // this device has one for every game (`/accounts/rc/offline#Marc`)
  if ((parts.length === 3 || parts.length === 4) && parts.slice(0, 2).join('/').toLowerCase() === RC_PATH) {
    const server = findServer(parts[2].toLowerCase())
    if (!server || (parts.length === 4) === !!server.offline) return { kind: 'home' }
    const named = hash.length > 1 ? decode(hash.slice(1)) : null
    const account = named ? knownAccount(server.id, named) : (chosenOn(server.id) ?? onlyAccountOn(server.id))
    if (!account) return { kind: 'menu', path: 'login', serverId: server.id, ...(named ? { username: named } : {}) }
    return { kind: 'menu', path: RC_PATH, serverId: server.id, username: account.username, ...(server.offline ? {} : { gameId: parts[3] }) }
  }
  const [verb, where, ...rest] = parts
  const server = where === undefined ? null : findServer(where.toLowerCase())
  if (!server) return { kind: 'home' }
  const serverId = server.id
  const what = verb.toLowerCase()
  // who: after the `#`; addresses from before that said it in the path (`/play/cdi/isorun/dcss-0.34`, `/watch/cdi/bob`)
  const tagged = hash.length > 1 ? decode(hash.slice(1)) : null
  if ((what === 'login' || what === 'register') && rest.length <= 1) {
    const named = rest[0] ?? tagged
    return { kind: 'menu', path: what, serverId, ...(named ? { username: knownAccount(serverId, named)?.username ?? named } : {}) }
  }
  if (what === 'play' && (rest.length === 1 || rest.length === 2)) {
    const game = rest[rest.length - 1]
    const named = rest.length === 2 ? rest[0] : tagged
    const account = named ? knownAccount(serverId, named) : (chosenOn(serverId) ?? onlyAccountOn(serverId))
    // an account this device does not have, or no telling which: its login, to add it or pick
    if (!account) return { kind: 'menu', path: 'login', serverId, ...(named ? { username: named } : {}) }
    return { kind: 'play', account, gameId: server.offline ? deviceGameAt(game) : game }
  }
  // nobody else plays on this device
  if (what !== 'watch' || server.offline || rest.length > 1) return { kind: 'home' }
  const watched = rest[0] ?? tagged
  if (!watched) return { kind: 'lobby', serverId, account: chosenOn(serverId) }
  return { kind: 'watch', serverId, account: chosenOn(serverId), username: watched }
}

/** a path of names, each encoded whole: a space, `/`, `?`, `#` or `%` in one stays in it */
function pathOf(...parts: string[]): string {
  return '/' + parts.map(encodeURIComponent).join('/')
}

/** who, after the `#`, encoded whole as a path's names are */
function tagOf(name: string | undefined): string {
  return name ? '#' + encodeURIComponent(name) : ''
}

/**
 * The address for a route. The query is carried through unchanged: the flags
 * that live there (`?perf`, `?fullscreen`) are set once when the page is
 * opened — by a Steam shortcut's launch options, say — and a device with no
 * address bar could not put one back, so a Play must not drop them.
 */
export function formatRoute(r: Route): string {
  const q = window.location.search
  if (r.kind === 'home') return '/' + q
  if (r.kind === 'menu') return (r.serverId ? pathOf(...r.path.split('/'), r.serverId, ...(r.gameId ? [r.gameId] : [])) : '/' + r.path) + q + tagOf(r.serverId ? r.username : undefined)
  if (r.kind === 'lobby') return pathOf('watch', r.serverId) + q
  if (r.kind === 'watch') return pathOf('watch', r.serverId) + q + tagOf(r.username)
  const { serverId, username } = r.account
  return pathOf('play', serverId, serverId === OFFLINE_SERVER.id ? deviceGameAddress(r.gameId) : r.gameId) + q + tagOf(username)
}

/**
 * How deep in the front end a route is: the home screen, or anything else
 * (a menu, the Watch screen, a game, a spectate). Leaving home leaves one history
 * entry behind, so Back is always the way out to the home screen; moving
 * between the deeper screens, or coming back up, rewrites that entry in place.
 */
function routeDepth(r: Route): number {
  return r.kind === 'home' ? 0 : 1
}

/**
 * Put the route in the address bar. Neither push nor replace fires
 * `popstate`, so this never comes back through `applyRoute`; only the
 * player's own Back, Forward or edit does.
 *
 * In a window of our own there is no Back to serve, and a second history
 * entry costs the way out: Chrome refuses `window.close()` on a window a
 * script did not open once the tab has more than one entry, so a Deck that
 * had played a game could no longer Quit (quit.ts). There the address is
 * only ever rewritten in place.
 */
export function setRoute(r: Route) {
  const next = formatRoute(r)
  const cur = window.location.pathname + window.location.search + window.location.hash
  if (cur === next) return
  if (!canQuit() && routeDepth(r) > routeDepth(parseRoute())) history.pushState(null, '', next)
  else history.replaceState(null, '', next)
  // a game names the tab after its character (main.ts updateTitle); every other screen after its page
  if (r.kind !== 'play' && r.kind !== 'watch') document.title = screenTitle()
}

/**
 * The title of a screen that is not a game. In a window of its own (an
 * installed app, a kiosk) the title bar names the app and nothing more; in a
 * tab it is the home page's headline.
 */
export function screenTitle(): string {
  return canQuit() ? HOME.name : HOME.title
}
