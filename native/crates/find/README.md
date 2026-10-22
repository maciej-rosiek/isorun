# isorun-find

The Find screen: everything crawl's stash tracker knows (items, shops, altars, stairs, portals
and branch entrances, traps and other features, on every level seen), by category, sorted for
the player, and a tap to travel there. It replaces crawl's bare "Search for what?" prompt as the
way to find things (Ctrl-F, the dock's Find, the pad's L3); its search field still runs crawl's
own search.

## How it works

The screen drives crawl's own stash search (stash.cc `search_stashes`) behind it, so the results
and the travel are crawl's, the same on a server (WebTiles) and offline (`driver.rs`):

- crawl's prompt (`init_input` tagged `stash_search`) is answered at once with `..`, which every
  result matches and crawl lists unhighlighted; the prompt is never drawn. More than 400 results
  ("Too many matches") and it asks for the player's level only (`@..`) and says so.
- crawl's results menu (tag `stash`) stays up under the screen, hidden; its rows are read into
  entries (`entry.rs`: `[D:2] 4 poisoned darts (3 further duplicates)`, `[right here]`,
  `[carried] c - ...`) and put in a category by their tile's name in the tile tables (the text's
  words where there is none).
- a pick is crawl's own: `menu_hover` on the row, Enter, then `.` on the level map crawl shows in
  travel mode. `=` shows or hides useless items and duplicates (crawl lists again); the field's
  words are a new crawl search; Escape closes crawl's menu with the screen.

The order (`rank.rs`): crawl's distance order is the backbone (the player's level first, then
by crawl's travel distance between levels, nearest first on the current level); within a level,
new things first (unidentified, artefacts, runes, unvisited shops), then the rest, then items of a
kind the player carries. A–Z sorts by name instead. The pack's own rows are not listed.

The screen (`screen.rs`): the panel fills a phone's safe area and stands in the middle of a big
screen; the categories are chips over the list on a narrow panel and a column beside it on a wide
one; a tap travels, a long press (350 ms) or the mouse's hover shows the details card; the keys
and the pad walk the rows (up, down), the categories (left, right, LB, RB), travel (A, Enter),
show the card (X), switch the sort (Y, `/`) and close (B, Escape); typing goes into the search.

## API

```rust
use isorun_find::{Driver, FindScreen, FindEvent, View, entry};

let mut driver = Driver::default();          // one per game
let mut screen = FindScreen::new();
// every frame, after the state:
driver.update(&state, watching, now_ms, &|t| entry::tile_name(&gd, t).map(str::to_string), &mut out);
// while driver.open():
let here = entry::level_label(&state.player.place, state.player.depth);
for e in screen.ui(ctx, &View { driver: &driver, here: &here, scale, pad, touch }, &mut icons) {
    match e {
        FindEvent::Travel(x) => driver.travel(&x, &mut out),
        FindEvent::Close => driver.close(&state, now_ms, &mut out),
        FindEvent::Search(words) => driver.search(&words, now_ms, &mut out),
        FindEvent::ToggleUseless => driver.toggle_useless(&mut out),
    }
}
// the overlays leave crawl's stash prompt and menu undrawn while driver.hides()
```

`screen.op(FindOp)` and `screen.hotkey(key)` take the router's operations and typed keys;
`screen.typing(ctx)` says the search field has the keys.

## Try it

    cargo test -p isorun-find
    ISORUN_DATA_DIR=/tmp/rec cargo run -p isorun-find --example record_stash   # a menu fixture

The tests read `native/fixtures/menus/menu-stash-1.json`, recorded from an offline game with the
example.
