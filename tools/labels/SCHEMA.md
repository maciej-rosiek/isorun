# Tile labels

Each sheet `sheet-NNN.png` shows up to 48 game tiles from Dungeon Crawl Stone Soup, 4x upscaled
on a grey background. The white number above each tile is its index `i`. `sheet-NNN.txt` lists
each index with the tile's internal name and its atlas:

- floor: floor tiles (seen from above, the ground)
- wall: wall faces
- feat: dungeon features (doors, stairs, altars, fountains, statues, traps, trees, ...)
- main: items lying on the ground, clouds, spell bolts
- player: monsters, and the parts of the player's paper doll (body armour, helmets, weapons in hand, hair, ...)

The name is a strong hint. Use both the name and what you see.

For every index, write one JSON object with exactly these fields:

| field | values | meaning |
|---|---|---|
| `i` | number | the index |
| `material` | `stone`, `brick`, `rock`, `marble`, `dirt`, `sand`, `mud`, `grass`, `moss`, `wood`, `metal`, `gold`, `crystal`, `gem`, `ice`, `glass`, `bone`, `flesh`, `fur`, `scales`, `chitin`, `slime`, `fire`, `lava`, `water`, `cloth`, `leather`, `paper`, `plant`, `magic`, `shadow`, `light`, `other` | what the tile mostly is made of |
| `relief` | 0, 1, 2, 3 | how bumpy the surface is: 0 flat (smooth, painted, liquid, abstract), 1 low (sand, dirt, carpet, fine grain), 2 medium (worn stone, planks, rough hide), 3 high (bricks, cobbles, rock faces, big blocks) |
| `bright_is_high` | true, false | true when the lighter pixels are the raised parts (stones over dark mortar); false when the lighter pixels are lower (light mortar or grout between darker stones) |
| `joints` | `none`, `bricks`, `blocks`, `cobbles`, `planks`, `tiles`, `cracks` | the pattern of joints between parts, if any |
| `gloss` | 0, 1, 2, 3 | 0 matte (dirt, cloth, rough stone, fur), 1 slight sheen (polished stone, varnished wood, skin), 2 shiny (metal, wet surfaces, glass, ice, slime), 3 very bright highlights (gold, gems, crystal, polished steel) |
| `wet` | true, false | the surface itself looks wet, slimy or dripping |
| `glow` | 0, 1, 2 | 0 gives no light; 1 some parts give light (runes, glowing eyes, flames, shining gems, glyphs); 2 the whole tile gives light (fire, lava, energy, light) |
| `sparkle` | true, false | crystal, gems, ice, glitter, stars |
| `translucent` | true, false | see-through: ghosts, spirits, jellies, glass, smoke, clouds |
| `motion` | `none`, `breathe`, `hover`, `flicker` | only for creatures: `breathe` a living creature that stands or walks, `hover` a flying or floating one (bats, insects, ghosts, eyes), `flicker` fire or energy beings; `none` for everything else (items, terrain, statues, constructs, skeletons, doll parts) |

Write the result for a sheet as a JSON array of these objects (one per index, in order) to
`<out dir>/sheet-NNN.json`, with the same NNN as the sheet. Write only the JSON
array to the file, no other text. Every index in the sheet's `.txt` must have one object.

Example object:
{"i":3,"material":"brick","relief":3,"bright_is_high":true,"joints":"bricks","gloss":0,"wet":false,"glow":0,"sparkle":false,"translucent":false,"motion":"none"}
