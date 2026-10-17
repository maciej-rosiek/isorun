//! The DCSS WebTiles protocol as a library (the port of `packages/webtiles`): what the server
//! sends, what a client sends back ([`cm`], [`key_message`]), and a reducer ([`reduce`]) that
//! turns the stream of server messages into one plain [`GameState`]. No networking, no renderer:
//! a socket hands each text frame to [`unpack_frame`] and each message to [`reduce`].

pub mod connection;
pub mod floor;
pub mod format;
pub mod links;
pub mod protocol;
pub mod state;
mod wire;

pub use connection::*;
pub use floor::*;
pub use format::*;
pub use links::*;
pub use protocol::*;
pub use state::*;
pub use wire::{js_string, truthy, Obj};
