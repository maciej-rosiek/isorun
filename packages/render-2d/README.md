# @isorun/render-2d

A top-down Canvas 2D renderer of a `@isorun/scene` `Scene`, drawing the
cells in the official client's layer order from any `TileSource`. `Render2d`
implements `MapRenderer`, so it is interchangeable with `@isorun/render-3d`;
Isorun also uses it for the minimap: `mode: 'minimap'` draws solid colour
blocks in the official client's minimap palette, and `'tiles'`, `'glyphs'`
and `'hybrid'` draw the level itself.

Depends on `@isorun/scene`. Part of
[Isorun](https://github.com/maciej-rosiek/isorun). AGPL-3.0-or-later.
