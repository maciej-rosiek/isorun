//! crawl's tiles in egui: each atlas of a game's gamedata uploaded once as an egui texture, and a
//! tile drawn from it as the web client's `<canvas>` icons draw it (the tile in its 32x32 cell,
//! offset as crawl places it).

use isorun_client::tiles::LoadedGamedata;
use isorun_render::TileSource;
use isorun_scene::{TileId, TileRect};
use isorun_webtiles::TileRef;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

/// An atlas uploaded once for every view of the page: the icons (`TileIcons`) and the 2D maps
/// (render2d's `MapTiles`) draw from the same texture. It lives while someone holds it.
pub type SharedTexture = Arc<egui::TextureHandle>;

/// The atlases uploaded so far, by their source (the address of its gamedata), name and
/// filtering, kept in the egui context so every view finds them.
#[derive(Clone, Default)]
struct AtlasCache(Arc<Mutex<HashMap<(usize, String, bool), Weak<egui::TextureHandle>>>>);

/// The identity of a tile source, for `shared_atlas`: the address of what the `Arc` holds.
pub fn source_key<T: ?Sized>(source: &Arc<T>) -> usize {
    Arc::as_ptr(source) as *const () as usize
}

/// The texture of the atlas `name` of the source `source` (see `source_key`), uploaded now from
/// `image` if no view holds it yet. `smooth` is linear filtering (the maps' `tile_filter_scaling`);
/// crawl's tiles are pixel art, so nearest is the rule and both kinds of view share it.
pub fn shared_atlas(ctx: &egui::Context, source: usize, name: &str, smooth: bool, image: impl FnOnce() -> Option<egui::ColorImage>) -> Option<SharedTexture> {
    let cache = ctx.data_mut(|d| d.get_temp_mut_or_default::<AtlasCache>(egui::Id::new("isorun-atlas-cache")).clone());
    let key = (source, name.to_string(), smooth);
    let mut map = cache.0.lock().unwrap();
    if let Some(t) = map.get(&key).and_then(Weak::upgrade) {
        return Some(t);
    }
    // the ones nobody holds any more go
    map.retain(|_, w| w.strong_count() > 0);
    let options = if smooth { egui::TextureOptions::LINEAR } else { egui::TextureOptions::NEAREST };
    let t = Arc::new(ctx.load_texture(format!("atlas:{name}{}", if smooth { ":smooth" } else { "" }), image()?, options));
    map.insert(key, Arc::downgrade(&t));
    Some(t)
}

/// How many atlases are uploaded and held now (for tests and the perf log).
pub fn shared_atlas_count(ctx: &egui::Context) -> usize {
    let cache = ctx.data_mut(|d| d.get_temp_mut_or_default::<AtlasCache>(egui::Id::new("isorun-atlas-cache")).clone());
    let n = cache.0.lock().unwrap().values().filter(|w| w.strong_count() > 0).count();
    n
}

/// The atlases of one gamedata as egui textures. Keep one per game screen; it takes an atlas
/// the first time a tile of it is drawn, from the page's shared ones (`shared_atlas`).
#[derive(Default)]
pub struct TileIcons {
    gd: Option<Arc<LoadedGamedata>>,
    textures: HashMap<String, SharedTexture>,
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

    fn texture(&mut self, ctx: &egui::Context, atlas: &str) -> Option<SharedTexture> {
        if let Some(t) = self.textures.get(atlas) {
            return Some(t.clone());
        }
        let gd = self.gd.as_ref()?;
        // nearest: crawl's tiles are pixel art, drawn at whole multiples where they can be
        let t = shared_atlas(ctx, source_key(gd), atlas, false, || {
            let img = gd.atlas(atlas)?;
            Some(egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.rgba))
        })?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_upload_for_every_view() {
        let ctx = egui::Context::default();
        let img = || Some(egui::ColorImage::new([2, 2], vec![egui::Color32::WHITE; 4]));
        let mut uploads = 0;
        let a = shared_atlas(&ctx, 1, "main", false, || {
            uploads += 1;
            img()
        })
        .unwrap();
        let b = shared_atlas(&ctx, 1, "main", false, || {
            uploads += 1;
            img()
        })
        .unwrap();
        assert_eq!(uploads, 1);
        assert_eq!(a.id(), b.id());
        // smoothed, or another source, is another texture
        let c = shared_atlas(&ctx, 1, "main", true, img).unwrap();
        assert_ne!(c.id(), a.id());
        assert_eq!(shared_atlas_count(&ctx), 2);
        // once nobody holds it, it goes
        drop((a, b, c));
        assert_eq!(shared_atlas_count(&ctx), 0);
    }
}
