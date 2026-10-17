//! The official client's monster list rules (monster_list.js): who is listed, in what order,
//! grouped how.

use std::cmp::Ordering;

use isorun_scene::{Attitude, Billboard, BillboardKind, Scene, Threat, Visibility};
use isorun_webtiles::{GameState, Monster};

/// monster_list.js `attitude_classes`, by the server's index; anything else is hostile.
pub fn attitude_of(m: &Monster) -> Attitude {
    match m.att {
        1 => Attitude::Neutral,
        2 | 3 => Attitude::GoodNeutral,
        4 => Attitude::Friendly,
        _ => Attitude::Hostile,
    }
}

/// monster_list.js `mthreat_classes`, by the server's index (0 when it sends none).
pub fn threat_of(m: &Monster) -> Option<Threat> {
    match m.threat.unwrap_or(0) {
        0 => Some(Threat::Trivial),
        1 => Some(Threat::Easy),
        2 => Some(Threat::Tough),
        3 => Some(Threat::Nasty),
        4 => Some(Threat::Invisible),
        _ => None,
    }
}

/// monster_list.js `is_excluded`: no-experience monsters stay out of the list, bar the two the
/// official client keeps.
pub fn is_excluded_from_list(m: &Monster) -> bool {
    m.typedata.as_ref().and_then(|t| t.no_exp).unwrap_or(false) && !(m.name == "active ballistomycete" || m.name.ends_with("tentacle"))
}

/// monster_list.js `monster_sort`, a port of `monster_info::less_than`: attitude, then average
/// hit points (descending), then type (descending), named monsters (by client id) after
/// unnamed of the same kind, then name (descending). Two monsters that compare equal share a
/// row. Two with the same client id compare equal (the TS says "after" both ways).
pub fn monster_sort(m1: &Monster, m2: &Monster) -> Ordering {
    let hp = |m: &Monster| m.typedata.as_ref().and_then(|t| t.avghp).unwrap_or(0);
    m1.att.cmp(&m2.att).then_with(|| hp(m2).cmp(&hp(m1))).then_with(|| m2.kind.unwrap_or(0).cmp(&m1.kind.unwrap_or(0))).then_with(|| {
        match (m1.clientid, m2.clientid) {
            (None, None) => m2.name.cmp(&m1.name),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(a), Some(b)) => a.cmp(&b),
        }
    })
}

/// The monster a billboard stands for: by its id, else the one the state has on its cell.
pub fn monster_of<'a>(state: &'a GameState, b: &Billboard) -> Option<&'a Monster> {
    b.ref_id.and_then(|id| state.map.monsters.get(&id)).or_else(|| state.map.monster_at(b.x, b.y))
}

/// Visible monsters in sidebar order. The TS reads the monster off the billboard's `ref`;
/// here it is looked up in the state the scene was built from.
pub fn visible_monsters<'a>(scene: &'a Scene, state: &GameState) -> Vec<&'a Billboard> {
    let mut out: Vec<(&Billboard, &Monster)> = scene
        .billboards
        .iter()
        .filter(|b| b.kind == BillboardKind::Monster && scene.cell(b.x, b.y).is_some_and(|c| c.visibility == Visibility::Visible))
        .filter_map(|b| monster_of(state, b).map(|m| (b, m)))
        .filter(|(_, m)| !is_excluded_from_list(m))
        .collect();
    out.sort_by(|a, b| monster_sort(a.1, b.1));
    out.into_iter().map(|(b, _)| b).collect()
}

/// monster_list.js `group_monsters`: the visible monsters sorted, then runs of monsters that
/// compare equal combined into one group. Each group is a row of the sidebar: up to six
/// sprites and either the name or "<count> <plural>".
pub fn monster_groups<'a>(scene: &'a Scene, state: &GameState) -> Vec<Vec<&'a Billboard>> {
    let mut groups: Vec<Vec<&Billboard>> = Vec::new();
    for b in visible_monsters(scene, state) {
        let joins = groups.last().is_some_and(|g| match (monster_of(state, g[0]), monster_of(state, b)) {
            (Some(a), Some(m)) => monster_sort(a, m) == Ordering::Equal,
            _ => false,
        });
        if joins {
            groups.last_mut().unwrap().push(b);
        } else {
            groups.push(vec![b]);
        }
    }
    groups
}
