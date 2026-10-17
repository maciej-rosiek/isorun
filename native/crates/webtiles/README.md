# isorun-webtiles

The DCSS [WebTiles](https://crawl.develz.org/wordpress/howto) protocol as a
Rust library, the port of `packages/webtiles`: what the server sends, what a
client sends back, and a reducer that turns the stream of server messages into
one plain `GameState`. No networking, no async, no renderer; serde_json,
regex and once_cell only.

- `protocol.rs`: `Keys` (the special keycodes the server expects for arrows,
  function keys and the keypad, `Keys::f(n)`, `Keys::numpad(n)`), `ctrl()`,
  the client message builders in `cm` (`login`, `register`, `play`, `watch`,
  `key`, `input`, `chat`, `click_cell`, `menu_scroll`, `set_rc`, and the
  rest; each a `serde_json::Value` with its keys in the official client's wire
  order), `key_message()`, and the `MouseMode`, `MenuFlag`, `UiState` and
  `MapFeature` constant tables.
- `connection.rs`: `unpack_frame()`, one server text frame to its messages
  (the `{"msgs": [...]}` batch in order, or the frame itself), and
  `is_ping()`: a ping is answered with `cm::pong()` and not reduced. Binary
  (compressed) frames are not decoded; ask for `no-compression`.
- `state.rs`: `initial_state()` / `GameState::default()` and
  `reduce(&mut state, &message)`: a tolerant reducer over every message type
  the official client handles: the map, the player, items, menus and popups,
  the UI-state stack, prompts, text input, `--more--`, the lobby roster and
  game links, chat, and the message log. A malformed message or field goes to
  `state.diagnostics`, never a panic. The fields the TS names are typed; the
  rest of what the server sent is kept in each struct's `extra` (a popup keeps
  its `ui-push` as sent, in `data`). Cells are keyed by `map_key(x, y)`; a
  monster with an id is shared by the cells that hold it
  (`MapStore::monsters`, ref-counted as map.js does), read through
  `MapStore::monster(cell)`. The `rev` counters, `dirty_cells`,
  `map_cleared` (`clear_dirty()` once a scene is built) and the player's
  `changed` / `previous` say what moved. Helpers: `top_menu`, `top_popup`,
  `has_status`, `action_panel_items`, `index_to_letter`.
- `floor.rs`: what lies underfoot, from the server's floor-items lines:
  `items_underfoot`, `floor_items_label`, `floor_items_count`,
  `is_stationary_item_name`.
- `links.rs`: the lobby's game links: `GameLink`, `game_link_version`,
  `latest_game_links`, `game_link_rows`, `game_version_rows`.
- `format.rs`: DCSS formatted strings (`<red>`, `<lightblue>` and friends)
  to HTML, plain text, or a span list (`FormattedSpan`), `coloured_text`, and
  the 16-colour terminal `PALETTE`.

```rust
use isorun_webtiles::{cm, initial_state, is_ping, reduce, unpack_frame};

let mut state = initial_state();
// for each text frame from the socket:
for m in unpack_frame(frame)? {
    if is_ping(&m) { send(cm::pong()); } else { reduce(&mut state, &m); }
}
send(cm::watch("somebody"));
```

Tested by replaying recorded sessions from a public server
(`native/fixtures/*.ndjson`) and the TypeScript package's own cases:
`cargo test -p isorun-webtiles`.

Part of [Isorun](https://github.com/maciej-rosiek/isorun). AGPL-3.0-or-later.
