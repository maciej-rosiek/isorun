import { formattedStringToHtml, formattedStringToText, type PlayerState } from '@isorun/webtiles'
import type { Gamedata } from '@isorun/gamedata'
import type { Billboard } from '@isorun/scene'
import { h } from './dom'
import { commandTileId, composeTiles, tileCanvas, type ClientOverlayOp, type TileRef } from './overlays'
import { upscaleImage } from '@isorun/render-iso'
import { DOLL_PLACES, OBJ, equipSlotOf, itemActionKeycaps, itemActions, itemTitle, paperDoll, type DollPlace, type EquipSlot, type ItemActionId, type PackItem, type Worn } from './gear'
import { itemTiles } from './quick-slots'
import { CHARACTER_COMMANDS, type CommandEntry } from './command-menu'
import { compareStats, itemNameParts } from './item-compare'
import { hudPx } from './hud-unit'
import { keycaps, tipCard, type Tip } from './tooltip'
import type { PadKind } from './gamepad'

/**
 * The screens the dock opens over the game, drawn by the client from the
 * server's data: the inventory (a paper doll and the backpack) and the
 * character sheet. Each is a panel and the stops the pad's cursor walks
 * (`ScreenStop`); Overlays.showScreen puts it up.
 */

export interface ScreenStop {
  /** stable across rebuilds, so the cursor stays on the same thing */
  id: string
  el: HTMLElement
  row: number
  col: number
  /** A, Enter, a click */
  activate(): void
  /** Y on the pad: the quick action (put on, take off, use) */
  quick?(): void
  /** X on the pad: describe */
  describe?(): void
  /** what Y does, for the prompt under the screen */
  quickLabel?: string
  /** what A does, for the prompt under the screen */
  selectLabel?: string
  /** the cursor came here */
  onFocus?(): void
}

export interface Screen {
  el: HTMLElement
  stops: ScreenStop[]
  input?(op: ClientOverlayOp): boolean
}

export interface InventoryHooks {
  act(item: PackItem, action: ItemActionId): void
  /** an empty place of the doll: what in the pack goes there, to put on */
  fill(place: DollPlace, label: string): void
  pin?(item: PackItem): void
}

const PLACE_LABEL: Record<DollPlace, string> = {
  helmet: 'Head',
  cloak: 'Cloak',
  body: 'Body armour',
  gloves: 'Hands',
  boots: 'Feet',
  amulet: 'Amulet',
  ring1: 'Ring',
  ring2: 'Ring',
  weapon: 'Weapon',
  offhand: 'Off hand',
  talisman: 'Talisman',
  gizmo: 'Gizmo',
  quiver: 'Quiver',
}

const PLACE_SILHOUETTE: Record<DollPlace, string> = {
  helmet: 'THELM_HELM',
  cloak: 'ARM_CLOAK',
  body: 'ARM_CHAIN_MAIL',
  gloves: 'ARM_GLOVES',
  boots: 'ARM_BOOTS',
  amulet: 'AMU_NORMAL_OFFSET',
  ring1: 'RING_NORMAL_OFFSET',
  ring2: 'RING_NORMAL_OFFSET',
  weapon: 'WPN_LONG_SWORD',
  offhand: 'ARM_KITE_SHIELD',
  talisman: 'TALISMAN_BLADE',
  gizmo: 'GIZMO',
  quiver: 'MI_ARROW',
}

/** Where each place stands on the doll: the column (0 left, 2 right) and the row. */
const PLACE_AT: Record<DollPlace, { col: number; row: number }> = {
  helmet: { col: 0, row: 0 },
  cloak: { col: 0, row: 1 },
  body: { col: 0, row: 2 },
  gloves: { col: 0, row: 3 },
  boots: { col: 0, row: 4 },
  amulet: { col: 2, row: 0 },
  ring1: { col: 2, row: 1 },
  ring2: { col: 2, row: 2 },
  weapon: { col: 2, row: 3 },
  offhand: { col: 2, row: 4 },
  talisman: { col: 0, row: 5 },
  gizmo: { col: 1, row: 5 },
  quiver: { col: 2, row: 5 },
}

const SLOT_WORD: Record<EquipSlot, string> = {
  weapon: 'Weapon',
  offhand: 'Shield',
  body: 'Body armour',
  cloak: 'Cloak',
  helmet: 'Helmet',
  gloves: 'Gloves',
  boots: 'Boots',
  amulet: 'Amulet',
  ring: 'Ring',
  talisman: 'Talisman',
  gizmo: 'Gizmo',
}

const CLASS_WORD: Readonly<Record<number, string>> = {
  [OBJ.MISSILES]: 'Ammunition',
  [OBJ.WANDS]: 'Wand',
  [OBJ.SCROLLS]: 'Scroll',
  [OBJ.POTIONS]: 'Potion',
  [OBJ.BOOKS]: 'Book',
  [OBJ.STAVES]: 'Staff',
  [OBJ.MISCELLANY]: 'Evocable',
  [OBJ.BAUBLES]: 'Bauble',
}

const BACKPACK_COLUMNS = 8
const BACKPACK_MIN_ROWS = 7
const PACK_LETTERS = 52
const SIZE = { item: 56, figure: 192, characterFigure: 160 }

/** The player's sprite as the dungeon draws it: the doll's parts, bottom to top. */
export function playerSprite(me: Billboard | undefined): TileRef[] {
  if (!me) return []
  return me.layers?.length ? me.layers.map((l) => ({ t: l.tile, ox: l.ox, oy: l.oy, ymax: l.ymax })) : [{ t: me.tile }]
}

const SPRITE_PX = 32

/**
 * The player's sprite shown large on the inventory and character screens: grown with xBR, which smooths the
 * pixel steps into clean edges, then scaled the rest of the way smoothly, where repeated pixels would show
 * every square of a 32-pixel sprite.
 */
function figureCanvas(gd: Gamedata | null, tiles: TileRef[], size: number): HTMLCanvasElement {
  const c = h('canvas', { class: 'tile figure' })
  c.style.width = size + 'px'
  c.style.height = size + 'px'
  const px = Math.round(size * (window.devicePixelRatio || 1))
  c.width = px
  c.height = px
  const sprite = document.createElement('canvas')
  sprite.width = SPRITE_PX
  sprite.height = SPRITE_PX
  const spriteCtx = sprite.getContext('2d', { willReadFrequently: true })
  const ctx = c.getContext('2d')
  if (!gd || !spriteCtx || !ctx) return c
  composeTiles(spriteCtx, gd, tiles, 1)
  const grown = upscaleImage(spriteCtx.getImageData(0, 0, SPRITE_PX, SPRITE_PX), 'xbr')
  const large = document.createElement('canvas')
  large.width = grown.width
  large.height = grown.height
  large.getContext('2d')?.putImageData(grown, 0, 0)
  ctx.imageSmoothingEnabled = true
  ctx.imageSmoothingQuality = 'high'
  ctx.drawImage(large, 0, 0, px, px)
  return c
}

const colour = (col: unknown) => `var(--color-${(typeof col === 'number' ? col : 7) & 15})`

interface MenuRow {
  label: string
  keys: string[]
  run(): void
}

interface OpenMenu {
  rows: MenuRow[]
  els: HTMLElement[]
  focus: number
  owner: HTMLElement
}

/**
 * The inventory: the paper doll on the left, what is worn on its places
 * (the item's own sprite, or a faint outline of what goes there), the
 * backpack on the right as a grid of sockets. Hovering an item shows its
 * card, beside what it would replace; a click (or Enter, or A) opens its
 * actions next to it, a double click does the first; the pad's Y does the
 * first too, X describes.
 */
export function inventoryScreen(
  opts: { player: PlayerState; items: PackItem[]; worn: ReadonlyMap<string, Worn>; me: Billboard | undefined; gd: Gamedata | null; checking: boolean; padKind: PadKind },
  hooks: InventoryHooks,
): Screen {
  const { items, worn, gd, padKind } = opts
  const doll = paperDoll(items, worn)
  const onDoll = new Set(Object.values(doll).map((i) => i.letter))
  const stops: ScreenStop[] = []
  const tipLayer = h('div', { class: 'inv-tips', hidden: true })
  const menuEl = h('div', { class: 'inv-menu', hidden: true, role: 'menu' })
  const el = h('div', { class: 'inventory' })
  let menu: OpenMenu | null = null

  const place = (floating: HTMLElement, anchor: HTMLElement) => {
    const host = el.getBoundingClientRect()
    const r = anchor.getBoundingClientRect()
    const gap = hudPx(10)
    const right = r.right - host.left + gap
    const left = right + floating.offsetWidth <= host.width ? right : Math.max(0, r.left - host.left - gap - floating.offsetWidth)
    const top = Math.max(0, Math.min(r.top - host.top, host.height - floating.offsetHeight))
    floating.style.left = left + 'px'
    floating.style.top = top + 'px'
  }

  const hideTip = () => {
    tipLayer.hidden = true
  }
  const showCards = (cards: HTMLElement[], anchor: HTMLElement) => {
    if (menu) return
    tipLayer.replaceChildren(...cards)
    tipLayer.hidden = false
    place(tipLayer, anchor)
  }
  const showItemTip = (item: PackItem, anchor: HTMLElement) => {
    const slot = equipSlotOf(item)
    const isWorn = onDoll.has(item.letter)
    const replaced = !isWorn && slot ? wornIn(doll, slot) : []
    const cards = replaced.map((w) => tipCard(itemTip(w, worn.get(w.letter), undefined, 'Equipped', false), padKind, 'equipped'))
    cards.push(tipCard(itemTip(item, worn.get(item.letter), replaced[0], isWorn ? 'Equipped' : undefined, true), padKind, isWorn ? 'equipped' : ''))
    showCards(cards, anchor)
  }

  const closeMenu = () => {
    if (!menu) return
    menu.owner.classList.remove('picked')
    menu = null
    menuEl.hidden = true
  }
  const focusMenu = (i: number) => {
    if (!menu) return
    menu.els[menu.focus]?.classList.remove('focused')
    menu.focus = (i + menu.rows.length) % menu.rows.length
    menu.els[menu.focus]?.classList.add('focused')
  }
  const runMenu = (i: number) => {
    const row = menu?.rows[i]
    closeMenu()
    row?.run()
  }
  const openMenu = (item: PackItem, anchor: HTMLElement) => {
    closeMenu()
    hideTip()
    const rows: MenuRow[] = itemActions(item, worn.get(item.letter)).map((a) => ({ label: a.label, keys: itemActionKeycaps(a, item.letter), run: () => hooks.act(item, a.id) }))
    const pin = hooks.pin
    if (pin && typeof item.action_verb === 'string') rows.push({ label: 'Add to quick slot', keys: [], run: () => pin(item) })
    const els = rows.map((row, i) => {
      const rowEl = h('button', { type: 'button', class: 'inv-menu-row' + (i === 0 ? ' primary' : ''), role: 'menuitem' }, h('span', { class: 'label' }, row.label), row.keys.length ? keycaps(row.keys) : null)
      rowEl.addEventListener('mouseenter', () => focusMenu(i))
      rowEl.addEventListener('click', (ev) => {
        ev.stopPropagation()
        runMenu(i)
      })
      return rowEl
    })
    menuEl.replaceChildren(h('div', { class: 'inv-menu-title', style: { color: colour(item.col) } }, cleanTitle(item)), ...els)
    menu = { rows, els, focus: 0, owner: anchor }
    anchor.classList.add('picked')
    menuEl.hidden = false
    focusMenu(0)
    place(menuEl, anchor)
  }

  el.addEventListener('pointerdown', (ev) => {
    if (menu && ev.target instanceof Node && !menuEl.contains(ev.target)) closeMenu()
  })
  el.addEventListener('contextmenu', (ev) => ev.preventDefault())

  const silhouette = (at: DollPlace) => {
    const id = commandTileId(gd, PLACE_SILHOUETTE[at])
    const c = tileCanvas(gd, id === undefined ? [] : [{ t: id }], 1, hudPx(SIZE.item))
    c.classList.add('silhouette')
    return c
  }

  const firstAction = (item: PackItem) => itemActions(item, worn.get(item.letter))[0]
  const itemSocket = (item: PackItem, cls: string) => {
    const sock = h('button', { type: 'button', class: 'inv-socket ' + cls }, tileCanvas(gd, itemTiles(item), 1, hudPx(SIZE.item)))
    const qty = typeof item.qty_field === 'string' ? item[item.qty_field] : item.quantity
    if (typeof qty === 'number' && qty > 1) sock.append(h('span', { class: 'qty' }, String(qty)))
    sock.append(h('span', { class: 'letter' }, item.letter))
    if (item.useless === true) sock.classList.add('useless')
    sock.style.setProperty('--item-colour', colour(item.col))
    sock.addEventListener('mouseenter', () => showItemTip(item, sock))
    sock.addEventListener('mouseleave', hideTip)
    sock.addEventListener('click', () => openMenu(item, sock))
    sock.addEventListener('contextmenu', () => openMenu(item, sock))
    sock.addEventListener('dblclick', () => {
      closeMenu()
      hooks.act(item, firstAction(item).id)
    })
    return sock
  }
  const emptyPlace = (at: DollPlace) => {
    const label = PLACE_LABEL[at]
    const sock = h('button', { type: 'button', class: 'inv-socket empty place-' + at, 'aria-label': label }, silhouette(at))
    const card = () => tipCard({ title: label, sub: 'Nothing here', hints: [{ keys: ['Click'], pad: 'A', label: 'Put something on' }] }, padKind, 'small')
    sock.addEventListener('mouseenter', () => showCards([card()], sock))
    sock.addEventListener('mouseleave', hideTip)
    sock.addEventListener('click', () => hooks.fill(at, label))
    return { sock, card }
  }
  const itemStop = (id: string, item: PackItem, sock: HTMLElement, row: number, col: number): ScreenStop => ({
    id,
    el: sock,
    row,
    col,
    activate: () => openMenu(item, sock),
    quick: () => hooks.act(item, firstAction(item).id),
    describe: () => hooks.act(item, 'describe'),
    quickLabel: firstAction(item).label,
    selectLabel: 'Actions',
    onFocus: () => showItemTip(item, sock),
  })

  const dollEl = h('div', { class: 'inv-doll' }, h('div', { class: 'inv-figure' }, figureCanvas(gd, playerSprite(opts.me), hudPx(SIZE.figure))))
  for (const at of DOLL_PLACES) {
    const item = doll[at]
    const spot = PLACE_AT[at]
    if (item) {
      const sock = itemSocket(item, 'place-' + at)
      dollEl.append(sock)
      sock.style.gridColumn = String(spot.col + 1)
      sock.style.gridRow = String(spot.row + 1)
      stops.push(itemStop('doll:' + at, item, sock, spot.row, spot.col))
      continue
    }
    const { sock, card } = emptyPlace(at)
    dollEl.append(sock)
    sock.style.gridColumn = String(spot.col + 1)
    sock.style.gridRow = String(spot.row + 1)
    stops.push({ id: 'doll:' + at, el: sock, row: spot.row, col: spot.col, activate: () => hooks.fill(at, PLACE_LABEL[at]), selectLabel: 'Put something on', onFocus: () => showCards([card()], sock) })
  }

  const pack = items.filter((i) => !onDoll.has(i.letter))
  const rows = Math.max(BACKPACK_MIN_ROWS, Math.ceil(pack.length / BACKPACK_COLUMNS))
  const grid = h('div', { class: 'inv-grid', style: { '--cols': String(BACKPACK_COLUMNS) } })
  for (let i = 0; i < rows * BACKPACK_COLUMNS; i++) {
    const item = pack[i]
    if (!item) {
      grid.append(h('div', { class: 'inv-socket bag empty' }))
      continue
    }
    const sock = itemSocket(item, 'bag')
    grid.append(sock)
    stops.push(itemStop('bag:' + item.letter, item, sock, Math.floor(i / BACKPACK_COLUMNS), 4 + (i % BACKPACK_COLUMNS)))
  }
  const p = opts.player
  const summary = h(
    'div',
    { class: 'inv-summary' },
    summaryStat('AC', p.ac, 'Armour class'),
    summaryStat('EV', p.ev, 'Evasion'),
    summaryStat('SH', p.sh, 'Shield'),
    h('span', { class: 'gold', title: 'Gold' }, h('span', { class: 'coin' }), String(p.gold)),
    opts.checking ? h('span', { class: 'checking' }, 'reading what is worn') : null,
    h('span', { class: 'slots', title: 'Items carried' }, `${items.length} / ${PACK_LETTERS}`),
  )
  el.append(h('div', { class: 'inv-panels' }, h('section', { class: 'inv-paper' }, dollEl), h('section', { class: 'inv-bag' }, grid)), summary, tipLayer, menuEl)
  const input = (op: ClientOverlayOp): boolean => {
    if (!menu) return false
    if (op === 'next') focusMenu(menu.focus + 1)
    else if (op === 'prev') focusMenu(menu.focus - 1)
    else if (op === 'select' || op === 'space' || op === 'submit') runMenu(menu.focus)
    else if (op === 'cancel') closeMenu()
    else if (op === 'close') return false
    return true
  }
  return { el, stops, input }
}

function wornIn(doll: Partial<Record<DollPlace, PackItem>>, slot: EquipSlot): PackItem[] {
  if (slot === 'ring') return [doll.ring1, doll.ring2].filter((i): i is PackItem => !!i)
  const placed = doll[slot]
  return placed ? [placed] : []
}

function cleanTitle(item: PackItem): string {
  return itemTitle(item).replace(/\s*\{[^}]*\}\s*/g, ' ').trim()
}

function itemTip(item: PackItem, worn: Worn | undefined, compareTo: PackItem | undefined, kicker: string | undefined, withHints: boolean): Tip {
  const slot = equipSlotOf(item)
  const parts = itemNameParts(item)
  const kind = slot ? SLOT_WORD[slot] : (CLASS_WORD[item.base_type ?? -1] ?? 'Item')
  const theirs = compareTo ? itemNameParts(compareTo).properties : []
  const ego = slot && parts.ego && !parts.properties.length ? [{ text: parts.ego }] : []
  const properties = parts.properties.map((text) => ({ text, tone: compareTo && !theirs.includes(text) ? propertyTone(text) : undefined }))
  const first = itemActions(item, worn)[0]
  return {
    kicker,
    title: cleanTitle(item),
    colour: colour(item.col),
    sub: [kind, worn ? wornWord(worn) : null].filter(Boolean).join(' · '),
    body: item.useless === true ? 'Of no use to you' : undefined,
    lines: compareStats(item, compareTo),
    tags: [...ego, ...properties],
    hints: withHints ? [{ keys: ['Enter'], pad: 'A', label: 'Actions' }, { keys: itemActionKeycaps(first, item.letter), pad: 'Y', label: first.label }] : [],
  }
}

function propertyTone(text: string): 'better' | 'worse' {
  return /^[-*]|-$/.test(text) ? 'worse' : 'better'
}

function summaryStat(label: string, value: number, title: string): HTMLElement {
  return h('span', { class: 'stat', title }, h('b', null, String(value)), h('span', { class: 'k' }, label))
}

function wornWord(w: Worn): string {
  return { weapon: 'wielded', offhand: 'in the off hand', worn: 'worn', installed: 'installed', melded: 'melded', quivered: 'quivered', haunted: 'haunted' }[w]
}

export interface CharacterHooks {
  run(entry: CommandEntry): void
}

/** The extra screens of crawl's the character sheet links to, after the ones the Start menu's Character tab lists. */
const OVERVIEW: CommandEntry = { label: 'Dungeon overview', key: 'Ctrl-O', sub: 'the levels, branches and altars you have found', tile: 'CMD_DISPLAY_OVERMAP', action: { kind: 'keys', label: 'Dungeon overview', seq: [{ key: 15 }] } }

/**
 * The character sheet: the figure, the name and the god, the numbers the
 * stats pane prints (health, magic, the defences, the attributes, the
 * level), the status lights with what each means, and crawl's own screens
 * as buttons: resistances and equipment, mutations, religion, skills, the
 * spell list, the overview and the rest of the Character commands.
 */
export function characterScreen(opts: { player: PlayerState; me: Billboard | undefined; gd: Gamedata | null; weapon: string; quiver: string }, hooks: CharacterHooks): Screen {
  const p = opts.player
  const gd = opts.gd
  const stops: ScreenStop[] = []
  const species = p.species_display_name || p.species
  const piety = p.god && p.god !== 'Gozag' ? '★'.repeat(Math.max(0, p.piety_rank)) + '☆'.repeat(Math.max(0, 6 - p.piety_rank)) : ''
  const head = h(
    'div',
    { class: 'char-head' },
    h('div', { class: 'char-figure' }, figureCanvas(gd, playerSprite(opts.me), hudPx(SIZE.characterFigure))),
    h(
      'div',
      { class: 'char-name' },
      h('h2', null, p.name),
      h('div', { class: 'char-title' }, (p.title || '').replace(/^,\s*/, '').trim()),
      h('div', { class: 'species' }, `Level ${p.xl} ${species}`),
      p.god ? h('div', { class: 'god' + (p.penance ? ' penance' : '') }, `of ${p.god} `, h('span', { class: 'piety' }, piety)) : h('div', { class: 'god none' }, 'No god'),
      h('div', { class: 'xp' }, h('div', { class: 'xp-track' }, h('div', { class: 'xp-fill', style: { width: Math.max(0, Math.min(100, p.progress)) + '%' } })), h('span', null, `${p.progress}% to level ${p.xl + 1}`)),
    ),
  )
  const attr = (label: string, now: number, max?: number, tip?: string) =>
    h('div', { class: 'char-stat' + (max !== undefined && now < max ? ' drained' : ''), title: tip ?? '' }, h('span', { class: 'k' }, label), h('span', { class: 'v' }, String(now) + (max !== undefined && now < max ? ` (${max})` : '')))
  const mod = (n: number) => (n ? (n > 0 ? ` +${n}` : ` ${n}`) : '')
  const vitals = h(
    'div',
    { class: 'char-block vitals' },
    h('h3', null, 'Vitals'),
    attr('Health', p.hp, undefined, 'Hit points'),
    h('div', { class: 'char-stat' }, h('span', { class: 'k' }, 'Max health'), h('span', { class: 'v' }, String(p.hp_max) + (p.real_hp_max > p.hp_max ? ` (${p.real_hp_max})` : ''))),
    attr('Magic', p.mp),
    h('div', { class: 'char-stat' }, h('span', { class: 'k' }, 'Max magic'), h('span', { class: 'v' }, String(p.mp_max))),
    h('div', { class: 'char-stat' }, h('span', { class: 'k' }, 'Gold'), h('span', { class: 'v gold' }, h('span', { class: 'coin' }), String(p.gold))),
    h('div', { class: 'char-stat' }, h('span', { class: 'k' }, 'Place'), h('span', { class: 'v' }, placeName(p))),
    h('div', { class: 'char-stat' }, h('span', { class: 'k' }, 'Turns'), h('span', { class: 'v' }, String(p.turn))),
  )
  const defence = h(
    'div',
    { class: 'char-block defence' },
    h('h3', null, 'Defence'),
    h('div', { class: 'char-stat', title: 'Armour class: how much of a blow your armour takes' }, h('span', { class: 'k' }, 'Armour'), h('span', { class: 'v' }, String(p.ac) + mod(p.ac_mod))),
    h('div', { class: 'char-stat', title: 'Evasion: how often blows and missiles miss you' }, h('span', { class: 'k' }, 'Evasion'), h('span', { class: 'v' }, String(p.ev) + mod(p.ev_mod))),
    h('div', { class: 'char-stat', title: 'Shield: how often your shield blocks' }, h('span', { class: 'k' }, 'Shield'), h('span', { class: 'v' }, String(p.sh) + mod(p.sh_mod))),
    h('h3', null, 'Attributes'),
    attr('Strength', p.str, p.str_max),
    attr('Intelligence', p.int, p.int_max),
    attr('Dexterity', p.dex, p.dex_max),
  )
  const arms = h(
    'div',
    { class: 'char-block arms' },
    h('h3', null, 'Arms'),
    h('div', { class: 'char-line', html: formattedStringToHtml(opts.weapon) }),
    h('div', { class: 'char-line', html: formattedStringToHtml(opts.quiver || 'Nothing quivered') }),
    h('h3', null, 'Status'),
    ...(p.status.filter((s) => s.light).length
      ? p.status.filter((s) => s.light).map((s) => h('div', { class: 'char-status' }, h('span', { class: 'light', style: { color: colour(s.col) } }, s.light || ''), h('span', { class: 'desc' }, s.desc || s.text || '')))
      : [h('div', { class: 'char-line dim' }, 'Nothing out of the ordinary.')]),
  )
  const commands = [CHARACTER_COMMANDS[0], CHARACTER_COMMANDS[2], CHARACTER_COMMANDS[1], CHARACTER_COMMANDS[6], CHARACTER_COMMANDS[5], CHARACTER_COMMANDS[4], OVERVIEW, ...CHARACTER_COMMANDS.slice(7)]
  const buttons = h('div', { class: 'char-commands' })
  commands.forEach((c, i) => {
    const id = commandTileId(gd, c.tile)
    const el = h('button', { type: 'button', class: 'char-command', title: c.sub ?? '' }, id === undefined ? h('span', { class: 'tile blank' }) : tileCanvas(gd, [{ t: id }], 1, hudPx(32)), h('span', { class: 'label' }, c.label), keycaps(c.key.split(' ')))
    el.addEventListener('click', () => hooks.run(c))
    buttons.append(el)
    stops.push({ id: 'cmd:' + c.key, el, row: Math.floor(i / 3), col: i % 3, activate: () => hooks.run(c) })
  })
  const el = h('div', { class: 'character' }, head, h('div', { class: 'char-blocks' }, vitals, defence, arms), h('h3', { class: 'char-more' }, 'More about you'), buttons)
  return { el, stops }
}

function placeName(p: PlayerState): string {
  return p.depth ? `${p.place}:${p.depth}` : p.place
}

/** The weapon line of the stats pane, as player.js writes it: the wielded item's name, or the unarmed attack. */
export function weaponLine(p: PlayerState): string {
  const item = p.weapon_index >= 0 ? p.inv[p.weapon_index] : undefined
  const name = item && typeof item.name === 'string' ? item.name : ''
  return name ? formattedStringToText(name) : p.unarmed_attack || 'Nothing wielded'
}
