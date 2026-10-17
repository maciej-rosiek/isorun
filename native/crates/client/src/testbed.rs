//! The test bed: the level of `apps/isorun/testbed/iso.des` with every kind of cell, a monster
//! of each status, and the human as the player. No server; it shows the look.

use crate::tiles::decode_png;
use isorun_render::{AtlasImage, TileSource};
use isorun_scene::*;
use std::sync::Arc;

pub const TORCH_GLB: &[u8] = include_bytes!("../assets/torch_mounted.glb");

pub struct Packed {
    atlas: isorun_vault::PackedAtlas,
    image: Arc<AtlasImage>,
}

impl TileSource for Packed {
    fn tile(&self, id: TileId) -> Option<TileRect> {
        self.atlas.tile(id).cloned()
    }
    fn atlas(&self, _: &str) -> Option<Arc<AtlasImage>> {
        Some(self.image.clone())
    }
}

/// The statuses each monster of the test bed shows (`STATUSES` in testbed/iso.ts).
fn statuses(name: &str) -> Vec<SpriteEffect> {
    match name {
        "orc" => vec![SpriteEffect::Wet],
        "jackal" => vec![SpriteEffect::Glowing],
        "goblin" => vec![SpriteEffect::Corroded],
        "kobold" => vec![SpriteEffect::Poisoned],
        "skeletal warrior" => vec![SpriteEffect::Petrified],
        "wight" => vec![SpriteEffect::Summoned],
        "necromancer" => vec![SpriteEffect::Burning],
        "ogre" => vec![SpriteEffect::Berserk],
        _ => vec![],
    }
}

/// The test bed's tiles and scene.
pub fn load() -> Result<(Arc<Packed>, Scene), String> {
    let atlas = isorun_vault::PackedAtlas::from_json(include_str!("../assets/atlas.json")).map_err(|e| format!("{e:?}"))?;
    let image = Arc::new(decode_png(include_bytes!("../assets/atlas.png"))?);
    let packed = Arc::new(Packed { atlas, image });
    let vault = isorun_vault::parse_des(include_str!("../assets/iso.des")).map_err(|e| format!("{e:?}"))?;
    let mut scene = isorun_vault::vault_scene(&vault, &packed.atlas).map_err(|e| format!("{e:?}"))?;
    let mut next = 1;
    for b in &mut scene.billboards {
        if b.kind != BillboardKind::Monster {
            continue;
        }
        b.scenery = false;
        b.effects = statuses(b.name.as_deref().unwrap_or(""));
        b.ref_id = Some(next);
        next += 1;
    }
    if let Some(human) = scene.billboards.iter_mut().find(|b| b.name.as_deref() == Some("human")) {
        human.kind = BillboardKind::Player;
        human.attitude = Some(Attitude::Friendly);
        human.ref_id = Some(PLAYER_REF);
        scene.player = (human.x, human.y);
    }
    scene.player_on_level = true;
    scene.revision += 1;
    Ok((packed, scene))
}

/// Move the player one cell, if the cell is open ground.
pub fn step(scene: &mut Scene, dx: i32, dy: i32) -> bool {
    let (x, y) = (scene.player.0 + dx, scene.player.1 + dy);
    if !isorun_render::ground_floor(scene, x, y) {
        return false;
    }
    if scene.billboards.iter().any(|b| b.x == x && b.y == y && b.kind == BillboardKind::Monster) {
        return false;
    }
    if let Some(p) = scene.billboards.iter_mut().find(|b| b.ref_id == Some(PLAYER_REF)) {
        p.x = x;
        p.y = y;
    }
    scene.player = (x, y);
    scene.revision += 1;
    true
}
