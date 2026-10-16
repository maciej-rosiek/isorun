import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { loadGamedata } from '../src/index'

const SHA = 'acd3d60e20f899c1c8a546953d6ffa0f6c7fe0c8'
const fixtures = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'scene-webtiles', 'test', 'fixtures', 'gamedata', SHA)

/** A tileinfo module as rltiles' tile_list_processor writes it, one 32x32 tile a column. */
function tileinfo(name: string, prefix: string, start: { module: string; value: string } | number, tiles: string[]): string {
  const from = typeof start === 'number' ? String(start) : `m.${start.value}`
  const dep = typeof start === 'number' ? '' : `"./tileinfo-${start.module}"`
  return `define([${dep}], function(m) {
var exports = {};
var val = ${from};
${tiles.map((t) => `exports.${t} = val++;`).join('\n')}
exports.${name.toUpperCase()}_MAX = exports.${prefix}_${name.toUpperCase()}_MAX = val++;
var tile_info = [${tiles.map((_, i) => `{w: 32, h: 32, ox: 0, oy: 0, sx: ${i * 32}, sy: 0, ex: ${i * 32 + 32}, ey: 32}`).join(',')}];
exports.get_tile_info = function (idx) { return tile_info[idx - ${from}]; };
exports.tile_count = function (idx) { return 1; };
exports.basetile = function (idx) { return idx; };
exports.get_img = function (idx) { return "${name}"; };
return exports;
});`
}

/** bcrawl's numbering: main, gui and icons start at 0 again, as DCSS did before it numbered them one after another. */
const bcrawl: Record<string, string> = {
  'enums.js': readFileSync(join(fixtures, 'enums.js'), 'utf8'),
  'tileinfo-floor.js': tileinfo('floor', 'TILE', 0, ['DNGN_UNSEEN', 'FLOOR_GREY_DIRT']),
  'tileinfo-wall.js': tileinfo('wall', 'TILE', { module: 'floor', value: 'TILE_FLOOR_MAX' }, ['WALL_BRICK_DARK']),
  'tileinfo-feat.js': tileinfo('feat', 'TILE', { module: 'wall', value: 'TILE_WALL_MAX' }, ['DNGN_ORCISH_IDOL', 'DNGN_ENTER']),
  'tileinfo-main.js': tileinfo('main', 'TILE', 0, ['MAIN_FILLER_0', 'MONS_RAT', 'MONS_GOBLIN']),
  'tileinfo-player.js': tileinfo('player', 'TILEP', { module: 'main', value: 'TILE_MAIN_MAX' }, ['BASE_HUMAN']),
  'tileinfo-gui.js': tileinfo('gui', 'TILEG', 0, ['GUI_FILLER_0', 'CMD_EXPLORE']),
  'tileinfo-icons.js': tileinfo('icons', 'TILEI', 0, ['ICONS_FILLER_0', 'CURSOR']),
}

describe('tile module numbering', () => {
  it('reads a server that starts main, gui and icons at 0 again with each tile from its own atlas', async () => {
    // given
    const gd = await loadGamedata({
      base: 'https://underhound.eu:8080',
      version: 'bcrawl',
      skipImages: true,
      io: {
        fetchText: async (url) => {
          const src = bcrawl[url.split('/').pop() ?? '']
          if (src === undefined) throw new Error('not published: ' + url)
          return src
        },
        loadImage: async () => {
          throw new Error('no images in this test')
        },
      },
    })

    // when
    const goblin = gd.tile(gd.fg(2).value)
    const explore = gd.tile(gd.gui.id('CMD_EXPLORE') ?? -1)
    const cursor = gd.tile(1, String(gd.enums.texture.ICONS))
    const floor = gd.tile(gd.bg(1).value)

    // then
    expect(goblin).toMatchObject({ atlas: 'main', sx: 64 })
    expect(explore).toMatchObject({ atlas: 'gui', sx: 32 })
    expect(cursor).toMatchObject({ atlas: 'icons', sx: 32 })
    expect(floor).toMatchObject({ atlas: 'floor', sx: 32 })
    expect(gd.main.name(gd.fg(2).value)).toBe('MONS_GOBLIN')
  })
})
