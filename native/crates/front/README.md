# isorun-front

Isorun's front end on egui: everything before and around a game. The port of
`apps/isorun/src/menu.ts`, the routing of `main.ts` that is not the game screen, `room/*`,
`settings-panel.ts`, `settings-rows.ts`, `graphics.ts`, `lights-panel.ts`, `news.ts`,
`markdown.ts`, `whereis.ts`, `ping.ts`, `splash.ts`, `visits.ts` / `visit-beacon.ts`, the
system menu of `overlays.ts`, and the `styles.css` rules they stand on.

## The API

```rust
use isorun_front::{FrontEnd, FrontOptions, FrontAction, GameEnd};

// once, in the eframe app's creation
let mut front = FrontEnd::new(&cc.egui_ctx, render_state, FrontOptions::default());
front.open("settings/graphics");            // optional: a screen by the web build's address

// every frame while no game is up
match front.ui(ui) {
    Some(FrontAction::Play { session, account, game_id }) => { /* the game screen, on `session` */ }
    Some(FrontAction::Watch { session, username }) => { /* the game screen, spectating */ }
    Some(FrontAction::Fullscreen(on)) => { /* ViewportCommand::Fullscreen(on) */ }
    Some(FrontAction::Quit) => { /* ViewportCommand::Close */ }
    Some(FrontAction::Testbed) => { /* open the test bed (FrontOptions::testbed) */ }
    None => {}
}
front.pad(PadEvent::Press(PadButton::A));  // the pad, mapped by the app

// when the game is over, hand the session back
front.game_over(GameEnd::Lobby(session));   // the server's go_lobby: death, save, quit
front.game_over(GameEnd::Leave(session));   // Leave game from the menu: a fresh connection
front.game_over(GameEnd::Dropped(session)); // the socket dropped: rejoin, then "Connection lost"
```

`FrontEnd` owns the warm connection of the chosen account (main.ts `warm`, `rewarm`, the
retry with a doubling wait, the rejoin after a drop) and hands it over when the server starts
a game or a spectate (`game_client`, `game_started`, `watching_started`). Its screens: home
(Continue / Play per version, Watch, Settings, Full screen, Quit, the footer's account,
What's new, Get the app, Add to Steam, About), the exit report over it, Watch (the roster with
held arrivals and the details line), Settings and each group's page, the gamepad sheet,
Accounts (offline profiles with rc and delete, server accounts with other versions, log in,
remove), Other versions, the rc editor, the server list with pings, Add a server, Log in, a new
offline player, Register, and the documents.

Input: the keyboard (arrows and the vim keys, Enter / Space, Escape, Tab, Page Up / Down,
Home / End, Y, Ctrl+S in the rc file, F11), the mouse and touch (rows, the setting arrows, a
drag on the scenery turns the room), and the pad through `FrontEnd::pad`.

Over the game, `GameMenus` (`overlay.rs`) is the Isorun menu (Start), the settings groups and
pages, the gamepad sheet and Tune the look (`LightsPanel`): `open_system(SystemOptions)`,
`open_settings()`, `tune_look()`, `keys(ctx)` / `pad(ev)` while `is_open()`, and `ui(ctx)`,
which returns `MenuAction`s (keys for crawl, Leave, Fullscreen, SettingsChanged, Lights).

The pieces stand on their own too: `settings` (the rows, `quality_for`, `apply_graphics`, the
store in `isorun.settings`), `room::RoomView` (a place drawn by `IsoRenderer` with a slowly
turning camera), `markdown`, `news`, `whereis`, `words` (the roster's and the exit report's
words), `focus` (the cursor's grid), `visits`.

## See it

    ISORUN_DATA_DIR=/tmp/front ISORUN_PROFILE=front cargo run -p isorun-front --example front -- [--size 390x844] [--touch] [--open settings]

Play on this device reaches a game in a stand-in game view; Escape there opens the Isorun
menu. `tools/web-front.mjs` screenshots the web build's screens by address for comparison.

## Not yet

- The on-screen keyboard for a pad (X on a text field): the input crate's `Osk` is the one to
  use; the fields here take the keyboard.
- The gamepad sheet is a fixed table of bindings.ts `controlSheet`; the binding tables live in
  the input crate.
- Needed from `isorun-client`: `Settings::resolution` and `torch_shadows` read only text, but
  the web build stores numbers (85, 4), which makes `servers::get_settings` fall back to every
  default; `settings::client_settings` converts. There is no public way to delete an offline
  profile's saves, so `front::delete_profile_saves` removes its folders itself.
