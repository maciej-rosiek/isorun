# isorun-overlays

The screens crawl puts over the game, on egui: the port of `apps/isorun/src/overlays.ts` with
what only it uses (`menu-nav.ts`, `focus.ts`, `crt-scrape.ts`, `level-up.ts` and `level-up.css`)
and the part of `context.ts` that reads crawl's prompts. It draws:

- **crawl's menus**: the title (or the filter's field, `title_prompt`), the rows with their item
  tiles, the `more` line with its switches (`[Esc] exit`, the shop's `[!] buy|examine`), the hover
  the cursor moves (told to crawl with `menu_hover` and a debounced `menu_scroll`), and the paging
  menu.js does on the client (PgUp / PgDn, Home / End, `<`, `>`, `-`, `+`, `;`, Space, Shift-arrows);
- **its popups**: the describe screens (item, spell, monster with its panes, god, feature, cards),
  the formatted scrollers (help, the overview, the character sheet, which dims the world), version,
  game over, progress bar, seed selection, the random-combo question;
- **character creation**: the species / background / weapon grids, the hero and description beside
  them (`outer_menu_focus` on every move), stacked on a phone;
- **the CRT screens** (the skills screen), with a cursor over the rows the scraper finds;
- **the prompt card**: a yes/no (A Yes, B No, Enter crawl's default as the chip says), a choice
  prompt (a row of chips the cursor walks), the letter picker of `=`;
- **the text field** of crawl's text prompts (history, the prompts' own hotkeys: textinput.js);
- **the level-up banner** with the gains read off the `player` message, and crawl's stat-gain
  prompt as three cards;
- the `show_dialog` html's buttons.

## API

```rust
use isorun_overlays::{Overlays, OverlayCx, FocusOp, MenuOp};

let mut overlays = Overlays::new();          // one per game screen
// each frame, after drawing the view:
overlays.messages(&new_lines);               // the message lines that arrived (the level-up banner)
let mut out = Vec::new();                    // client messages the overlays send
let mut cx = OverlayCx::new(&state, &mut tile_icons, &mut out);
cx.device = Device::Pad;                     // who spoke last (key caps or pad glyphs on chips)
cx.touch = true;                             // a coarse pointer: round close buttons
cx.ui_scale = 1.0;
overlays.ui(ctx, &mut cx);                   // draw what crawl has up, answer the mouse
let took = overlays.input(ctx, &mut cx);     // this frame's keys: true when they were the overlays'
for m in out { session.send(m) }             // send what they asked
```

- `Overlays::up(&state)` / `Overlays::mode(&state)`: whether something of crawl's is up, the mode
  (`prompt::Mode`, context.ts `deriveMode`).
- `key(&egui::Event, cx) -> bool`: one key (`input` calls it for every event of the frame). On a
  screen the keyboard is a pad (bindings.ts `screenKey`): Enter is A, Escape B, Space A (pages text
  to read), the arrows the d-pad, PgUp / PgDn the bumpers. Keys nothing takes go to crawl as the
  official client sends them (`input::raw_key`, keys.ts `keydownMessage`). A key also answers the
  level-up banner.
- `focus_op(FocusOp, cx)` and `menu_op(MenuOp, cx)`: the pad's operations (focus.ts `FocusOp`, the
  menu ops of bindings.ts), the same path the keys take; `focus_info(&state)` is what A and B do,
  for an action bar; `popup_actions()` / `trigger_popup_action(i, cx)` the top popup's verbs (X);
  `pageable(cx)` whether the bumpers have a page to turn; `hover_hotkey(cx, key)`; `back(cx)`.
- `OverlayCx::hide_menu`: a menu the game opened only to read (game.ts `probeGear`) is not drawn;
  `OverlayCx::back`: what a popup's close button and the phone's Back do (Escape without it).
- The pure parts have their own modules and tests: `focus` (the cursor), `menu_nav` (menu.js),
  `crt_scrape`, `prompt` (deriveMode, parsePrompt, promptLead), `rich` (formatted strings and the
  server's HTML as egui text, with CSS `pre-wrap`).

The shared look (the popup panel, key caps, cards, buttons) is `isorun_ui::widgets`.

## Not here

- The client's own overlays of overlays.ts (the command palette, the Isorun menu, the settings,
  the controls sheet, X's action tabs, the dock's screens) belong to the HUD and the front end; the
  pack's page tabs (pack-tabs.ts) too.
- The on-screen keyboard (osk.ts) for typing with a pad, and the pad's button glyphs (glyphs.ts):
  a chip names its button in text (`(A)`) until they are ported.
- `--more--` has no overlay: the message pane (the HUD) shows it.

## Try it

    cargo run -p isorun-overlays --example overlays

plays an offline game with the overlays over it (see the example's header for `OVERLAYS_FAKE`,
which puts up a level-up, the stat-gain cards, a yes/no, the letter picker or the skills screen).
`cargo test -p isorun-overlays` runs the tests.
