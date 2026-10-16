// @vitest-environment happy-dom
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { addServer, formatRoute, parseRoute, setChosenAccount, setRoute, type Account, type Route } from '../src/servers'

const base = 'http://localhost:5173/'

// happy-dom's localStorage has no working methods; give servers.ts a plain one
const store = new Map<string, string>()
Object.defineProperty(globalThis, 'localStorage', {
  configurable: true,
  value: {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, String(v)),
    removeItem: (k: string) => void store.delete(k),
  },
})

/**
 * The path says what and where, the fragment who: `/play/cdi/dcss-0.34#isorun`,
 * `/watch/cdi#bob`, so a link opens the same thing on another device while
 * the page view the site counts names nobody; the front end's screens have
 * addresses of their own, and the root is the home screen.
 */
describe('routes', () => {
  const isorun: Account = { serverId: 'cdi', username: 'isorun' }
  const someone: Account = { serverId: 'cko', username: 'someone' }
  beforeEach(() => {
    store.clear()
    // an account list, so the chosen one is one of them
    store.set('isorun.accounts', JSON.stringify([isorun, someone]))
    setChosenAccount(null)
  })

  it('the root is home, whatever the query says', () => {
    expect(parseRoute(base)).toEqual({ kind: 'home' })
    expect(parseRoute(base + '?server=crawl.dcss.io')).toEqual({ kind: 'home' })
  })

  it('watches on a server with no account at all, so a link to a game opens anywhere', () => {
    expect(parseRoute(base + 'watch/cdi')).toEqual({ kind: 'lobby', serverId: 'cdi', account: null })
    expect(parseRoute(base + 'watch/cdi#bob')).toEqual({ kind: 'watch', serverId: 'cdi', account: null, username: 'bob' })
    expect(parseRoute(base + 'watch/cdi#')).toEqual({ kind: 'lobby', serverId: 'cdi', account: null })
    // the address from before, with the name in the path, still opens the spectate
    expect(parseRoute(base + 'watch/cdi/bob')).toEqual({ kind: 'watch', serverId: 'cdi', account: null, username: 'bob' })
    // by host as well as by id, in any case
    expect(parseRoute(base + 'watch/crawl.dcss.io#bob')).toEqual({ kind: 'watch', serverId: 'cdi', account: null, username: 'bob' })
    expect(parseRoute(base + 'Watch/CDI#bob')).toEqual({ kind: 'watch', serverId: 'cdi', account: null, username: 'bob' })
    // as the chosen account, when it is on that server
    setChosenAccount(isorun)
    expect(parseRoute(base + 'watch/cdi#bob')).toEqual({ kind: 'watch', serverId: 'cdi', account: isorun, username: 'bob' })
    expect(parseRoute(base + 'watch/cko')).toEqual({ kind: 'lobby', serverId: 'cko', account: null })
  })

  it('plays as the account it names, or asks for a login there', () => {
    expect(parseRoute(base + 'play/cdi/dcss-0.34#isorun')).toEqual({ kind: 'play', account: isorun, gameId: 'dcss-0.34' })
    expect(parseRoute(base + 'play/cdi/dcss-0.34#ISORUN')).toEqual({ kind: 'play', account: isorun, gameId: 'dcss-0.34' })
    expect(parseRoute(base + 'play/cdi/dcss-0.34#stranger')).toEqual({ kind: 'menu', path: 'login', serverId: 'cdi', username: 'stranger' })
    // the address from before, with the name in the path, still opens the game
    expect(parseRoute(base + 'play/cdi/isorun/dcss-0.34')).toEqual({ kind: 'play', account: isorun, gameId: 'dcss-0.34' })
    expect(parseRoute(base + 'play/cdi/stranger/dcss-0.34')).toEqual({ kind: 'menu', path: 'login', serverId: 'cdi', username: 'stranger' })
  })

  it('plays an address that names nobody as the chosen account, or the only one on that server', () => {
    // the only account on cdi
    expect(parseRoute(base + 'play/cdi/dcss-0.34')).toEqual({ kind: 'play', account: isorun, gameId: 'dcss-0.34' })
    const other: Account = { serverId: 'cdi', username: 'other' }
    store.set('isorun.accounts', JSON.stringify([isorun, other, someone]))
    // two on cdi and none chosen: no telling which, so the login, to pick
    expect(parseRoute(base + 'play/cdi/dcss-0.34')).toEqual({ kind: 'menu', path: 'login', serverId: 'cdi' })
    setChosenAccount(other)
    expect(parseRoute(base + 'play/cdi/dcss-0.34')).toEqual({ kind: 'play', account: other, gameId: 'dcss-0.34' })
    // the chosen account is on another server: not who a cdi game is for
    setChosenAccount(someone)
    expect(parseRoute(base + 'play/cdi/dcss-0.34')).toEqual({ kind: 'menu', path: 'login', serverId: 'cdi' })
  })

  it('gives the front end’s screens addresses of their own', () => {
    for (const path of ['settings', 'settings/camera', 'settings/controls/gamepad', 'accounts', 'accounts/add', 'accounts/add/server', 'watch', 'watch/add', 'whats-new', 'about']) {
      expect(parseRoute(base + path)).toEqual({ kind: 'menu', path })
      expect(parseRoute(base + path + '/')).toEqual({ kind: 'menu', path })
    }
    expect(parseRoute(base + 'Settings/Camera')).toEqual({ kind: 'menu', path: 'settings/camera' })
    expect(parseRoute(base + 'login/cdi')).toEqual({ kind: 'menu', path: 'login', serverId: 'cdi' })
    expect(parseRoute(base + 'register/cdi')).toEqual({ kind: 'menu', path: 'register', serverId: 'cdi' })
    expect(parseRoute(base + 'login/cdi#ISORUN')).toEqual({ kind: 'menu', path: 'login', serverId: 'cdi', username: 'isorun' })
    expect(parseRoute(base + 'login/cdi/ISORUN')).toEqual({ kind: 'menu', path: 'login', serverId: 'cdi', username: 'isorun' })
  })

  it('gives an account’s Other versions an address, named as a play address names who plays', () => {
    const versions = { kind: 'menu', path: 'accounts/versions', serverId: 'cdi', username: 'isorun' } as const
    expect(formatRoute(versions)).toBe('/accounts/versions/cdi#isorun')
    expect(parseRoute(base + 'accounts/versions/cdi#ISORUN')).toEqual(versions)
    expect(parseRoute(base + 'Accounts/Versions/CDI#isorun')).toEqual(versions)
    // the only account on cdi
    expect(parseRoute(base + 'accounts/versions/cdi')).toEqual(versions)
    // an account this device does not have: its login, to add it
    expect(parseRoute(base + 'accounts/versions/cdi#stranger')).toEqual({ kind: 'menu', path: 'login', serverId: 'cdi', username: 'stranger' })
    for (const path of ['versions/cdi', 'accounts/versions', 'accounts/versions/nowhere', 'accounts/versions/cdi/isorun']) expect(parseRoute(base + path), path).toEqual({ kind: 'home' })
  })

  it('gives an account’s rc file an address: the game that reads it on a server, none on this device', () => {
    const rc = { kind: 'menu', path: 'accounts/rc', serverId: 'cdi', username: 'isorun', gameId: 'dcss-0.34' } as const
    expect(formatRoute(rc)).toBe('/accounts/rc/cdi/dcss-0.34#isorun')
    expect(parseRoute(base + 'accounts/rc/cdi/dcss-0.34#ISORUN')).toEqual(rc)
    expect(parseRoute(base + 'Accounts/RC/CDI/dcss-0.34#isorun')).toEqual(rc)
    // the only account on cdi
    expect(parseRoute(base + 'accounts/rc/cdi/dcss-0.34')).toEqual(rc)
    expect(parseRoute(base + 'accounts/rc/cdi/dcss-0.34#stranger')).toEqual({ kind: 'menu', path: 'login', serverId: 'cdi', username: 'stranger' })
    // a server's file is always some game's
    for (const path of ['rc/cdi', 'accounts/rc', 'accounts/rc/cdi', 'accounts/rc/nowhere/dcss-0.34', 'accounts/rc/cdi/dcss-0.34/more']) expect(parseRoute(base + path), path).toEqual({ kind: 'home' })
    // offline is not offered under test (servers.ts offlineOffered)
    vi.stubEnv('MODE', 'development')
    const marc: Account = { serverId: 'offline', username: 'Marc' }
    store.set('isorun.accounts', JSON.stringify([isorun, marc]))
    const device = { kind: 'menu', path: 'accounts/rc', serverId: 'offline', username: 'Marc' } as const
    expect(formatRoute(device)).toBe('/accounts/rc/offline#Marc')
    expect(parseRoute(base + 'accounts/rc/offline#marc')).toEqual(device)
    expect(parseRoute(base + 'accounts/rc/offline')).toEqual(device)
    // a player on this device has one file for every game: an address naming one is not the file's
    expect(parseRoute(base + 'accounts/rc/offline/dcss-0.34#Marc')).toEqual({ kind: 'home' })
    vi.unstubAllEnvs()
  })

  it('ignores paths it does not know', () => {
    for (const path of ['nowhere', 'play', 'play/cdi', 'play/cdi#isorun', 'play/cdi/isorun/dcss-0.34/more', 'watch/nowhere.example', 'watch/cdi/bob/more', 'lobby/cdi', 'about/new', 'login/cdi/isorun/more'])
      expect(parseRoute(base + path)).toEqual({ kind: 'home' })
  })

  it('names the games on this device as CDI names its own', () => {
    // offline is not offered under test (servers.ts offlineOffered)
    vi.stubEnv('MODE', 'development')
    const marc: Account = { serverId: 'offline', username: 'Marc' }
    store.set('isorun.accounts', JSON.stringify([isorun, marc]))
    const play: Route = { kind: 'play', account: marc, gameId: 'offline-0.34' }
    const trunk: Route = { ...play, gameId: 'offline-trunk' }
    expect(formatRoute(play)).toBe('/play/offline/dcss-0.34#Marc')
    expect(formatRoute(trunk)).toBe('/play/offline/dcss-git#Marc')
    expect(parseRoute(base + 'play/offline/dcss-0.34#marc')).toEqual(play)
    expect(parseRoute(base + 'play/offline/dcss-git#Marc')).toEqual(trunk)
    // the only player on this device
    expect(parseRoute(base + 'play/offline/dcss-0.34')).toEqual(play)
    // addresses from before: the name in the path, the bare slot
    expect(parseRoute(base + 'play/offline/marc/0.34')).toEqual(play)
    expect(parseRoute(base + 'play/offline/Marc/trunk')).toEqual(trunk)
    expect(formatRoute({ kind: 'menu', path: 'login', serverId: 'offline' })).toBe('/login/offline')
    // nobody else plays on this device
    expect(parseRoute(base + 'watch/offline')).toEqual({ kind: 'home' })
    expect(parseRoute(base + 'watch/offline#bob')).toEqual({ kind: 'home' })
    expect(parseRoute(base + 'watch/offline/bob')).toEqual({ kind: 'home' })
    vi.unstubAllEnvs()
  })

  it('round-trips through formatRoute, encoding each name whole', () => {
    expect(formatRoute({ kind: 'play', account: isorun, gameId: 'seeded-web-trunk' })).toBe('/play/cdi/seeded-web-trunk#isorun')
    expect(formatRoute({ kind: 'watch', serverId: 'cdi', account: null, username: 'a b' })).toBe('/watch/cdi#a%20b')
    expect(formatRoute({ kind: 'menu', path: 'login', serverId: 'cdi', username: 'isorun' })).toBe('/login/cdi#isorun')
    expect(formatRoute({ kind: 'lobby', serverId: 'cko', account: someone })).toBe('/watch/cko')
    setChosenAccount(isorun)
    // names with what a path or a URL would otherwise take apart: spaces, slashes, a query, a fragment, escapes, pluses
    const odd = ['a b', 'a/b', 'a?b', 'a#b', '100%', 'a+b', 'a%20b', 'ünï cödé', 'a@b', 'a.b', '..b']
    const custom: Account = { serverId: addServer('http://crawl.example.org:8080').id, username: 'a b/c' }
    store.set('isorun.accounts', JSON.stringify([isorun, someone, ...odd.map((username) => ({ serverId: 'cdi', username })), custom]))
    const routes: Route[] = [
      { kind: 'play', account: isorun, gameId: 'seeded-web-trunk' },
      { kind: 'watch', serverId: 'cdi', account: isorun, username: 'someone' },
      { kind: 'lobby', serverId: 'cko', account: null },
      { kind: 'lobby', serverId: 'cdi', account: isorun },
      { kind: 'menu', path: 'settings/interface' },
      { kind: 'menu', path: 'login', serverId: 'cko', username: 'someone' },
      { kind: 'play', account: custom, gameId: 'dcss-0.34' },
      { kind: 'watch', serverId: custom.serverId, account: null, username: 'bob' },
      ...odd.flatMap((name): Route[] => [
        { kind: 'play', account: { serverId: 'cdi', username: name }, gameId: name },
        { kind: 'play', account: { serverId: 'cdi', username: name }, gameId: 'dcss-0.34' },
        { kind: 'watch', serverId: 'cdi', account: isorun, username: name },
        { kind: 'menu', path: 'login', serverId: 'cdi', username: name },
      ]),
    ]
    for (const r of routes) expect(parseRoute(new URL(formatRoute(r), base).href)).toEqual(r)
    expect(formatRoute({ kind: 'home' })).toBe('/')
  })

  /** the address without its fragment is the path alone: nobody is named in it */
  it('keeps every name out of the path', () => {
    const routes: Route[] = [
      { kind: 'play', account: isorun, gameId: 'dcss-0.34' },
      { kind: 'watch', serverId: 'cdi', account: isorun, username: 'someone' },
      { kind: 'menu', path: 'login', serverId: 'cko', username: 'someone' },
    ]
    for (const r of routes) {
      const { pathname, search } = new URL(formatRoute(r), base)
      expect(pathname + search).not.toMatch(/isorun|someone/)
    }
  })

  it('carries the query through, so the flags set at launch survive a Play', () => {
    setChosenAccount(isorun)
    history.replaceState(null, '', '/?fullscreen&perf')
    try {
      expect(formatRoute({ kind: 'home' })).toBe('/?fullscreen&perf')
      expect(formatRoute({ kind: 'play', account: isorun, gameId: 'dcss-web-trunk' })).toBe('/play/cdi/dcss-web-trunk?fullscreen&perf#isorun')
      setRoute({ kind: 'play', account: isorun, gameId: 'dcss-web-trunk' })
      expect(window.location.search).toBe('?fullscreen&perf')
      expect(window.location.pathname).toBe('/play/cdi/dcss-web-trunk')
      expect(window.location.hash).toBe('#isorun')
    } finally {
      history.replaceState(null, '', '/')
    }
  })

  /**
   * The address bar is also the way back: opening a game leaves the screen it
   * was opened from in the history, so the browser's Back (and a handheld's
   * back gesture) returns to the lobby, or to the account list if that is
   * where the game was started from.
   */
  describe('history', () => {
    beforeEach(() => {
      setChosenAccount(isorun)
      history.replaceState(null, '', '/')
    })

    it('adds one entry leaving home, so Back is the way out of a game, straight to the root', () => {
      const before = history.length
      setRoute({ kind: 'lobby', serverId: 'cdi', account: isorun })
      setRoute({ kind: 'play', account: isorun, gameId: 'dcss-0.34' })
      expect(window.location.pathname).toBe('/play/cdi/dcss-0.34')
      expect(history.length).toBe(before + 1)
    })

    it('rewrites the entry coming back up, so Back never returns to the game just left', () => {
      setRoute({ kind: 'lobby', serverId: 'cdi', account: isorun })
      setRoute({ kind: 'play', account: isorun, gameId: 'dcss-0.34' })
      const deep = history.length
      setRoute({ kind: 'lobby', serverId: 'cdi', account: isorun })
      setRoute({ kind: 'home' })
      expect(window.location.pathname).toBe('/')
      expect(history.length).toBe(deep)
    })

    it('gives the tab back its own title out of a game, and leaves a game’s to the game', () => {
      document.title = 'isorun the Chiller | Vine Stalker - Isorun'
      setRoute({ kind: 'play', account: isorun, gameId: 'dcss-0.34' })
      expect(document.title).toBe('isorun the Chiller | Vine Stalker - Isorun')
      setRoute({ kind: 'menu', path: 'settings' })
      expect(document.title).toBe('Isorun - Dungeon Crawl Stone Soup')
    })

    it('names only the app in a window of its own', () => {
      history.replaceState(null, '', '/?fullscreen')
      setRoute({ kind: 'menu', path: 'settings' })
      expect(document.title).toBe('Isorun')
    })

    it('leaves the address alone when it already says this', () => {
      setRoute({ kind: 'lobby', serverId: 'cdi', account: isorun })
      const len = history.length
      setRoute({ kind: 'lobby', serverId: 'cdi', account: isorun })
      expect(history.length).toBe(len)
    })

    /**
     * A window of our own has no Back to serve, and Chrome refuses to close a
     * window a script did not open once its history holds more than one
     * entry — so an entry here would cost the Quit row (quit.ts).
     */
    it('adds no entry in a window of its own, so Quit can still close it', () => {
      history.replaceState(null, '', '/?fullscreen')
      try {
        const before = history.length
        setRoute({ kind: 'lobby', serverId: 'cdi', account: isorun })
        setRoute({ kind: 'play', account: isorun, gameId: 'dcss-0.34' })
        expect(window.location.pathname).toBe('/play/cdi/dcss-0.34')
        expect(history.length).toBe(before)
      } finally {
        history.replaceState(null, '', '/')
      }
    })

    it('does not stack an entry per game: one spectate to the next replaces it', () => {
      setRoute({ kind: 'lobby', serverId: 'cdi', account: isorun })
      setRoute({ kind: 'watch', serverId: 'cdi', account: isorun, username: 'someone' })
      const len = history.length
      setRoute({ kind: 'watch', serverId: 'cdi', account: isorun, username: 'other' })
      expect(window.location.pathname + window.location.hash).toBe('/watch/cdi#other')
      expect(history.length).toBe(len)
    })
  })
})
