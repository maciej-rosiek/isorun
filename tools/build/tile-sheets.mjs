#!/usr/bin/env node
/**
 * Contact sheets of every tile of a gamedata version, for labelling the tiles
 * (tools/labels/README.md): numbered grids of the first variant of each base
 * tile, four times their size on grey, and index.json with each entry's name,
 * atlas, ids and the pixel hash of every variant (rltiles.mjs `tileHash`).
 *
 *   node tools/build/tile-sheets.mjs <gamedata dir> <out dir>
 *
 * `gamedata dir` holds a version's tileinfo-*.js modules and atlas images, as
 * engine/dist/gamedata/<version> does after an engine build.
 */
import fs from 'node:fs'
import path from 'node:path'
import { loadTileinfo, readPng, tileHash, writePng } from './rltiles.mjs'

const [gamedataDir, outDir] = process.argv.slice(2)
if (!gamedataDir || !outDir) {
  console.error('usage: node tools/build/tile-sheets.mjs <gamedata dir> <out dir>')
  process.exit(2)
}

const ORDER = ['floor', 'wall', 'feat', 'main', 'player']
const COLUMNS = 8
const ROWS = 6
const SCALE = 4
const TILE = 32 * SCALE
const GAP = 8
const STRIP = 16
const CELL_W = TILE + GAP
const CELL_H = TILE + STRIP + GAP
const BACKGROUND = [128, 128, 128]

const modules = loadTileinfo(gamedataDir, ORDER)
const images = Object.fromEntries(ORDER.map((name) => [name, readPng(path.join(gamedataDir, `${name}.png`))]))

/** The base tiles of a module with their variants, named as tileinfo names them; an unnamed id joins the base before it. */
function baseTiles(module) {
  const names = new Map()
  for (const [k, v] of Object.entries(module.exports)) {
    if (typeof v !== 'number' || !/^[A-Z][A-Z0-9_]*$/.test(k) || /_MAX$/.test(k)) continue
    if (v < module.first || v >= module.end) continue
    const known = names.get(v)
    if (!known || k.length < known.length) names.set(v, k)
  }
  const bases = []
  let id = module.first
  while (id < module.end) {
    let count = 1
    try {
      count = Math.max(1, module.exports.tile_count(id) || 1)
    } catch {
      count = 1
    }
    const ids = []
    for (let i = id; i < Math.min(module.end, id + count); i++) ids.push(i)
    const name = names.get(id)
    if (name || !bases.length) bases.push({ name: name ?? `${module.atlas}_${id}`, atlas: module.atlas, ids })
    else bases.at(-1).ids.push(...ids)
    id += count
  }
  return bases
}

function pixels(atlas, id) {
  const info = modules[atlas].exports.get_tile_info(id)
  if (!info) return null
  const w = info.ex - info.sx
  const h = info.ey - info.sy
  if (w <= 0 || h <= 0) return null
  const img = images[atlas]
  const data = Buffer.alloc(w * h * 4)
  for (let r = 0; r < h; r++) img.data.copy(data, r * w * 4, ((info.sy + r) * img.width + info.sx) * 4, ((info.sy + r) * img.width + info.sx + w) * 4)
  return { w, h, data }
}

const DIGITS = ['111101101101111', '010110010010111', '111001111100111', '111001111001111', '101101111001001', '111100111001111', '111100111101111', '111001001001001', '111101111101111', '111101111001111']

function drawNumber(sheet, n, x, y) {
  const scale = 2
  for (const [k, ch] of [...String(n)].entries()) {
    const glyph = DIGITS[Number(ch)]
    for (let gy = 0; gy < 5; gy++)
      for (let gx = 0; gx < 3; gx++) {
        if (glyph[gy * 3 + gx] !== '1') continue
        for (let sy = 0; sy < scale; sy++)
          for (let sx = 0; sx < scale; sx++) {
            const o = ((y + gy * scale + sy) * sheet.width + x + k * 4 * scale + gx * scale + sx) * 4
            sheet.data[o] = sheet.data[o + 1] = sheet.data[o + 2] = 255
          }
      }
  }
}

function drawTile(sheet, tile, x, y) {
  const scale = Math.max(1, Math.floor(Math.min(TILE / tile.w, TILE / tile.h)))
  const left = x + Math.floor((TILE - tile.w * scale) / 2)
  const top = y + Math.floor((TILE - tile.h * scale) / 2)
  for (let ty = 0; ty < tile.h * scale; ty++)
    for (let tx = 0; tx < tile.w * scale; tx++) {
      const s = (Math.floor(ty / scale) * tile.w + Math.floor(tx / scale)) * 4
      const a = tile.data[s + 3] / 255
      const o = ((top + ty) * sheet.width + left + tx) * 4
      for (let c = 0; c < 3; c++) sheet.data[o + c] = Math.round(tile.data[s + c] * a + sheet.data[o + c] * (1 - a))
    }
}

function blankSheet() {
  const width = COLUMNS * CELL_W
  const height = ROWS * CELL_H
  const data = Buffer.alloc(width * height * 4)
  for (let i = 0; i < width * height; i++) {
    data.set(BACKGROUND, i * 4)
    data[i * 4 + 3] = 255
  }
  for (let r = 0; r < ROWS; r++)
    for (let y = 0; y < STRIP; y++)
      for (let x = 0; x < width; x++) data.fill(24, ((r * CELL_H + y) * width + x) * 4, ((r * CELL_H + y) * width + x) * 4 + 3)
  return { width, height, data }
}

fs.mkdirSync(outDir, { recursive: true })
const entries = []
for (const name of ORDER)
  for (const base of baseTiles(modules[name])) {
    const tiles = base.ids.map((id) => ({ id, tile: pixels(base.atlas, id) })).filter((t) => t.tile)
    if (!tiles.length) continue
    entries.push({ name: base.name, atlas: base.atlas, ids: tiles.map((t) => t.id), hashes: [...new Set(tiles.map((t) => tileHash(t.tile.data, t.tile.w, t.tile.h)))], first: tiles[0].tile })
  }

const perSheet = COLUMNS * ROWS
const index = []
for (let s = 0; s * perSheet < entries.length; s++) {
  const sheet = blankSheet()
  const file = `sheet-${String(s).padStart(3, '0')}.png`
  const listed = entries.slice(s * perSheet, (s + 1) * perSheet).map((e, i) => {
    const x = (i % COLUMNS) * CELL_W
    const y = Math.floor(i / COLUMNS) * CELL_H
    drawNumber(sheet, i, x + 3, y + 3)
    drawTile(sheet, e.first, x, y + STRIP)
    return { i, name: e.name, atlas: e.atlas, ids: e.ids, hashes: e.hashes }
  })
  writePng(path.join(outDir, file), sheet)
  index.push({ sheet: file, entries: listed })
}
fs.writeFileSync(path.join(outDir, 'index.json'), JSON.stringify({ gamedata: path.basename(path.resolve(gamedataDir)), sheets: index }, null, 1) + '\n')
console.log(`${entries.length} base tiles of ${entries.reduce((n, e) => n + e.ids.length, 0)} ids on ${index.length} sheets in ${outDir}`)
