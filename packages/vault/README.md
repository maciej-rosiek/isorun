# @isorun/vault

A supported subset of DCSS's `.des` vault syntax, parsed and compiled to a
`@isorun/scene` `Scene` that the renderers draw exactly as they draw a live
level. Written for Isorun's front room, a place authored by hand from the
game's own tiles; useful for any static scene, test bed, or level preview.

Accepted: `NAME`, `MAP … ENDMAP`, `TILE`, `FTILE`, `KFEAT`, `KITEM`,
`KMONS`, comments, and two directives of Isorun's own, `EYE` (where the
camera stands and faces) and `LIGHT`. Every other directive of the vault
language (`SUBST`, `SHUFFLE`, `MARKER`, Lua, and so on) is refused with its
line number rather than passed over, so a file that parses is a file that
is fully honoured.

- `parseDes(text)` → `Vault`, or throws `DesError` with a line number.
- `vaultScene(vault, names)` → `Scene`, given a `TileNames` resolver: any
  `id(name)` lookup, such as a packed atlas or a `@isorun/gamedata` name
  index.
- `vaultCamera(vault)` → the `EYE` as a `Camera`.
- `vaultTileNames(vault)` lists every tile the file asks for, so a build can
  check its atlas has them.

Depends on `@isorun/scene` and `@isorun/scene-webtiles`. Part of
[Isorun](https://github.com/maciej-rosiek/isorun). AGPL-3.0-or-later.
