# isorun-vault

A supported subset of DCSS's `.des` vault syntax, parsed and compiled to an
`isorun-scene` `Scene` that the renderers draw exactly as they draw a live
level; the port of `packages/vault`. Written for Isorun's front room and the
isometric test bed (`apps/isorun/testbed/iso.des`), places authored by hand
from the game's own tiles; useful for any static scene or level preview.

Accepted: `NAME`, `MAP … ENDMAP`, `TILE`, `FTILE`, `KFEAT`, `KITEM`,
`KMONS`, comments, and two directives of Isorun's own, `EYE` (where the
camera stands and faces) and `LIGHT`. Every other directive of the vault
language (`SUBST`, `SHUFFLE`, `MARKER`, Lua, and so on) is refused with its
line number rather than passed over, so a file that parses is a file that is
fully honoured.

- `parse_des(text)` → `Vault`, or a `DesError` with its `line`.
- `vault_scene(&vault, &names)` → `Scene`, given a `TileNames` resolver: any
  `Fn(&str) -> Option<TileId>`, a `PackedAtlas`, or your own impl with
  `tile_count` for the variants (`variant_at` picks one by position).
- `vault_camera(&vault)` → the `EYE` as a `Camera`.
- `vault_tile_names(&vault)` lists every tile the file asks for, so a build
  can check its atlas has them.
- `atlas`: `PackedAtlas::from_json(atlas.json)`, a packed atlas as
  `tools/build/pack-room.mjs` writes it (the port of the room's
  `RoomTiles`): `tile(id)` → `TileRect` (every rect names the one atlas
  `PACKED_ATLAS`, "atlas"; the caller decodes `atlas.png` and binds it under
  that name), `id(name)`, `tile_count(id)`, and `sprite_rects` (the tiles that
  came from crawl's feature, item and monster atlases).

```rust
use isorun_vault::{parse_des, vault_scene, PackedAtlas};

let atlas = PackedAtlas::from_json(&std::fs::read_to_string("public/iso/atlas.json")?)?;
let scene = vault_scene(&parse_des(&std::fs::read_to_string("testbed/iso.des")?)?, &atlas)?;
```

Depends on `isorun-scene` and `isorun-scene-webtiles`. Part of
[Isorun](https://github.com/maciej-rosiek/isorun). AGPL-3.0-or-later.
