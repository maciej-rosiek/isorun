# isorun-hud

Isorun's HUD on egui: the port of `apps/isorun/src` `hud.ts`, `dock.ts` + `dock.css` (and the
compact dock of `touch.css`), `quick-slots.ts`, `dock-actions.ts`, `dock-memory.ts`,
`hud-unit.ts`, `tooltip.ts`, `gear.ts`, `item-compare.ts`, `screens.ts`, `pack-tabs.ts`,
`damage-numbers.ts` + `.css`, `chat.ts`, `glyphs.ts` and `grid/`.

| module | what | from |
|---|---|---|
| `hud` | `Hud`: the whole HUD as the game screen drives it, and the game screen's helpers that belong to it | `game.ts` `relayout`, `updateDock`, `slotSources`, `loadMemory`, `learnFromMenus`, `probeGear`, `dockButton`, `showInventory`, `itemAct`, `fillPlace`, `trackResume` |
| `dock` | `Dock`: orbs, status chips, AC/EV/SH and attributes, quick slots, the square's actions, the screen buttons, the pad's cursor on it; the compact dock under 900 × 560 | `dock.ts`, `dock.css`, `touch.css` |
| `classic` | the panes on the grid: stats pane with portrait, monster list, status strip, trapped mark, action panel, message pane and its `--more--` scrim, the pad's prompts and menu buttons, the status line, the inspect card; the minimap's slot | `hud.ts` |
| `screens` | the inventory (paper doll, backpack, item cards against what they replace, item menu) and the character sheet | `screens.ts` |
| `tooltip` | the cards and the keycaps | `tooltip.ts` |
| `glyphs` | the pad's button glyphs, drawn from the SVGs' paths | `glyphs.ts` |
| `damage_numbers`, `chat` | the numbers over the actors; the WebTiles chat box | |
| `quick_slots`, `dock_actions`, `dock_memory`, `gear`, `item_compare`, `pack_tabs` | pure logic, unit-tested | the same names |
| `grid` | the console grid (`fit`, `game_split`), text rows, the stats and message panes as rows, the action panel's arithmetic, `paint_row` | `grid/*.ts` |
| `paint` | CSS boxes: gradients (linear, radial, conic, repeating), borders and inset shadows rasterised once into textures (`BoxCache`: big ones soft at first and sharp from a thread, kept for minutes), outer shadows, text shadows | `dock.css`, `styles.css` |
| `style` | the colours of `dock.css :root`, Cinzel's 600/700/900 weights, `--hu` (`hud_unit`), the compact window | `hud-unit.ts`, `touch.ts` |

## Driving it

Once at startup, after `isorun_ui::install(ctx)`:

```rust
isorun_hud::install(ctx); // Cinzel's weights beside isorun-ui's fonts
let mut hud = isorun_hud::Hud::new();
```

Each frame, the layout first (it tells the view where to draw and how far to lift its centre),
then the view, then the HUD over it:

```rust
let layout = hud.layout(ctx, screen_rect, &state, &settings, map_view);
renderer.set_centre_lift(layout.centre_lift);     // IsoRender3d.setCentreLift
view.paint(ui, rs, layout.view);
// the 2D renderer draws the minimap in layout.minimap (empty Rect::NOTHING: none)
let out = hud.ui(ui, &mut HudCx {
    state: &state, scene: &scene, tiles: &mut tile_icons, settings: &settings, now,
    mode,                 // the game screen's context mode (or isorun_hud::hud::guess_mode)
    device, pad, touch,   // what spoke last, the pad's kind, a coarse pointer
    facing,               // the camera's facing, for the monster list's arrows
    map_view,
    dock_context: None,   // Some(DockContext { under, hostiles_in_view }) from the context, or read off the scene
    prompts, menu_prompts, holding,   // the pad's prompts (bindings.ts promptLabels / barLabels)
    panel,                // a client panel's rect: the prompts stand under it
    inspect,              // the card about the cell under the pointer (cell-info.ts)
    client_overlay,       // a client overlay of the game screen's is up
    screen_of: &|x, y| ..., // a cell's actor on the screen (renderer.screen_of + the view's corner)
    out: &mut messages,   // client messages for crawl: send them
    commands: &mut asks,  // HudCommand: run, system menu, action tabs, choices, ...
});
if out.consumed_keys { /* no keys to crawl this frame */ }
if out.hide_menu { /* crawl's pack is up only to be read: do not draw it */ }
```

`HudCommand`s for the game screen:

- `Run(Action)`: through the runner (`Keys` with steps that wait for crawl's prompt, `Fire`,
  `Send`, `SendThen` a describe and then keys);
- `SystemMenu`, `ActionTab("spells" | "abilities" | "quiver")`, `CloseClientOverlay`;
- `Choices { title, choices, remember }`: show them with the overlays' chooser; call
  `hud.choose(row, cx)` with the pick, or `hud.choices_closed(cx)`;
- `SelectMonster { x, y }` (a monster row tapped), `MinimapClick`, `PanelSettings`,
  `PanelShown(bool)` (game.ts `setPanelShown`), `Prompt(i)` (the pad prompt `i` was clicked).

Other calls: `hud.status(text, now)` (the line at the top for three seconds),
`hud.dock_focus()` and `hud.dock_nav(NavOp, cx)` (L3 and the pad on the dock),
`hud.damage.landing(Landing, cell, now, enabled)` when the renderer shows a blow
(`IsoRender3d.act`'s `onShown`), `hud.classic.scroll_log(delta)`, `hud.chat.focus()` (F12),
`hud.screen_open()`, `hud.close_screen()`, `hud.screen_op(ScreenOp)` (the pad on a screen: Next / Prev /
Left / Right walk the stops, Select opens an item's actions or runs a command, Quick is Y's first
action, Describe is X, Cancel closes the item menu then the screen, the bumpers page ten stops).
A game screen with a router sets `hud.screens_read_keys = false` and hands every key as an op;
the pad's chips (A, Y, X, B) show under the screen while the pad speaks.

What the dock knows of a character (pins, spells, abilities, what is worn) is kept in storage
under `isorun.dock`, in the web build's shape.

## See it

    ISORUN_PROFILE=hud cargo run -p isorun-hud --example hud   # HUD=classic for the classic panes

The example plays an offline game in the harness with the HUD over it; `i` and `@` open the
inventory and the character screens.

## Fonts

`assets/fonts/cinzel-{600,700,900}.ttf` are static instances of the variable Cinzel
(`isorun-ui`'s `cinzel-latin.ttf`, SIL Open Font License), cut with fontTools'
`varLib.instancer`: the web build asks for those weights and egui reads only a font's default
instance.
