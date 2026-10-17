/**
 * The offline engine's channels (engine/build.sh publishes `stable` and `trunk`, each an
 * engine.json under the engine base) as the native app's web build plays them: which build a
 * channel points at, the save slot it plays in, and the saves in IndexedDB.
 */

/** One playable engine build (engine/dist/builds/<build>/), as the client's lobby offers it. */
export interface OfflineChannel {
  /** The game id, e.g. `offline-0.34`, `offline-trunk`. */
  id: string
  /** The name the lobby shows, e.g. `DCSS 0.34`: a version or `trunk`, which the client sorts rows by. */
  label: string
  /** The crawl commit built. */
  commit: string
  /** The directory its files sit under: the commit and the recipe (`buildOf`). */
  build?: string
  /** The gamedata version: a directory under the engine base's `gamedata/`. */
  gamedata: string
  /** Where this build keeps its saves (engine/wasm/pre.js `saveDir`). */
  saveDir: string
}

/** Where crawl finds the profile's rc file: outside the save directory, so it is never written into a save's database. */
export const RC_PATH = '/rc/init.txt'

/** A channel's engine.json (engine/build.sh, engine/pointer.mjs): the build it points at. */
export interface EngineInfo {
  channel: string
  commit: string
  /** The directory its files sit under, `builds/<build>/`: the commit and the recipe (engine/recipe.mjs), so a rebuild with new patches is a new build. Older pointers have none, and sit under the commit. */
  build?: string
  /** crawl's own version string, `git describe`: `0.34.1-4-g0e95e087e2`, `0.35-a0-1079-ga0251cc2b5`. */
  version: string
  stamp: string
  gamedata: string
  /** Every file a device keeps to play the build, relative to the engine base, with its size in bytes. */
  files: [path: string, size: number][]
}

/** The directory a build's files sit under, `builds/<build>/`. */
export function buildOf(info: Pick<EngineInfo, 'commit' | 'build'>): string {
  return info.build ?? info.commit
}

/** The release a build belongs to, `0.34`; null for trunk, or a version that names none. */
export function releaseOf(info: Pick<EngineInfo, 'channel' | 'version'>): string | null {
  return info.channel === 'trunk' ? null : (/^(\d+\.\d+)/.exec(info.version)?.[1] ?? null)
}

/**
 * What a build is played as on this device: `trunk`, or its release. Trunk
 * updates in place; a new release is a slot of its own beside the last one,
 * since crawl carries a save forward into a new release and never back.
 */
export function slotOf(info: Pick<EngineInfo, 'channel' | 'version'>): string {
  return releaseOf(info) ?? info.channel
}

/** Where a slot's saves are (engine/wasm/pre.js mounts it): the default profile's, and the stem of every other's. */
function slotSaveDir(slot: string): string {
  return `/crawl-${slot}`
}

/**
 * The lobby entry for a build. The label carries the version the client
 * sorts game rows by (webtiles `gameLinkRows`): `trunk` for trunk, the
 * release's major.minor for anything else.
 */
export function channelOf(info: EngineInfo): OfflineChannel {
  const slot = slotOf(info)
  const release = releaseOf(info)
  return {
    id: `offline-${slot}`,
    label: release ? `DCSS ${release}` : 'DCSS trunk',
    commit: info.commit,
    build: buildOf(info),
    gamedata: info.gamedata,
    saveDir: slotSaveDir(slot),
  }
}

/** The profile that keeps its saves in the slot's own directory: the one a server given no name plays as, and a new device's own player. */
export const DEFAULT_PROFILE = 'Player'

/**
 * Where a profile keeps its saves for a slot: a directory of its own, so
 * one profile's saved characters, high scores and morgues never show in
 * another's. IDBFS names its IndexedDB database after the directory, so
 * each is a database of its own too (deleteProfileSaves). Names match in any
 * case, as accounts do.
 */
export function profileSaveDir(slotDir: string, profile: string): string {
  const p = profile.toLowerCase()
  return p === DEFAULT_PROFILE.toLowerCase() ? slotDir : `${slotDir}~${p}`
}

/**
 * The saved characters in one save directory's database: the `.cs` files
 * under `saves/`, by their full path. Null when the database cannot be read.
 */
export function saveFiles(idb: IDBFactory, name: string): Promise<string[] | null> {
  return new Promise((resolve) => {
    const req = idb.open(name)
    req.onerror = () => resolve(null)
    req.onsuccess = () => {
      const db = req.result
      if (!db.objectStoreNames.contains('FILE_DATA')) {
        db.close()
        return resolve([])
      }
      const keys = db.transaction('FILE_DATA').objectStore('FILE_DATA').getAllKeys()
      keys.onsuccess = () => {
        db.close()
        resolve(keys.result.filter((k): k is string => typeof k === 'string' && /\/saves\/[^/]+\.cs$/.test(k)))
      }
      keys.onerror = () => {
        db.close()
        resolve(null)
      }
    }
  })
}
