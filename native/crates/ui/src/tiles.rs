//! crawl's tiles in egui: each atlas of a game's gamedata uploaded once as an egui texture, and a
//! tile drawn from it as the web client's `<canvas>` icons draw it (the tile in its 32x32 cell,
//! offset as crawl places it).

use isorun_client::tiles::LoadedGamedata;
use isorun_render::TileSource;
use isorun_scene::{TileId, TileRect};
use isorun_webtiles::TileRef;
use std::collections::HashMap;
use std::sync::Arc;

/// The atlases of one gamedata as egui textures. Keep one per game screen; it uploads an atlas
/// the first time a tile of it is drawn.
#[derive(Default)]
pub struct TileIcons {
    gd: Option<Arc<LoadedGamedata>>,
    textures: HashMap<String, egui::TextureHandle>,
}

impl TileIcons {
    /// Use this gamedata; a different one drops the textures of the last.
    pub fn set_gamedata(&mut self, gd: Option<Arc<LoadedGamedata>>) {
        let same = match (&self.gd, &gd) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.textures.clear();
            self.gd = gd;
        }
    }

    pub fn gamedata(&self) -> Option<&Arc<LoadedGamedata>> {
        self.gd.as_ref()
    }

    /// Where a tile is, by the client's id.
    pub fn rect(&self, id: TileId) -> Option<TileRect> {
        self.gd.as_ref()?.gd.tile(id, None)
    }

    /// Where a tile the server named (`{t, tex}`, as in menu items and the dock's spells) is.
    pub fn rect_of_ref(&self, r: &TileRef) -> Option<TileRect> {
        self.gd.as_ref()?.gd.tile_for_texture(r.t as TileId, r.tex as u32)
    }

    fn texture(&mut self, ctx: &egui::Context, atlas: &str) -> Option<egui::TextureHandle> {
        if let Some(t) = self.textures.get(atlas) {
            return Some(t.clone());
        }
        let img = self.gd.as_ref()?.atlas(atlas)?;
        let colour = egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.rgba);
        // nearest: crawl's tiles are pixel art, drawn at whole multiples where they can be
        let t = ctx.load_texture(format!("atlas:{atlas}"), colour, egui::TextureOptions::NEAREST);
        self.textures.insert(atlas.to_string(), t.clone());
        Some(t)
    }

    /// Paint a tile into `cell`, a square that stands for crawl's 32x32 cell: the tile keeps its
    /// offset and size within it, so a small item sits where crawl puts it.
    pub fn paint(&mut self, painter: &egui::Painter, rect: &TileRect, cell: egui::Rect, tint: egui::Color32) {
        let Some(tex) = self.texture(painter.ctx(), &rect.atlas) else { return };
        let size = tex.size_vec2();
        let scale = cell.width() / rect.cell.max(1) as f32;
        let min = cell.min + egui::vec2(rect.ox as f32, rect.oy as f32) * scale;
        let dest = egui::Rect::from_min_size(min, egui::vec2(rect.w as f32, rect.h as f32) * scale);
        let uv = egui::Rect::from_min_max(egui::pos2(rect.sx as f32 / size.x, rect.sy as f32 / size.y), egui::pos2((rect.sx + rect.w) as f32 / size.x, (rect.sy + rect.h) as f32 / size.y));
        painter.image(tex.id(), dest, uv, tint);
    }

    /// Paint a tile by the client's id; false when there is no such tile.
    pub fn paint_id(&mut self, painter: &egui::Painter, id: TileId, cell: egui::Rect) -> bool {
        match self.rect(id) {
            Some(r) => {
                self.paint(painter, &r, cell, egui::Color32::WHITE);
                true
            }
            None => false,
        }
    }

    /// Paint a tile the server named.
    pub fn paint_ref(&mut self, painter: &egui::Painter, r: &TileRef, cell: egui::Rect) -> bool {
        match self.rect_of_ref(r) {
            Some(rect) => {
                self.paint(painter, &rect, cell, egui::Color32::WHITE);
                true
            }
            None => false,
        }
    }

    /// The tile's id by its name in crawl's tile tables (`tileinfo-*.js`), e.g. `"STAIRS_DOWN"`:
    /// the dungeon's, then the main, player, gui and icon tables.
    pub fn id_of(&self, name: &str) -> Option<TileId> {
        let gd = &self.gd.as_ref()?.gd;
        [&gd.dngn, &gd.main, &gd.player, &gd.gui, &gd.icons].into_iter().find_map(|i| i.id(name))
    }
}
