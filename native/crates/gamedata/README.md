# isorun-gamedata

Reads a DCSS WebTiles server's own version-specific data, the way the
official client does: `enums.js`, the `tileinfo-*.js` modules for every
atlas and `status-icon-sizes.js`. Nothing version-specific is hardcoded, so
stable, trunk and forks all work and a new release needs no update here. The
port of `packages/gamedata`.

The browser client evaluates these scripts; here a small interpreter
(`src/js.rs`) runs the subset of JavaScript they are written in (AMD
`define`, `val++` numbering and its aliases, the `tile_info` arrays and the
functions over them, the flag tables, `switch`), so they are read by
structure, not by layout. What it does not know is an error naming the file
and the line; every script runs on a step budget. No IO: the caller hands it
the text of each file and decodes the atlas images itself.

- `load_gamedata(source, version)` reads the scripts through a
  `GamedataSource` (any `FnMut(&str) -> Result<String, String>`, by file
  name) and returns a `Gamedata`, or a `GamedataError` naming the file;
  `load_gamedata_at` names it under `<base>/gamedata/<version>`,
  `load_gamedata_from_map` and `Gamedata::from_files` take the files
  already fetched. A missing `status-icon-sizes.js` and other non-fatal
  problems land in `Gamedata::diagnostics`.
- `SCRIPTS`, `script_files()` (with `status-icon-sizes.js`),
  `gamedata_files()` (with the `<atlas>.png`s, last) and `gamedata_urls(base,
  version)` list what to fetch up front.
- `Gamedata` answers `tile(id, layer)` with the atlas and source rect of a
  tile (`isorun_scene::TileRect`; the layer is an atlas name, a texture index
  with the server's id, or none), `tile_in`, `tile_for_texture`,
  `tile_info`, `tile_count(id)`, `basetile(id)`, `atlas_names()`,
  `sprite_atlases()` and `status_icon_size(id)`. Its name indices (`dngn`,
  `main`, `player`, `gui`, `icons`) map `name(id)`, `id(name)` and
  `base_name(id)`; `ranges` gives each module's id span; `fg(word)` and
  `bg(word)` decode cell flag words (a JSON number, or `[lo, hi]` past 32
  bits; `fg_bits`, `bg_bits` take the halves) as the server's
  `prepare_fg_flags` and `prepare_bg_flags` do, into a `FlagWord` with the
  tile `value` and `get(flag)`; `enums` holds every enum table.
- A server that starts some modules at 0 again (bcrawl) has them moved up to
  stand one after the other: every id `Gamedata` takes or hands out is the
  client's, `server_tile` moves one the server sent, and `TileModule::offset`
  says by how much.
- `verify_enums(gd, expected)` reports any enum value a client assumed that
  the server disagrees with.

A tile of odd width is half a texel off centre in the JavaScript client;
`TileRect::ox` is whole, so it is rounded down (`tile_info` has the raw
entry).

Depends on `isorun-scene`, `serde`, `serde_json`, `regex` and `once_cell`.
Part of [Isorun](https://github.com/maciej-rosiek/isorun). AGPL-3.0-or-later.
