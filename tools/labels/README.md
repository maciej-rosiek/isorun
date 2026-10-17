# Tile labels

What each DCSS tile is made of and how its surface behaves: the material, the relief and its joints,
whether its light texels are the high ones, the gloss, whether it is wet, whether it glows or
sparkles, whether it is see-through, and how a creature moves. The renderer builds each tile's
height, gloss, glow and sparkle from them (`native/crates/render/src/enhance.rs`), and an actor's
motion and see-through look (the render crate, `labels.rs`).

- `labels.json`: the labels by tile name, as the labelling gave them (the source of truth).
- `native/crates/render/src/tile-labels.json`: the same labels by the pixel hash of every variant
  (`tileHash`), which the renderer reads. A tile finds its label by its art, so the labels hold on
  any server and version that has the same art, bcrawl included; a tile with no label gets a
  little relief and nothing else (`UNLABELLED`).
- `SCHEMA.md`: the task and the fields, as the labelling agents were given them.

To label a new gamedata version:

1. Build the engine (CLAUDE.md), or fetch the version's `tileinfo-*.js` and atlas PNGs into one
   directory.
2. `node tools/build/tile-sheets.mjs engine/dist/gamedata/<version> /tmp/isorun-labels/sheets`
   writes numbered contact sheets of every base tile, a `.txt` with the names on each, and
   `index.json` with the ids and hashes.
3. Label each sheet as `SCHEMA.md` says into `/tmp/isorun-labels/out/sheet-NNN.json`. The labels
   in the repository were made by Claude Haiku agents, about ten sheets each, with the sheet image
   and its names.
4. `node tools/labels/merge.mjs /tmp/isorun-labels/sheets /tmp/isorun-labels/out` checks every
   sheet against the schema, reports sheets with missing or odd labels, and writes both files.
