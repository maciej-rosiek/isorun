//! Isorun's input, the controller-first heart of the client (the port of `bindings.ts`,
//! `keys.ts`, `runner.ts`, `context.ts`, `gamepad.ts`, `gamepad-hints.ts`, `command-menu.ts`,
//! `action-tabs.ts`, `map-hold.ts`, `cell-menu.ts`, `cell-info.ts`, `touch.ts`, `touch-bar.ts`,
//! `osk.ts`, `back-button.ts`, `combat.ts` and the input routing of `game.ts`). See README.md.

pub mod action_tabs;
pub mod back_button;
pub mod bindings;
pub mod cell_info;
pub mod cell_menu;
pub mod combat;
pub mod command_menu;
pub mod context;
pub mod focus;
pub mod gamepad;
pub mod gamepad_hints;
pub mod keys;
pub mod map_hold;
pub mod osk;
pub mod pack_tabs;
pub mod router;
pub mod runner;
pub mod touch;
pub mod touch_bar;

pub use bindings::{Action, SeqItem, Stroke};
pub use context::{derive_context, derive_mode, Context, Layer, Mode, Target};
pub use gamepad::{Button, GamepadInput, PadEvent};
pub use keys::{direction_key, key_inputs, keydown_message, KeyInput, KeyLike};
pub use runner::{Env, Runner, RunnerHooks};
