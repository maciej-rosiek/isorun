//! The game's sound (the port of `apps/isorun/src/audio`): the catalog of effects, beds and
//! tracks, the pure cues (message line, fight, hearing, place to sounds), the `SoundEngine` on
//! kira, and `GameAudio`, which feeds the engine from a game screen. See README.md.

pub mod catalog;
pub mod cues;
pub mod director;
pub mod engine;
mod mixer;

#[cfg(not(target_arch = "wasm32"))]
pub mod embedded;
#[cfg(target_arch = "wasm32")]
mod web;

pub use catalog::{all_files, music, AmbientLevels, AmbientName, AmbientSpec, Mood, SoundName, SoundSpec};
pub use cues::{CombatEvent, Cue, Fighter, Heard, UseKind};
pub use director::{monster_cell, AudioFrame, AudioView, GameAudio, RendererView};
pub use engine::{AudioSettings, Clock, ManualClock, PlayOptions, SoundEngine, SystemClock};
#[cfg(not(target_arch = "wasm32"))]
pub use mixer::OfflineOutput;
