//! The classification rules, exported so a renderer or HUD can ask the same questions the
//! builder does: what a feature is, how it stands, how a level looks, what a liquid or an item
//! tile is called.

use isorun_gamedata::Gamedata;
use isorun_scene::{Beacon, DoorState, FaceTiles, Feature, ItemRarity, Liquid, LiquidKind, Sky, Stance, UpDown};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::enums::MfTable;

fn re(s: &str) -> Regex {
    Regex::new(s).unwrap()
}

macro_rules! regex {
    ($name:ident = $s:expr) => {
        static $name: Lazy<Regex> = Lazy::new(|| re($s));
    };
}

regex!(N_EXIT_DUNGEON = r"^DNGN_EXIT_DUNGEON$");
regex!(N_STAIRS_DOWN = r"^DNGN_STONE_STAIRS_DOWN");
regex!(N_STAIRS_UP = r"^DNGN_STONE_STAIRS_UP");
regex!(N_HATCH_DOWN = r"^DNGN_ESCAPE_HATCH_DOWN|^DNGN_TRAP_SHAFT");
regex!(N_HATCH_UP = r"^DNGN_ESCAPE_HATCH_UP");
regex!(N_SHOP = r"^DNGN_ENTER_SHOP|^DNGN_.*SHOP|^SHOP_");
regex!(N_ALTAR = r"^DNGN_ALTAR_");
regex!(N_FOUNTAIN = r"FOUNTAIN");
regex!(N_TRANSPORTER = r"^DNGN_TRANSPORTER$|^DNGN_TRANSPORTER_");
regex!(N_LANDING = r"LANDING");
regex!(N_TRAP = r"^DNGN_TRAP_");
regex!(N_DOOR_SHUT = r"^DNGN_(CLOSED|RUNED|SEALED)_.*DOOR|^DNGN_.*CLOSED_DOOR|_DOOR_CLOSED");
regex!(N_DOOR_OPEN = r"^DNGN_OPEN_.*DOOR|_DOOR_OPEN|^DNGN_.*OPEN_DOOR");
regex!(N_PORTAL = r"^DNGN_ENTER_|^DNGN_RETURN_|^DNGN_EXIT_|^DNGN_PORTAL|^DNGN_ABYSSAL_STAIR|^DNGN_.*GATE|^DNGN_TRANSIT|^DNGN_ENTRANCE");

fn strip<'a>(s: &'a str, prefix: &str) -> &'a str {
    s.strip_prefix(prefix).unwrap_or(s)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum From {
    Name,
    Glyph,
    Mf,
}

fn stairs(dir: UpDown) -> Feature {
    Feature::Stairs { dir, branch: false, exit: false }
}

fn door(state: DoorState) -> Feature {
    Feature::Door { state }
}

/// What two features are the same kind of, for the vote: stairs and hatches by direction.
fn type_of(f: &Feature) -> String {
    match f {
        Feature::Stairs { dir, .. } | Feature::Hatch { dir } => format!("stairs:{dir:?}"),
        Feature::Door { .. } => "door".into(),
        Feature::Altar { .. } => "altar".into(),
        Feature::Shop => "shop".into(),
        Feature::Fountain { .. } => "fountain".into(),
        Feature::Portal => "portal".into(),
        Feature::Transporter => "transporter".into(),
        Feature::Trap { .. } => "trap".into(),
        Feature::Other { .. } => "other".into(),
    }
}

/// Feature classification from name, glyph and minimap category, with the protocol's
/// map-feature table; see [`classify_feature_with`].
pub fn classify_feature(name: Option<&str>, glyph: Option<&str>, mf: Option<i64>) -> Option<Feature> {
    classify_feature_with(name, glyph, mf, &MfTable::default())
}

/// Feature classification from name, glyph, and minimap category. Two of three must agree.
/// Under the player only two signals exist: the server sends the screen glyph, `@` on the
/// player's own cell (tileweb.cc `_send_cell`), so the name and the minimap category settle
/// it there. feature-data.h gives every branch exit and `DNGN_EXIT_DUNGEON` an `EXIT_` name
/// with `MF_STAIR_UP`: when the name reads as a portal but the minimap says stair, the stair
/// wins, since the minimap category is the direction.
pub fn classify_feature_with(name: Option<&str>, glyph: Option<&str>, mf: Option<i64>, t: &MfTable) -> Option<Feature> {
    let n = name.unwrap_or("");
    let mut votes: Vec<(Feature, From)> = Vec::new();
    // name
    let by_name = if N_EXIT_DUNGEON.is_match(n) {
        Some(Feature::Stairs { dir: UpDown::Up, branch: false, exit: true })
    } else if N_STAIRS_DOWN.is_match(n) {
        Some(stairs(UpDown::Down))
    } else if N_STAIRS_UP.is_match(n) {
        Some(stairs(UpDown::Up))
    } else if N_HATCH_DOWN.is_match(n) {
        Some(Feature::Hatch { dir: UpDown::Down })
    } else if N_HATCH_UP.is_match(n) {
        Some(Feature::Hatch { dir: UpDown::Up })
    } else if N_SHOP.is_match(n) {
        Some(Feature::Shop)
    } else if N_ALTAR.is_match(n) {
        Some(Feature::Altar { god: Some(strip(n, "DNGN_ALTAR_").to_lowercase()) })
    } else if N_FOUNTAIN.is_match(n) {
        Some(Feature::Fountain { kind: strip(n, "DNGN_").to_lowercase() })
    } else if N_TRANSPORTER.is_match(n) && !N_LANDING.is_match(n) {
        Some(Feature::Transporter)
    } else if N_TRAP.is_match(n) {
        Some(Feature::Trap { kind: Some(strip(n, "DNGN_TRAP_").to_lowercase()) })
    } else if N_DOOR_SHUT.is_match(n) {
        Some(door(if n.contains("RUNED") {
            DoorState::Runed
        } else if n.contains("SEALED") {
            DoorState::Sealed
        } else {
            DoorState::Closed
        }))
    } else if N_DOOR_OPEN.is_match(n) {
        Some(door(DoorState::Open))
    } else if N_PORTAL.is_match(n) {
        Some(Feature::Portal)
    } else if !n.is_empty() {
        Some(Feature::Other { name: strip(n, "DNGN_").to_lowercase() })
    } else {
        None
    };
    if let Some(f) = by_name {
        votes.push((f, From::Name));
    }
    // glyph
    let by_glyph = match glyph {
        Some(">") => Some(stairs(UpDown::Down)),
        Some("<") => Some(stairs(UpDown::Up)),
        Some("+") => Some(door(DoorState::Closed)),
        Some("'") => Some(door(DoorState::Open)),
        Some("_") => Some(Feature::Altar { god: None }),
        Some("^") => Some(Feature::Trap { kind: None }),
        Some("\\") | Some("{") | Some("}") => Some(Feature::Portal),
        _ => None,
    };
    if let Some(f) = by_glyph {
        votes.push((f, From::Glyph));
    }
    // map feature category
    let by_mf = match mf {
        Some(m) if m == t.stair_down => Some(stairs(UpDown::Down)),
        Some(m) if m == t.stair_up => Some(stairs(UpDown::Up)),
        Some(m) if m == t.stair_branch => Some(Feature::Portal),
        Some(m) if m == t.door => Some(door(DoorState::Closed)),
        Some(m) if m == t.portal => Some(Feature::Portal),
        Some(m) if m == t.trap => Some(Feature::Trap { kind: None }),
        Some(m) if m == t.transporter => Some(Feature::Transporter),
        _ => None,
    };
    if let Some(f) = by_mf {
        votes.push((f, From::Mf));
    }

    // group by type, in the order first voted; the name vote's detail is the best
    let mut counts: Vec<(String, usize, Feature)> = Vec::new();
    for (f, from) in &votes {
        let ty = type_of(f);
        match counts.iter_mut().find(|c| c.0 == ty) {
            None => counts.push((ty, 1, f.clone())),
            Some(c) => {
                c.1 += 1;
                if *from == From::Name {
                    c.2 = f.clone();
                }
            }
        }
    }
    let mut winner: Option<&(String, usize, Feature)> = None;
    for c in &counts {
        if winner.is_none_or(|w| c.1 > w.1) {
            winner = Some(c);
        }
    }
    if let Some((_, n, best)) = winner {
        if *n >= 2 {
            // stairs into a branch: the name says entrance (portal) but the minimap says branch stair
            if let Feature::Stairs { dir, exit, .. } = best {
                if mf == Some(t.stair_branch) {
                    return Some(Feature::Stairs { dir: *dir, branch: true, exit: *exit });
                }
            }
            return Some(best.clone());
        }
    }
    let name_vote = votes.iter().find(|v| v.1 == From::Name).map(|v| &v.0);
    // an `EXIT_` / `RETURN_` name over a stair minimap category: a branch exit, a stair up
    let mf_vote = votes.iter().find(|v| v.1 == From::Mf).map(|v| &v.0);
    if let (Some(Feature::Portal), Some(s @ Feature::Stairs { .. })) = (name_vote, mf_vote) {
        return Some(s.clone());
    }
    // a single name vote is still a good hint for the reticle label
    name_vote.cloned()
}

regex!(
    UPRIGHT = r"HATCH|DOOR|ARCH|GATE|ALTAR|FOUNTAIN|STATUE|IDOL|ENSIGN|GRAVE|SARCOPHAG|COLUMN|PILLAR|SHRINE|PORTAL|ENTER_|EXIT_|RETURN_|ENTRANCE|SHOP|TREE|MANGROVE|PLANT|BUSH|FUNGUS|LANTERN|TORCH|ORCISH_IDOL|STONE_ARCH|WAX_WALL|TRANSPORTER$|RUNELIGHT|SEAL|CRYSTAL|STAIRS|ABYSSAL_STAIR|WELL"
);
regex!(DECAL = r"TRAP|TELEPORT|LANDING|SHAFT|MAGIC_CIRCLE|WATER|LAVA|BLOOD|MOLD|SLIME");
regex!(LYING = r"(STAIRS|HATCH)_DOWN|^DNGN_TRAP_|TELEPORTER|^DNGN_TRANSPORTER$");

/// Whether a feature stands as a billboard in its cell, lies on the floor as a block, or is
/// painted on it. A way up stands: a player steers by it. What lies on the floor for the
/// player to use or step on (a way down, a trap, a transporter) lies with a corpse's
/// thickness, so it reads as a thing and not a stain. The rest stays painted on.
pub fn stance_for(name: Option<&str>, feature: Option<&Feature>) -> Stance {
    let n = name.unwrap_or("");
    match feature {
        Some(Feature::Stairs { dir, .. }) | Some(Feature::Hatch { dir }) => {
            return if *dir == UpDown::Up { Stance::Upright } else { Stance::Lying };
        }
        Some(Feature::Trap { .. }) | Some(Feature::Transporter) => return Stance::Lying,
        Some(Feature::Door { .. }) | Some(Feature::Altar { .. }) | Some(Feature::Shop) | Some(Feature::Fountain { .. }) | Some(Feature::Portal) => {
            return Stance::Upright
        }
        _ => {}
    }
    if LYING.is_match(n) {
        Stance::Lying
    } else if DECAL.is_match(n) {
        Stance::Decal
    } else if UPRIGHT.is_match(n) {
        Stance::Upright
    } else {
        Stance::Decal
    }
}

/// The beacon a feature puts over its cell: the way down, up, or out.
pub fn beacon_for(feature: Option<&Feature>) -> Option<Beacon> {
    match feature? {
        Feature::Stairs { dir, .. } | Feature::Hatch { dir } => Some(if *dir == UpDown::Up { Beacon::Up } else { Beacon::Down }),
        Feature::Portal | Feature::Transporter => Some(Beacon::Portal),
        _ => None,
    }
}

regex!(BUILT_IN = r"DOOR|GATE|RUNELIGHT|STAIR|HATCH");

/// Whether a feature stands on the dungeon rather than being built into it. Doors and gates
/// in every state are masonry, and so are Zot's runelights; stairs and hatches belong to the
/// ground. What is left (statues, altars, fountains, shops, portals, trees) stands in its
/// cell like a monster, so a renderer that turns the ground can stand it upright over it.
pub fn stands_free(name: Option<&str>, feature: Option<&Feature>) -> bool {
    if stance_for(name, feature) != Stance::Upright {
        return false;
    }
    if matches!(feature, Some(Feature::Door { .. }) | Some(Feature::Stairs { .. }) | Some(Feature::Hatch { .. })) {
        return false;
    }
    !BUILT_IN.is_match(name.unwrap_or(""))
}

/// A branch's sky and light tint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelPresentation {
    pub sky: Sky,
    pub tint: [f32; 3],
}

regex!(P_OPEN = r"shoal|swamp");
regex!(P_ABYSS = r"abyss");
regex!(P_HELL = r"zot|pandemonium|hell|gehenna|cocytus|tartarus|dis\b");
regex!(P_GREEN = r"lair|snake|spider|slime|orc");
regex!(P_RED = r"vault|crypt|tomb|depths|elf");

/// Branch presentation from the place name.
pub fn level_presentation(place: &str) -> LevelPresentation {
    let p = place.to_lowercase();
    let (sky, tint) = if P_OPEN.is_match(&p) {
        (Sky::Open, if p.contains("shoal") { [0.8, 0.9, 1.0] } else { [0.85, 1.0, 0.85] })
    } else if P_ABYSS.is_match(&p) {
        (Sky::Dark, [0.9, 0.8, 1.0])
    } else if P_HELL.is_match(&p) {
        (Sky::Dark, if p.contains("cocytus") { [0.8, 0.9, 1.0] } else { [1.0, 0.85, 0.85] })
    } else if P_GREEN.is_match(&p) {
        (Sky::None, [0.9, 1.0, 0.9])
    } else if P_RED.is_match(&p) {
        (Sky::None, [1.0, 0.9, 0.9])
    } else {
        (Sky::None, [1.0, 1.0, 1.0])
    };
    LevelPresentation { sky, tint }
}

regex!(GATE_PIECE = r"^DNGN_(V?)GATE_(.+)_(LEFT|MIDDLE|RIGHT|UP|DOWN)(_CRYPT)?$");

/// The faces of a closed door in a row of doors (`SceneCell::face_tiles`). crawl picks each
/// door's piece of gate art from the doors beside it (tilepick.cc `apply_variations`): left,
/// middle and right along a row running east–west, up, middle and down along one running
/// north–south. The left piece is the west end as seen from the south; from the north it is
/// on the viewer's right, so there it shows the right piece. A north–south gate wears the
/// east–west pieces on its sides.
pub fn gate_faces(gd: &Gamedata, name: Option<&str>) -> Option<FaceTiles> {
    let m = GATE_PIECE.captures(name.unwrap_or(""))?;
    let v = !m[1].is_empty();
    let set = &m[2];
    let part = &m[3];
    let crypt = m.get(4).map_or("", |c| c.as_str());
    let piece = |p: &str| gd.dngn.id(&format!("DNGN_GATE_{set}_{p}{crypt}"));
    let (start, mid, end) = (piece("LEFT")?, piece("MIDDLE")?, piece("RIGHT")?);
    if part == "MIDDLE" {
        return v.then_some(FaceTiles { w: Some(mid), e: Some(mid), ..FaceTiles::default() });
    }
    let first = part == "LEFT" || part == "UP";
    Some(if v {
        if first {
            FaceTiles { w: Some(start), e: Some(end), ..FaceTiles::default() }
        } else {
            FaceTiles { w: Some(end), e: Some(start), ..FaceTiles::default() }
        }
    } else if first {
        FaceTiles { n: Some(end), ..FaceTiles::default() }
    } else {
        FaceTiles { n: Some(start), ..FaceTiles::default() }
    })
}

regex!(L_WATER = r"^(?:DNGN|SHOALS)_(DEEP|SHALLOW)_WATER(_MURKY)?|^DNGN_OPEN_SEA");

/// A cell's liquid, from its tile name. crawl puts `TILE_FLAG_WATER` on lava too, so the
/// name is what tells them apart.
pub fn liquid_of(tile: Option<&str>) -> Option<Liquid> {
    let tile = tile.filter(|t| !t.is_empty())?;
    let l = |kind, deep| Some(Liquid { kind, deep });
    if tile.starts_with("DNGN_LAVA") {
        return l(LiquidKind::Lava, true);
    }
    if tile.starts_with("DNGN_TOXIC_BOG") {
        return l(LiquidKind::Bog, false);
    }
    if tile.starts_with("DNGN_STARWATER") {
        return l(LiquidKind::Starwater, true);
    }
    if tile.starts_with("LIQUEFACTION") {
        return l(LiquidKind::Mud, false);
    }
    if tile.starts_with("FLOOR_SLIME_ACIDIC") {
        return l(LiquidKind::Acid, false);
    }
    if tile.starts_with("FLOOR_SLIME") {
        return l(LiquidKind::Slime, false);
    }
    let w = L_WATER.captures(tile)?;
    let kind = if w.get(2).is_some() { LiquidKind::Murky } else { LiquidKind::Water };
    l(kind, w.get(1).map(|m| m.as_str()) != Some("SHALLOW"))
}

/// What the reticle calls a liquid.
pub fn liquid_label(liquid: &Liquid) -> &'static str {
    match liquid.kind {
        LiquidKind::Lava => "lava",
        LiquidKind::Bog => "toxic bog",
        LiquidKind::Mud => "mud",
        LiquidKind::Acid => "corroding ground",
        LiquidKind::Slime => "floor",
        _ if liquid.deep => "deep water",
        _ => "shallow water",
    }
}

regex!(SHOP_KIND = r"^SHOP_(.+)$");

/// What kind of shop the tile is (tilepick.cc `tileidx_shop`), as its wares read: "weapon
/// shop". The shopkeeper's own name is never sent per square, so the kind is as close as a
/// label can get.
pub fn shop_label(name: Option<&str>) -> String {
    match SHOP_KIND.captures(name.unwrap_or("")) {
        None => "shop".into(),
        Some(m) => {
            let w = m[1].to_lowercase().replace('_', " ");
            format!("{} shop", w.strip_suffix('s').unwrap_or(&w))
        }
    }
}

/// `s.replace(/\b\w/g, c => c.toUpperCase())`.
fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_word = false;
    for c in s.chars() {
        let word = c.is_ascii_alphanumeric() || c == '_';
        out.push(if word && !prev_word { c.to_ascii_uppercase() } else { c });
        prev_word = word;
    }
    out
}

/// The reticle label of a feature.
pub fn feature_label(f: Option<&Feature>, name: Option<&str>) -> String {
    let Some(f) = f else {
        return strip(name.unwrap_or("feature"), "DNGN_").to_lowercase().replace('_', " ");
    };
    match f {
        Feature::Stairs { dir, branch, exit } => {
            if *exit {
                "exit from the dungeon"
            } else if *branch {
                "branch entrance"
            } else if *dir == UpDown::Down {
                "stairs down"
            } else {
                "stairs up"
            }
        }
        .into(),
        Feature::Hatch { dir } => if *dir == UpDown::Down { "hatch down" } else { "hatch up" }.into(),
        Feature::Door { state } => format!("{} door", door_state_name(*state)),
        Feature::Altar { god } => match god.as_deref().filter(|g| !g.is_empty()) {
            Some(g) => format!("altar of {}", title_case(g)),
            None => "altar".into(),
        },
        Feature::Shop => shop_label(name),
        Feature::Fountain { kind } => kind.replace('_', " "),
        Feature::Portal => "portal".into(),
        Feature::Transporter => "transporter".into(),
        Feature::Trap { kind } => match kind.as_deref().filter(|k| !k.is_empty()) {
            Some(k) => format!("{} trap", k.replace('_', " ")),
            None => "trap".into(),
        },
        Feature::Other { name } => name.replace('_', " "),
    }
}

pub(crate) fn door_state_name(s: DoorState) -> &'static str {
    match s {
        DoorState::Open => "open",
        DoorState::Closed => "closed",
        DoorState::Runed => "runed",
        DoorState::Sealed => "sealed",
    }
}

regex!(VEGETATION = r"(?i)tree|mangrove|plant|bush|fungus|oklob|lotus");
regex!(RUBBLE_RE = r"(?i)pile of debris|briar patch|pillar of salt|block of ice");

/// Monsters that are stationary vegetation, drawn as fixtures rather than actors.
pub fn is_vegetation(name: Option<&str>) -> bool {
    VEGETATION.is_match(name.unwrap_or(""))
}

pub(crate) fn is_rubble(name: &str) -> bool {
    RUBBLE_RE.is_match(name)
}

/// Stationary non-living monsters that are really terrain: piles of debris, briar patches,
/// pillars of salt, blocks of ice, and vegetation. A monster behind one opens a sight window
/// in it instead of being painted over.
pub fn is_scenery(name: Option<&str>) -> bool {
    is_vegetation(name) || is_rubble(name.unwrap_or(""))
}

regex!(I_PREFIX = r"^(UNSEEN|WPN|ARM|MI|FOOD|MISC)_");
regex!(I_OFFSET = r"_(NORMAL|RANDART)?_?OFFSET(_\d+)?$");
regex!(I_DIGITS = r"_?\d+$");
regex!(I_SUFFIX = r"_(MAGIC|RANDART|INERT)$");
regex!(I_PLAIN = r"^(BOOK|GEM|TALISMAN)(_|$)");
regex!(I_CORPSE = r"^CORPSE_(.+)$");
regex!(I_TYPED = r"^(WAND|RING|AMU|AMULET|POT|POTION|SCR|SCROLL|STAFF)(?:_(.+))?$");
regex!(I_ADJ = r"^(.+)_(STEEL|SILVER|POISONED|CURARE|BLINDING|FRENZY)$");
// a branded missile is MI_*_MAGIC too, and too common to stand out
regex!(I_EPIC = r"^(WPN|ARM|THELM)_.*_MAGIC(_?\d+)?$");

/// How good an item looks by its tile (tilepick.cc `tileidx_enchant_equ`): an unrandart has
/// its own UNRAND_ tile, a random artefact the _RANDART one, and a weapon or armour that is
/// glowing, runed or shiny the _MAGIC one.
pub fn item_rarity(tile: &str) -> Option<ItemRarity> {
    if tile.starts_with("UNRAND_") {
        Some(ItemRarity::Unique)
    } else if tile.contains("_RANDART") {
        Some(ItemRarity::Legendary)
    } else if I_EPIC.is_match(tile) {
        Some(ItemRarity::Epic)
    } else {
        None
    }
}

/// What a map cell's item tile says the top item is, in words: the tile's base name with the
/// sprite-set affixes dropped and the typed classes joined with "of" (WAND_FLAME "wand of
/// flame", CORPSE_BAT "bat corpse", MI_ARROW_STEEL "steel arrow"). The server sends a cell
/// no item name, only its tile. "" when the tile has no name.
pub fn item_tile_name(tile: Option<&str>) -> String {
    let Some(tile) = tile.filter(|t| !t.is_empty()) else { return String::new() };
    let n = I_PREFIX.replace(tile, "");
    let n = I_OFFSET.replace(&n, "");
    let n = I_DIGITS.replace(&n, "");
    let n = I_SUFFIX.replace(&n, "");
    let mut n = I_DIGITS.replace(&n, "").into_owned();
    if n.starts_with("PARCHMENT_") {
        return "parchment".into();
    }
    if let Some(m) = I_PLAIN.captures(&n) {
        return m[1].to_lowercase();
    }
    if let Some(m) = I_CORPSE.captures(&n) {
        return format!("{} corpse", m[1].to_lowercase().replace('_', " "));
    }
    if let Some(m) = I_TYPED.captures(&n) {
        let cls = match &m[1] {
            "AMU" => "amulet".to_string(),
            "POT" => "potion".to_string(),
            "SCR" => "scroll".to_string(),
            other => other.to_lowercase(),
        };
        return match m.get(2) {
            Some(t) => format!("{cls} of {}", t.as_str().to_lowercase().replace('_', " ")),
            None => cls,
        };
    }
    // a missile's material or coating is a suffix on the tile and an adjective in the name
    if let Some(m) = I_ADJ.captures(&n) {
        n = format!("{}_{}", &m[2], &m[1]);
    }
    n.to_lowercase().replace('_', " ")
}
