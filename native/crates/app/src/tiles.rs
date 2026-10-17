//! A server's gamedata with its atlases decoded: the renderer's tile source.

use isorun_gamedata::Gamedata;
use isorun_render::{AtlasImage, TileSource};
use isorun_scene::{TileId, TileRect};
use std::collections::HashMap;
use std::sync::Arc;

pub struct LoadedGamedata {
    pub version: String,
    pub gd: Gamedata,
    pub atlases: HashMap<String, Arc<AtlasImage>>,
}

impl TileSource for LoadedGamedata {
    fn tile(&self, id: TileId) -> Option<TileRect> {
        self.gd.tile(id, None)
    }
    fn atlas(&self, name: &str) -> Option<Arc<AtlasImage>> {
        self.atlases.get(name).cloned()
    }
}

/// A PNG's pixels, RGBA.
pub fn decode_png(bytes: &[u8]) -> Result<AtlasImage, String> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).map_err(|e| e.to_string())?.to_rgba8();
    Ok(AtlasImage { width: img.width(), height: img.height(), rgba: img.into_raw() })
}

/// Parse the scripts and decode the atlases fetched for one version.
pub fn assemble(version: &str, files: HashMap<String, Vec<u8>>) -> Result<LoadedGamedata, String> {
    let texts: HashMap<String, String> = files.iter().filter(|(k, _)| k.ends_with(".js")).map(|(k, v)| (k.clone(), String::from_utf8_lossy(v).into_owned())).collect();
    let gd = isorun_gamedata::load_gamedata_from_map(&texts, version).map_err(|e| format!("{}: {}", e.path, e.message))?;
    let mut atlases = HashMap::new();
    for name in isorun_gamedata::ATLASES {
        let file = format!("{name}.png");
        let Some(bytes) = files.get(&file) else { continue };
        atlases.insert(name.to_string(), Arc::new(decode_png(bytes).map_err(|e| format!("{file}: {e}"))?));
    }
    Ok(LoadedGamedata { version: version.to_string(), gd, atlases })
}
