//! A supported subset of DCSS's `.des` vault syntax, compiled to a Scene the renderers draw
//! exactly as they draw a live level (the port of `packages/vault`). Written for the front
//! end's room: a place authored by hand from the game's own tiles, with nothing made up.
//!
//! This is a subset, not the vault language. What is accepted:
//!
//! ```text
//! NAME: <name>                 the vault's name
//! MAP … ENDMAP                 the map, one row per line; ' ' is void
//! TILE:  <glyph> = <tile>      the glyph is masonry drawn with this wall tile
//! FTILE: <glyph> = <tile>      the floor tile under cells of this glyph (FTILE: . = … is the
//!                              floor everywhere else); a glyph with only an FTILE is open
//!                              floor of that tile: a pool of water, a patch of moss
//! KFEAT: <glyph> = <tile>      a dungeon feature: stairs, door, altar, fountain, statue, tree
//! KITEM: <glyph> = <tile>      an item lying on the floor
//! KMONS: <glyph> = <tile>      a monster standing on the floor
//! EYE:   <glyph> [<heading>]   Isorun's own: where the camera stands and the way it faces
//!                              (N NE E SE S SW W NW, N by default); the glyph is floor
//! LIGHT: <glyph>               Isorun's own: where the light is, when it is not the eye
//! # …                          a comment, outside the map
//! ```
//!
//! Tile names are the ones the server's tileinfo publishes, in either case. Every other
//! directive of the vault language (SUBST, SHUFFLE, MARKER, Lua, ...) is refused with its line
//! number rather than passed over: a line that does nothing here would be a lie in the file.

use std::collections::HashMap;
use std::fmt;

use isorun_scene::{
    cell_key, Attitude, Beacon, Billboard, BillboardKind, Camera, CellKind, Dir8, DoorState, Feature, LiquidKind, Rect, Scene, SceneCell, SceneLevel, Sky,
    Stance, TileId, UpDown, Visibility, WallStyle,
};
use isorun_scene_webtiles::{classify_feature, is_vegetation, liquid_label, liquid_of, stance_for, stands_free};
use once_cell::sync::Lazy;
use regex::Regex;

pub mod atlas;

pub use atlas::{PackedAtlas, PackedAtlasJson, PackedName, PackedTile, SpriteRect, PACKED_ATLAS};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LegendKind {
    Wall,
    Floor,
    Feat,
    Item,
    Mons,
}

impl LegendKind {
    pub fn name(self) -> &'static str {
        match self {
            LegendKind::Wall => "wall",
            LegendKind::Floor => "floor",
            LegendKind::Feat => "feat",
            LegendKind::Item => "item",
            LegendKind::Mons => "mons",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegendEntry {
    pub glyph: char,
    pub kind: LegendKind,
    /// The tile's name, upper case as tileinfo publishes it.
    pub tile: String,
    pub line: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Eye {
    pub x: i32,
    pub y: i32,
    pub facing: Dir8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vault {
    pub name: String,
    pub width: usize,
    pub height: usize,
    /// The map, every row padded to `width` with void.
    pub map: Vec<Vec<char>>,
    pub legend: Vec<LegendEntry>,
    pub eye: Eye,
    pub light: (i32, i32),
}

impl Vault {
    /// The map's rows as strings.
    pub fn rows(&self) -> Vec<String> {
        self.map.iter().map(|r| r.iter().collect()).collect()
    }
}

/// A mistake in a `.des` file, with the line it is on (0 for one about the whole room).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesError {
    pub message: String,
    pub line: usize,
}

impl DesError {
    fn new(message: impl Into<String>, line: usize) -> Self {
        DesError { message: message.into(), line }
    }
}

impl fmt::Display for DesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for DesError {}

pub const DIRECTIVES: [&str; 10] = ["NAME", "TILE", "FTILE", "KFEAT", "KITEM", "KMONS", "EYE", "LIGHT", "MAP", "ENDMAP"];

const HEADINGS: [(&str, Dir8); 8] = [("N", 0), ("NE", 1), ("E", 2), ("SE", 3), ("S", 4), ("SW", 5), ("W", 6), ("NW", 7)];

fn legend_kind(key: &str) -> Option<LegendKind> {
    Some(match key {
        "TILE" => LegendKind::Wall,
        "FTILE" => LegendKind::Floor,
        "KFEAT" => LegendKind::Feat,
        "KITEM" => LegendKind::Item,
        "KMONS" => LegendKind::Mons,
        _ => return None,
    })
}

static DIRECTIVE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^([A-Z]+):\s*(.*)$").unwrap());
static LEGEND: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\S)\s*=\s*(\S+)$").unwrap());
static TILE_NAME: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[A-Z0-9_]+$").unwrap());

/// Parse the subset; a [`DesError`], naming the line, on anything outside it.
pub fn parse_des(text: &str) -> Result<Vault, DesError> {
    let lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    let mut name = String::new();
    let mut map: Vec<String> = Vec::new();
    let mut legend: Vec<LegendEntry> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut eye_glyph: Option<char> = None;
    let mut eye_facing: Dir8 = 0;
    let mut light_glyph: Option<char> = None;
    let (mut in_map, mut saw_map, mut map_line) = (false, false, 0);
    for (i, raw) in lines.iter().enumerate() {
        let n = i + 1;
        if in_map {
            if raw.trim() == "ENDMAP" {
                in_map = false;
            } else {
                map.push(raw.trim_end().to_string());
            }
            continue;
        }
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "MAP" {
            if saw_map {
                return Err(DesError::new("a second MAP; one map per vault", n));
            }
            in_map = true;
            saw_map = true;
            map_line = n;
            continue;
        }
        if line == "ENDMAP" {
            return Err(DesError::new("ENDMAP without MAP", n));
        }
        let Some(m) = DIRECTIVE.captures(line) else {
            return Err(DesError::new(format!("not a directive: {line}"), n));
        };
        let (key, rest) = (m.get(1).unwrap().as_str(), m.get(2).unwrap().as_str());
        if !DIRECTIVES.contains(&key) {
            return Err(DesError::new(format!("{key} is not in the supported subset (accepted: {})", DIRECTIVES.join(", ")), n));
        }
        if key == "NAME" {
            name = rest.trim().to_string();
            continue;
        }
        if key == "EYE" || key == "LIGHT" {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            let glyph = parts.first().filter(|g| g.chars().count() == 1).and_then(|g| g.chars().next());
            let Some(glyph) = glyph else { return Err(DesError::new(format!("{key} needs one glyph"), n)) };
            if key == "EYE" {
                if eye_glyph.is_some() {
                    return Err(DesError::new("a second EYE", n));
                }
                eye_glyph = Some(glyph);
                if let Some(h) = parts.get(1) {
                    let Some(&(_, f)) = HEADINGS.iter().find(|(k, _)| *k == h.to_uppercase()) else {
                        let all: Vec<&str> = HEADINGS.iter().map(|(k, _)| *k).collect();
                        return Err(DesError::new(format!("EYE heading {h} is not one of {}", all.join(" ")), n));
                    };
                    eye_facing = f;
                }
                if parts.len() > 2 {
                    return Err(DesError::new("EYE takes a glyph and a heading, nothing more", n));
                }
            } else {
                if light_glyph.is_some() {
                    return Err(DesError::new("a second LIGHT", n));
                }
                if parts.len() > 1 {
                    return Err(DesError::new("LIGHT takes one glyph", n));
                }
                light_glyph = Some(glyph);
            }
            continue;
        }
        let Some(lm) = LEGEND.captures(rest.trim()) else {
            return Err(DesError::new(format!("{key} reads \"{key}: <glyph> = <tile>\""), n));
        };
        let glyph = lm[1].chars().next().unwrap();
        let tile = lm[2].to_uppercase();
        if glyph == ' ' || (glyph == '.' && key != "FTILE") {
            let only = if glyph == '.' { " (only FTILE: . = …)" } else { "" };
            return Err(DesError::new(format!("{key}: \"{glyph}\" cannot carry a {key} entry{only}"), n));
        }
        if !TILE_NAME.is_match(&tile) {
            return Err(DesError::new(format!("{tile} is not a tile name"), n));
        }
        let kind = legend_kind(key).unwrap();
        let slot = if kind == LegendKind::Floor { format!("F{glyph}") } else { glyph.to_string() };
        if let Some(dupe) = seen.get(&slot) {
            return Err(DesError::new(format!("\"{glyph}\" is already defined on line {dupe}"), n));
        }
        seen.insert(slot, n);
        legend.push(LegendEntry { glyph, kind, tile, line: n });
    }
    if in_map {
        return Err(DesError::new("MAP without ENDMAP", map_line));
    }
    if !saw_map {
        return Err(DesError::new("no MAP", lines.len()));
    }
    while map.last().is_some_and(|r| r.trim().is_empty()) {
        map.pop();
    }
    while map.first().is_some_and(|r| r.trim().is_empty()) {
        map.remove(0);
    }
    if map.is_empty() {
        return Err(DesError::new("the map is empty", map_line));
    }
    let width = map.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    let rows: Vec<Vec<char>> = map
        .iter()
        .map(|r| {
            let mut row: Vec<char> = r.chars().collect();
            row.resize(width, ' ');
            row
        })
        .collect();
    // the floor under everything
    if !legend.iter().any(|e| e.kind == LegendKind::Floor && e.glyph == '.') {
        return Err(DesError::new("no floor: add \"FTILE: . = <tile>\"", map_line));
    }
    // every glyph in the map is spoken for
    let mut known: Vec<char> = vec![' ', '.'];
    known.extend(legend.iter().map(|e| e.glyph));
    known.extend(eye_glyph);
    known.extend(light_glyph);
    let mut eye: Option<Eye> = None;
    let mut light: Option<(i32, i32)> = None;
    for (y, row) in rows.iter().enumerate() {
        for (x, &g) in row.iter().enumerate() {
            let at = map_line + 1 + y;
            if !known.contains(&g) {
                return Err(DesError::new(format!("\"{g}\" at {x},{y} of the map has no legend entry"), at));
            }
            if Some(g) == eye_glyph {
                if eye.is_some() {
                    return Err(DesError::new(format!("a second \"{g}\" (the eye) at {x},{y}"), at));
                }
                eye = Some(Eye { x: x as i32, y: y as i32, facing: eye_facing });
            }
            if Some(g) == light_glyph {
                if light.is_some() {
                    return Err(DesError::new(format!("a second \"{g}\" (the light) at {x},{y}"), at));
                }
                light = Some((x as i32, y as i32));
            }
        }
    }
    let Some(eye_glyph) = eye_glyph else { return Err(DesError::new("no EYE: say where the camera stands", lines.len())) };
    let Some(eye) = eye else { return Err(DesError::new(format!("the eye's glyph \"{eye_glyph}\" is not on the map"), map_line)) };
    if let (Some(g), None) = (light_glyph, light) {
        return Err(DesError::new(format!("the light's glyph \"{g}\" is not on the map"), map_line));
    }
    // the eye and the light stand on floor: a glyph of theirs must not also be a wall or a feature
    for (g, what) in [(Some(eye_glyph), "eye"), (light_glyph, "light")] {
        let Some(g) = g else { continue };
        if let Some(e) = legend.iter().find(|l| l.glyph == g && l.kind != LegendKind::Floor) {
            return Err(DesError::new(format!("\"{g}\" is the {what} and cannot also be a {}", e.kind.name()), e.line));
        }
    }
    Ok(Vault { name, width, height: rows.len(), map: rows, legend, eye, light: light.unwrap_or((eye.x, eye.y)) })
}

/// The tile names a vault uses, each once, in legend order.
pub fn vault_tile_names(v: &Vault) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in &v.legend {
        if !out.contains(&e.tile) {
            out.push(e.tile.clone());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Compiling to a Scene

/// What a Scene needs of a tile set: an id per name, and how many variants stand behind an
/// id. Any `Fn(&str) -> Option<TileId>` is one with no variants.
pub trait TileNames {
    fn id(&self, name: &str) -> Option<TileId>;
    /// Variants of a base tile (tileinfo `tile_count`); 1 when unknown.
    fn tile_count(&self, _id: TileId) -> u32 {
        1
    }
}

impl<F: Fn(&str) -> Option<TileId>> TileNames for F {
    fn id(&self, name: &str) -> Option<TileId> {
        self(name)
    }
}

/// Which variant of a tile a cell gets: the server hands every floor and wall cell one of its
/// variants at level build, and the vault does the same by position, so a room reads the same
/// every time it is built. JavaScript's 32-bit integer arithmetic, so both clients agree.
pub fn variant_at(base: TileId, count: u32, x: i32, y: i32) -> TileId {
    if count <= 1 {
        return base;
    }
    let int32 = |v: i64| v as i32;
    let h = int32(x as i64 * 73856093) ^ int32(y as i64 * 19349663) ^ int32(base as i64 * 83492791);
    let h = h as u32;
    let h = h ^ (h >> 13);
    base + h % count
}

fn height_of(kind: LegendKind, name: &str) -> f32 {
    if kind == LegendKind::Item {
        0.4
    } else if is_vegetation(Some(name)) {
        0.95
    } else {
        0.6
    }
}

static SEE_THROUGH: Lazy<Regex> = Lazy::new(|| Regex::new("TRANSPARENT|CRYSTAL|GLASS").unwrap());

fn words(tile: &str, prefix: &str) -> String {
    tile.strip_prefix(prefix).unwrap_or(tile).to_lowercase().replace('_', " ")
}

/// The vault as a Scene: every cell seen, the light where the vault says, revisions set once
/// so a renderer never rebuilds the geometry. A [`DesError`] when a tile name is not in
/// `names`, the eye or the light stand in a wall, or an open cell touches void (the room must
/// be closed; a tree may stand at the edge of the void, the dark showing between the trunks).
pub fn vault_scene(v: &Vault, names: &dyn TileNames) -> Result<Scene, DesError> {
    let missing: Vec<String> = vault_tile_names(v).into_iter().filter(|n| names.id(n).is_none()).collect();
    if let Some(first) = missing.first() {
        let line = v.legend.iter().find(|e| &e.tile == first).map_or(0, |e| e.line);
        return Err(DesError::new(format!("tiles not in this tile set: {}", missing.join(", ")), line));
    }
    let id_of = |tile: &str| names.id(tile).unwrap();
    let mut by_glyph: HashMap<char, &LegendEntry> = HashMap::new();
    let mut floor_by_glyph: HashMap<char, &LegendEntry> = HashMap::new();
    for e in &v.legend {
        if e.kind == LegendKind::Floor { &mut floor_by_glyph } else { &mut by_glyph }.insert(e.glyph, e);
    }
    let floor_of = |g: char| floor_by_glyph.get(&g).or_else(|| floor_by_glyph.get(&'.')).copied().unwrap();
    let mut scene = Scene { bounds: Rect { left: 0, top: 0, right: v.width as i32 - 1, bottom: v.height as i32 - 1 }, ..Scene::default() };
    // the lid's votes, in the order the walls were met
    let mut wall_bases: Vec<(TileId, usize)> = Vec::new();
    for (y, row) in v.map.iter().enumerate() {
        for (x, &g) in row.iter().enumerate() {
            if g == ' ' {
                continue;
            }
            let (x, y) = (x as i32, y as i32);
            let floor = floor_of(g);
            let fid = id_of(&floor.tile);
            let mut cell = SceneCell {
                x,
                y,
                kind: CellKind::Floor,
                visibility: Visibility::Visible,
                occluder: false,
                floor_tile: variant_at(fid, names.tile_count(fid), x, y),
                glyph: Some(g.to_string()),
                label: Some("floor".into()),
                ..SceneCell::default()
            };
            if let Some(liquid) = liquid_of(Some(&floor.tile)) {
                cell.liquid = Some(liquid);
                cell.flags.water = matches!(liquid.kind, LiquidKind::Water | LiquidKind::Murky | LiquidKind::Starwater);
                cell.flags.lava = liquid.kind == LiquidKind::Lava;
                cell.label = Some(liquid_label(&liquid).into());
            }
            match by_glyph.get(&g) {
                Some(e) if e.kind == LegendKind::Wall => {
                    let id = id_of(&e.tile);
                    cell.kind = CellKind::Wall;
                    cell.occluder = true;
                    cell.wall_tile = Some(variant_at(id, names.tile_count(id), x, y));
                    cell.wall_style = Some(if SEE_THROUGH.is_match(&e.tile) { WallStyle::Transparent } else { WallStyle::Solid });
                    cell.flags.torch_wall = e.tile.to_uppercase().contains("_TORCH");
                    cell.label = Some(words(&e.tile, "WALL_"));
                    cell.glyph = Some("#".into());
                    match wall_bases.iter_mut().find(|w| w.0 == id) {
                        Some(w) => w.1 += 1,
                        None => wall_bases.push((id, 1)),
                    }
                }
                Some(e) if e.kind == LegendKind::Feat => {
                    let id = id_of(&e.tile);
                    let f = classify_feature(Some(&e.tile), None, None);
                    // trees take a variant by position as floors do (a wood is not one tree); a
                    // statue's variants are other statues, so it stays the one named
                    cell.feature_tile = Some(if is_vegetation(Some(&e.tile)) { variant_at(id, names.tile_count(id), x, y) } else { id });
                    match &f {
                        Some(Feature::Door { state }) if *state != DoorState::Open => {
                            cell.kind = CellKind::Door;
                            cell.occluder = true;
                            cell.wall_tile = Some(id);
                            cell.stance = Some(Stance::Upright);
                            cell.label = Some(if *state == DoorState::Closed { "closed door".into() } else { format!("{} door", door_word(*state)) });
                        }
                        _ => {
                            cell.kind = CellKind::Feature;
                            cell.stance = Some(stance_for(Some(&e.tile), f.as_ref()));
                            cell.freestanding = stands_free(Some(&e.tile), f.as_ref());
                            cell.beacon = match &f {
                                Some(Feature::Stairs { dir, .. }) | Some(Feature::Hatch { dir }) => {
                                    Some(if *dir == UpDown::Up { Beacon::Up } else { Beacon::Down })
                                }
                                Some(Feature::Portal) | Some(Feature::Transporter) => Some(Beacon::Portal),
                                _ => None,
                            };
                            cell.label = Some(words(&e.tile, "DNGN_"));
                        }
                    }
                    cell.feature = f;
                }
                Some(e) => {
                    let id = id_of(&e.tile);
                    let name = words(&e.tile, "MONS_");
                    let mons = e.kind == LegendKind::Mons;
                    let tile = if mons { variant_at(id, names.tile_count(id), x, y) } else { id };
                    let kind = if mons { BillboardKind::Monster } else { BillboardKind::Item };
                    let mut b = Billboard::new(x, y, tile, kind, height_of(e.kind, &name));
                    b.name = Some(name);
                    if mons {
                        b.attitude = Some(Attitude::Hostile);
                        b.scenery = true;
                    }
                    scene.billboards.push(b);
                }
                None => {}
            }
            scene.cells.insert(cell_key(x, y), cell);
        }
    }
    // closed: an open cell never touches void, so the eye never looks into nothing; except a
    // tree, which may stand at the edge of it: a wood is a boundary too
    let is_tree = |c: &SceneCell| c.kind == CellKind::Feature && is_vegetation(c.label.as_deref());
    let mut keys: Vec<_> = scene.cells.keys().copied().collect();
    keys.sort_unstable();
    for k in keys {
        let c = &scene.cells[&k];
        if c.occluder || is_tree(c) {
            continue;
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                if !scene.cells.contains_key(&cell_key(c.x + dx, c.y + dy)) {
                    return Err(DesError::new(format!("the room is open at {},{}: an open cell touches void", c.x + dx, c.y + dy), 0));
                }
            }
        }
    }
    for (what, (x, y)) in [("eye", (v.eye.x, v.eye.y)), ("light", v.light)] {
        if scene.cell(x, y).is_none_or(|c| c.occluder) {
            return Err(DesError::new(format!("the {what} at {x},{y} does not stand on open floor"), 0));
        }
    }
    // the lid is the masonry most of the room is built of, as the game's levels take theirs
    let mut lid: Option<TileId> = None;
    let mut most = 0;
    for &(id, n) in &wall_bases {
        if n > most {
            most = n;
            lid = Some(id);
        }
    }
    scene.level = SceneLevel { ceiling_tile: lid, sky: Sky::None, tint: [1.0, 1.0, 1.0] };
    scene.player = v.light;
    scene.player_on_level = true;
    scene.revision = 1;
    scene.layout_revision = 1;
    Ok(scene)
}

fn door_word(s: DoorState) -> &'static str {
    match s {
        DoorState::Open => "open",
        DoorState::Closed => "closed",
        DoorState::Runed => "runed",
        DoorState::Sealed => "sealed",
    }
}

/// The camera where the vault's EYE stands, facing its heading.
pub fn vault_camera(v: &Vault) -> Camera {
    Camera::new(v.eye.x, v.eye.y, v.eye.facing)
}
