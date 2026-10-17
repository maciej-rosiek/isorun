//! What the player holds.

use isorun_gamedata::Gamedata;
use isorun_scene::{HandItem, HandLayer, TileId, Viewmodel};
use isorun_webtiles::{map_key, GameState, InvItem, ItemTile, TileLayer};

/// The player's hands. `weapon_index` in the `player` message is the wielded item's inventory
/// slot, -1 when unarmed; `offhand_index` is an off-hand *weapon*, -1 otherwise. Neither says
/// which armour is worn, so a shield or orb comes from the paperdoll on the player's cell: the
/// doll part whose player-tile name is `HAND2_*`, drawn from the player texture. With no
/// gamedata, or no such part, the off hand is empty. An item's own `tile` list is the icon,
/// in either wire shape: plain main-range ids, or `{t, tex, ymax}` records naming the texture.
pub fn viewmodel_for(state: &GameState, gd: Option<&Gamedata>) -> Viewmodel {
    let p = &state.player;
    Viewmodel {
        weapon: hand_item(p.inv.get(&p.weapon_index)),
        offhand: hand_item(p.inv.get(&p.offhand_index)).or_else(|| gd.and_then(|gd| doll_offhand(state, gd))),
    }
}

/// The paperdoll's off-hand part (`HAND2_*`: shields, orbs) on the player's cell, if any.
fn doll_offhand(state: &GameState, gd: &Gamedata) -> Option<HandItem> {
    if !state.map.player_on_level {
        return None;
    }
    let pos = state.player.pos;
    let doll = state.map.cells.get(&map_key(pos.x, pos.y))?.t.as_ref()?.doll.as_ref()?;
    let tex = gd.enums.get("texture", "PLAYER");
    for d in doll {
        let Some(name) = gd.player.name(gd.server_tile(d[0] as TileId, "player")) else { continue };
        if !name.starts_with("HAND2_") {
            continue;
        }
        return Some(HandItem {
            layers: vec![HandLayer { tile: d[0] as TileId, layer: Some(tex.map_or("player".into(), |t| t.to_string())), ymax: Some(d[1] as f32) }],
            name: Some(name.to_string()),
        });
    }
    None
}

fn hand_item(item: Option<&InvItem>) -> Option<HandItem> {
    let item = item?;
    let Some(ItemTile::Layers(tiles)) = &item.tile else { return None };
    let layers: Vec<HandLayer> = tiles
        .iter()
        .map(|t| match t {
            TileLayer::Id(id) => HandLayer { tile: *id as TileId, layer: None, ymax: None },
            TileLayer::Tex(r) => HandLayer { tile: r.t as TileId, layer: Some(r.tex.to_string()), ymax: r.ymax.map(|y| y as f32) },
        })
        .collect();
    if layers.is_empty() {
        return None;
    }
    Some(HandItem { layers, name: item.name.clone() })
}
