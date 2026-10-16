#!/usr/bin/env node
/**
 * Pack the front room's tiles (apps/isorun/src/room/antechamber.des) from a
 * pinned DCSS gamedata version into one small atlas the menu draws without
 * a server: apps/isorun/public/room/atlas.png and atlas.json.
 *
 *   node tools/build/pack-room.mjs <images dir> [des] [out dir] [tileinfo dir]
 *
 * `images dir` holds the pinned version's floor.png, wall.png, feat.png, player.png and
 * main.png (fetch them once with the curl lines the README of
 * apps/isorun/public/room gives; they are not committed). The tileinfo
 * modules come from the checked-in fixture of the same version by default.
 * Nothing here talks to the network: an ordinary build never runs this, it
 * ships what this wrote.
 *
 * Tiles keep the offsets and sizes tileinfo gives them (raw ox/oy/w/h and
 * the drawn rect), and the JSON records which atlas and version each came
 * from. DCSS's tile art (rltiles) is CC0; ATTRIBUTION.md says so for these.
 *
 * No dependencies: the PNGs are read and written by rltiles.mjs (8-bit RGB/RGBA,
 * non-interlaced, which is what rltiles' packer emits).
 */
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { loadTileinfo, readPng, writePng } from './rltiles.mjs'

/** The version the room is built from: DCSS 0.34 on crawl.dcss.io, the version the fixtures pin. */
const ROOM_GAMEDATA_VERSION = 'acd3d60e20f899c1c8a546953d6ffa0f6c7fe0c8'
const ROOM_GAMEDATA_HOST = 'crawl.dcss.io'

const here = path.dirname(fileURLToPath(import.meta.url))
const root = path.resolve(here, '..', '..')
const [imagesDir, desPath = path.join(root, 'apps/isorun/src/room/antechamber.des'), outDir = path.join(root, 'apps/isorun/public/room'), tileinfoDir = path.join(root, 'packages/scene-webtiles/test/fixtures/gamedata', ROOM_GAMEDATA_VERSION)] = process.argv.slice(2)
if (!imagesDir) {
  console.error('usage: node tools/build/pack-room.mjs <images dir> [des] [out dir] [tileinfo dir]')
  process.exit(2)
}

// ---------------------------------------------------------------- the legend
/** the names the vault uses, by kind: masonry, floors, monsters and the trees among features take every variant (a wood is not one tree), other features and items the one named (a statue's variants are other statues) */
function legendNames(des) {
  const out = []
  des.split(/\r?\n/).forEach((line, i) => {
    const m = /^(TILE|FTILE|KFEAT|KITEM|KMONS):\s*(\S)\s*=\s*(\S+)\s*$/.exec(line.trim())
    if (m) out.push({ kind: m[1], glyph: m[2], name: m[3].toUpperCase(), line: i + 1 })
  })
  return out
}

// ---------------------------------------------------------------- tileinfo
const ORDER = ['floor', 'wall', 'feat', 'main', 'player']
const modules = loadTileinfo(tileinfoDir, ORDER)
const moduleFor = (id) => ORDER.map((n) => modules[n]).find((m) => id >= m.first && id < m.end)
const idOf = (name) => {
  for (const n of ORDER) if (typeof modules[n].exports[name] === 'number') return modules[n].exports[name]
  return undefined
}

// ---------------------------------------------------------------- the tiles
const des = fs.readFileSync(desPath, 'utf8')
const legend = legendNames(des)
const wanted = new Map() // id -> { name, from }
const names = {}
const missing = []
for (const e of legend) {
  const id = idOf(e.name)
  if (id === undefined) {
    missing.push(`${e.name} (line ${e.line})`)
    continue
  }
  const m = moduleFor(id)
  let count = 1
  try {
    count = m.exports.tile_count(id) || 1
  } catch {
    count = 1
  }
  const variants = e.kind === 'KITEM' || (e.kind === 'KFEAT' && !/TREE|MANGROVE/.test(e.name)) ? 1 : count
  names[e.name] = { id, count: variants }
  for (let i = 0; i < variants; i++) wanted.set(id + i, { name: i ? `${e.name} +${i}` : e.name, from: m.atlas })
}
if (missing.length) {
  console.error(`tiles not in gamedata ${ROOM_GAMEDATA_VERSION}: ${missing.join(', ')}`)
  process.exit(1)
}
const images = {}
const tiles = {}
const entries = []
for (const [id, w] of [...wanted].sort((a, b) => a[0] - b[0])) {
  const m = moduleFor(id)
  const info = m.exports.get_tile_info(id)
  if (!info) throw new Error(`${w.name}: no tile info for ${id}`)
  entries.push({ id, name: w.name, from: m.atlas, info, dw: info.ex - info.sx, dh: info.ey - info.sy })
}
// shelves of the tallest first, rows of up to 512 px
entries.sort((a, b) => b.dh - a.dh || a.id - b.id)
const WIDTH = 512
let x = 0, y = 0, shelf = 0, height = 0
for (const e of entries) {
  if (x + e.dw > WIDTH) {
    x = 0
    y += shelf
    shelf = 0
  }
  e.px = x
  e.py = y
  x += e.dw
  shelf = Math.max(shelf, e.dh)
  height = Math.max(height, y + e.dh)
}
const out = { width: WIDTH, height, data: Buffer.alloc(WIDTH * height * 4) }
for (const e of entries) {
  const img = (images[e.from] ??= readPng(path.join(imagesDir, `${e.from}.png`)))
  for (let r = 0; r < e.dh; r++) {
    const s = ((e.info.sy + r) * img.width + e.info.sx) * 4
    img.data.copy(out.data, ((e.py + r) * WIDTH + e.px) * 4, s, s + e.dw * 4)
  }
  // the offsets exactly as @isorun/gamedata derives a TileRect from tileinfo (tileUncached)
  tiles[e.id] = {
    name: e.name,
    from: e.from,
    sx: e.px,
    sy: e.py,
    w: e.dw,
    h: e.dh,
    ox: e.info.ox + (32 / 2 - e.info.w / 2),
    oy: e.info.oy + (32 - e.info.h),
    raw: { w: e.info.w, h: e.info.h, ox: e.info.ox, oy: e.info.oy, sx: e.info.sx, sy: e.info.sy },
  }
}
fs.mkdirSync(outDir, { recursive: true })
writePng(path.join(outDir, 'atlas.png'), out)
const json = {
  source: { host: ROOM_GAMEDATA_HOST, version: ROOM_GAMEDATA_VERSION, atlases: [...new Set(entries.map((e) => e.from))].sort(), des: path.relative(root, desPath) },
  license: 'DCSS tile art (rltiles) is CC0; see ATTRIBUTION.md',
  cell: 32,
  width: WIDTH,
  height,
  names,
  tiles,
}
fs.writeFileSync(path.join(outDir, 'atlas.json'), JSON.stringify(json, null, 1) + '\n')
const png = fs.statSync(path.join(outDir, 'atlas.png')).size
const js = fs.statSync(path.join(outDir, 'atlas.json')).size
console.log(`${entries.length} tiles from ${json.source.atlases.join(', ')} of ${ROOM_GAMEDATA_VERSION}: atlas.png ${WIDTH}x${height}, ${png} bytes; atlas.json ${js} bytes`)
