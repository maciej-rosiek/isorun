//! The game as the server has described it so far: [`GameState`], built by [`reduce`] from the
//! stream of server messages. Tolerant: a malformed message or field is noted in
//! `diagnostics` and skipped, never a panic.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use regex::Regex;
use serde_json::Value;

use crate::floor::{items_underfoot, note_floor_items, FloorItems};
use crate::links::{parse_game_links, GameLink};
use crate::protocol::MouseMode;
use crate::wire::{impl_wire, int, js_number, js_string, num, opt_string, str_or, string, truthy, truthy_opt, wire_fields, Assign, Obj, Wire};

// ---------------------------------------------------------------------------
// Map

/// A packed tile word from the server: a number, or `[lo, hi]` for one over 32 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct TileWord {
    pub lo: u32,
    pub hi: u32,
}

impl TileWord {
    pub fn new(lo: u32, hi: u32) -> Self {
        TileWord { lo, hi }
    }

    /// The whole word.
    pub fn value(self) -> u64 {
        (u64::from(self.hi) << 32) | u64::from(self.lo)
    }
}

impl Wire for TileWord {
    fn from_wire(v: &Value) -> Option<Self> {
        match v {
            Value::Number(_) => {
                let n = int(Some(v))?;
                Some(TileWord { lo: n as u32, hi: if (0..1 << 32).contains(&n) { 0 } else { (n >> 32) as u32 } })
            }
            Value::Array(a) => {
                let lo = a.first().and_then(|e| int(Some(e)))?;
                let hi = a.get(1).map_or(Some(0), |e| int(Some(e)))?;
                Some(TileWord { lo: lo as u32, hi: hi as u32 })
            }
            _ => None,
        }
    }
    fn to_wire(&self) -> Value {
        if self.hi == 0 {
            Value::from(self.lo)
        } else {
            Value::from(vec![self.lo, self.hi])
        }
    }
}

/// A cell's flavour: the floor and the special tile variants.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Flv {
    pub f: Option<i64>,
    pub s: Option<i64>,
}
wire_fields!(Flv closed { f: "f", s: "s" });
impl_wire!(Flv);

/// A cell's `t`: its tiles, merged key by key across updates.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CellTiles {
    pub fg: Option<TileWord>,
    pub bg: Option<TileWord>,
    pub cloud: Option<TileWord>,
    pub base: Option<i64>,
    pub overlay1: Option<i64>,
    pub overlay2: Option<i64>,
    /// The player doll's layers, `[tile, ymax]`.
    pub doll: Option<Vec<[i64; 2]>>,
    /// A monster's composed layers, `[tile, ox, oy]`.
    pub mcache: Option<Vec<[i64; 3]>>,
    pub trans: Option<bool>,
    pub flv: Option<Flv>,
    pub ov: Option<Vec<i64>>,
    pub icons: Option<Vec<i64>>,
    pub bloody: Option<bool>,
    pub old_blood: Option<bool>,
    pub blood_rotation: Option<i64>,
    pub moldy: Option<bool>,
    pub glowing_mold: Option<bool>,
    pub silenced: Option<bool>,
    pub halo: Option<i64>,
    pub sanctuary: Option<bool>,
    pub blasphemy: Option<bool>,
    pub liquefied: Option<bool>,
    pub orb_glow: Option<i64>,
    pub quad_glow: Option<bool>,
    pub disjunct: Option<i64>,
    pub mangrove_water: Option<bool>,
    pub awakened_forest: Option<bool>,
    pub travel_trail: Option<i64>,
    pub highlighted_summoner: Option<bool>,
    pub has_bfb_corpse: Option<bool>,
    pub extra: Obj,
}
wire_fields!(CellTiles {
    fg: "fg",
    bg: "bg",
    cloud: "cloud",
    base: "base",
    overlay1: "overlay1",
    overlay2: "overlay2",
    doll: "doll",
    mcache: "mcache",
    trans: "trans",
    flv: "flv",
    ov: "ov",
    icons: "icons",
    bloody: "bloody",
    old_blood: "old_blood",
    blood_rotation: "blood_rotation",
    moldy: "moldy",
    glowing_mold: "glowing_mold",
    silenced: "silenced",
    halo: "halo",
    sanctuary: "sanctuary",
    blasphemy: "blasphemy",
    liquefied: "liquefied",
    orb_glow: "orb_glow",
    quad_glow: "quad_glow",
    disjunct: "disjunct",
    mangrove_water: "mangrove_water",
    awakened_forest: "awakened_forest",
    travel_trail: "travel_trail",
    highlighted_summoner: "highlighted_summoner",
    has_bfb_corpse: "has_bfb_corpse",
});
impl_wire!(CellTiles);

/// A monster type's shared data.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TypeData {
    pub avghp: Option<i64>,
    pub no_exp: Option<bool>,
}
wire_fields!(TypeData closed { avghp: "avghp", no_exp: "no_exp" });
impl_wire!(TypeData);

/// A monster as the server describes it, merged across updates (`mergeMonster`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Monster {
    pub id: Option<i64>,
    pub name: String,
    pub plural: Option<String>,
    /// `type` on the wire.
    pub kind: Option<i64>,
    pub typedata: Option<TypeData>,
    pub att: i64,
    pub btype: Option<i64>,
    pub threat: Option<i64>,
    pub clientid: Option<i64>,
    /// How many cells hold this monster (map.js keeps the count on the object).
    pub refs: i64,
    pub extra: Obj,
}
wire_fields!(Monster {
    id: "id",
    name: "name",
    plural: "plural",
    kind: "type",
    typedata: "typedata",
    att: "att",
    btype: "btype",
    threat: "threat",
    clientid: "clientid",
    refs: "refs",
});

/// The monster a cell holds: one with an id is shared by every cell that names it, in
/// [`MapStore::monsters`] (map.js `monster_table`), so an update to it is seen from all of them;
/// one without an id is the cell's own.
#[derive(Clone, Debug, PartialEq)]
pub enum CellMonster {
    Shared(i64),
    Own(Box<Monster>),
}

/// One cell of the level, merged key by key across `map` messages.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MapCell {
    pub x: i32,
    pub y: i32,
    /// Dungeon feature id (version specific, informational).
    pub f: Option<i64>,
    /// Map-feature category for the minimap ([`crate::MapFeature`]).
    pub mf: Option<i64>,
    pub g: Option<String>,
    pub col: Option<i64>,
    pub t: Option<CellTiles>,
    /// Read it through [`MapStore::monster`].
    pub mon: Option<CellMonster>,
    pub flc: Option<i64>,
    pub fla: Option<i64>,
    pub extra: Obj,
}
wire_fields!(MapCell { f: "f", mf: "mf", g: "g", col: "col", flc: "flc", fla: "fla" });

/// Coordinates in [-512, 512) pack into one key, as the TS `mapKey` packs them (the same as the
/// scene crate's `cell_key`).
pub fn map_key(x: i32, y: i32) -> u32 {
    (y.wrapping_add(512).wrapping_shl(10) | x.wrapping_add(512)) as u32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

impl Wire for Pos {
    fn from_wire(v: &Value) -> Option<Self> {
        let x = num(v.get("x"))?;
        let y = num(v.get("y"))?;
        Some(Pos { x: x as i32, y: y as i32 })
    }
    fn to_wire(&self) -> Value {
        serde_json::json!({ "x": self.x, "y": self.y })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Bounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MapStore {
    pub cells: HashMap<u32, MapCell>,
    /// The monsters with an id, each shared by the cells that hold it; dropped when none does.
    pub monsters: HashMap<i64, Monster>,
    pub bounds: Option<Bounds>,
    /// The view centre (vgrdc).
    pub view_center: Option<Pos>,
    pub player_on_level: bool,
    pub invisible_monster_desc: String,
}

impl MapStore {
    pub fn cell(&self, x: i32, y: i32) -> Option<&MapCell> {
        self.cells.get(&map_key(x, y))
    }

    /// The monster in a cell, merged: the shared one for an id, else the cell's own.
    pub fn monster<'a>(&'a self, cell: &'a MapCell) -> Option<&'a Monster> {
        match cell.mon.as_ref()? {
            CellMonster::Shared(id) => self.monsters.get(id),
            CellMonster::Own(m) => Some(m),
        }
    }

    pub fn monster_at(&self, x: i32, y: i32) -> Option<&Monster> {
        self.monster(self.cell(x, y)?)
    }
}

// ---------------------------------------------------------------------------
// Player

/// One layer of an item's tile: a plain id in the main texture, or a tile naming its texture.
#[derive(Clone, Debug, PartialEq)]
pub enum TileLayer {
    Id(i64),
    Tex(TileRef),
}

impl Wire for TileLayer {
    fn from_wire(v: &Value) -> Option<Self> {
        if v.is_object() {
            TileRef::from_wire(v).map(TileLayer::Tex)
        } else {
            int(Some(v)).map(TileLayer::Id)
        }
    }
    fn to_wire(&self) -> Value {
        match self {
            TileLayer::Id(i) => Value::from(*i),
            TileLayer::Tex(r) => r.to_wire(),
        }
    }
}

/// tileweb.cc `_send_item`: plain ids in the main texture, base item first; older layers name
/// their own texture.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemTile {
    Id(i64),
    Layers(Vec<TileLayer>),
}

impl Wire for ItemTile {
    fn from_wire(v: &Value) -> Option<Self> {
        match v {
            Value::Array(_) => Vec::from_wire(v).map(ItemTile::Layers),
            _ => int(Some(v)).map(ItemTile::Id),
        }
    }
    fn to_wire(&self) -> Value {
        match self {
            ItemTile::Id(i) => Value::from(*i),
            ItemTile::Layers(l) => l.to_wire(),
        }
    }
}

/// A tile in a texture: `t` the tile, `tex` the texture.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TileRef {
    pub t: i64,
    pub tex: i64,
    pub ymax: Option<f64>,
}
wire_fields!(TileRef closed { t: "t", tex: "tex", ymax: "ymax" });

impl Wire for TileRef {
    fn from_wire(v: &Value) -> Option<Self> {
        let o = v.as_object()?;
        let mut r = TileRef::default();
        // `t` and `tex` are the tile; without them it is not one
        if int(o.get("t")).is_none() || int(o.get("tex")).is_none() {
            return None;
        }
        r.merge(o);
        Some(r)
    }
    fn to_wire(&self) -> Value {
        Value::Object(self.to_object())
    }
}

/// An inventory slot, merged key by key across `player` updates.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct InvItem {
    pub slot: i64,
    pub name: Option<String>,
    pub base_type: Option<i64>,
    pub sub_type: Option<i64>,
    pub quantity: Option<i64>,
    pub col: Option<i64>,
    pub action_panel_order: Option<i64>,
    pub tile: Option<ItemTile>,
    pub extra: Obj,
}
wire_fields!(InvItem {
    slot: "slot",
    name: "name",
    base_type: "base_type",
    sub_type: "sub_type",
    quantity: "quantity",
    col: "col",
    action_panel_order: "action_panel_order",
    tile: "tile",
});

/// A status light or effect.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct StatusEntry {
    pub light: Option<String>,
    pub text: Option<String>,
    pub col: Option<i64>,
    pub desc: Option<String>,
}
wire_fields!(StatusEntry closed { light: "light", text: "text", col: "col", desc: "desc" });
impl_wire!(StatusEntry);

/// The character sheet, merged key by key across `player` updates.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerState {
    pub name: String,
    pub title: String,
    pub species: String,
    pub species_display_name: String,
    pub god: String,
    pub piety_rank: i64,
    pub penance: bool,
    pub ostracism_pips: i64,
    pub hp: i64,
    pub hp_max: i64,
    pub real_hp_max: i64,
    pub poison_survival: i64,
    pub mp: i64,
    pub mp_max: i64,
    pub dd_real_mp_max: i64,
    pub ac: i64,
    pub ev: i64,
    pub sh: i64,
    pub ac_mod: i64,
    pub ev_mod: i64,
    pub sh_mod: i64,
    pub str: i64,
    pub int: i64,
    pub dex: i64,
    pub str_max: i64,
    pub int_max: i64,
    pub dex_max: i64,
    pub xl: i64,
    /// Percent to the next level, 0..=100 (crawl sends it as an int8; see `progress_from_int8`).
    pub progress: i64,
    pub gold: i64,
    pub place: String,
    pub depth: i64,
    pub time: i64,
    pub time_delta: i64,
    pub turn: i64,
    pub time_last_input: Option<i64>,
    pub status: Vec<StatusEntry>,
    /// By slot.
    pub inv: BTreeMap<i64, InvItem>,
    pub weapon_index: i64,
    pub weapon_colour: i64,
    pub offhand_index: i64,
    pub offhand_weapon: bool,
    pub offhand_weapon_colour: i64,
    pub unarmed_attack: String,
    pub quiver_item: i64,
    pub quiver_desc: String,
    pub pos: Pos,
    pub contam: i64,
    pub doom: i64,
    pub doom_desc: String,
    pub noise: i64,
    pub adjusted_noise: i64,
    pub wizard: bool,
    pub explore: bool,
    pub form: i64,
    /// `turn` when `pos` or `place` / `depth` last changed: the turn the player arrived where
    /// they stand.
    pub arrived_turn: i64,
    /// Whether that arrival's `player` update also gained inventory or gold: autopickup took
    /// something, which ends a second turn (main.cc `_input`: `world_reacts` after the move,
    /// `autopickup`, then `world_reacts` again when it picked up), so `item_check`'s line about
    /// what it left was printed one turn before `arrived_turn` (cdi-0.34-autopickup.ndjson).
    pub arrived_pickup: bool,
    /// The server's latest floor-items announcement; see [`items_underfoot`].
    pub floor_items: Option<FloorItems>,
    /// Whether a `player` message has arrived for this game. Until it has, the sheet is this
    /// record's zeros, not the character's: a new game sends none while the chooser is up
    /// (newgame.cc `choose_game` puts the UI in `UI_CRT`; tileweb.cc `redraw` sends the first
    /// `player` in the same batch as the `ui_state` back to normal), and a continued game sends
    /// none until the save has loaded.
    pub received: bool,
    /// The keys the last `player` update changed.
    pub changed: BTreeSet<String>,
    /// The values those keys had before it (`null` for none).
    pub previous: Obj,
    /// `time` at the last update that carried it (client-side, for `time_delta`).
    pub last_time: Option<i64>,
    pub extra: Obj,
}
wire_fields!(PlayerState {
    name: "name",
    title: "title",
    species: "species",
    species_display_name: "species_display_name",
    god: "god",
    piety_rank: "piety_rank",
    penance: "penance",
    ostracism_pips: "ostracism_pips",
    hp: "hp",
    hp_max: "hp_max",
    real_hp_max: "real_hp_max",
    poison_survival: "poison_survival",
    mp: "mp",
    mp_max: "mp_max",
    dd_real_mp_max: "dd_real_mp_max",
    ac: "ac",
    ev: "ev",
    sh: "sh",
    ac_mod: "ac_mod",
    ev_mod: "ev_mod",
    sh_mod: "sh_mod",
    str: "str",
    int: "int",
    dex: "dex",
    str_max: "str_max",
    int_max: "int_max",
    dex_max: "dex_max",
    xl: "xl",
    progress: "progress",
    gold: "gold",
    place: "place",
    depth: "depth",
    time: "time",
    time_delta: "time_delta",
    turn: "turn",
    time_last_input: "time_last_input",
    status: "status",
    weapon_index: "weapon_index",
    weapon_colour: "weapon_colour",
    offhand_index: "offhand_index",
    offhand_weapon: "offhand_weapon",
    offhand_weapon_colour: "offhand_weapon_colour",
    unarmed_attack: "unarmed_attack",
    quiver_item: "quiver_item",
    quiver_desc: "quiver_desc",
    pos: "pos",
    contam: "contam",
    doom: "doom",
    doom_desc: "doom_desc",
    noise: "noise",
    adjusted_noise: "adjusted_noise",
    wizard: "wizard",
    explore: "explore",
    form: "form",
});

impl Default for PlayerState {
    fn default() -> Self {
        PlayerState {
            name: String::new(),
            title: String::new(),
            species: String::new(),
            species_display_name: String::new(),
            god: String::new(),
            piety_rank: 0,
            penance: false,
            ostracism_pips: 0,
            hp: 0,
            hp_max: 0,
            real_hp_max: 0,
            poison_survival: 0,
            mp: 0,
            mp_max: 0,
            dd_real_mp_max: 0,
            ac: 0,
            ev: 0,
            sh: 0,
            ac_mod: 0,
            ev_mod: 0,
            sh_mod: 0,
            str: 0,
            int: 0,
            dex: 0,
            str_max: 0,
            int_max: 0,
            dex_max: 0,
            xl: 0,
            progress: 0,
            gold: 0,
            place: String::new(),
            depth: 0,
            time: 0,
            time_delta: 0,
            turn: 0,
            time_last_input: None,
            status: Vec::new(),
            inv: BTreeMap::new(),
            weapon_index: -1,
            weapon_colour: -1,
            offhand_index: -1,
            offhand_weapon: false,
            offhand_weapon_colour: -1,
            unarmed_attack: String::new(),
            quiver_item: -1,
            quiver_desc: String::new(),
            pos: Pos::default(),
            contam: 0,
            doom: 0,
            doom_desc: String::new(),
            noise: 0,
            adjusted_noise: 0,
            wizard: false,
            explore: false,
            form: 0,
            arrived_turn: 0,
            arrived_pickup: false,
            floor_items: None,
            received: false,
            changed: BTreeSet::new(),
            previous: Obj::new(),
            last_time: None,
            extra: Obj::new(),
        }
    }
}

/// Whether any status's text matches.
pub fn has_status(player: &PlayerState, pattern: &Regex) -> bool {
    player.status.iter().any(|s| s.text.as_deref().is_some_and(|t| !t.is_empty() && pattern.is_match(t)))
}

// ---------------------------------------------------------------------------
// Messages, menus, popups

/// Message channels the client reads, numbered as `mpr.h` `msg_channel_type` declares them
/// (`message.cc` writes the number on each `msgs` line; the official `messages.js` ignores it).
/// Only the ones Isorun acts on are named.
pub struct Msgch;

impl Msgch {
    pub const PLAIN: i64 = 0;
    /// "various prompts": "Cast which spell? (? or * to list)" (spl-cast.cc `cast_a_spell`),
    /// "Where to?" (travel.cc).
    pub const PROMPT: i64 = 2;
    /// "much less serious threats": `i_feel_safe`'s "X is nearby!" (nearby-danger.cc).
    pub const WARN: i64 = 6;
    /// `item_check` (items.cc): "You see here X.", "Items here: ...", "Things that are here:",
    /// "There are many items here."
    pub const FLOOR_ITEMS: i64 = 21;
    /// "Foo comes into view", "You encounter Foo." (player-notices.cc, delay.cc).
    pub const MONSTER_WARNING: i64 = 31;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Marker {
    Turn,
    Command,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct GameMessage {
    pub text: String,
    pub turn: Option<i64>,
    pub channel: Option<i64>,
    /// Set by the client when a new command or turn starts after this line.
    pub marker: Option<Marker>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MessageLog {
    pub lines: Vec<GameMessage>,
    pub more: bool,
    pub more_text: String,
    pub pane_height: i64,
    pub text_cursor: bool,
    /// How many of `lines` the player had on screen the last time the server read a key from
    /// them: crawl's `message_window::input_line` (message.cc `got_input`, called from
    /// `msgwin_reply`, `clear_messages` and every key read). A full `--more--` fires when the
    /// lines after it fill the window (`make_space` scrolls off only the ones before it), so
    /// `lines[read_to..]` is what the more is about; the reducer sets it whenever the server
    /// leaves a mode that was reading a key (any mode but [`MouseMode::NORMAL`]).
    pub read_to: usize,
}

impl Default for MessageLog {
    fn default() -> Self {
        MessageLog { lines: Vec::new(), more: false, more_text: String::new(), pane_height: 7, text_cursor: false, read_to: 0 }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MenuItem {
    pub text: String,
    pub level: Option<i64>,
    pub colour: Option<i64>,
    pub hotkeys: Option<Vec<i64>>,
    pub tiles: Option<Vec<TileRef>>,
    pub q: Option<i64>,
    pub preselected: Option<bool>,
    pub extra: Obj,
}
wire_fields!(MenuItem { text: "text", level: "level", colour: "colour", hotkeys: "hotkeys", tiles: "tiles", q: "q", preselected: "preselected" });
impl_wire!(MenuItem);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MenuTitle {
    pub text: String,
    pub extra: Obj,
}
wire_fields!(MenuTitle { text: "text" });
impl_wire!(MenuTitle);

/// `title_prompt`: the title is a text field (the menu filter); `raw` while the macro editor
/// reads a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitlePrompt {
    pub prompt: String,
    pub raw: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MenuState {
    pub tag: String,
    /// `type` on the wire.
    pub kind: Option<String>,
    pub flags: i64,
    pub title: MenuTitle,
    pub more: String,
    pub alt_more: String,
    pub total_items: i64,
    /// By index; `None` for an item the server has not sent yet.
    pub items: Vec<Option<MenuItem>>,
    pub last_hovered: i64,
    pub chunk_start: i64,
    pub ui_centred: Option<bool>,
    pub jump_to: Option<i64>,
    /// Client-side: the first visible index the server reported (for spectators).
    pub server_first_visible: Option<i64>,
    pub title_prompt: Option<TitlePrompt>,
    /// Client-side: this menu's place in the server's one ui stack (`ui_stack_order`).
    pub stack_order: i64,
    /// Client-side: whether a `ui_cutoff` hid it.
    pub hidden: bool,
    pub extra: Obj,
}
wire_fields!(MenuState {
    tag: "tag",
    kind: "type",
    flags: "flags",
    title: "title",
    more: "more",
    alt_more: "alt_more",
    total_items: "total_items",
    last_hovered: "last_hovered",
    chunk_start: "chunk_start",
    ui_centred: "ui-centred",
    jump_to: "jump_to",
});

/// A popup on the ui stack (`ui-push`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Popup {
    /// `type` on the wire.
    pub kind: String,
    pub generation_id: Option<i64>,
    pub ui_centred: Option<bool>,
    /// The `ui-push` message as sent (title, body, actions, text, ...).
    pub data: Obj,
    /// `ui-state` updates merged in.
    pub state: Obj,
    /// Client-side: this popup's place in the server's one ui stack (`ui_stack_order`).
    pub stack_order: i64,
    /// Client-side: whether a `ui_cutoff` hid it.
    pub hidden: bool,
    /// The last `ui-state-sync` for this popup (spectators follow the player's focus and widget
    /// state).
    pub sync: Option<UiStateSync>,
    /// The last scroll line the server sent for this popup: `ui-scroller-scroll` for most
    /// popups, the `scroll` field of a `ui-state` for a formatted scroller (ui-layouts.js
    /// recv_ui_scroll / formatted_scroller_update). The client applies it unless it came from
    /// its own scrolling (`scroll_from_webtiles`) and it is not spectating; `scroll_seq` counts
    /// arrivals (0: none yet) so a repeat of the same line is applied again.
    pub scroll: Option<i64>,
    pub scroll_from_webtiles: bool,
    pub scroll_seq: u64,
}

/// A `ui-state-sync` message: the player's focus or widget state, for spectators to follow.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct UiStateSync {
    pub generation_id: Option<i64>,
    /// `data-sync-id` of the widget, or `None` for the popup itself.
    pub widget_id: Option<String>,
    pub has_focus: Option<bool>,
    /// True when relayed by the server from the player's own client.
    pub from_webtiles: bool,
    /// Every field as sent (widget state such as `checked`, `text`, `scroll`).
    pub data: Obj,
    /// Bumps on every sync so consumers can react to a repeat of the same state.
    pub seq: u64,
}

/// The most recent `ui-scroller-scroll`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct UiScroll {
    pub scroll: i64,
    pub from_webtiles: bool,
    pub seq: u64,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TextInputState {
    /// `type` on the wire: "messages", "generic" or "seed-selection".
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub prompt: Option<String>,
    pub maxlen: Option<i64>,
    pub size: Option<i64>,
    pub prefill: Option<String>,
    pub history_id: Option<String>,
    pub select_prefill: Option<bool>,
    /// The current text (client-side).
    pub text: String,
    pub extra: Obj,
}
wire_fields!(TextInputState {
    kind: "type",
    tag: "tag",
    prompt: "prompt",
    maxlen: "maxlen",
    size: "size",
    prefill: "prefill",
    history_id: "historyId",
    select_prefill: "select_prefill",
    text: "text",
});

/// The text areas (`txt`): the crt screen's lines, and other areas by id.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CrtScreen {
    pub lines: BTreeMap<i64, String>,
    /// Areas other than "crt"; [`CrtScreen::area`] reads any.
    pub areas: HashMap<String, BTreeMap<i64, String>>,
}

impl CrtScreen {
    /// An area by id; "crt" is `lines` (the TS keeps the one map under both names).
    pub fn area(&self, id: &str) -> Option<&BTreeMap<i64, String>> {
        if id == "crt" {
            Some(&self.lines)
        } else {
            self.areas.get(id)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct LobbyEntry {
    pub id: i64,
    pub username: String,
    pub game_id: String,
    pub spectator_count: Option<i64>,
    pub idle_time: Option<i64>,
    pub xl: Option<String>,
    pub char: Option<String>,
    pub place: Option<String>,
    pub turn: Option<String>,
    pub dur: Option<String>,
    pub god: Option<String>,
    pub title: Option<String>,
    pub milestone: Option<String>,
    pub extra: Obj,
}
wire_fields!(LobbyEntry {
    id: "id",
    username: "username",
    game_id: "game_id",
    spectator_count: "spectator_count",
    idle_time: "idle_time",
    xl: "xl",
    char: "char",
    place: "place",
    turn: "turn",
    dur: "dur",
    god: "god",
    title: "title",
    milestone: "milestone",
});

#[derive(Clone, Debug, PartialEq, Default)]
pub struct LoginCookie {
    pub cookie: String,
    pub expires: i64,
}

/// `rcfile_contents`: the rc file the lobby asked for with `get_rc`, until the editor is closed.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RcFile {
    pub game_id: Option<String>,
    pub contents: String,
}

/// `stale_processes`: the server is about to stop old game processes unless a key is pressed.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct StaleProcesses {
    pub game: String,
    pub timeout: i64,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct LobbyState {
    /// The roster, in the order the server first listed each entry.
    pub entries: Vec<LobbyEntry>,
    pub complete: bool,
    pub game_links_html: String,
    pub games: Vec<GameLink>,
    pub banner_html: String,
    pub username: Option<String>,
    pub admin: bool,
    pub login_failed: Option<String>,
    pub login_cookie: Option<LoginCookie>,
    pub register_failed: Option<String>,
    pub rcfile: Option<RcFile>,
    pub stale_processes: Option<StaleProcesses>,
    /// `force_terminate?`: a stale process would not stop; the server asks whether to kill it.
    pub force_terminate: bool,
    /// `set_account_hold` / `clear_account_hold`: the account awaits administrator approval.
    pub account_hold: bool,
    /// `reload_url`: the server wants the client reloaded (a new client version).
    pub reload_url: bool,
    /// `admin_log` lines, newest last, for the admin panel.
    pub admin_log: Vec<String>,
}

impl LobbyState {
    pub fn entry(&self, id: i64) -> Option<&LobbyEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

/// A chat line, as chat.js keeps it: `html` is the server's rendered line (a `chat_sender` span
/// and a `chat_msg` span), `meta` lines are notices that do not count as unread, `automated`
/// lines are the client's own (a dump url, an announcement).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ChatLine {
    pub sender: String,
    pub html: String,
    pub meta: bool,
    pub automated: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub text: String,
    /// The `msg` of the message it is about.
    pub msg: Option<String>,
    pub detail: Option<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Phase {
    #[default]
    Lobby,
    Loading,
    Playing,
    Watching,
    Ended,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct VersionInfo {
    pub game_id: Option<String>,
    pub gamedata_version: Option<String>,
    pub text: Option<String>,
    pub content: Option<String>,
}

/// `game_ended`: why the last game ended, its parting words and its morgue / dump URL (less the
/// `.txt`); `watched` names the player when it was a watched game.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GameExit {
    pub reason: Option<String>,
    pub message: Option<String>,
    pub dump: Option<String>,
    pub watched: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Spectators {
    pub count: i64,
    pub names: String,
}

/// Monotonic counters: each bumps on every change of its part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Revisions {
    pub map: u64,
    pub player: u64,
    pub messages: u64,
    pub ui: u64,
    pub lobby: u64,
    pub chat: u64,
    pub any: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GameState {
    pub phase: Phase,
    pub lobby: LobbyState,
    pub version: VersionInfo,
    pub map: MapStore,
    pub player: PlayerState,
    pub options: Obj,
    pub messages: MessageLog,
    pub input_mode: i64,
    pub ui_state: i64,
    /// By cursor id; `None` for one that is off.
    pub cursors: Vec<Option<Pos>>,
    pub ui: Vec<Popup>,
    /// The last `ui_cutoff` the server sent; what it hid is on each menu's and popup's `hidden`.
    pub ui_cutoff: i64,
    /// The next `stack_order`: menus and popups share the server's one stack, and the cutoff
    /// counts them together.
    pub ui_stack_order: i64,
    /// The most recent `ui-state-sync`, also attached to its popup as `sync`.
    pub ui_sync: Option<UiStateSync>,
    /// The most recent `ui-scroller-scroll`, also attached to the popup as `scroll`.
    pub ui_scroll: Option<UiScroll>,
    pub menus: Vec<MenuState>,
    pub crt: CrtScreen,
    pub dialog: Option<String>,
    pub text_input: Option<TextInputState>,
    /// Whose game is watched.
    pub watching: Option<String>,
    pub exit: Option<GameExit>,
    pub spectators: Option<Spectators>,
    pub chat: Vec<ChatLine>,
    /// `toggle_chat` / `super_hide_chat`: whether the chat panel is shown at all;
    /// `super_hide_chat` removes even the restore button.
    pub chat_visible: bool,
    pub chat_super_hidden: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub rev: Revisions,
    /// The map keys touched since the last [`GameState::clear_dirty`].
    pub dirty_cells: HashSet<u32>,
    /// The map was cleared since the last [`GameState::clear_dirty`].
    pub map_cleared: bool,
    pub flush_pending: bool,
}

impl Default for GameState {
    fn default() -> Self {
        GameState {
            phase: Phase::Lobby,
            lobby: LobbyState::default(),
            version: VersionInfo::default(),
            map: MapStore::default(),
            player: PlayerState::default(),
            options: Obj::new(),
            messages: MessageLog::default(),
            input_mode: MouseMode::NORMAL,
            ui_state: 0,
            cursors: Vec::new(),
            ui: Vec::new(),
            ui_cutoff: -1,
            ui_stack_order: 0,
            ui_sync: None,
            ui_scroll: None,
            menus: Vec::new(),
            crt: CrtScreen::default(),
            dialog: None,
            text_input: None,
            watching: None,
            exit: None,
            spectators: None,
            chat: Vec::new(),
            chat_visible: true,
            chat_super_hidden: false,
            diagnostics: Vec::new(),
            rev: Revisions::default(),
            dirty_cells: HashSet::new(),
            map_cleared: false,
            flush_pending: false,
        }
    }
}

pub fn initial_state() -> GameState {
    GameState::default()
}

impl GameState {
    /// Forget which cells changed: a scene builder calls this once it has read `dirty_cells`
    /// and `map_cleared`.
    pub fn clear_dirty(&mut self) {
        self.dirty_cells.clear();
        self.map_cleared = false;
    }

    pub fn top_menu(&self) -> Option<&MenuState> {
        self.menus.last()
    }

    pub fn top_popup(&self) -> Option<&Popup> {
        self.ui.last()
    }
}

pub fn top_menu(state: &GameState) -> Option<&MenuState> {
    state.menus.last()
}

pub fn top_popup(state: &GameState) -> Option<&Popup> {
    state.ui.last()
}

// ---------------------------------------------------------------------------
// Reducer

/// The longest list a menu is grown to; a `total_items` past it is noted and cut.
const MAX_MENU_ITEMS: usize = 1 << 20;
/// The highest cursor id kept.
const MAX_CURSOR_ID: i64 = 255;

pub(crate) fn diag(state: &mut GameState, text: impl Into<String>, msg: Option<&Obj>, detail: Option<Value>) {
    let msg = msg.and_then(|m| m.get("msg")).map(js_string);
    state.diagnostics.push(Diagnostic { text: text.into(), msg, detail });
    if state.diagnostics.len() > 200 {
        state.diagnostics.remove(0);
    }
}

/// chat.js receive_message: append, keep the history bounded.
fn push_chat(state: &mut GameState, line: ChatLine) {
    state.chat.push(line);
    if state.chat.len() > 500 {
        state.chat.remove(0);
    }
    state.rev.chat += 1;
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn reset_game(state: &mut GameState) {
    state.map = MapStore::default();
    state.player = PlayerState::default();
    state.options = Obj::new();
    state.messages = MessageLog::default();
    state.input_mode = MouseMode::NORMAL;
    state.ui_state = 0;
    state.cursors.clear();
    state.ui.clear();
    state.ui_cutoff = -1;
    state.ui_stack_order = 0;
    state.ui_sync = None;
    state.ui_scroll = None;
    state.menus.clear();
    state.crt = CrtScreen::default();
    state.dialog = None;
    state.text_input = None;
    state.spectators = None;
    // client.js game_cleanup: chat.clear()
    state.chat.clear();
    state.rev.chat += 1;
    state.dirty_cells.clear();
    state.map_cleared = true;
    state.rev.map += 1;
    state.rev.player += 1;
    state.rev.messages += 1;
    state.rev.ui += 1;
}

/// map.js `merge_monster`: a monster with an id is one object for every cell that names it,
/// counted by `refs`, so an update to it is seen from all of them. The cell's old monster loses
/// a ref; a new id starts from a copy of the old monster (refs and all, as map.js copies them)
/// with the update over it.
fn merge_monster(map: &mut MapStore, old: Option<CellMonster>, mon: Option<&Obj>, bad: &mut Vec<String>) -> Option<CellMonster> {
    let mut old = old;
    match &mut old {
        Some(CellMonster::Shared(id)) => {
            if let Some(m) = map.monsters.get_mut(id) {
                if m.refs != 0 {
                    m.refs -= 1;
                }
            }
        }
        Some(CellMonster::Own(m)) if m.refs != 0 => m.refs -= 1,
        _ => {}
    }
    let mon = mon?;
    // `id: null` is no id
    let id = int(mon.get("id"));
    if let Some(last) = id.and_then(|id| map.monsters.get_mut(&id)) {
        bad.extend(last.merge(mon));
        last.refs += 1;
        return id.map(CellMonster::Shared);
    }
    let mut last = match &old {
        Some(CellMonster::Shared(oid)) => map.monsters.get(oid).cloned().unwrap_or_default(),
        Some(CellMonster::Own(m)) => (**m).clone(),
        None => Monster::default(),
    };
    bad.extend(last.merge(mon));
    match id {
        Some(id) => {
            last.refs += 1;
            map.monsters.insert(id, last);
            Some(CellMonster::Shared(id))
        }
        None => Some(CellMonster::Own(Box::new(last))),
    }
}

/// map.js `merge_cells`: a cell without `x` is the one after the last, without `y` on the last
/// one's row; each field replaces the cell's, `t` key by key, `mon` through [`merge_monster`].
fn merge_cells(state: &mut GameState, cells: &[Value]) {
    let mut last_x: i32 = 0;
    let mut last_y: i32 = 0;
    for val in cells {
        let Some(o) = val.as_object() else {
            diag(state, "map cell is not an object; skipped", None, Some(val.clone()));
            continue;
        };
        let x = match o.get("x") {
            None => Some(last_x.wrapping_add(1)),
            Some(v) => num(Some(v)).map(|f| f as i32),
        };
        let y = match o.get("y") {
            None => Some(last_y),
            Some(v) => num(Some(v)).map(|f| f as i32),
        };
        let (Some(x), Some(y)) = (x, y) else {
            diag(state, "map cell has a bad position; skipped", None, Some(val.clone()));
            continue;
        };
        last_x = x;
        last_y = y;
        let key = map_key(x, y);
        let mut problems: Vec<&'static str> = Vec::new();
        let mut bad_keys: Vec<String> = Vec::new();
        let mut entry = state.map.cells.remove(&key).unwrap_or_else(|| MapCell { x, y, ..MapCell::default() });
        for (prop, v) in o {
            match prop.as_str() {
                "x" | "y" => {}
                "mon" => {
                    if !v.is_null() && !v.is_object() {
                        problems.push("map cell mon is not an object; skipped");
                        continue;
                    }
                    let old = entry.mon.take();
                    entry.mon = merge_monster(&mut state.map, old, v.as_object(), &mut bad_keys);
                }
                "t" => {
                    let Some(t) = v.as_object() else {
                        // the official client would throw here; keep what we know and say so
                        problems.push("map cell t is not an object; skipped");
                        continue;
                    };
                    let tiles = entry.t.get_or_insert_with(CellTiles::default);
                    bad_keys.extend(tiles.merge(t));
                    if truthy_opt(t.get("doll")) && !t.contains_key("trans") {
                        tiles.trans = Some(false);
                    }
                }
                _ => {
                    if entry.assign(prop, v) == Assign::BadType {
                        bad_keys.push(prop.clone());
                    }
                }
            }
        }
        state.map.cells.insert(key, entry);
        for p in problems {
            diag(state, p, None, Some(val.clone()));
        }
        if !bad_keys.is_empty() {
            diag(state, format!("map cell fields have the wrong type; kept the old values: {}", bad_keys.join(", ")), None, Some(val.clone()));
        }
        state.dirty_cells.insert(key);
        match &mut state.map.bounds {
            Some(b) => {
                b.left = b.left.min(x);
                b.right = b.right.max(x);
                b.top = b.top.min(y);
                b.bottom = b.bottom.max(y);
            }
            None => state.map.bounds = Some(Bounds { left: x, top: y, right: x, bottom: y }),
        }
    }
    state.map.monsters.retain(|_, m| m.refs != 0);
}

const INT8_RANGE: f64 = 256.0;

/// Experience progress: crawl sends it as an int8, so past 127 it arrives negative.
fn progress_from_int8(sent: &Value) -> i64 {
    let Some(sent) = sent.as_f64() else { return 0 };
    let unwrapped = if sent < 0.0 { sent + INT8_RANGE } else { sent };
    unwrapped.clamp(0.0, 100.0) as i64
}

/// JavaScript `Number(key)` for an object key: a finite number, as an integer.
fn key_number(k: &str) -> Option<i64> {
    let f = js_number(&Value::String(k.to_owned()));
    f.is_finite().then_some(f as i64)
}

fn apply_player(state: &mut GameState, data: &Obj) {
    {
        let p = &mut state.player;
        p.received = true;
        p.changed.clear();
        p.previous.clear();
    }
    // items taken from the floor: slots whose quantity grew (`_send_item`, tileweb.cc, sends it
    // when it changes) and gold gained
    let mut gained = 0;
    match data.get("inv") {
        Some(Value::Object(inv)) => {
            for (i, v) in inv {
                let (Some(slot), Some(vo)) = (key_number(i), v.as_object()) else {
                    diag(state, "player inv entry is malformed; skipped", Some(data), Some(serde_json::json!({ "slot": i, "value": v })));
                    continue;
                };
                let item = state.player.inv.entry(slot).or_insert_with(|| InvItem { slot, ..InvItem::default() });
                if num(vo.get("quantity")).is_some_and(|q| q > item.quantity.unwrap_or(0) as f64) {
                    gained += 1;
                }
                let bad = item.merge(vo);
                item.slot = slot;
                if !bad.is_empty() {
                    diag(state, format!("player inv fields have the wrong type: {}", bad.join(", ")), Some(data), Some(v.clone()));
                }
            }
            state.player.changed.insert("inv".into());
        }
        Some(_) => diag(state, "player inv is not an object; skipped", Some(data), None),
        None => {}
    }
    if num(data.get("gold")).is_some_and(|g| g > state.player.gold as f64) {
        gained += 1;
    }
    let mut arrived = false;
    for (k, v) in data {
        if k == "msg" || k == "inv" {
            continue;
        }
        let nv = if k == "progress" { Value::from(progress_from_int8(v)) } else { v.clone() };
        let p = &mut state.player;
        if k == "pos" {
            let Some(np) = Pos::from_wire(&nv) else {
                diag(state, "player pos is malformed; kept the previous position", Some(data), Some(nv));
                continue;
            };
            if p.pos != np {
                p.changed.insert("pos".into());
                p.previous.insert("pos".into(), p.pos.to_wire());
                arrived = true;
            }
            p.pos = np;
            continue;
        }
        if k == "status" {
            p.changed.insert("status".into());
            p.previous.insert("status".into(), p.status.to_wire());
            p.status = match &nv {
                Value::Array(a) => a.iter().filter_map(StatusEntry::from_wire).collect(),
                _ => Vec::new(),
            };
            if truthy(&nv) && !nv.is_array() {
                diag(state, "player status is not a list; cleared", Some(data), Some(nv));
            }
            continue;
        }
        match p.assign(k, &nv) {
            Assign::Same | Assign::Unknown => {}
            Assign::Changed(old) => {
                // a new level under the same coordinates is a new cell too
                if k == "place" || k == "depth" {
                    arrived = true;
                }
                p.changed.insert(k.clone());
                p.previous.insert(k.clone(), old);
            }
            Assign::BadType => diag(state, format!("player {k} has the wrong type; kept the old value"), Some(data), Some(nv)),
        }
    }
    // after the loop, so `turn` in the same update counts whatever order the keys came in
    if arrived {
        state.player.arrived_turn = state.player.turn;
        state.player.arrived_pickup = gained > 0;
    }
    // taken from the pile underfoot (a manual pick-up, which prints no new floor-items line); on
    // arrival the gain is autopickup's, and `item_check`'s line about what it left follows in
    // the same batch
    else if gained > 0 && state.player.floor_items.is_some() && items_underfoot(state) {
        if let Some(f) = &mut state.player.floor_items {
            f.picked += gained;
        }
    }
    let p = &mut state.player;
    if data.contains_key("time_last_input") {
        if state.watching.is_some() {
            p.time_delta = p.time - p.time_last_input.unwrap_or(p.time);
        }
        p.last_time = Some(p.time);
    } else if data.contains_key("time") {
        if let Some(last) = p.last_time {
            p.time_delta = p.time - last;
        }
        p.last_time = Some(p.time);
        mark_messages(state, Marker::Turn);
    }
    state.rev.player += 1;
}

fn mark_messages(state: &mut GameState, marker: Marker) {
    if let Some(last) = state.messages.lines.last_mut() {
        last.marker = Some(marker);
    }
    state.messages.more = false;
    state.rev.messages += 1;
}

fn apply_menu(state: &mut GameState, data: &Obj) {
    if truthy_opt(data.get("replace")) {
        state.menus.pop();
    }
    let empty = Vec::new();
    let chunk = match data.get("items") {
        Some(Value::Array(a)) => a,
        Some(_) => {
            diag(state, "menu items is not a list; ignored", Some(data), None);
            &empty
        }
        None => &empty,
    };
    let total_items = match data.get("total_items") {
        None | Some(Value::Null) => chunk.len() as f64,
        Some(v) => js_number(v),
    };
    let mut menu = MenuState {
        tag: str_or(data.get("tag"), ""),
        kind: string(data.get("type")).map(str::to_owned),
        flags: data.get("flags").filter(|v| truthy(v)).and_then(|v| int(Some(v))).unwrap_or(0),
        title: data.get("title").filter(|v| truthy(v)).and_then(MenuTitle::from_wire).unwrap_or_default(),
        more: str_or(data.get("more"), ""),
        alt_more: str_or(data.get("alt_more"), ""),
        total_items: if total_items.is_finite() { total_items as i64 } else { 0 },
        last_hovered: match data.get("last_hovered") {
            None | Some(Value::Null) => -1,
            Some(v) => int(Some(v)).unwrap_or(-1),
        },
        chunk_start: data.get("chunk_start").filter(|v| truthy(v)).and_then(|v| int(Some(v))).unwrap_or(0),
        ui_centred: data.get("ui-centred").and_then(bool::from_wire),
        jump_to: int(data.get("jump_to")),
        ..MenuState::default()
    };
    const NAMED: [&str; 12] =
        ["tag", "type", "flags", "title", "more", "alt_more", "total_items", "items", "last_hovered", "chunk_start", "ui-centred", "jump_to"];
    for (k, v) in data {
        if k != "msg" && !NAMED.contains(&k.as_str()) {
            menu.extra.insert(k.clone(), v.clone());
        }
    }
    let len = if total_items.is_finite() && total_items > 0.0 { total_items as usize } else { 0 };
    if len > MAX_MENU_ITEMS {
        diag(state, "menu total_items is too large; cut", Some(data), None);
    }
    menu.items = vec![None; len.min(MAX_MENU_ITEMS)];
    let start = menu.chunk_start as f64;
    fill_items(state, &mut menu, start, chunk);
    menu.stack_order = state.ui_stack_order;
    state.ui_stack_order += 1;
    state.menus.push(menu);
    state.rev.ui += 1;
}

/// `ui_cutoff`: the server hides everything on its ui stack at the moment it says so. crawl
/// pushes one (`ui::cutoff_point`, ui.cc `push_cutoff`) around a screen that reads keys of its
/// own, targeting above all (directn.cc `direction_chooser::choose_direction`), and `cutoff` is
/// the size of its stack then (tileweb.cc `push_ui_cutoff`: `m_menu_stack.size()`, menus, crt
/// screens and popups counted together). The official client applies it once, to the overlays
/// present when the message arrives (game.js `handle_set_ui_cutoff`), so a popup pushed
/// afterwards, the description `v` opens in look mode, is not hidden by it; the pop (`cutoff`
/// back to -1, or the enclosing one) unhides. Hence a flag on each overlay rather than an index
/// applied at every rebuild.
fn apply_cutoff(state: &mut GameState, cutoff: i64) {
    state.ui_cutoff = cutoff;
    // menus first, then popups, then by stack order (a stable sort, as Array.sort is)
    let mut all: Vec<(i64, bool, usize)> = state.menus.iter().enumerate().map(|(i, m)| (m.stack_order, true, i)).collect();
    all.extend(state.ui.iter().enumerate().map(|(i, p)| (p.stack_order, false, i)));
    all.sort_by_key(|e| e.0);
    for (n, (_, is_menu, i)) in all.into_iter().enumerate() {
        let hidden = (n as i64) <= cutoff;
        if is_menu {
            state.menus[i].hidden = hidden;
        } else {
            state.ui[i].hidden = hidden;
        }
    }
}

/// menu.js `update_item_range`: items from `start` on; a string is a level-2 text line, a
/// missing colour, tiles or hotkeys clear the old ones.
fn fill_items(state: &mut GameState, menu: &mut MenuState, start: f64, items: &[Value]) {
    let start = if start.is_finite() && start >= 0.0 { start as usize } else { 0 };
    for (i, it) in items.iter().enumerate() {
        let idx = start.saturating_add(i);
        if idx >= MAX_MENU_ITEMS {
            diag(state, "menu item index is too large; skipped", None, Some(serde_json::json!({ "index": idx })));
            break;
        }
        if idx >= menu.items.len() {
            menu.items.resize(idx + 1, None);
        }
        let string_item;
        let it = match it {
            Value::String(s) => {
                string_item = serde_json::json!({ "text": s, "level": 2 });
                string_item.as_object()
            }
            other => other.as_object(),
        };
        let Some(it) = it else {
            // a null item leaves the slot as it was; the official client would throw
            diag(state, "menu item is not an object; slot left as it was", None, Some(serde_json::json!({ "index": idx, "item": items[i] })));
            continue;
        };
        let merged = menu.items[idx].get_or_insert_with(MenuItem::default);
        let bad = merged.merge(it);
        if !it.contains_key("colour") {
            merged.colour = None;
        }
        if !it.contains_key("tiles") {
            merged.tiles = None;
        }
        if !it.contains_key("hotkeys") {
            merged.hotkeys = None;
        }
        if merged.level.is_none() {
            merged.level = Some(2);
        }
        if !bad.is_empty() {
            diag(state, format!("menu item fields have the wrong type: {}", bad.join(", ")), None, Some(Value::Object(it.clone())));
        }
    }
}

fn apply_text(state: &mut GameState, data: &Obj) {
    let id = str_or(data.get("id"), "crt");
    let empty = Obj::new();
    let lines = data.get("lines").and_then(Value::as_object).unwrap_or(&empty);
    let mut parsed: Vec<(i64, String)> = Vec::new();
    for (k, v) in lines {
        match key_number(k) {
            Some(n) => parsed.push((n, if v.is_null() { String::new() } else { js_string(v) })),
            None => diag(state, "txt line number is not a number; skipped", Some(data), Some(Value::String(k.clone()))),
        }
    }
    let clear = truthy_opt(data.get("clear"));
    let area = if id == "crt" { &mut state.crt.lines } else { state.crt.areas.entry(id).or_default() };
    if clear {
        area.retain(|k, _| lines.contains_key(&k.to_string()));
    }
    for (n, text) in parsed {
        area.insert(n, text);
    }
    state.rev.ui += 1;
}

/// Apply one server message to the state, in place. Never panics on unknown or malformed
/// input; problems go to `state.diagnostics`.
pub fn reduce(state: &mut GameState, m: &Value) {
    state.rev.any += 1;
    let Some(o) = m.as_object().filter(|o| o.get("msg").is_some_and(Value::is_string)) else {
        diag(state, "message is not an object with a msg field; ignored", None, Some(m.clone()));
        return;
    };
    reduce_message(state, o);
}

fn reduce_message(state: &mut GameState, m: &Obj) {
    let msg = m.get("msg").and_then(Value::as_str).unwrap_or_default();
    match msg {
        // ---- lobby / session
        "ping" => {}
        "lobby_clear" => {
            state.lobby.entries.clear();
            state.lobby.complete = false;
            state.rev.lobby += 1;
        }
        "lobby_entry" => {
            let mut e = LobbyEntry::default();
            let fields: Obj = m.iter().filter(|(k, _)| *k != "msg").map(|(k, v)| (k.clone(), v.clone())).collect();
            let bad = e.merge(&fields);
            if !bad.is_empty() {
                diag(state, format!("lobby entry fields have the wrong type: {}", bad.join(", ")), Some(m), None);
            }
            match state.lobby.entries.iter_mut().find(|x| x.id == e.id) {
                Some(slot) => *slot = e,
                None => state.lobby.entries.push(e),
            }
            state.rev.lobby += 1;
        }
        "lobby_remove" => {
            let id = int(m.get("id"));
            state.lobby.entries.retain(|e| Some(e.id) != id);
            state.rev.lobby += 1;
        }
        "lobby_complete" => {
            state.lobby.complete = true;
            state.rev.lobby += 1;
        }
        "set_game_links" => {
            state.lobby.game_links_html = str_or(m.get("content"), "");
            match parse_game_links(&state.lobby.game_links_html) {
                Ok(games) => {
                    state.lobby.games = games;
                    state.rev.lobby += 1;
                }
                // a malformed escape: the TS throws, and the reducer notes it and keeps the old links
                Err(e) => diag(state, format!("reducer error: {e}"), Some(m), None),
            }
        }
        "html" => {
            if m.get("id").and_then(Value::as_str) == Some("banner") {
                state.lobby.banner_html = str_or(m.get("content"), "");
            }
            state.rev.lobby += 1;
        }
        "login_success" => {
            state.lobby.username = opt_string(m.get("username"));
            state.lobby.admin = truthy_opt(m.get("admin"));
            state.lobby.login_failed = None;
            state.rev.lobby += 1;
        }
        "login_fail" => {
            state.lobby.login_failed = Some(str_or(m.get("reason"), "Login failed."));
            state.rev.lobby += 1;
        }
        "login_cookie" => {
            state.lobby.login_cookie = Some(LoginCookie { cookie: str_or(m.get("cookie"), ""), expires: int(m.get("expires")).unwrap_or(0) });
            state.rev.lobby += 1;
        }
        "register_fail" => {
            state.lobby.register_failed = Some(str_or(m.get("reason"), "Registration failed."));
            state.rev.lobby += 1;
        }
        "logout" => {
            state.lobby.username = None;
            state.rev.lobby += 1;
        }
        "go_lobby" => {
            state.phase = Phase::Lobby;
            state.watching = None;
            reset_game(state);
            state.rev.lobby += 1;
        }
        "login_required" => {
            state.phase = Phase::Lobby;
            state.lobby.login_failed = Some("Login required to play.".into());
            state.rev.lobby += 1;
        }
        "game_client" => {
            // the game's client bundle, sent after `game_started` (ws_handler.py start_crawl, then
            // the process's client_path): a game already started stays playing, else the
            // gamedata is loading for one about to
            state.phase = if state.watching.is_some() {
                Phase::Watching
            } else if state.phase == Phase::Playing {
                Phase::Playing
            } else {
                Phase::Loading
            };
            state.version.gamedata_version = opt_string(m.get("version"));
            state.version.content = Some(str_or(m.get("content"), "")).filter(|s| !s.is_empty());
            reset_game(state);
        }
        "game_started" => {
            state.phase = Phase::Playing;
            state.watching = None;
            state.exit = None;
            // the stale game is gone (or never was); the notice about it goes with it
            state.lobby.stale_processes = None;
            state.lobby.force_terminate = false;
        }
        "watching_started" => {
            state.watching = Some(opt_string(m.get("username")).unwrap_or_default());
            state.phase = Phase::Watching;
            state.exit = None;
        }
        "game_ended" => {
            state.phase = Phase::Ended;
            // client.js crawl_ended / go_lobby: the exit is shown back in the lobby, worded for a
            // watched game when it was one
            state.exit = Some(GameExit {
                reason: opt_string(m.get("reason")),
                message: opt_string(m.get("message")),
                dump: opt_string(m.get("dump")),
                watched: state.watching.clone(),
            });
        }
        "update_spectators" => {
            state.spectators = Some(Spectators { count: int(m.get("count")).unwrap_or(0), names: opt_string(m.get("names")).unwrap_or_default() });
            state.rev.chat += 1;
        }
        "chat" => {
            let line = ChatLine {
                sender: str_or(m.get("sender"), ""),
                html: str_or(m.get("content"), ""),
                meta: truthy_opt(m.get("meta")),
                automated: false,
            };
            push_chat(state, line);
        }
        "server_announcement" => {
            // client.js server_announcement: shown in chat with a magenta caption
            let html = format!("<span class=\"fg5\">Serverwide announcement: </span><span>{}</span>", str_or(m.get("text"), ""));
            push_chat(state, ChatLine { sender: "server".into(), html, meta: true, automated: true });
        }
        "dump" => {
            // chat.js handle_dump: the character dump url is posted into chat
            let html = format!("<span class=\"chat_msg chat_automated\">{}</span>", escape_html(&format!("{}.txt", str_or(m.get("url"), ""))));
            push_chat(state, ChatLine { sender: String::new(), html, meta: false, automated: true });
        }
        "toggle_chat" => {
            state.chat_visible = !state.chat_visible;
            state.rev.chat += 1;
        }
        "super_hide_chat" => {
            state.chat_visible = false;
            state.chat_super_hidden = true;
            state.rev.chat += 1;
        }
        // the official client's crt / normal layer switch; Isorun derives its screens from state
        "layer" => {}
        "reload_url" => {
            state.lobby.reload_url = true;
            state.rev.lobby += 1;
        }
        "stale_processes" => {
            state.lobby.stale_processes = Some(StaleProcesses {
                game: str_or(m.get("game"), ""),
                timeout: m.get("timeout").filter(|v| truthy(v)).and_then(|v| int(Some(v))).unwrap_or(0),
            });
            state.rev.lobby += 1;
        }
        "force_terminate?" => {
            state.lobby.force_terminate = true;
            state.rev.lobby += 1;
        }
        "set_account_hold" => {
            state.lobby.account_hold = true;
            state.rev.lobby += 1;
        }
        "clear_account_hold" => {
            state.lobby.account_hold = false;
            state.rev.lobby += 1;
        }
        "go_admin" => {
            state.lobby.admin = true;
            state.rev.lobby += 1;
        }
        "admin_log" => {
            state.lobby.admin_log.push(str_or(m.get("text"), ""));
            if state.lobby.admin_log.len() > 200 {
                state.lobby.admin_log.remove(0);
            }
            state.rev.lobby += 1;
        }
        "rcfile_contents" => {
            state.lobby.rcfile = Some(RcFile { game_id: opt_string(m.get("game_id")), contents: str_or(m.get("contents"), "") });
            state.rev.lobby += 1;
        }
        "auth_error" => {
            state.lobby.login_failed = Some(str_or(m.get("reason"), "Authentication error."));
            state.rev.lobby += 1;
        }

        // ---- game
        "version" => state.version.text = opt_string(m.get("text")),
        "options" => {
            state.options = m.get("options").and_then(Value::as_object).cloned().unwrap_or_default();
            state.rev.player += 1;
        }
        "set_option" => {
            let name = js_string(m.get("name").unwrap_or(&Value::String("undefined".into())));
            state.options.insert(name, m.get("value").cloned().unwrap_or(Value::Null));
            state.rev.player += 1;
        }
        "layout" => {
            if let Some(pane) = m.get("message_pane").and_then(Value::as_object) {
                if let Some(h) = pane.get("height").filter(|v| truthy(v)).and_then(|v| int(Some(v))) {
                    state.messages.pane_height = h + if truthy_opt(pane.get("small_more")) { 0 } else { -1 };
                }
            }
        }
        "ui_state" => {
            match int(m.get("state")) {
                Some(s) => state.ui_state = s,
                None => diag(state, "ui_state without a numeric state; ignored", Some(m), None),
            }
            state.rev.ui += 1;
        }
        "ui_cutoff" => {
            // a missing cutoff hides nothing, as `i <= undefined` is false
            apply_cutoff(state, int(m.get("cutoff")).unwrap_or(-1));
            state.rev.ui += 1;
        }
        "input_mode" => {
            match int(m.get("mode")) {
                Some(mode) if mode != state.input_mode => {
                    // leaving a mode that read a key is the key being read: message.cc `got_input`
                    if state.input_mode != MouseMode::NORMAL {
                        state.messages.read_to = state.messages.lines.len();
                    }
                    state.input_mode = mode;
                    if mode == MouseMode::COMMAND {
                        mark_messages(state, Marker::Command);
                    }
                }
                Some(_) => {}
                None => diag(state, "input_mode without a numeric mode; ignored", Some(m), None),
            }
            state.rev.ui += 1;
        }
        "delay" | "flush_messages" => state.flush_pending = true,
        "map" => {
            if truthy_opt(m.get("clear")) {
                state.map = MapStore::default();
                state.dirty_cells.clear();
                state.map_cleared = true;
            }
            if let Some(v) = m.get("player_on_level").filter(|v| !v.is_null()) {
                state.map.player_on_level = truthy(v);
            }
            if let Some(v) = m.get("vgrdc").filter(|v| truthy(v)) {
                match Pos::from_wire(v) {
                    Some(p) => state.map.view_center = Some(p),
                    None => diag(state, "map vgrdc is malformed; ignored", Some(m), Some(v.clone())),
                }
            }
            match m.get("cells") {
                Some(Value::Array(cells)) => merge_cells(state, cells),
                Some(v) if truthy(v) => diag(state, "map cells is not a list; ignored", Some(m), None),
                _ => {}
            }
            if m.contains_key("invis_mon_desc") {
                state.map.invisible_monster_desc = str_or(m.get("invis_mon_desc"), "");
            }
            state.rev.map += 1;
        }
        "player" => apply_player(state, m),
        "msgs" => reduce_msgs(state, m),
        "text_cursor" => {
            state.messages.text_cursor = truthy_opt(m.get("enabled"));
            state.rev.messages += 1;
        }
        "cursor" => {
            match int(m.get("id")).filter(|id| (0..=MAX_CURSOR_ID).contains(id)) {
                Some(id) => {
                    let id = id as usize;
                    if state.cursors.len() <= id {
                        state.cursors.resize(id + 1, None);
                    }
                    state.cursors[id] = m.get("loc").and_then(Pos::from_wire);
                }
                None => diag(state, "cursor id is not a small number; ignored", Some(m), None),
            }
            state.rev.ui += 1;
        }
        "menu" => apply_menu(state, m),
        "update_menu" => {
            let Some(mut menu) = state.menus.pop() else { return };
            let mut bad = Vec::new();
            for (k, v) in m {
                if k != "msg" && k != "items" && menu.assign(k, v) == Assign::BadType {
                    bad.push(k.clone());
                }
            }
            if let Some(n) = num(m.get("total_items")) {
                if n >= 0.0 && n.fract() == 0.0 {
                    let n = n as usize;
                    if n > MAX_MENU_ITEMS {
                        bad.push("total_items (too large)".into());
                    }
                    menu.items.resize(n.min(MAX_MENU_ITEMS), None);
                } else {
                    bad.push("total_items (not a length)".into());
                }
            }
            state.menus.push(menu);
            if !bad.is_empty() {
                diag(state, format!("update_menu fields have the wrong type: {}", bad.join(", ")), Some(m), None);
            }
            state.rev.ui += 1;
        }
        "update_menu_items" => {
            let Some(mut menu) = state.menus.pop() else { return };
            // `(chunk_start as number) || 0`, then `Number.isFinite`, which takes no string
            let start = num(m.get("chunk_start")).unwrap_or(0.0);
            let empty = Vec::new();
            let items = m.get("items").and_then(Value::as_array).unwrap_or(&empty);
            fill_items(state, &mut menu, start, items);
            state.menus.push(menu);
            state.rev.ui += 1;
        }
        "menu_scroll" => {
            let watching = state.watching.is_some();
            let Some(menu) = state.menus.last_mut() else { return };
            menu.server_first_visible = int(m.get("first"));
            if truthy_opt(m.get("force")) || watching {
                if let Some(h) = int(m.get("last_hovered")) {
                    menu.last_hovered = h;
                }
            }
            state.rev.ui += 1;
        }
        "title_prompt" => {
            let Some(menu) = state.menus.last_mut() else { return };
            menu.title_prompt = if truthy_opt(m.get("close")) {
                None
            } else {
                Some(TitlePrompt { prompt: str_or(m.get("prompt"), "Select what? (regex) "), raw: truthy_opt(m.get("raw")) })
            };
            state.rev.ui += 1;
        }
        "close_menu" => {
            state.menus.pop();
            state.rev.ui += 1;
        }
        "close_all_menus" => {
            state.menus.clear();
            state.rev.ui += 1;
        }
        "ui-push" => {
            state.ui.push(Popup {
                kind: opt_string(m.get("type")).unwrap_or_default(),
                generation_id: int(m.get("generation_id")),
                ui_centred: m.get("ui-centred").and_then(bool::from_wire),
                data: m.clone(),
                state: Obj::new(),
                stack_order: state.ui_stack_order,
                ..Popup::default()
            });
            state.ui_stack_order += 1;
            state.rev.ui += 1;
        }
        "ui-pop" => {
            state.ui.pop();
            state.rev.ui += 1;
        }
        "ui-stack" => {
            // only meaningful for spectators joining mid-popup
            match m.get("items") {
                Some(Value::Array(items)) => {
                    if state.ui.is_empty() {
                        for it in items {
                            reduce(state, it);
                        }
                    }
                }
                Some(v) if truthy(v) => diag(state, "ui-stack items is not a list; ignored", Some(m), None),
                _ => {}
            }
        }
        "ui-state" => {
            let watching = state.watching.is_some();
            if let Some(top) = state.ui.last_mut() {
                // ui-layouts.js newgame_choice_update: the player's own client returns at once on
                // the echo of its `outer_menu_focus` (`from_client`), so the popup is not touched
                // and a click on the button under the mouse lands; only a spectator follows the echo
                let echo = top.kind == "newgame-choice" && m.get("from_client") == Some(&Value::Bool(true)) && !watching;
                for (k, v) in m {
                    top.state.insert(k.clone(), v.clone());
                }
                if echo {
                    return;
                }
                let formatted = m.get("type").and_then(Value::as_str) == Some("formatted-scroller");
                if formatted {
                    if let Some(text) = m.get("text").filter(|v| v.is_string()) {
                        top.data.insert("text".into(), text.clone());
                    }
                    // formatted_scroller_update: the scroller's own scroll line rides on its ui-state
                    if let Some(scroll) = int(m.get("scroll")) {
                        top.scroll = Some(scroll);
                        top.scroll_from_webtiles = truthy_opt(m.get("from_webtiles"));
                        top.scroll_seq += 1;
                    }
                }
            }
            state.rev.ui += 1;
        }
        "ui-state-sync" => {
            // ui.js: spectators follow the player's focus and widget state; the player's own
            // client ignores echoes of what it sent (from_webtiles)
            let sync = UiStateSync {
                generation_id: int(m.get("generation_id")),
                widget_id: string(m.get("widget_id")).map(str::to_owned),
                has_focus: m.get("has_focus").map(truthy),
                from_webtiles: truthy_opt(m.get("from_webtiles")),
                data: m.clone(),
                seq: state.ui_sync.as_ref().map_or(0, |s| s.seq) + 1,
            };
            if let Some(top) = state.ui.last_mut() {
                if sync.generation_id.is_none() || top.generation_id.is_none() || top.generation_id == sync.generation_id {
                    top.sync = Some(sync.clone());
                }
            }
            state.ui_sync = Some(sync);
            state.rev.ui += 1;
        }
        "ui-scroller-scroll" => {
            // ui-layouts.js recv_ui_scroll: a line number for the top popup's scroller (formatted
            // scrollers and menus sync their own way)
            let Some(scroll) = int(m.get("scroll")) else {
                diag(state, "ui-scroller-scroll without a numeric scroll; ignored", Some(m), None);
                return;
            };
            let from_webtiles = truthy_opt(m.get("from_webtiles"));
            state.ui_scroll = Some(UiScroll { scroll, from_webtiles, seq: state.ui_scroll.map_or(0, |s| s.seq) + 1 });
            if let Some(top) = state.ui.last_mut() {
                if top.kind != "formatted-scroller" && top.kind != "menu" {
                    top.scroll = Some(scroll);
                    top.scroll_from_webtiles = from_webtiles;
                    top.scroll_seq += 1;
                }
            }
            state.rev.ui += 1;
        }
        "txt" => apply_text(state, m),
        "clear" => {
            state.crt.lines.clear();
            state.rev.ui += 1;
        }
        "show_dialog" => {
            state.dialog = Some(str_or(m.get("html"), ""));
            state.rev.ui += 1;
        }
        "hide_dialog" => {
            state.dialog = None;
            state.rev.ui += 1;
            // process_handler.py _kill_stale_process sends this once the stale game is stopped:
            // the lobby's notices about it close too
            if state.lobby.stale_processes.is_some() || state.lobby.force_terminate {
                state.lobby.stale_processes = None;
                state.lobby.force_terminate = false;
                state.rev.lobby += 1;
            }
        }
        "init_input" => {
            let mut input = TextInputState::default();
            let fields: Obj = m.iter().filter(|(k, _)| *k != "msg").map(|(k, v)| (k.clone(), v.clone())).collect();
            let bad = input.merge(&fields);
            input.text = str_or(m.get("prefill"), "").trim().to_owned();
            if !bad.is_empty() {
                diag(state, format!("init_input fields have the wrong type: {}", bad.join(", ")), Some(m), None);
            }
            state.text_input = Some(input);
            state.rev.ui += 1;
        }
        "close_input" => {
            state.text_input = None;
            state.rev.ui += 1;
        }
        "update_input" => {
            if let Some(input) = &mut state.text_input {
                input.text = str_or(m.get("input_text"), "");
            }
            state.rev.ui += 1;
        }
        "game_ended_placeholder" => {}
        _ => diag(state, "unknown message type", Some(m), None),
    }
}

/// `max(0, Number(v) || 0)`, as an integer count.
fn count_of(v: Option<&Value>) -> usize {
    let n = v.map(js_number).unwrap_or(0.0);
    if n.is_finite() && n > 0.0 {
        n as usize
    } else {
        0
    }
}

fn reduce_msgs(state: &mut GameState, m: &Obj) {
    // messages.js applies both when both are present, in this order
    for k in ["rollback", "old_msgs"] {
        let n = count_of(m.get(k));
        if n > 0 {
            let lines = &mut state.messages.lines;
            lines.truncate(lines.len().saturating_sub(n));
        }
    }
    if m.contains_key("more") {
        state.messages.more = truthy_opt(m.get("more"));
        state.messages.more_text = match m.get("more_text").and_then(Value::as_str) {
            Some(t) if !t.is_empty() => t.to_owned(),
            _ => "--more--".into(),
        };
    }
    match m.get("messages") {
        Some(Value::Array(msgs)) => {
            for line in msgs {
                let Some(lo) = line.as_object() else {
                    diag(state, "message line is not an object; skipped", Some(m), Some(line.clone()));
                    continue;
                };
                let text = match lo.get("text") {
                    None | Some(Value::Null) => String::new(),
                    Some(v) => js_string(v),
                };
                state.messages.lines.push(GameMessage {
                    text: text.clone(),
                    turn: int(lo.get("turn")),
                    channel: int(lo.get("channel")),
                    marker: None,
                });
                note_floor_items(state, &text, num(lo.get("turn")), lo.get("channel").and_then(Value::as_f64));
            }
        }
        Some(_) => diag(state, "messages is not a list; ignored", Some(m), None),
        None => {}
    }
    let log = &mut state.messages;
    if log.lines.len() > 500 {
        let over = log.lines.len() - 500;
        log.read_to = log.read_to.saturating_sub(over);
        log.lines.drain(..over);
    }
    log.read_to = log.read_to.min(log.lines.len());
    state.rev.messages += 1;
}

// ---------------------------------------------------------------------------
// Derived helpers over GameState

/// An inventory item on the consumables action panel, with its inventory letter.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionPanelItem {
    pub item: InvItem,
    pub letter: String,
}

/// The consumables action panel, as action_panel.js builds it: inventory items with a quantity
/// and `action_panel_order >= 0`, sorted by that order and ties by `sub_type` as its own
/// comparator does (by slot when an item came without one). Each entry carries its inventory
/// letter so a binding can act on it.
pub fn action_panel_items(state: &GameState) -> Vec<ActionPanelItem> {
    let mut out: Vec<ActionPanelItem> = state
        .player
        .inv
        .values()
        .filter(|item| item.action_panel_order.is_some_and(|o| o >= 0) && item.quantity.is_some_and(|q| q != 0))
        .map(|item| ActionPanelItem { item: item.clone(), letter: index_to_letter(item.slot) })
        .collect();
    out.sort_by(|a, b| {
        let (oa, ob) = (a.item.action_panel_order.unwrap_or(0), b.item.action_panel_order.unwrap_or(0));
        oa.cmp(&ob).then_with(|| match (a.item.sub_type, b.item.sub_type) {
            (Some(sa), Some(sb)) if sa != sb => sa.cmp(&sb),
            _ => a.item.slot.cmp(&b.item.slot),
        })
    });
    out
}

/// An inventory slot's letter: a..z, then A..Z; `-` for none.
pub fn index_to_letter(index: i64) -> String {
    if index == -1 {
        return "-".into();
    }
    let code = if index < 26 { 97 + index } else { 65 + index - 26 };
    char::from_u32(code.rem_euclid(0x10000) as u32).map_or_else(|| "?".into(), String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters() {
        assert_eq!(index_to_letter(0), "a");
        assert_eq!(index_to_letter(25), "z");
        assert_eq!(index_to_letter(26), "A");
        assert_eq!(index_to_letter(-1), "-");
    }

    #[test]
    fn tile_words() {
        assert_eq!(TileWord::from_wire(&serde_json::json!(5)), Some(TileWord::new(5, 0)));
        assert_eq!(TileWord::from_wire(&serde_json::json!([5, 2])).map(TileWord::value), Some((2 << 32) | 5));
        assert_eq!(TileWord::new(7, 1).to_wire(), serde_json::json!([7, 1]));
    }
}
