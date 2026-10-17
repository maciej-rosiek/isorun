//! crawl's atlases as egui textures for the 2D views, and tiles looked up into them as source
//! rectangles in the texture's own 0..1 coordinates. One `MapTiles` serves the level map and
//! the minimap together; it uploads an atlas the first time a tile of it is drawn.

use isorun_render::TileSource;
use isorun_scene::{TileId, TileRect};
use std::collections::HashMap;
use std::sync::Arc;

/// A tile as the views draw it: which atlas, where it lies in it (0..1), and where it is drawn
/// in its cell (`TileRect`'s offsets and size, in texels of a `cell`-texel square).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sprite {
    pub atlas: u8,
    /// Texture coordinates of the tile's top-left corner, and of a texel's size.
    pub uv: [f32; 2],
    pub texel: [f32; 2],
    pub ox: f32,
    pub oy: f32,
    pub w: f32,
    pub h: f32,
    pub cell: f32,
}

struct Atlas {
    name: String,
    size: [f32; 2],
    texture: Option<egui::TextureHandle>,
}

/// The tile source's atlases, uploaded on first use.
#[derive(Default)]
pub struct MapTiles {
    source: Option<Arc<dyn TileSource>>,
    atlases: Vec<Atlas>,
    by_name: HashMap<String, u8>,
    /// Smooth scaled tiles (WebTiles' `tile_filter_scaling`); nearest by default, as crawl's tiles are pixel art.
    filter: bool,
    /// Bumped when the source or the filter changes, so the views build their meshes again.
    pub version: u64,
}

impl MapTiles {
    /// Use this tile source (a game's gamedata); a different one drops the textures of the last.
    pub fn set_source(&mut self, source: Option<Arc<dyn TileSource>>) {
        let same = match (&self.source, &source) {
            (Some(a), Some(b)) => std::ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b)),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.source = source;
            self.atlases.clear();
            self.by_name.clear();
            self.version += 1;
        }
    }

    pub fn has_source(&self) -> bool {
        self.source.is_some()
    }

    /// Smooth the scaled tiles or keep them square-pixelled (`tile_filter_scaling`).
    pub fn set_filter(&mut self, filter: bool) {
        if filter != self.filter {
            self.filter = filter;
            for a in &mut self.atlases {
                a.texture = None;
            }
            self.version += 1;
        }
    }

    /// A tile by the client's id; `None` when the source has no such tile or atlas.
    pub fn sprite(&mut self, id: TileId) -> Option<Sprite> {
        let rect = self.source.as_ref()?.tile(id)?;
        self.sprite_of(&rect)
    }

    fn sprite_of(&mut self, rect: &TileRect) -> Option<Sprite> {
        let atlas = match self.by_name.get(&rect.atlas) {
            Some(&i) => i,
            None => {
                let img = self.source.as_ref()?.atlas(&rect.atlas)?;
                let i = self.atlases.len() as u8;
                self.atlases.push(Atlas { name: rect.atlas.clone(), size: [img.width as f32, img.height as f32], texture: None });
                self.by_name.insert(rect.atlas.clone(), i);
                i
            }
        };
        let size = self.atlases[atlas as usize].size;
        Some(Sprite {
            atlas,
            uv: [rect.sx as f32 / size[0], rect.sy as f32 / size[1]],
            texel: [1.0 / size[0], 1.0 / size[1]],
            ox: rect.ox as f32,
            oy: rect.oy as f32,
            w: rect.w as f32,
            h: rect.h as f32,
            cell: rect.cell.max(1) as f32,
        })
    }

    /// The egui texture of an atlas, uploaded now if it is not yet.
    pub fn texture(&mut self, ctx: &egui::Context, atlas: u8) -> Option<egui::TextureId> {
        let a = self.atlases.get_mut(atlas as usize)?;
        if a.texture.is_none() {
            let img = self.source.as_ref()?.atlas(&a.name)?;
            let colour = egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.rgba);
            let options = if self.filter { egui::TextureOptions::LINEAR } else { egui::TextureOptions::NEAREST };
            a.texture = Some(ctx.load_texture(format!("map2d:{}", a.name), colour, options));
        }
        a.texture.as_ref().map(|t| t.id())
    }
}
