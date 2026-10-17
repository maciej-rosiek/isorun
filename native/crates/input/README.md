# isorun-input

Isorun's input, the controller-first heart of the client, ported from the web build:
`bindings.ts`, `keys.ts`, `runner.ts`, `context.ts`, `gamepad.ts`, `gamepad-hints.ts`,
`command-menu.ts`, `action-tabs.ts`, `map-hold.ts`, `cell-menu.ts`, `cell-info.ts`, `touch.ts`,
`touch-bar.ts` (+ `touch.css`), `osk.ts`, `back-button.ts`, `combat.ts`, `pack-tabs.ts` (which the
context needs), and the input routing of `game.ts` (`onKeyDown`, `pad`, `fireHolds`,
`executePadAction`, `aimKey`, `stickAim`, `panMap`, `menuKey`, `back`, the input half of `uiOp`,
the pointer and touch handlers). The game screen only wires it.

## The pieces

| module | what |
|---|---|
| `keys` | `KeyLike` (a DOM `KeyboardEvent`'s fields), `keydown_message` (key_conversion.js), `direction_key`, and `key_inputs`: egui's `Key` / `Text` / `Copy` / `Cut` events as the DOM would deliver them, with the browser's keyCodes |
| `context` | `derive_context(&GameState, &Scene, heading, Layer) -> Context`: the mode, what is ahead and underfoot, the prompt parsed off the log, the menu, the shop |
| `bindings` | `Action`, `Button` tables by mode (`binding_table`), `resolve(&PadEvent, &Context) -> Option<Action>`, `screen_key` (the keyboard as a pad on screens), the bar's labels (`bar_labels`, `prompt_labels`, `action_label`), the controls sheet |
| `runner` | `Runner`: executes actions; the only path to crawl. Sends into `runner.outbox` (`take_outbox()` each frame, then `session.send`) |
| `router` | `InputRouter`: the game screen's routing of keys (`key_down`, `key_up`), pad events (`pad`, `fire_holds`, `frame`), gestures (`gesture`), Back (`back`), the menu keys F2–F5 (`menu_key`) |
| `gamepad` | `GamepadInput` (buttons, repeats, stick sectors with hysteresis, the primary pad) over `RawPad`s from a `PadSource`: `GilrsSource` (feature `gilrs`), `WebSource` (wasm32), `ScriptedPad` (tests, demos); `pad_key_input` (Shift+F1 / Shift+F2 stand-ins) |
| `gamepad_hints` | `GamepadHints`: the adaptive lessons and the prompts the pad shows (`prompts(ctx, HintMode)`), learned outcomes kept in the device's storage (`HintStore`) |
| `touch` | `Gestures`: egui's touch and pointer events to taps, drags, long presses, two-finger zoom / turn / tilt, the wheel; `compact_window`, `touch_screen` |
| `touch_bar` | `touch_labels`, and `touch_bar(ui, ctx, shown, screen)`, the egui widget |
| `cell_menu`, `cell_info` | `cell_menu_rows`, `cell_details`, and `CellMenu` (the egui widget a long press opens) |
| `osk` | `Osk` (the pad's on-screen keyboard: ops to `OskEdit`s), `OskText`, `Osk::show` (egui) |
| `back_button` | `BackButton` over a `History` (`WebHistory` on wasm32); `back_pressed` for Android's Back |
| `combat` | `combat_events(lines, scene, monsters)`: the fight in the message lines; `lines_since(lines, last, turn)` with `LastLine::newest` (found where it stood, by text, turn and channel, not `marker`) |
| `action_tabs`, `command_menu`, `map_hold`, `pack_tabs`, `focus` | as in the web build |

## Wiring it into the game screen

Each frame, after the session has polled:

```rust
let mut ctx = derive_context(&state, &scene, runner.heading, Layer::Micro);
router.fill_context(&mut ctx, &mut runner, state.input_mode, now_ms); // examining, exploring, LT on Hold
// the overlays fill ctx.focus, ctx.popup_actions, ctx.pageable
let env = Env { state: &state, scene: &scene, ctx: &ctx, camera: &mut camera, now: now_ms, watching, confirm_dangerous };
let mut io = Io { runner: &mut runner, env, host: &mut screen };   // screen: impl GameHost
if state_changed { router.server_state(&mut io) }                   // a sequence waiting on a prompt goes on
if scene_changed { io.runner.on_scene(); router.refresh_aim(&mut io) }
router.tick(&mut io);
for k in key_inputs(&events, modifiers) {
    if pad_key_input(&mut pad, &k, now_ms) { continue }
    match k { KeyInput::Down { key, repeat } => { router.key_down(&key, repeat, &mut io); } KeyInput::Up { key } => router.key_up(&key, &mut io) }
}
pad.poll_source(now_ms, &mut source);
for ev in pad.take_events() { router.pad(&ev, &mut io) }
gestures.view = view_rect;
for g in gestures.feed(&events, now_ms) { router.gesture(&g, &mut io) }
pad.right_stick_turns = router.frame(&mut io, &|b| pad.is_held(b), dt);
for msg in runner.take_outbox() { session.send(msg) }
```

`GameHost` is the screen around the router: what is up (a client overlay, the dock, the chat,
the cell menu, the look panel) and what is not input (open a screen, draw the aim arrow, pick the
cell under a point). Every method defaults to "nothing is up, do nothing", so a screen fills in
what it has; `RunnerHooks` (its supertrait) takes the runner's `ui`, `menu`, `focus`, `osk`,
`status`, `swing` and `keep_map`. The router keeps the `ui` ops that are input (explore while held
or at once, the level map, the view's reset, the palette, the log) and hands on the rest. The
interaction chooser's two rows call `router.contextual(false | true, io)`.

Directions are screen-relative: the runner reads the camera's `facing()` for the keys and the
pad, `stick_to_dir` with its `yaw_goal()` for the aim arrow, and turns it (`turn`) for a turn.

## Differences from the web build

- egui does not tell the numpad from the digit row, so no key has a `Numpad*` code: the numpad's
  digits are digits (the DOM path, with real codes, is kept for the web's own events).
- The runner does not subscribe to the session: its waits (a key held for its prompt, a door's
  direction) move on when the game calls `server_state` / `on_scene` / `tick`.
- `gilrs` is a feature, off by default: on Linux it links libudev and needs its development files,
  which this machine lacks (`pkg-config libudev`). The web reads the Gamepad API (`WebSource`).

## Tests and the demo

`cargo test -p isorun-input`: the TypeScript client's tests ported (keys, bindings,
context, runner, gamepad, hints, cells, combat, map hold, OSK, touch, routing).

`examples/input.rs` plays an offline game in the harness with every key, the pad and the pointer
going through this crate, and logs each message it sends (`SENT <json>`; `ISORUN_INPUT_LOG` to a
file). `ISORUN_PAD_SCRIPT="t:BUTTONS:LX,LY,RX,RY;..."` plays a scripted pad.
