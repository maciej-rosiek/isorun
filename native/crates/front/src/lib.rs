//! Isorun's front end on egui (the port of `menu.ts`, the routing of `main.ts` that is not the
//! game screen, `room/*`, the settings, What's new, About and the rest): everything before and
//! around a game. See README.md for the API.

pub mod docs;
pub mod focus;
pub mod front;
pub mod layout;
pub mod lights_panel;
pub mod markdown;
pub mod news;
mod osk;
pub mod overlay;
pub mod paint;
pub mod ping;
pub mod regions;
pub mod room;
mod screens;
pub mod settings;
pub mod splash;
pub mod whereis;
pub mod words;

pub use front::{FrontAction, FrontEnd, FrontOptions, GameEnd, Intent, PadButton, PadEvent, View};
pub use lights_panel::{LightsEvent, LightsPanel};
pub use overlay::{GameMenus, MenuAction};
pub use words::NavDir;
