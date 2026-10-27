//! The isometric renderer (the port of `packages/render-iso`): the DCSS tiles upscaled four
//! times with generated normal maps, lit by a fill light and the wall torches, drawn with wgpu. `IsoRenderer`
//! takes a `isorun_scene::Scene`, a camera and a look (`IsoLights`) and draws a frame into a
//! texture view; `IsoCamera` follows the player, turns, tilts and zooms.

pub mod actors;
pub mod camera;
pub mod combat;
pub mod enhance;
pub mod features;
pub mod fog;
pub mod gpu;
pub mod labels;
pub mod layers;
pub mod lights;
pub mod liquids;
pub mod loot;
pub mod memory;
pub mod mesh;
pub mod mipmap;
pub mod overlay;
pub mod pages;
pub mod particles;
pub mod plan;
pub mod post;
pub mod remote;
mod renderer;
pub mod scalefx;
pub mod steps;
pub mod torch_light;
pub mod torches;
pub mod uniforms;

pub use camera::{CameraAngles, IsoCamera, DEFAULT_ANGLES, TILT_MAX, TILT_MIN, TURN_DEGREES, ZOOM_MAX, ZOOM_MIN};
pub use lights::{lights, quality, IsoLights, IsoQuality, SurfaceDetail, LIGHTS_NAMES};
pub use loot::Landing;
pub use plan::{stick_to_dir, AimIntent, IsoAim, IsoRoute, IsoView, DEFAULT_VIEW};
pub use renderer::{ground_floor, ActorAction, ActorStep, FrameEvents, IsoRenderer, PLAYER_ACTOR};

use isorun_scene::{TileId, TileRect};
use std::sync::Arc;

/// An atlas's pixels, RGBA, rows top to bottom.
#[derive(Clone, Debug)]
pub struct AtlasImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// What the renderer needs from a tile provider: where a tile is, and the atlas's pixels.
pub trait TileSource: Send + Sync {
    fn tile(&self, id: TileId) -> Option<TileRect>;
    fn atlas(&self, name: &str) -> Option<Arc<AtlasImage>>;
}

/// Seconds on a monotonic clock, for timing work within a frame.
pub fn now_seconds() -> f64 {
    use std::sync::OnceLock;
    use web_time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}
