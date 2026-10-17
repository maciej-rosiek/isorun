#!/usr/bin/env node
/**
 * Merge the labels of the contact sheets (tools/labels/README.md) into
 * tools/labels/labels.json, by tile name, and into the renderer's
 * native/crates/render/src/tile-labels.json, by the pixel hash of every
 * variant (rltiles.mjs `tileHash`).
 *
 *   node tools/labels/merge.mjs <sheets dir> <labels dir>
 *
 * A sheet whose labels are missing or do not follow the schema is reported and
 * left out; run its labelling again and merge again.
 */
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const root = path.resolve(here, '..', '..')
const [sheetsDir, labelsDir] = process.argv.slice(2)
if (!sheetsDir || !labelsDir) {
  console.error('usage: node tools/labels/merge.mjs <sheets dir> <labels dir>')
  process.exit(2)
}

export const MATERIALS = ['stone', 'brick', 'rock', 'marble', 'dirt', 'sand', 'mud', 'grass', 'moss', 'wood', 'metal', 'gold', 'crystal', 'gem', 'ice', 'glass', 'bone', 'flesh', 'fur', 'scales', 'chitin', 'slime', 'fire', 'lava', 'water', 'cloth', 'leather', 'paper', 'plant', 'magic', 'shadow', 'light', 'other']
export const JOINTS = ['none', 'bricks', 'blocks', 'cobbles', 'planks', 'tiles', 'cracks']
export const MOTIONS = ['none', 'breathe', 'hover', 'flicker']

function parse(label) {
  if (typeof label !== 'object' || label === null) return null
  const material = MATERIALS.indexOf(label.material)
  const joints = JOINTS.indexOf(label.joints)
  const motion = MOTIONS.indexOf(label.motion)
  const level = (v, max) => (Number.isInteger(v) && v >= 0 && v <= max ? v : -1)
  const relief = level(label.relief, 3)
  const gloss = level(label.gloss, 3)
  const glow = level(label.glow, 2)
  const flags = [label.bright_is_high, label.wet, label.sparkle, label.translucent]
  if (material < 0 || joints < 0 || motion < 0 || relief < 0 || gloss < 0 || glow < 0 || flags.some((f) => typeof f !== 'boolean')) return null
  return [material, relief, label.bright_is_high ? 1 : 0, joints, gloss, label.wet ? 1 : 0, glow, label.sparkle ? 1 : 0, label.translucent ? 1 : 0, motion]
}

const index = JSON.parse(fs.readFileSync(path.join(sheetsDir, 'index.json'), 'utf8'))
const byName = {}
const problems = []
for (const sheet of index.sheets) {
  const file = path.join(labelsDir, sheet.sheet.replace('.png', '.json'))
  if (!fs.existsSync(file)) {
    problems.push(`${sheet.sheet}: no labels`)
    continue
  }
  let labels
  try {
    labels = JSON.parse(fs.readFileSync(file, 'utf8'))
  } catch (e) {
    problems.push(`${sheet.sheet}: ${e.message}`)
    continue
  }
  const at = new Map(Array.isArray(labels) ? labels.map((l) => [l?.i, l]) : [])
  const parsed = sheet.entries.map((e) => [e, parse(at.get(e.i))])
  const bad = parsed.filter(([, p]) => !p).map(([e]) => e.i)
  if (bad.length) {
    problems.push(`${sheet.sheet}: ${bad.length} labels missing or off the schema (${bad.slice(0, 8).join(', ')})`)
    continue
  }
  const kinds = new Set(parsed.map(([, p]) => p.join(','))).size
  if (sheet.entries.length >= 16 && kinds < 4) problems.push(`${sheet.sheet}: only ${kinds} different labels for ${sheet.entries.length} tiles, check it`)
  for (const [e, p] of parsed) byName[e.name] = { atlas: e.atlas, label: p, hashes: e.hashes }
}

const palette = new Map()
const tiles = {}
for (const { label, hashes } of Object.values(byName)) {
  const key = label.join(',')
  if (!palette.has(key)) palette.set(key, palette.size)
  for (const h of hashes) tiles[h] ??= palette.get(key)
}
const names = Object.fromEntries(Object.entries(byName).map(([n, v]) => [n, v.label]))
fs.writeFileSync(path.join(here, 'labels.json'), JSON.stringify({ gamedata: index.gamedata, fields: ['material', 'relief', 'bright_is_high', 'joints', 'gloss', 'wet', 'glow', 'sparkle', 'translucent', 'motion'], materials: MATERIALS, joints: JOINTS, motions: MOTIONS, tiles: names }, null, 0).replace(/\],"/g, '],\n"') + '\n')
const runtime = { gamedata: index.gamedata, materials: MATERIALS, joints: JOINTS, motions: MOTIONS, labels: [...palette.keys()].map((k) => k.split(',').map(Number)), tiles }
fs.writeFileSync(path.join(root, 'native/crates/render/src/tile-labels.json'), JSON.stringify(runtime) + '\n')
console.log(`${Object.keys(byName).length} base tiles labelled, ${palette.size} distinct labels, ${Object.keys(tiles).length} tile hashes`)
for (const p of problems) console.log(`problem: ${p}`)
