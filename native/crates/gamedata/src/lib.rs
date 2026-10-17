//! A WebTiles server's own version-specific gamedata (the port of `packages/gamedata`):
//! `enums.js`, the `tileinfo-*.js` module of every atlas and `status-icon-sizes.js`, read
//! the way the official client reads them, and answered as tile rects, tile counts, name
//! indices, decoded cell flag words and enum tables. Nothing version-specific is hardcoded,
//! so stable, trunk and forks all work and a new release needs no update here.
//!
//! The browser client evaluates these scripts; here a small interpreter (`js.rs`) runs the
//! subset of JavaScript they are written in, so the generated shapes are read by structure
//! (the `val++` numbering and its aliases, the `tile_info` arrays, the functions over them,
//! the flag tables) and nothing depends on a file's layout. The crate does no IO: the caller
//! hands it the text of each file ([`GamedataSource`]), and decodes the atlas images itself.

use std::collections::HashMap;
use std::fmt;

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

pub use isorun_scene::{TileId, TileRect};

mod flags;
mod js;

use flags::FlagSpec;
pub use flags::FlagWord;
use js::{to_number, Interp, Value};

/// Every atlas, in the order the tileinfo modules number their tiles. Each is `<name>.png`.
pub const ATLASES: [&str; 7] = ["floor", "wall", "feat", "main", "player", "gui", "icons"];

/// The atlases whose tiles stand in the world: features, items and monsters.
pub const SPRITE_ATLASES: [&str; 3] = ["feat", "main", "player"];

/// The scripts [`load_gamedata`] needs, in the order it evaluates them.
pub const SCRIPTS: [&str; 8] = [
    "enums.js",
    "tileinfo-floor.js",
    "tileinfo-wall.js",
    "tileinfo-feat.js",
    "tileinfo-main.js",
    "tileinfo-player.js",
    "tileinfo-gui.js",
    "tileinfo-icons.js",
];

/// The optional script with how status icons lay out; a server may predate it.
pub const STATUS_ICON_SIZES: &str = "status-icon-sizes.js";

/// Every script [`load_gamedata`] reads: [`SCRIPTS`], then [`STATUS_ICON_SIZES`].
pub fn script_files() -> Vec<&'static str> {
    SCRIPTS.iter().copied().chain([STATUS_ICON_SIZES]).collect()
}

/// Every file of one version's gamedata a client wants, the atlases last (they are nearly
/// all of the bytes): the scripts, `status-icon-sizes.js`, then `<atlas>.png`.
pub fn gamedata_files() -> Vec<String> {
    script_files()
        .into_iter()
        .map(String::from)
        .chain(ATLASES.iter().map(|a| format!("{a}.png")))
        .collect()
}

/// The directory one version's gamedata is published under: `<base>/gamedata/<version>`.
pub fn gamedata_root(base: &str, version: &str) -> String {
    format!(
        "{}/gamedata/{version}",
        base.strip_suffix('/').unwrap_or(base)
    )
}

/// [`gamedata_files`] under [`gamedata_root`].
pub fn gamedata_urls(base: &str, version: &str) -> Vec<String> {
    let root = gamedata_root(base, version);
    gamedata_files()
        .into_iter()
        .map(|f| format!("{root}/{f}"))
        .collect()
}

/// Where the files come from: the text of a file by its name (`enums.js`, ...).
pub trait GamedataSource {
    fn fetch_text(&mut self, name: &str) -> Result<String, String>;
}

impl<F: FnMut(&str) -> Result<String, String>> GamedataSource for F {
    fn fetch_text(&mut self, name: &str) -> Result<String, String> {
        self(name)
    }
}

/// A load that failed: which file and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GamedataError {
    /// The file's name, e.g. `enums.js`.
    pub file: String,
    /// The file as the message names it: under the gamedata root when a base was given.
    pub path: String,
    pub message: String,
}

impl fmt::Display for GamedataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "gamedata {}: {}", self.path, self.message)
    }
}

impl std::error::Error for GamedataError {}

/// A tile's entry in a `tile_info` array, as the server writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize)]
pub struct TileInfoRaw {
    pub w: i32,
    pub h: i32,
    pub ox: i32,
    pub oy: i32,
    pub sx: i32,
    pub sy: i32,
    pub ex: i32,
    pub ey: i32,
}

/// One `tileinfo-<atlas>.js` module, its ids the client's (see [`TileModule::offset`]).
#[derive(Clone, Debug)]
pub struct TileModule {
    /// The atlas, also the png's base name.
    pub atlas: &'static str,
    /// First id (inclusive) and end (exclusive).
    pub first: i64,
    pub end: i64,
    /// What is added to the server's id of a tile in this module to get the client's. Zero
    /// where the server numbers every module after the one before it, as DCSS does; a server
    /// that starts some modules at 0 again (bcrawl's main, gui and icons) has its modules
    /// moved up to stand one after the other.
    pub offset: i64,
    /// The module's named ids (moved by `offset`), in the order the module sets them.
    exports: Vec<(String, i64)>,
    export_index: HashMap<String, usize>,
    infos: Vec<Option<TileInfoRaw>>,
    counts: Vec<u32>,
    bases: Vec<Option<i64>>,
}

impl TileModule {
    pub fn contains(&self, id: i64) -> bool {
        id >= self.first && id < self.end
    }

    fn slot(&self, id: i64) -> Option<usize> {
        if self.contains(id) {
            Some((id - self.first) as usize)
        } else {
            None
        }
    }

    /// The `tile_info` entry of a tile, by the client's id.
    pub fn tile_info(&self, id: i64) -> Option<TileInfoRaw> {
        self.slot(id).and_then(|i| self.infos[i])
    }

    /// The module's `tile_count`, 1 where it gives none.
    pub fn tile_count(&self, id: i64) -> u32 {
        self.slot(id).map(|i| self.counts[i]).unwrap_or(1)
    }

    /// The module's `basetile`: the first variant of the tile's group.
    pub fn basetile(&self, id: i64) -> Option<i64> {
        self.slot(id).and_then(|i| self.bases[i])
    }

    /// A named id the module exports (`FLOOR_GREY_DIRT`, `TILE_FLOOR_MAX`, ...).
    pub fn export(&self, name: &str) -> Option<i64> {
        self.export_index.get(name).map(|&i| self.exports[i].1)
    }

    /// Every named id, in the order the module sets them.
    pub fn exports(&self) -> &[(String, i64)] {
        &self.exports
    }
}

/// Names of the tiles of one or more modules.
#[derive(Clone, Debug, Default)]
pub struct NameIndex {
    names: HashMap<i64, String>,
    ids: HashMap<String, i64>,
    /// (first, end, basetile per id) of each module
    mods: Vec<(i64, i64, Vec<Option<i64>>)>,
}

impl NameIndex {
    fn new(mods: &[&TileModule]) -> NameIndex {
        let mut names: HashMap<i64, String> = HashMap::new();
        let mut ids: HashMap<String, i64> = HashMap::new();
        // Each module's `*_MAX` sentinel is the id of the next module's first tile
        // (TILE_WALL_MAX is DNGN_TREE, TILE_FEAT_MAX is UNRAND_AIR, ...), so a sentinel only
        // names an id when nothing else does.
        let mut sentinels: Vec<(i64, &str)> = Vec::new();
        for m in mods {
            for (k, v) in &m.exports {
                ids.insert(k.clone(), *v);
                if k.ends_with("_MAX") {
                    sentinels.push((*v, k));
                } else {
                    names.entry(*v).or_insert_with(|| k.clone());
                }
            }
        }
        for (v, k) in sentinels {
            names.entry(v).or_insert_with(|| k.to_string());
        }
        let mods = mods
            .iter()
            .map(|m| (m.first, m.end, m.bases.clone()))
            .collect();
        NameIndex { names, ids, mods }
    }

    pub fn name(&self, id: TileId) -> Option<&str> {
        self.names.get(&(id as i64)).map(String::as_str)
    }

    pub fn id(&self, name: &str) -> Option<TileId> {
        self.ids.get(name).and_then(|&v| TileId::try_from(v).ok())
    }

    /// The canonical name of the base tile (a variation resolves to its base).
    pub fn base_name(&self, id: TileId) -> Option<&str> {
        let id = id as i64;
        let base = self
            .mods
            .iter()
            .find(|(first, end, _)| id >= *first && id < *end)
            .map(|(first, _, bases)| bases[(id - first) as usize]);
        match base {
            None => self.names.get(&id),
            Some(base) => base
                .and_then(|b| self.names.get(&b))
                .or_else(|| self.names.get(&id)),
        }
        .map(String::as_str)
    }
}

/// Range boundaries in the client's tile id space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize)]
pub struct Ranges {
    pub floor_max: i64,
    pub wall_max: i64,
    pub feat_max: i64,
    pub main_max: i64,
    pub player_max: i64,
    pub gui_max: i64,
    pub icons_max: i64,
    pub first_transparent: i64,
    pub mcache_start: i64,
}

/// Every constant and enum table `enums.js` publishes (`gxm`, `ui`, `mouse_mode`, `texture`,
/// `menu_flag`, `CHATTR`, `MF_*`, ...), as JSON: numbers, and objects of numbers. Functions
/// are left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Enums {
    values: serde_json::Map<String, serde_json::Value>,
}

impl Enums {
    /// Everything, in the order `enums.js` sets it.
    pub fn values(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.values
    }

    /// A published value: `group` "" for a top-level constant.
    pub fn value(&self, group: &str, name: &str) -> Option<&serde_json::Value> {
        if group.is_empty() {
            self.values.get(name)
        } else {
            self.table(group)?.get(name)
        }
    }

    /// An integer constant: `get("texture", "ICONS")`, `get("", "MF_WALL")`.
    pub fn get(&self, group: &str, name: &str) -> Option<i64> {
        self.value(group, name)?.as_i64()
    }

    /// A table (`mouse_mode`, `texture`, `menu_flag`, `ui`, `CHATTR`, ...).
    pub fn table(&self, group: &str) -> Option<&serde_json::Map<String, serde_json::Value>> {
        self.values.get(group)?.as_object()
    }
}

/// Compare a table of constants a client hardcodes against what the server's enums.js
/// publishes: one line per mismatch or missing name, so a caller can log a clear diagnostic
/// instead of silently drifting. A group of "" is the top-level constants.
pub fn verify_enums(gd: &Gamedata, expected: &[(&str, &[(&str, i64)])]) -> Vec<String> {
    let mut out = Vec::new();
    for (group, table) in expected {
        let published = if group.is_empty() {
            Some(gd.enums.values())
        } else {
            gd.enums.table(group)
        };
        let Some(published) = published else {
            out.push(format!(
                "enums.js does not publish {}",
                if group.is_empty() {
                    "top-level constants"
                } else {
                    group
                }
            ));
            continue;
        };
        let prefix = if group.is_empty() {
            String::new()
        } else {
            format!("{group}.")
        };
        for (name, value) in table.iter() {
            match published.get(*name) {
                None => out.push(format!(
                    "enums.js has no {prefix}{name} (client assumes {value})"
                )),
                Some(got) if got.as_f64() == Some(*value as f64) => {}
                Some(got) => out.push(format!(
                    "enums.js {prefix}{name} = {}, client assumes {value}",
                    json_to_js_string(got)
                )),
            }
        }
    }
    out
}

/// A JSON value as JavaScript's `String()` would print it.
fn json_to_js_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(_) => "[object Object]".into(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => a
            .iter()
            .map(json_to_js_string)
            .collect::<Vec<_>>()
            .join(","),
        serde_json::Value::Number(n) => n
            .as_f64()
            .map(js::num_to_string)
            .unwrap_or_else(|| n.to_string()),
        other => other.to_string(),
    }
}

#[derive(Clone, Debug)]
struct IconSizes {
    first: i64,
    table: Vec<Option<f64>>,
    /// What the function answers for an id it does not name.
    default: Option<f64>,
}

/// One server version's gamedata.
#[derive(Clone)]
pub struct Gamedata {
    pub version: String,
    /// A short hash of the module layout, which changes whenever the tile numbering does.
    pub fingerprint: String,
    pub enums: Enums,
    modules: Vec<TileModule>,
    /// Name indices: the dungeon (floor, wall, feat), main, player, gui and icons.
    pub dngn: NameIndex,
    pub main: NameIndex,
    pub player: NameIndex,
    pub gui: NameIndex,
    pub icons: NameIndex,
    pub ranges: Ranges,
    /// The module (index into [`ATLASES`]) of each texture index, from `enums.texture`.
    texture_order: Vec<Option<usize>>,
    icon_sizes: Option<IconSizes>,
    fg_flags: FlagSpec,
    bg_flags: FlagSpec,
    /// Non-fatal problems met while loading (an optional file missing, a value left out).
    pub diagnostics: Vec<String>,
}

impl fmt::Debug for Gamedata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Gamedata")
            .field("version", &self.version)
            .field("fingerprint", &self.fingerprint)
            .field("ranges", &self.ranges)
            .field("diagnostics", &self.diagnostics)
            .finish_non_exhaustive()
    }
}

fn atlas_index(name: &str) -> Option<usize> {
    ATLASES.iter().position(|a| *a == name)
}

impl Gamedata {
    /// Load from files already fetched, by name. A file missing from the map is an error,
    /// except the optional `status-icon-sizes.js`.
    pub fn from_files(
        files: &HashMap<String, String>,
        version: &str,
    ) -> Result<Gamedata, GamedataError> {
        load_gamedata_from_map(files, version)
    }

    /// Every module, in [`ATLASES`] order.
    pub fn modules(&self) -> &[TileModule] {
        &self.modules
    }

    /// A module by its atlas name.
    pub fn module(&self, atlas: &str) -> Option<&TileModule> {
        atlas_index(atlas).map(|i| &self.modules[i])
    }

    /// The module a client id falls in.
    pub fn module_for(&self, id: TileId) -> Option<&TileModule> {
        self.modules.iter().find(|m| m.contains(id as i64))
    }

    /// A module by the server's texture index (tiles.h TextureID). The order comes from the
    /// server's own `enums.texture` (FLOOR, WALL, FEAT, PLAYER, DEFAULT, GUI, ICONS); it is
    /// not the tileinfo dependency order.
    pub fn module_for_texture(&self, tex: u32) -> Option<&TileModule> {
        self.texture_order
            .get(tex as usize)
            .copied()
            .flatten()
            .map(|i| &self.modules[i])
    }

    /// The client's id of a tile the server sent from a module named by its atlas. Zero, the
    /// server's "no tile", stays zero. Every id this type hands out or takes is the client's,
    /// except one [`Gamedata::tile_for_texture`] takes, which is the server's.
    pub fn server_tile(&self, id: TileId, atlas: &str) -> TileId {
        self.moved(id, self.module(atlas))
    }

    /// [`Gamedata::server_tile`] for a module named by the server's texture index.
    pub fn server_tile_for_texture(&self, id: TileId, tex: u32) -> TileId {
        self.moved(id, self.module_for_texture(tex))
    }

    fn moved(&self, id: TileId, m: Option<&TileModule>) -> TileId {
        match m {
            Some(m) if id != 0 => (id as i64 + m.offset) as TileId,
            _ => id,
        }
    }

    /// Where a tile is drawn from, as the TypeScript client's `tile(id, layer)` answers it:
    /// `layer` is an atlas name, a texture index in decimal (the id is then the server's),
    /// or none (the module is the one the id falls in).
    pub fn tile(&self, id: TileId, layer: Option<&str>) -> Option<TileRect> {
        match layer {
            Some(l) if !l.is_empty() && l.bytes().all(|b| b.is_ascii_digit()) => {
                self.tile_for_texture(id, l.parse().ok()?)
            }
            Some(l) if atlas_index(l).is_some() => self.tile_in(id, l),
            _ => self.rect(id as i64, self.module_for(id)?),
        }
    }

    /// A tile of the module of this atlas, by the client's id.
    pub fn tile_in(&self, id: TileId, atlas: &str) -> Option<TileRect> {
        self.rect(id as i64, self.module(atlas)?)
    }

    /// A tile by the server's id and texture index.
    pub fn tile_for_texture(&self, id: TileId, tex: u32) -> Option<TileRect> {
        let m = self.module_for_texture(tex)?;
        self.rect(id as i64 + m.offset, m)
    }

    /// The raw `tile_info` entry of a tile, by the client's id.
    pub fn tile_info(&self, id: TileId) -> Option<TileInfoRaw> {
        self.module_for(id)?.tile_info(id as i64)
    }

    fn rect(&self, id: i64, m: &TileModule) -> Option<TileRect> {
        let info = m.tile_info(id)?;
        Some(TileRect {
            atlas: m.atlas.to_string(),
            sx: info.sx.max(0) as u32,
            sy: info.sy.max(0) as u32,
            w: (info.ex - info.sx).max(0) as u32,
            h: (info.ey - info.sy).max(0) as u32,
            // 32 / 2 - w / 2: a tile of odd width sits half a texel left, rounded down here
            ox: info.ox + (32 - info.w).div_euclid(2),
            oy: info.oy + (32 - info.h),
            cell: 32,
        })
    }

    /// How many variants the tile has (1 where the module says none).
    pub fn tile_count(&self, id: TileId) -> u32 {
        self.module_for(id)
            .map(|m| m.tile_count(id as i64))
            .unwrap_or(1)
    }

    /// The first variant of the tile's group; the tile itself where the module says none.
    pub fn basetile(&self, id: TileId) -> TileId {
        self.module_for(id)
            .and_then(|m| m.basetile(id as i64))
            .and_then(|b| TileId::try_from(b).ok())
            .unwrap_or(id)
    }

    pub fn atlas_names(&self) -> &'static [&'static str] {
        &ATLASES
    }

    pub fn sprite_atlases(&self) -> &'static [&'static str] {
        &SPRITE_ATLASES
    }

    /// The png of an atlas: `<name>.png` under the version's directory.
    pub fn atlas_file(name: &str) -> String {
        format!("{name}.png")
    }

    /// How a status icon lays out in the shifting row: -1 not drawn, 0 drawn at its fixed
    /// place without shifting the row, n drawn at the row's shift and the row moves by n
    /// texels. The icon's own width when the server does not publish `status-icon-sizes.js`,
    /// which is what older clients did.
    pub fn status_icon_size(&self, id: TileId) -> i32 {
        if let Some(s) = &self.icon_sizes {
            let i = id as i64 - s.first;
            let n = if i >= 0 && (i as usize) < s.table.len() {
                s.table[i as usize]
            } else {
                s.default
            };
            if let Some(n) = n.filter(|n| !n.is_nan()) {
                return n as i32;
            }
        }
        self.tile(id, None).map(|r| r.w as i32).unwrap_or(0)
    }

    /// Whether the server published `status-icon-sizes.js`.
    pub fn has_status_icon_sizes(&self) -> bool {
        self.icon_sizes.is_some()
    }

    /// A foreground word (a monster, an item, the player: main or player tiles), decoded as
    /// `prepare_fg_flags` does; its `value` is the client's id. A word is a number, or
    /// `[lo, hi]` for more than 32 bits.
    pub fn fg(&self, word: &serde_json::Value) -> FlagWord {
        self.fg_flags
            .decode(&flags::words_of(word))
            .moved(self.modules[3].offset)
    }

    /// A background word (dungeon tiles), decoded as `prepare_bg_flags` does; its `value` is
    /// the client's id.
    pub fn bg(&self, word: &serde_json::Value) -> FlagWord {
        self.bg_flags
            .decode(&flags::words_of(word))
            .moved(self.modules[0].offset)
    }

    /// [`Gamedata::fg`] of a word given as its two 32-bit halves.
    pub fn fg_bits(&self, lo: u32, hi: u32) -> FlagWord {
        self.fg_flags
            .decode(&[lo as f64, hi as f64])
            .moved(self.modules[3].offset)
    }

    /// [`Gamedata::bg`] of a word given as its two 32-bit halves.
    pub fn bg_bits(&self, lo: u32, hi: u32) -> FlagWord {
        self.bg_flags
            .decode(&[lo as f64, hi as f64])
            .moved(self.modules[0].offset)
    }

    /// Every foreground flag name, as [`FlagWord::is_set`] indexes them.
    pub fn fg_flag_names(&self) -> &[String] {
        self.fg_flags.names()
    }

    /// Every background flag name, as [`FlagWord::is_set`] indexes them.
    pub fn bg_flag_names(&self) -> &[String] {
        self.bg_flags.names()
    }
}

/// Steps one script may take while it loads (the largest, tileinfo-player.js, takes about
/// 100 000), and one call of a function it made.
const MODULE_BUDGET: u64 = 50_000_000;
const CALL_BUDGET: u64 = 1_000_000;
/// More tiles than any atlas has; a module claiming more is broken.
const MAX_MODULE_TILES: i64 = 1_000_000;

static HTML: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^\s*(<!|<html|<\?xml)").unwrap());
static MAX_KEY: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(FLOOR|WALL|FEAT|MAIN|PLAYER|GUI|ICONS)_MAX$").unwrap());

/// Evaluate one gamedata module. A shell whose gamedata proxy is missing answers with its
/// own app shell, a 200 whose body is HTML: say so instead of a syntax error.
fn eval_script(interp: &mut Interp, src: &str, dep: Value) -> Result<Value, String> {
    if HTML.is_match(src) {
        return Err(
            "expected JavaScript, got an HTML page (is the gamedata proxy serving this path?)"
                .into(),
        );
    }
    let script = js::parse(src)?;
    interp.run_module(&script, dep, MODULE_BUDGET)
}

/// Load enums, tileinfo modules and status icon sizes for one server version.
pub fn load_gamedata(
    source: impl GamedataSource,
    version: &str,
) -> Result<Gamedata, GamedataError> {
    load_gamedata_at(source, "", version)
}

/// [`load_gamedata`] from files already fetched, by name.
pub fn load_gamedata_from_map(
    files: &HashMap<String, String>,
    version: &str,
) -> Result<Gamedata, GamedataError> {
    load_gamedata(
        |name: &str| {
            files
                .get(name)
                .cloned()
                .ok_or_else(|| format!("{name} was not fetched"))
        },
        version,
    )
}

/// [`load_gamedata`], its errors naming each file under [`gamedata_root`]`(base, version)`.
pub fn load_gamedata_at(
    mut source: impl GamedataSource,
    base: &str,
    version: &str,
) -> Result<Gamedata, GamedataError> {
    let path = |file: &str| {
        if base.is_empty() {
            format!("{version}/{file}")
        } else {
            format!("{}/{file}", gamedata_root(base, version))
        }
    };
    let fail = |file: &str, message: String| GamedataError {
        file: file.to_string(),
        path: path(file),
        message,
    };
    let mut texts = Vec::with_capacity(SCRIPTS.len());
    for f in SCRIPTS {
        texts.push(source.fetch_text(f).map_err(|e| fail(f, e))?);
    }
    let mut diags: Vec<String> = Vec::new();
    let mut interp = Interp::new();

    let enums_value =
        eval_script(&mut interp, &texts[0], Value::Undef).map_err(|e| fail(SCRIPTS[0], e))?;
    let enums_obj = enums_value
        .as_obj()
        .ok_or_else(|| fail(SCRIPTS[0], "the module is not an object".into()))?;
    let enums = Enums {
        values: obj_to_json(enums_obj, 0),
    };
    let flag_spec =
        |fn_name: &str, fallback: &str, interp: &mut Interp, diags: &mut Vec<String>| {
            let f = enums_value
                .prop(fn_name)
                .ok_or_else(|| fail(SCRIPTS[0], format!("enums.js has no {fn_name}")))?;
            // prepare_fg_flags(tileidx) calls prepare_flags(tileidx, fg_flags, fg_cache): the table
            // is the second argument, read in the scope the function closes over
            let table = interp
                .find_call_arg(&f, 1)
                .ok_or_else(|| fail(SCRIPTS[0], format!("{fn_name} passes on no flag table")))?;
            FlagSpec::from_value(&table, fallback, diags).map_err(|e| fail(SCRIPTS[0], e))
        };
    let fg_flags = flag_spec("prepare_fg_flags", "fg_flags", &mut interp, &mut diags)?;
    let bg_flags = flag_spec("prepare_bg_flags", "bg_flags", &mut interp, &mut diags)?;

    let mut modules: Vec<TileModule> = Vec::with_capacity(ATLASES.len());
    let mut prev = Value::Undef;
    let mut fp = String::new();
    for (i, name) in ATLASES.iter().enumerate() {
        let file = SCRIPTS[i + 1];
        let exports =
            eval_script(&mut interp, &texts[i + 1], prev.clone()).map_err(|e| fail(file, e))?;
        let prev_end = modules.last().map(|m| m.end);
        let m = build_module(
            &mut interp,
            name_static(name),
            file,
            &exports,
            prev_end,
            &mut diags,
        )
        .map_err(|e| fail(file, e))?;
        fp.push_str(&format!("{name}:{};", m.end));
        modules.push(m);
        prev = exports;
    }

    let ranges = {
        let m = |i: usize| &modules[i];
        Ranges {
            floor_max: m(0).end,
            wall_max: m(1).end,
            feat_max: m(2).end,
            main_max: m(3).end,
            player_max: m(4).end,
            gui_max: m(5).end,
            icons_max: m(6).end,
            first_transparent: m(1).export("DNGN_FIRST_TRANSPARENT").unwrap_or(m(1).end),
            mcache_start: m(4).export("MCACHE_START").unwrap_or(m(4).end),
        }
    };
    let texture_order = texture_order(&enums);
    let icon_sizes = load_icon_sizes(&mut source, &mut interp, &modules[6], &mut diags);
    let names =
        |idx: &[usize]| NameIndex::new(&idx.iter().map(|&i| &modules[i]).collect::<Vec<_>>());
    Ok(Gamedata {
        version: version.to_string(),
        fingerprint: fnv1a(&fp),
        enums,
        dngn: names(&[0, 1, 2]),
        main: names(&[3]),
        player: names(&[4]),
        gui: names(&[5]),
        icons: names(&[6]),
        ranges,
        texture_order,
        icon_sizes,
        fg_flags,
        bg_flags,
        diagnostics: diags,
        modules,
    })
}

fn name_static(name: &str) -> &'static str {
    ATLASES
        .iter()
        .find(|a| **a == name)
        .copied()
        .unwrap_or("main")
}

/// `status-icon-sizes.js`, its function asked once for every icon. Any problem is a
/// diagnostic: status icons then shift by their own width.
fn load_icon_sizes(
    source: &mut impl GamedataSource,
    interp: &mut Interp,
    icons: &TileModule,
    diags: &mut Vec<String>,
) -> Option<IconSizes> {
    let fallback = "status icons shift by their own width";
    let src = match source.fetch_text(STATUS_ICON_SIZES) {
        Ok(s) => s,
        Err(e) => {
            diags.push(format!(
                "{STATUS_ICON_SIZES} not available; {fallback} ({e})"
            ));
            return None;
        }
    };
    // it reads the icons by the client's ids
    let dep = Value::new_obj();
    if let Value::Obj(o) = &dep {
        let mut o = o.borrow_mut();
        for (k, v) in &icons.exports {
            o.set(k.as_str().into(), Value::Num(*v as f64));
        }
    }
    let module = match eval_script(interp, &src, dep) {
        Ok(m) => m,
        Err(e) => {
            diags.push(format!(
                "{STATUS_ICON_SIZES} not available; {fallback} ({e})"
            ));
            return None;
        }
    };
    let Some(f) = module.prop("status_icon_size").filter(Value::is_callable) else {
        diags.push(format!(
            "{STATUS_ICON_SIZES} has no status_icon_size; {fallback}"
        ));
        return None;
    };
    let mut ask = |id: f64| -> Option<f64> {
        match interp.call_fn(&f, vec![Value::Num(id)], CALL_BUDGET) {
            Ok(Value::Num(n)) => Some(n),
            _ => None,
        }
    };
    let table = (icons.first..icons.end).map(|id| ask(id as f64)).collect();
    let default = ask(f64::NAN);
    Some(IconSizes {
        first: icons.first,
        table,
        default,
    })
}

/// A module's first tile id, as the server numbers it: its lowest named id, the end when it
/// names none.
fn first_tile_id(numbers: &[(String, f64)], end: f64) -> f64 {
    numbers
        .iter()
        .filter(|(k, _)| !k.ends_with("_MAX"))
        .map(|(_, v)| *v)
        .fold(end, |first, v| if v < first { v } else { first })
}

fn whole(n: f64, what: &str) -> Result<i64, String> {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        Ok(n as i64)
    } else {
        Err(format!(
            "{what} is not a whole number ({})",
            js::num_to_string(n)
        ))
    }
}

fn build_module(
    interp: &mut Interp,
    atlas: &'static str,
    file: &str,
    exports: &Value,
    prev_end: Option<i64>,
    diags: &mut Vec<String>,
) -> Result<TileModule, String> {
    let obj = exports.as_obj().ok_or("the module is not an object")?;
    let (numbers, max_key): (Vec<(String, f64)>, Option<String>) = {
        let o = obj.borrow();
        let numbers = o
            .entries()
            .iter()
            .filter_map(|(k, v)| v.as_num().map(|n| (k.to_string(), n)))
            .collect();
        let max_key = o
            .entries()
            .iter()
            .map(|(k, _)| k)
            .find(|k| MAX_KEY.is_match(k))
            .map(|k| k.to_string());
        (numbers, max_key)
    };
    let end = match &max_key {
        Some(k) => exports
            .prop(k)
            .map(|v| to_number(&v))
            .filter(|n| *n != 0.0 && !n.is_nan())
            .unwrap_or(0.0),
        None => {
            diags.push(format!(
                "{file}: no {}_MAX; the module has no tiles",
                atlas.to_uppercase()
            ));
            0.0
        }
    };
    let first = whole(first_tile_id(&numbers, end), "the first tile id")?;
    let end = whole(end, "the module's end")?;
    if end - first > MAX_MODULE_TILES {
        return Err(format!("{} tiles is more than an atlas holds", end - first));
    }
    let offset = prev_end.unwrap_or(0) - first;

    let mut moved = Vec::with_capacity(numbers.len());
    let mut skipped = 0;
    for (k, v) in numbers {
        match whole(v, &k) {
            Ok(v) => moved.push((k, v + offset)),
            Err(_) => skipped += 1,
        }
    }
    if skipped > 0 {
        diags.push(format!(
            "{file}: {skipped} exported values are not tile ids; left out of the names"
        ));
    }
    let export_index = moved
        .iter()
        .enumerate()
        .map(|(i, (k, _))| (k.clone(), i))
        .collect();

    let function = |name: &str, diags: &mut Vec<String>| {
        let f = exports.prop(name).filter(Value::is_callable);
        if f.is_none() {
            diags.push(format!("{file}: no {name} function"));
        }
        f
    };
    let get_tile_info = function("get_tile_info", diags);
    let tile_count = function("tile_count", diags);
    let basetile = function("basetile", diags);

    let n = (end - first).max(0) as usize;
    let mut infos = Vec::with_capacity(n);
    let mut counts = Vec::with_capacity(n);
    let mut bases = Vec::with_capacity(n);
    let mut failures: [(usize, Option<String>); 3] = Default::default();
    let mut note = |which: usize, e: String| {
        failures[which].0 += 1;
        failures[which].1.get_or_insert(e);
    };
    for sid in first..end {
        let arg = || vec![Value::Num(sid as f64)];
        let info = match &get_tile_info {
            Some(f) => match interp.call_fn(f, arg(), CALL_BUDGET) {
                Ok(v) => tile_info_of(&v),
                Err(e) => {
                    note(0, e);
                    None
                }
            },
            None => None,
        };
        let count = match &tile_count {
            Some(f) => match interp.call_fn(f, arg(), CALL_BUDGET) {
                Ok(v) => {
                    // tile_count(id) || 1
                    let n = to_number(&v);
                    if n != 0.0 && !n.is_nan() && n > 0.0 {
                        n.min(u32::MAX as f64) as u32
                    } else {
                        1
                    }
                }
                Err(e) => {
                    note(1, e);
                    1
                }
            },
            None => 1,
        };
        let base = match &basetile {
            Some(f) => match interp.call_fn(f, arg(), CALL_BUDGET) {
                Ok(v) => whole(to_number(&v), "").ok().map(|b| b + offset),
                Err(e) => {
                    note(2, e);
                    None
                }
            },
            None => None,
        };
        infos.push(info);
        counts.push(count);
        bases.push(base);
    }
    for (which, (count, first_error)) in ["get_tile_info", "tile_count", "basetile"]
        .iter()
        .zip(failures)
    {
        if count > 0 {
            diags.push(format!(
                "{file}: {which} failed for {count} of {n} tiles (first: {})",
                first_error.unwrap_or_default()
            ));
        }
    }
    Ok(TileModule {
        atlas,
        first: first + offset,
        end: end + offset,
        offset,
        exports: moved,
        export_index,
        infos,
        counts,
        bases,
    })
}

fn tile_info_of(v: &Value) -> Option<TileInfoRaw> {
    let o = v.as_obj()?.borrow();
    let field = |k: &str| -> Option<i32> {
        let n = to_number(o.get(k)?);
        (n.is_finite() && n.fract() == 0.0 && n.abs() < 1e9).then_some(n as i32)
    };
    Some(TileInfoRaw {
        w: field("w")?,
        h: field("h")?,
        ox: field("ox")?,
        oy: field("oy")?,
        sx: field("sx")?,
        sy: field("sy")?,
        ex: field("ex")?,
        ey: field("ey")?,
    })
}

/// The module of each texture index from `enums.texture`; the tiles.h order when the server
/// publishes none.
fn texture_order(enums: &Enums) -> Vec<Option<usize>> {
    let atlas_of = |k: &str| match k {
        "FLOOR" => Some(0),
        "WALL" => Some(1),
        "FEAT" => Some(2),
        "DEFAULT" | "MAIN" => Some(3),
        "PLAYER" => Some(4),
        "GUI" => Some(5),
        "ICONS" => Some(6),
        _ => None,
    };
    let mut out: Vec<Option<usize>> = Vec::new();
    if let Some(tex) = enums.table("texture") {
        for (k, v) in tex {
            if let (Some(a), Some(i)) = (atlas_of(k), v.as_u64()) {
                if i < 64 {
                    let i = i as usize;
                    if out.len() <= i {
                        out.resize(i + 1, None);
                    }
                    out[i] = Some(a);
                }
            }
        }
    }
    if out.is_empty() {
        // floor, wall, feat, player, main, gui, icons
        return [0, 1, 2, 4, 3, 5, 6].into_iter().map(Some).collect();
    }
    out
}

fn fnv1a(text: &str) -> String {
    let mut h: u32 = 0x811c9dc5;
    for c in text.encode_utf16() {
        h ^= c as u32;
        h = h.wrapping_mul(0x01000193);
    }
    format!("{h:08x}")
}

fn to_json(v: &Value, depth: u32) -> Option<serde_json::Value> {
    use serde_json::Value as J;
    Some(match v {
        Value::Null => J::Null,
        Value::Bool(b) => J::Bool(*b),
        Value::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 9e15 {
                J::from(*n as i64)
            } else {
                J::from(serde_json::Number::from_f64(*n)?)
            }
        }
        Value::Str(s) => J::String(s.to_string()),
        Value::Arr(a) if depth < 4 => J::Array(
            a.borrow()
                .iter()
                .map(|x| to_json(x, depth + 1).unwrap_or(J::Null))
                .collect(),
        ),
        Value::Obj(o) if depth < 4 => J::Object(obj_to_json(o, depth + 1)),
        _ => return None,
    })
}

fn obj_to_json(o: &js::ObjRef, depth: u32) -> serde_json::Map<String, serde_json::Value> {
    o.borrow()
        .entries()
        .iter()
        .filter_map(|(k, v)| Some((k.to_string(), to_json(v, depth)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_fnv1a_as_the_typescript_client_computes_it() {
        assert_eq!(fnv1a(""), "811c9dc5");
        assert_eq!(fnv1a("a"), "e40c292c");
    }

    #[test]
    fn lists_the_files_scripts_before_atlases() {
        let urls = gamedata_urls("/gamedata-proxy/crawl.dcss.io/", "abc");
        let root = "/gamedata-proxy/crawl.dcss.io/gamedata/abc/";
        assert!(urls.iter().all(|u| u.starts_with(root)));
        assert_eq!(urls[0], format!("{root}enums.js"));
        let tail: Vec<String> = ATLASES.iter().map(|a| format!("{root}{a}.png")).collect();
        assert_eq!(urls[urls.len() - ATLASES.len()..], tail[..]);
        assert!(urls.contains(&format!("{root}status-icon-sizes.js")));
    }
}
