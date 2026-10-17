//! What a tile is and how its surface behaves, as tools/labels gave it (the port of
//! `tile-labels.ts`), found by a hash of the tile's pixels.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Motion {
    None,
    Breathe,
    Hover,
    Flicker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Joints {
    None,
    Bricks,
    Blocks,
    Cobbles,
    Planks,
    Tiles,
    Cracks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileLabel {
    /// Index into the label file's `materials`.
    pub material: u8,
    /// How bumpy, 0 (flat) to 3.
    pub relief: u8,
    /// Whether the light texels are the raised parts.
    pub bright_is_high: bool,
    pub joints: Joints,
    /// 0 matte to 3 very bright highlights.
    pub gloss: u8,
    pub wet: bool,
    /// 0 none, 1 some parts give light, 2 all of it.
    pub glow: u8,
    pub sparkle: bool,
    pub translucent: bool,
    pub motion: Motion,
}

/// A tile no label names: a little relief from its brightness, matte, giving no light.
pub const UNLABELLED: TileLabel = TileLabel { material: 32, relief: 1, bright_is_high: true, joints: Joints::None, gloss: 0, wet: false, glow: 0, sparkle: false, translucent: false, motion: Motion::None };

#[derive(Deserialize)]
struct LabelFile {
    labels: Vec<Vec<u8>>,
    tiles: HashMap<String, usize>,
}

fn decode(v: &[u8]) -> TileLabel {
    let at = |i: usize| v.get(i).copied().unwrap_or(0);
    TileLabel {
        material: at(0),
        relief: at(1),
        bright_is_high: at(2) == 1,
        joints: match at(3) {
            1 => Joints::Bricks,
            2 => Joints::Blocks,
            3 => Joints::Cobbles,
            4 => Joints::Planks,
            5 => Joints::Tiles,
            6 => Joints::Cracks,
            _ => Joints::None,
        },
        gloss: at(4),
        wet: at(5) == 1,
        glow: at(6),
        sparkle: at(7) == 1,
        translucent: at(8) == 1,
        motion: match at(9) {
            1 => Motion::Breathe,
            2 => Motion::Hover,
            3 => Motion::Flicker,
            _ => Motion::None,
        },
    }
}

fn table() -> &'static HashMap<u32, TileLabel> {
    static TABLE: OnceLock<HashMap<u32, TileLabel>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let file: LabelFile = serde_json::from_str(include_str!("tile-labels.json")).expect("tile-labels.json");
        let labels: Vec<TileLabel> = file.labels.iter().map(|l| decode(l)).collect();
        file.tiles
            .into_iter()
            .filter_map(|(hash, i)| Some((u32::from_str_radix(&hash, 16).ok()?, labels.get(i).copied().unwrap_or(UNLABELLED))))
            .collect()
    })
}

/// The label of a tile by its pixels (`tile_hash`), or none when no label names that art.
pub fn label_of(hash: u32) -> Option<TileLabel> {
    table().get(&hash).copied()
}

/// FNV-1a over a tile's size and its pixels, with every texel under half alpha as nothing.
pub fn tile_hash(rgba: &[u8], width: u32, height: u32) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    let mut mix = |byte: u32| {
        h ^= byte;
        h = h.wrapping_mul(0x01000193);
    };
    for v in [width & 255, width >> 8, height & 255, height >> 8] {
        mix(v);
    }
    for px in rgba.chunks_exact(4).take((width * height) as usize) {
        let opaque = px[3] >= 128;
        mix(if opaque { px[0] as u32 } else { 0 });
        mix(if opaque { px[1] as u32 } else { 0 });
        mix(if opaque { px[2] as u32 } else { 0 });
        mix(opaque as u32);
    }
    h
}
