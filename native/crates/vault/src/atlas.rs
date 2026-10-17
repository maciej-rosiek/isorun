//! A packed atlas: one small image of exactly the tiles a place uses, packed at build time from
//! a pinned DCSS gamedata version (tools/build/pack-room.mjs), with an `atlas.json` beside it
//! (the port of `apps/isorun/src/room/tiles.ts`). The front room and the test bed are drawn
//! from one of these, never from a server's gamedata. No image decoding here: the caller
//! decodes `atlas.png` and binds it under [`PACKED_ATLAS`].

use std::collections::{BTreeMap, HashMap};

use isorun_scene::{TileId, TileRect};
use serde::Deserialize;

use crate::TileNames;

/// The one atlas name every rect of a packed atlas names.
pub const PACKED_ATLAS: &str = "atlas";

/// The game's atlases whose tiles stand in the world: features, items and monsters; not floor
/// and wall.
const SPRITE_SOURCES: [&str; 3] = ["feat", "main", "player"];

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct PackedSource {
    pub host: String,
    pub version: String,
    pub atlases: Vec<String>,
    pub des: String,
}

/// A tile name's base id and how many variants follow it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct PackedName {
    pub id: TileId,
    pub count: u32,
}

/// Where a tile is in the packed image, with the offsets tileinfo gave it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PackedTile {
    #[serde(default)]
    pub name: String,
    /// The game atlas it came from.
    pub from: String,
    pub sx: u32,
    pub sy: u32,
    pub w: u32,
    pub h: u32,
    pub ox: i32,
    pub oy: i32,
}

/// `atlas.json` as the packer writes it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PackedAtlasJson {
    #[serde(default)]
    pub source: PackedSource,
    #[serde(default)]
    pub license: String,
    pub cell: u32,
    pub width: u32,
    pub height: u32,
    /// Tile name → base id and variant count.
    #[serde(default)]
    pub names: BTreeMap<String, PackedName>,
    /// Tile id (as a decimal string) → where it is in the image.
    pub tiles: BTreeMap<String, PackedTile>,
}

/// A source rect in the packed image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteRect {
    pub sx: u32,
    pub sy: u32,
    pub w: u32,
    pub h: u32,
}

/// A packed atlas, answering what a renderer and a vault ask of a tile set.
#[derive(Clone, Debug)]
pub struct PackedAtlas {
    pub json: PackedAtlasJson,
    rects: HashMap<TileId, TileRect>,
    counts: HashMap<TileId, u32>,
    ids: HashMap<String, TileId>,
    /// The rects of the tiles crawl keeps in its sprite atlases.
    sprites: Vec<SpriteRect>,
}

impl PackedAtlas {
    /// Read `atlas.json`.
    pub fn from_json(text: &str) -> Result<PackedAtlas, serde_json::Error> {
        Ok(PackedAtlas::new(serde_json::from_str(text)?))
    }

    pub fn new(json: PackedAtlasJson) -> PackedAtlas {
        let mut rects = HashMap::new();
        let mut sprites = Vec::new();
        // in id order, as JavaScript walks an object's integer keys
        let mut tiles: Vec<(TileId, &PackedTile)> = json.tiles.iter().filter_map(|(id, t)| Some((id.parse::<TileId>().ok()?, t))).collect();
        tiles.sort_by_key(|(id, _)| *id);
        for (id, t) in tiles {
            rects.insert(id, TileRect { atlas: PACKED_ATLAS.into(), sx: t.sx, sy: t.sy, w: t.w, h: t.h, ox: t.ox, oy: t.oy, cell: json.cell });
            if SPRITE_SOURCES.contains(&t.from.as_str()) {
                sprites.push(SpriteRect { sx: t.sx, sy: t.sy, w: t.w, h: t.h });
            }
        }
        let mut ids = HashMap::new();
        let mut counts = HashMap::new();
        for (name, n) in &json.names {
            ids.insert(name.clone(), n.id);
            counts.insert(n.id, n.count);
        }
        PackedAtlas { json, rects, counts, ids, sprites }
    }

    pub fn tile(&self, id: TileId) -> Option<&TileRect> {
        self.rects.get(&id)
    }

    pub fn id(&self, name: &str) -> Option<TileId> {
        self.ids.get(name).copied()
    }

    /// Variants of a base tile; 1 when unknown.
    pub fn tile_count(&self, id: TileId) -> u32 {
        self.counts.get(&id).copied().unwrap_or(1)
    }

    pub fn atlas_names(&self) -> &'static [&'static str] {
        &[PACKED_ATLAS]
    }

    /// The one atlas here packs every kind together, where the game keeps floors and walls in
    /// atlases of their own that the renderer never peels the ink off. So the sprite art is the
    /// tiles that came from the atlases the game does peel, and a floor's dark edge stays.
    pub fn sprite_rects(&self, name: &str) -> Option<&[SpriteRect]> {
        (name == PACKED_ATLAS).then_some(self.sprites.as_slice())
    }

    pub fn sprite_atlases(&self) -> &'static [&'static str] {
        &[PACKED_ATLAS]
    }
}

impl TileNames for PackedAtlas {
    fn id(&self, name: &str) -> Option<TileId> {
        PackedAtlas::id(self, name)
    }

    fn tile_count(&self, id: TileId) -> u32 {
        PackedAtlas::tile_count(self, id)
    }
}
