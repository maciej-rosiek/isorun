//! The screens the dock opens over the game, drawn by the client from the server's data
//! (screens.ts): the inventory (a paper doll and the backpack) and the character sheet. Each is
//! a panel (`.popup.dock-screen`) with a title and a close button, hanging from 5% of the
//! screen's height, and the stops the pad's cursor walks.

use crate::gear::{
    equip_slot_of, item_action_keycaps, item_actions, item_title, obj, paper_doll, DollPlace, EquipSlot, ItemActionId, PackItem, Worn, DOLL_PLACES,
};
use crate::glyphs::{Glyph, PadKind};
use crate::icons::{self, Placed};
use crate::item_compare::{compare_stats, item_name_parts, property_tone};
use crate::paint::{hex, inset, outer, rgba, s, sa, At, Border, BoxCache, BoxStyle, Layer, RadialSize, Stop, C, TRANSPARENT};
use crate::quick_slots::item_tiles;
use crate::style::{self, Face, TextStyle, DANGER, GOLD, GOLD_BRIGHT, INK, INK_DIM, TRIM};
use crate::tooltip::{keycaps_width, layout_card, paint_keycaps, CardKind, KeyHint, KeycapSize, Tip, TipTag};
use crate::{Action, KeyStep};
use egui::{Color32, Painter, Pos2, Rect, Sense, Vec2};
use isorun_scene::Billboard;
use isorun_ui::tiles::TileIcons;
use isorun_webtiles::{formatted_string_to_text, PlayerState};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenId {
    Inventory,
    Character,
}

impl ScreenId {
    pub fn title(self) -> &'static str {
        match self {
            ScreenId::Inventory => "Inventory",
            ScreenId::Character => "Character",
        }
    }
}

/// What a screen asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum ScreenEvent {
    /// an item's action from the inventory (game.ts `itemAct`)
    Act(PackItem, ItemActionId),
    /// an empty place of the doll: what in the pack goes there, to put on (game.ts `fillPlace`)
    Fill(DollPlace, &'static str),
    /// put the item on the first free quick slot
    Pin(PackItem),
    /// one of crawl's own screens from the character sheet (game.ts `characterCommand`)
    Command(Action),
    Close,
}

/// What the pad (or a key the router reads as one) does on a screen (overlays.ts `screenInput`,
/// the client overlay ops): the cursor walks the stops by rows and columns, A acts, Y is the
/// stop's quick action and X describes; on an open item menu up and down walk its rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenOp {
    Next,
    Prev,
    Left,
    Right,
    /// A, Enter, Space: open the item's menu, run a command, fill an empty place
    Select,
    /// Y: the item's first action (put on, take off, use)
    Quick,
    /// X: describe the item
    Describe,
    /// B, Escape: the open menu goes, else the screen
    Cancel,
    /// the bumpers: ten stops on or back (overlays.ts `pageNext`)
    PageNext,
    PagePrev,
    First,
    Last,
}

fn item_of_stop<'a>(id: &str, data: &'a ScreenData) -> Option<&'a PackItem> {
    let slot: i64 = id.strip_prefix("item:")?.parse().ok()?;
    data.items.iter().find(|i| i.slot() == slot)
}

fn place_label(p: DollPlace) -> &'static str {
    match p {
        DollPlace::Helmet => "Head",
        DollPlace::Cloak => "Cloak",
        DollPlace::Body => "Body armour",
        DollPlace::Gloves => "Hands",
        DollPlace::Boots => "Feet",
        DollPlace::Amulet => "Amulet",
        DollPlace::Ring1 | DollPlace::Ring2 => "Ring",
        DollPlace::Weapon => "Weapon",
        DollPlace::Offhand => "Off hand",
        DollPlace::Talisman => "Talisman",
        DollPlace::Gizmo => "Gizmo",
        DollPlace::Quiver => "Quiver",
    }
}

pub fn place_label_of(p: DollPlace) -> &'static str {
    place_label(p)
}

fn place_silhouette(p: DollPlace) -> &'static str {
    match p {
        DollPlace::Helmet => "THELM_HELM",
        DollPlace::Cloak => "ARM_CLOAK",
        DollPlace::Body => "ARM_CHAIN_MAIL",
        DollPlace::Gloves => "ARM_GLOVES",
        DollPlace::Boots => "ARM_BOOTS",
        DollPlace::Amulet => "AMU_NORMAL_OFFSET",
        DollPlace::Ring1 | DollPlace::Ring2 => "RING_NORMAL_OFFSET",
        DollPlace::Weapon => "WPN_LONG_SWORD",
        DollPlace::Offhand => "ARM_KITE_SHIELD",
        DollPlace::Talisman => "TALISMAN_BLADE",
        DollPlace::Gizmo => "GIZMO",
        DollPlace::Quiver => "MI_ARROW",
    }
}

/// Where each place stands on the doll: the column (0 left, 2 right) and the row.
fn place_at(p: DollPlace) -> (usize, usize) {
    match p {
        DollPlace::Helmet => (0, 0),
        DollPlace::Cloak => (0, 1),
        DollPlace::Body => (0, 2),
        DollPlace::Gloves => (0, 3),
        DollPlace::Boots => (0, 4),
        DollPlace::Amulet => (2, 0),
        DollPlace::Ring1 => (2, 1),
        DollPlace::Ring2 => (2, 2),
        DollPlace::Weapon => (2, 3),
        DollPlace::Offhand => (2, 4),
        DollPlace::Talisman => (0, 5),
        DollPlace::Gizmo => (1, 5),
        DollPlace::Quiver => (2, 5),
    }
}

fn slot_word(s: EquipSlot) -> &'static str {
    match s {
        EquipSlot::Weapon => "Weapon",
        EquipSlot::Offhand => "Shield",
        EquipSlot::Body => "Body armour",
        EquipSlot::Cloak => "Cloak",
        EquipSlot::Helmet => "Helmet",
        EquipSlot::Gloves => "Gloves",
        EquipSlot::Boots => "Boots",
        EquipSlot::Amulet => "Amulet",
        EquipSlot::Ring => "Ring",
        EquipSlot::Talisman => "Talisman",
        EquipSlot::Gizmo => "Gizmo",
    }
}

fn class_word(base: Option<i64>) -> &'static str {
    match base {
        Some(obj::MISSILES) => "Ammunition",
        Some(obj::WANDS) => "Wand",
        Some(obj::SCROLLS) => "Scroll",
        Some(obj::POTIONS) => "Potion",
        Some(obj::BOOKS) => "Book",
        Some(obj::STAVES) => "Staff",
        Some(obj::MISCELLANY) => "Evocable",
        Some(obj::BAUBLES) => "Bauble",
        _ => "Item",
    }
}

const BACKPACK_COLUMNS: usize = 8;
const BACKPACK_MIN_ROWS: usize = 7;
const PACK_LETTERS: usize = 52;
/// The smallest unit the screens' small text (a socket's letter and count, the cards, the item
/// menu) is set in when a phone's screen scales the rest down: 11px letters at 0.8 still read.
const SMALL_TEXT_UNIT: f32 = 0.8;

/// How the inventory's panels stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InvArrange {
    /// the doll beside the backpack, the summary under both (the desktop's)
    Wide,
    /// the doll beside the backpack, the summary under the backpack (a phone on its side)
    Beside,
    /// the doll over the backpack (a phone held upright)
    Stacked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InvLayout {
    arrange: InvArrange,
    cols: usize,
}

impl InvLayout {
    fn rows(&self, items: usize) -> usize {
        let min_rows = match self.arrange {
            InvArrange::Wide => BACKPACK_MIN_ROWS,
            InvArrange::Stacked => 4,
            // as many rows as the pack's letters need, all of them in sight: never fewer than
            // the 52 letters' worth spread over the columns
            InvArrange::Beside => PACK_LETTERS.div_ceil(self.cols).min(BACKPACK_MIN_ROWS),
        };
        min_rows.max(items.div_ceil(self.cols))
    }

    /// The panel's size at unit `u` for a pack of `items` (not on the doll).
    fn size_at(&self, u: f32, items: usize) -> (f32, f32) {
        let cols = self.cols;
        let rows = self.rows(items);
        let doll_w = 84.0 * u * 4.4 + 16.0 * u;
        let doll_h = 6.0 * 84.0 * u + 5.0 * 8.0 * u;
        let grid_w = cols as f32 * 72.0 * u + (cols - 1) as f32 * 6.0 * u;
        let grid_h = rows as f32 * 72.0 * u + (rows - 1) as f32 * 6.0 * u;
        let title_h = 12.0 * u + 28.0 * 1.1 * u + 10.0 * u + 1.0;
        let summary = 12.0 * u + (4.0 * u + 19.0 * u) + 6.0 * u + 24.0 * u + 6.0 * u;
        let around = 1.0 + title_h + 18.0 * u + summary + 1.0;
        let panel = |h: f32| h + 28.0 * u + 2.0;
        let (w, h) = match self.arrange {
            InvArrange::Stacked => (22.0 * u + (doll_w.max(grid_w) + 28.0 * u + 2.0) + 22.0 * u, around + panel(doll_h) + 22.0 * u + panel(grid_h)),
            InvArrange::Wide => (22.0 * u + panel(doll_w) + 22.0 * u + panel(grid_w) + 22.0 * u, around + panel(doll_h.max(grid_h))),
            InvArrange::Beside => {
                let right = panel(grid_h) + summary;
                (22.0 * u + panel(doll_w) + 22.0 * u + panel(grid_w) + 22.0 * u, around - summary + panel(doll_h).max(right))
            }
        };
        (w + 2.0, h)
    }
}
/// the character sheet's line height: the body's `normal` in DejaVu Sans Mono
const LINE: f32 = 1.17;

fn colour(col: i64) -> C {
    style::term(col)
}

/// The item's name for a card's title: no article, no inscription.
fn clean_title(item: &PackItem) -> String {
    let t = item_title(item.name());
    let re = regex::Regex::new(r"\s*\{[^}]*\}\s*").unwrap();
    re.replace_all(&t, " ").trim().to_string()
}

/// An item's card (screens.ts `itemTip`): what it is, what it is worn as, its stats against what
/// it would replace, its properties, and the keys.
pub fn item_tip(item: &PackItem, worn: Option<Worn>, compare_to: Option<&PackItem>, kicker: Option<&str>, with_hints: bool) -> Tip {
    let slot = equip_slot_of(&item.item);
    let parts = item_name_parts(item.name());
    let kind = slot.map(slot_word).unwrap_or_else(|| class_word(item.item.base_type));
    let theirs = compare_to.map(|c| item_name_parts(c.name()).properties).unwrap_or_default();
    let mut tags: Vec<TipTag> = vec![];
    if slot.is_some() && !parts.ego.is_empty() && parts.properties.is_empty() {
        tags.push(TipTag { text: parts.ego.clone(), tone: None });
    }
    for p in &parts.properties {
        tags.push(TipTag { text: p.clone(), tone: (compare_to.is_some() && !theirs.contains(p)).then(|| property_tone(p)) });
    }
    let first = item_actions(item, worn).into_iter().next();
    let mut sub = kind.to_string();
    if let Some(w) = worn {
        sub += &format!(" · {}", w.word());
    }
    Tip {
        kicker: kicker.map(String::from),
        title: clean_title(item),
        colour: Some(colour(item.col())),
        sub: Some(sub),
        body: item.useless().then(|| "Of no use to you".to_string()),
        lines: compare_stats(item, compare_to),
        tags,
        hints: if with_hints {
            let mut h = vec![KeyHint::keys(&["Enter"]).pad(Glyph::A).label("Actions")];
            if let Some(f) = first {
                h.push(KeyHint { keys: item_action_keycaps(&f, &item.letter), pad: Some(Glyph::Y), pad_hold: false, label: Some(f.label.clone()) });
            }
            h
        } else {
            vec![]
        },
    }
}

/// What is worn where the item would go, rings both.
fn worn_in<'a>(doll: &BTreeMap<DollPlace, &'a PackItem>, slot: EquipSlot) -> Vec<&'a PackItem> {
    if slot == EquipSlot::Ring {
        return [DollPlace::Ring1, DollPlace::Ring2].iter().filter_map(|p| doll.get(p).copied()).collect();
    }
    DOLL_PLACES.iter().filter(|p| p.slot() == Some(slot)).filter_map(|p| doll.get(p).copied()).take(1).collect()
}

/// One of the character sheet's buttons to crawl's own screens (command-menu.ts
/// `CHARACTER_COMMANDS`, in the sheet's order, with the overview).
struct Command {
    label: &'static str,
    key: &'static str,
    sub: &'static str,
    tile: &'static str,
    action: Action,
}

fn commands() -> Vec<Command> {
    let cmd =
        |label, key: &'static str, sub, tile| Command { label, key, sub, tile, action: Action::Keys { label: label.into(), seq: vec![KeyStep::text(key)] } };
    let ctl = |label, key, code, sub, tile| Command { label, key, sub, tile, action: Action::Keys { label: label.into(), seq: vec![KeyStep::key(code)] } };
    vec![
        cmd("Skills", "m", "what you are training, and how fast", "CMD_DISPLAY_SKILLS"),
        cmd("Resistances / equipment", "%", "what you resist, and what you are wearing", "CMD_RESISTS_SCREEN"),
        cmd("Character status", "@", "what is on you right now", "CMD_DISPLAY_CHARACTER_STATUS"),
        cmd("Mutations", "A", "what the dungeon has made of you", "CMD_DISPLAY_MUTATIONS"),
        cmd("Religion", "^", "your god, and what they ask of you", "CMD_DISPLAY_RELIGION"),
        cmd("Spell list", "I", "the spells you know, and what they cost", "BOOK"),
        ctl("Dungeon overview", "Ctrl-O", 15, "the levels, branches and altars you have found", "CMD_DISPLAY_OVERMAP"),
        ctl("Message history", "Ctrl-P", 16, "everything the game has told you", "CMD_REPLAY_MESSAGES"),
        cmd("Adjust inventory letters", "=", "move an item to a letter you will remember", "CMD_DISPLAY_INVENTORY"),
        cmd("Known items / autopickup", "\\", "what you have identified, and what to pick up", "CMD_KNOWN_ITEMS"),
        cmd("Runes collected", "}", "the runes you are carrying out", "MISC_RUNE_OF_ZOT"),
        cmd("Show gold", "$", "what you have found, and what you have spent", "GOLD01"),
    ]
}

/// A stop the pad's cursor walks on a screen.
#[derive(Clone, Debug)]
struct Stop2 {
    id: String,
    rect: Rect,
    row: usize,
    col: usize,
}

/// An open item menu: its rows and the one lit.
struct ItemMenu {
    item: PackItem,
    anchor: Rect,
    rows: Vec<(String, Vec<String>, Option<ItemActionId>)>,
    focus: usize,
}

/// A screen up: which, and the state it keeps across frames.
pub struct Screen {
    pub id: ScreenId,
    menu: Option<ItemMenu>,
    focus: Option<String>,
    stops: Vec<Stop2>,
    /// the player's figure, grown smooth once (screens.ts `figureCanvas`)
    figure: Option<(Vec<u32>, egui::TextureHandle)>,
    /// the screen reads the keyboard itself (`keys`); off where a router hands it `op`s
    pub own_keys: bool,
    /// the panel as last drawn
    panel: Option<Rect>,
    /// how far the body is scrolled where it is taller than the screen (a phone)
    scroll: f32,
    /// the stop last brought into view
    scrolled_to: Option<String>,
}

/// What the screens are drawn from.
pub struct ScreenData<'a> {
    pub player: &'a PlayerState,
    pub items: &'a [PackItem],
    pub worn: &'a BTreeMap<String, Worn>,
    pub me: Option<&'a Billboard>,
    /// reading what is worn off crawl's pack is under way
    pub checking: bool,
    pub pad: Option<PadKind>,
    pub weapon: String,
    pub quiver: String,
}

fn screen_style(u: f32) -> BoxStyle {
    BoxStyle {
        layers: vec![
            Layer::Radial(RadialSize::EllipseFarthest, (0.5, 0.0), vec![sa(rgba(120, 90, 40, 0.16)), s(TRANSPARENT, 0.6)]),
            Layer::RepeatingLinear(
                0.0,
                vec![
                    Stop(rgba(255, 255, 255, 0.012), At::Pt(0.0)),
                    Stop(rgba(255, 255, 255, 0.012), At::Pt(2.0)),
                    Stop(TRANSPARENT, At::Pt(2.0)),
                    Stop(TRANSPARENT, At::Pt(6.0)),
                ],
            ),
            Layer::Linear(180.0, vec![sa(hex(0x1b1612)), sa(hex(0x0c0a08))]),
        ],
        radius: [6.0 * u; 4],
        border: Border::all(1.0, TRIM),
        insets: vec![inset(0.0, 0.0, 0.0, 1.0, hex(0x000000)), inset(0.0, 0.0, 0.0, 4.0 * u, rgba(184, 145, 63, 0.14))],
        outers: vec![outer(0.0, 12.0, 48.0, 0.0, rgba(0, 0, 0, 0.85))],
        ..Default::default()
    }
}

/// `.inv-socket` and its variants: the item's colour in its border and glow.
fn socket_style(u: f32, item_colour: Option<C>, doll: bool, hover: bool) -> BoxStyle {
    let base_border = if doll { hex(0x4b3b1d) } else { hex(0x3e3119) };
    let border = if hover && item_colour.is_some() {
        GOLD_BRIGHT
    } else if let Some(c) = item_colour {
        c.mix(hex(0x4b3b1d), 0.5)
    } else if hover && doll {
        style::BRONZE
    } else {
        base_border
    };
    let mut layers = vec![];
    if let Some(c) = item_colour {
        layers.push(Layer::Radial(RadialSize::CircleFarthest, (0.5, 0.6), vec![sa(c.mix(TRANSPARENT, 0.14)), s(TRANSPARENT, 0.7)]));
    }
    layers.push(Layer::Radial(RadialSize::CircleFarthest, (0.5, 0.38), vec![sa(hex(0x221c16)), s(hex(0x080605), 0.8)]));
    BoxStyle {
        layers,
        radius: [if doll { 6.0 } else { 4.0 } * u; 4],
        border: Border::all(1.0, border),
        insets: vec![inset(0.0, 0.0, 0.0, 1.0, hex(0x000000)), inset(0.0, 0.0, 14.0 * u, 0.0, rgba(0, 0, 0, 0.9))],
        ..Default::default()
    }
}

fn panel_style(u: f32, paper: bool, char_block: bool) -> BoxStyle {
    let mut layers = vec![Layer::Solid(rgba(0, 0, 0, if char_block { 0.35 } else { 0.38 }))];
    if paper {
        layers.insert(0, Layer::Radial(RadialSize::Ellipse(0.45, 0.4), (0.5, 0.45), vec![sa(rgba(216, 180, 95, 0.12)), s(TRANSPARENT, 0.7)]));
    }
    BoxStyle {
        layers,
        radius: [5.0 * u; 4],
        border: Border::all(1.0, hex(0x33280f)),
        insets: vec![inset(0.0, 0.0, if char_block { 18.0 } else { 24.0 * u }, 0.0, rgba(0, 0, 0, if char_block { 0.6 } else { 0.7 }))],
        ..Default::default()
    }
}

impl Screen {
    /// The panel as last drawn (a tap outside it closes the screen).
    pub fn panel(&self) -> Option<egui::Rect> {
        self.panel
    }

    pub fn new(id: ScreenId) -> Screen {
        Screen { id, menu: None, focus: None, stops: vec![], figure: None, own_keys: true, panel: None, scroll: 0.0, scrolled_to: None }
    }

    /// Keys while the screen is up, for a HUD that reads its own keys (the examples): the arrows
    /// walk the stops (or the open menu), Enter opens or runs, Escape closes the menu or the
    /// screen. The game screen routes its keys and the pad through its router to `op` instead.
    pub fn keys(&mut self, ui: &egui::Ui, data: &ScreenData) -> Vec<ScreenEvent> {
        let pressed = |k: egui::Key| ui.input(|i| i.key_pressed(k));
        let keys = [
            (egui::Key::ArrowDown, ScreenOp::Next),
            (egui::Key::ArrowUp, ScreenOp::Prev),
            (egui::Key::ArrowLeft, ScreenOp::Left),
            (egui::Key::ArrowRight, ScreenOp::Right),
            (egui::Key::Enter, ScreenOp::Select),
            (egui::Key::Space, ScreenOp::Select),
            (egui::Key::Escape, ScreenOp::Cancel),
        ];
        let mut out = vec![];
        for (k, op) in keys {
            if pressed(k) {
                out.extend(self.op(op, data));
            }
        }
        out
    }

    /// One operation of the pad's on the screen; what it asks of the HUD comes back.
    pub fn op(&mut self, op: ScreenOp, data: &ScreenData) -> Vec<ScreenEvent> {
        let mut out = vec![];
        // an item's menu is up: it takes the moves, A and B (screens.ts `input`)
        if let Some(m) = &mut self.menu {
            let n = m.rows.len().max(1);
            match op {
                ScreenOp::Next => m.focus = (m.focus + 1) % n,
                ScreenOp::Prev => m.focus = (m.focus + n - 1) % n,
                ScreenOp::Select => {
                    let m = self.menu.take().unwrap();
                    out.push(menu_event(&m, m.focus));
                }
                ScreenOp::Cancel => self.menu = None,
                _ => {}
            }
            return out;
        }
        match op {
            ScreenOp::Cancel => out.push(ScreenEvent::Close),
            ScreenOp::Next => self.step(0, 1),
            ScreenOp::Prev => self.step(0, -1),
            ScreenOp::Left => self.step(-1, 0),
            ScreenOp::Right => self.step(1, 0),
            ScreenOp::PageNext | ScreenOp::PagePrev | ScreenOp::First | ScreenOp::Last => {
                if self.stops.is_empty() {
                    return out;
                }
                let at = self.focus.as_ref().and_then(|f| self.stops.iter().position(|s| &s.id == f)).unwrap_or(0);
                let last = self.stops.len() - 1;
                let to = match op {
                    ScreenOp::PageNext => (at + 10).min(last),
                    ScreenOp::PagePrev => at.saturating_sub(10),
                    ScreenOp::First => 0,
                    _ => last,
                };
                self.focus = Some(self.stops[to].id.clone());
            }
            ScreenOp::Select => {
                if let Some(f) = self.focus.clone() {
                    out.extend(self.activate(&f, data));
                }
            }
            ScreenOp::Quick | ScreenOp::Describe => {
                let item = self.focus.as_deref().and_then(|f| item_of_stop(f, data));
                if let Some(item) = item {
                    let id = if op == ScreenOp::Describe {
                        Some(ItemActionId::Describe)
                    } else {
                        item_actions(item, data.worn.get(&item.letter).copied()).first().map(|a| a.id)
                    };
                    if let Some(id) = id {
                        out.push(ScreenEvent::Act(item.clone(), id));
                    }
                }
            }
        }
        out
    }

    /// An item's menu is open on the screen.
    pub fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    /// The stop the cursor is on (its id: `item:<slot>`, `place:<place>`, `cmd:<key>`).
    pub fn focused(&self) -> Option<&str> {
        self.focus.as_deref()
    }

    /// The pad's buttons for the stop the cursor is on, for the screen's foot (overlays.ts
    /// `paintScreenPad`): A, and Y and X where the stop has them, and B.
    pub fn pad_chips(&self, data: &ScreenData) -> Vec<(Glyph, String)> {
        let mut out = vec![];
        if self.menu.is_some() {
            out.push((Glyph::A, "Select".to_string()));
            out.push((Glyph::B, "Back".to_string()));
            return out;
        }
        let item = self.focus.as_deref().and_then(|f| item_of_stop(f, data));
        if self.focus.is_some() {
            out.push((Glyph::A, if item.is_some() { "Actions" } else { "Select" }.to_string()));
        }
        if let Some(item) = item {
            if let Some(a) = item_actions(item, data.worn.get(&item.letter).copied()).first() {
                out.push((Glyph::Y, a.label.clone()));
            }
            out.push((Glyph::X, "Describe".to_string()));
        }
        out.push((Glyph::B, "Close".to_string()));
        out
    }

    /// The cursor to the nearest stop that way (overlays.ts `focusSlots`, by rows and columns).
    fn step(&mut self, dx: i32, dy: i32) {
        let Some(cur) = self.focus.as_ref().and_then(|f| self.stops.iter().find(|s| &s.id == f)).cloned() else {
            self.focus = self.stops.first().map(|s| s.id.clone());
            return;
        };
        let c = cur.rect.center();
        let best = self
            .stops
            .iter()
            .filter(|s| {
                let d = s.rect.center() - c;
                if dx != 0 {
                    d.x * dx as f32 > 1.0 && d.y.abs() < d.x.abs() * 2.0 + 10.0
                } else {
                    d.y * dy as f32 > 1.0
                }
            })
            .min_by(|a, b| {
                let da = a.rect.center() - c;
                let db = b.rect.center() - c;
                let wa = if dx != 0 { da.x.abs() + da.y.abs() * 3.0 } else { da.y.abs() + da.x.abs() * 0.5 };
                let wb = if dx != 0 { db.x.abs() + db.y.abs() * 3.0 } else { db.y.abs() + db.x.abs() * 0.5 };
                wa.total_cmp(&wb)
            });
        if let Some(b) = best {
            self.focus = Some(b.id.clone());
        }
        let _ = (cur.row, cur.col);
    }

    fn focus_follows_pointer(&mut self, ctx: &egui::Context, pad: Option<PadKind>, pointed: Option<String>) {
        let touch = isorun_ui::pointer::touch_mode(ctx) && pad.is_none();
        if touch || self.menu.is_some() || ctx.input(|i| i.pointer.delta() == Vec2::ZERO) {
            return;
        }
        self.focus = pointed;
        self.scrolled_to = self.focus.clone();
    }

    fn activate(&mut self, id: &str, data: &ScreenData) -> Option<ScreenEvent> {
        if let Some(rest) = id.strip_prefix("cmd:") {
            return commands().into_iter().find(|c| c.key == rest).map(|c| ScreenEvent::Command(c.action));
        }
        if let Some(place) = id.strip_prefix("place:") {
            let p = DOLL_PLACES.iter().find(|p| p.id() == place)?;
            return Some(ScreenEvent::Fill(*p, place_label(*p)));
        }
        let item = item_of_stop(id, data)?;
        let rect = self.stops.iter().find(|s| s.id == id)?.rect;
        self.open_menu(item, rect, data);
        None
    }

    fn open_menu(&mut self, item: &PackItem, anchor: Rect, data: &ScreenData) {
        let mut rows: Vec<(String, Vec<String>, Option<ItemActionId>)> = item_actions(item, data.worn.get(&item.letter).copied())
            .into_iter()
            .map(|a| (a.label.clone(), item_action_keycaps(&a, &item.letter), Some(a.id)))
            .collect();
        if item.action_verb().is_some() {
            rows.push(("Add to quick slot".into(), vec![], None));
        }
        self.menu = Some(ItemMenu { item: item.clone(), anchor, rows, focus: 0 });
    }

    /// Draw the screen over the game and take its input.
    pub fn ui(&mut self, ctx: &egui::Context, data: &ScreenData, tiles: &mut TileIcons, boxes: &mut BoxCache, u: f32) -> Vec<ScreenEvent> {
        let screen = isorun_ui::safe::rect(ctx);
        let mut events = vec![];
        // the world under the panel takes no clicks while it is up
        let modal = egui::Area::new(egui::Id::unique("hud-screen-modal")).order(egui::Order::Foreground).fixed_pos(screen.min);
        modal.show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            let _ = rect;
            let _ = resp;
        });
        let area = egui::Area::new(egui::Id::unique("hud-screen")).order(egui::Order::Foreground).fixed_pos(screen.min).interactable(true);
        let shown = area.show(ctx, |ui| {
            ui.set_clip_rect(screen);
            events = match self.id {
                ScreenId::Inventory => self.inventory(ui, data, tiles, boxes, u, screen),
                ScreenId::Character => self.character(ui, data, tiles, boxes, u, screen),
            };
            if self.own_keys {
                let mut keyed = self.keys(ui, data);
                events.append(&mut keyed);
            }
        });
        // egui raises a layer that is pressed: the full-screen blocker under the panel would rise
        // over it at the first tap and take every later one; the panel stays on top
        ctx.move_to_top(shown.response.layer_id);
        // the pad's buttons under the panel, while the pad speaks
        if let (Some(kind), Some(panel)) = (data.pad, self.panel) {
            let chips = self.pad_chips(data);
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("hud-screen-pad")));
            pad_footer(&p, panel.center().x, panel.bottom() + 8.0 * u, u, kind, &chips);
        }
        events
    }
}

fn menu_event(m: &ItemMenu, i: usize) -> ScreenEvent {
    match m.rows.get(i).and_then(|r| r.2) {
        Some(a) => ScreenEvent::Act(m.item.clone(), a),
        None => ScreenEvent::Pin(m.item.clone()),
    }
}

/// The panel and its title bar: returns the body's top-left corner.
fn frame(ui: &mut egui::Ui, boxes: &mut BoxCache, rect: Rect, title: &str, u: f32) -> (Pos2, bool) {
    let painter = ui.painter().clone();
    boxes.paint_box(&painter, &format!("dock-screen:{}x{}", rect.width() as i32, rect.height() as i32), rect, &screen_style(u));
    let ts = TextStyle::new(Face::Caps700, 28.0 * u, GOLD_BRIGHT).spacing(0.12).line(1.1);
    let g = ts.galley(ui.ctx(), title, None);
    let bar_h = 12.0 * u + 28.0 * 1.1 * u + 10.0 * u;
    let bar = Rect::from_min_size(rect.min + egui::vec2(1.0, 1.0), egui::vec2(rect.width() - 2.0, bar_h));
    crate::paint::gradient_rect(&painter, bar, true, &[(0.0, rgba(60, 45, 20, 0.5)), (1.0, TRANSPARENT)]);
    painter.rect_filled(Rect::from_min_size(egui::pos2(bar.left(), bar.bottom()), egui::vec2(bar.width(), 1.0)), 0.0, hex(0x3a2d16).to_color32());
    let line = Rect::from_min_size(egui::pos2(rect.left() + rect.width() * 0.2, bar.bottom()), egui::vec2(rect.width() * 0.6, 1.0));
    crate::paint::gradient_rect(&painter, line, false, &[(0.0, TRANSPARENT), (0.5, GOLD_BRIGHT), (1.0, TRANSPARENT)]);
    crate::paint::shadowed_galley(
        &painter,
        egui::pos2(bar.center().x - g.size().x / 2.0, bar.top() + 12.0 * u),
        g,
        GOLD_BRIGHT.to_color32(),
        &[(0.0, 2.0, 6.0, Color32::BLACK)],
    );
    // the close button
    let cr = Rect::from_center_size(egui::pos2(bar.right() - 12.0 * u - 17.0 * u, bar.center().y), Vec2::splat(34.0 * u));
    let resp = ui.interact(cr, ui.scope_id().with("screen-close"), Sense::click());
    let hot = resp.hovered();
    boxes.paint_box(
        &painter,
        &format!("screen-close:{hot}"),
        cr,
        &BoxStyle {
            layers: vec![Layer::Linear(180.0, vec![sa(hex(0x2a221a)), sa(hex(0x120e0a))])],
            border: Border::all(1.0, if hot { GOLD_BRIGHT } else { hex(0x4f3e20) }),
            ..Default::default()
        }
        .round(17.0 * u),
    );
    painter.text(cr.center(), egui::Align2::CENTER_CENTER, "✕", style::font(Face::MonoBold, 15.0 * u), if hot { GOLD_BRIGHT } else { INK_DIM }.to_color32());
    (egui::pos2(rect.left() + 1.0, bar.bottom() + 1.0), resp.clicked())
}

/// The footer under a screen (overlays.ts `clientFooter`): "Enter Select · Esc Back", for the keyboard.
fn footer(painter: &Painter, boxes: &mut BoxCache, centre_x: f32, top: f32, u: f32) -> f32 {
    let ctx = painter.ctx();
    let st = TextStyle::new(Face::Caps600, 12.0 * u, hex(0x857759)).spacing(0.06).line(1.0);
    let parts: [(Option<&str>, &str); 4] = [(Some("Enter"), ""), (None, " Select · "), (Some("Esc"), ""), (None, " Back")];
    let kc = KeycapSize { h: 20.0 * u, min_w: 20.0 * u, pad: 5.0 * u, font: 11.0 * u, wide_font: 11.0 * u };
    let widths: Vec<f32> = parts
        .iter()
        .map(|(k, t)| if let Some(k) = k { keycaps_width(ctx, &[k.to_string()], kc, u) + 0.35 * 12.0 * u } else { st.galley(ctx, t, None).size().x })
        .collect();
    let total: f32 = widths.iter().sum();
    let mut x = centre_x - total / 2.0;
    let y = top + 4.0 * u;
    for ((k, t), w) in parts.iter().zip(widths) {
        if let Some(k) = k {
            paint_keycaps(painter, boxes, egui::pos2(x, y), &[k.to_string()], kc, u);
        } else {
            let g = st.galley(ctx, t, None);
            painter.galley(egui::pos2(x, y + 10.0 * u - g.size().y / 2.0), g, Color32::WHITE);
        }
        x += w;
    }
    4.0 * u + 20.0 * u
}

/// The pad's chips under a screen (styles.css `.screen-pad`): each button's glyph and what it does.
fn pad_footer(painter: &Painter, centre_x: f32, top: f32, u: f32, kind: PadKind, chips: &[(Glyph, String)]) {
    let ctx = painter.ctx();
    let em = 18.0 * u;
    let st = TextStyle::new(Face::Caps600, 12.0 * u, hex(0xc9b98f)).spacing(0.06).line(1.0);
    let gap = 18.0 * u;
    let laid: Vec<_> = chips.iter().map(|(g, t)| (*g, crate::glyphs::glyph_size(*g, em), st.galley(ctx, t, None))).collect();
    let total: f32 = laid.iter().map(|(_, gs, l)| gs.x + 6.0 * u + l.size().x).sum::<f32>() + gap * laid.len().saturating_sub(1) as f32;
    let mut x = centre_x - total / 2.0;
    let mid = top + em / 2.0;
    for (g, gs, l) in laid {
        crate::glyphs::paint_glyph(painter, g, kind, egui::pos2(x, mid - gs.y / 2.0), em, hex(0xefe4cc).to_color32());
        x += gs.x + 6.0 * u;
        let w = l.size().x;
        painter.galley(egui::pos2(x, mid - l.size().y / 2.0), l, Color32::WHITE);
        x += w + gap;
    }
}

impl Screen {
    #[allow(clippy::too_many_arguments)]
    fn inventory(&mut self, ui: &mut egui::Ui, data: &ScreenData, tiles: &mut TileIcons, boxes: &mut BoxCache, u: f32, screen: Rect) -> Vec<ScreenEvent> {
        let mut events = vec![];
        let ctx = ui.ctx().clone();
        let doll = paper_doll(data.items, data.worn);
        let on_doll: Vec<&str> = doll.values().map(|i| i.letter.as_str()).collect();
        let pack: Vec<&PackItem> = data.items.iter().filter(|i| !on_doll.contains(&i.letter.as_str())).collect();
        // the arrangement and the size: on a desktop the doll beside an eight-wide backpack, as the
        // web build has it. A compact window (a phone) takes the one that fits it largest: held
        // upright the panels one over the other with the backpack six wide (touch.css `@media
        // (max-width: 600px)`); on its side the doll beside a wider, shorter backpack with the
        // summary under it, since the height is what is short there.
        let compact = style::compact_window(screen.size());
        let margin = if compact { 4.0 } else { 0.0 };
        let avail = screen.shrink(margin);
        let mut layouts = vec![];
        if compact {
            layouts.push(InvLayout { arrange: InvArrange::Stacked, cols: 6 });
            for cols in 6..=13 {
                layouts.push(InvLayout { arrange: InvArrange::Beside, cols });
            }
        } else {
            layouts.push(InvLayout { arrange: InvArrange::Wide, cols: BACKPACK_COLUMNS });
        }
        let fit_of = |l: &InvLayout| {
            let (w, h) = l.size_at(1.0, pack.len());
            (avail.width() / w).min(avail.height() * if compact { 1.0 } else { 0.95 } / h)
        };
        let lay = layouts
            .iter()
            .copied()
            .max_by(|a, b| fit_of(a).min(u).total_cmp(&fit_of(b).min(u)).then(b.cols.cmp(&a.cols)))
            .unwrap_or(InvLayout { arrange: InvArrange::Wide, cols: BACKPACK_COLUMNS });
        let stacked = lay.arrange == InvArrange::Stacked;
        let side_by = lay.arrange == InvArrange::Beside;
        let cols = lay.cols;
        let rows = lay.rows(pack.len());
        let u = u.min(fit_of(&lay));
        // small labels (the letters, the counts) keep a size that reads on a phone
        let tu = u.max(SMALL_TEXT_UNIT);
        let place = 84.0 * u;
        let sock = 72.0 * u;
        let doll_w = place * 4.4 + 16.0 * u;
        let doll_h = 6.0 * place + 5.0 * 8.0 * u;
        let grid_w = cols as f32 * sock + (cols - 1) as f32 * 6.0 * u;
        let grid_h = rows as f32 * sock + (rows - 1) as f32 * 6.0 * u;
        let (paper_h, bag_h) = if stacked || side_by {
            (doll_h + 28.0 * u + 2.0, grid_h + 28.0 * u + 2.0)
        } else {
            (doll_h.max(grid_h) + 28.0 * u + 2.0, doll_h.max(grid_h) + 28.0 * u + 2.0)
        };
        let summary_h = 4.0 * u + 19.0 * u;
        let (width, height) = lay.size_at(u, pack.len());
        let top =
            if compact { avail.top() + ((avail.height() - height) / 2.0).clamp(0.0, avail.height() * 0.05) } else { screen.top() + screen.height() * 0.05 };
        let rect = Rect::from_min_size(egui::pos2((screen.center().x - width / 2.0).round(), top.round()), egui::vec2(width, height));
        self.panel = Some(rect);
        // a click on the panel is the panel's, not the world's
        ui.interact(rect, ui.scope_id().with("inv-panel"), Sense::click());
        let (body, closed) = frame(ui, boxes, rect, "Inventory", u);
        if closed {
            events.push(ScreenEvent::Close);
        }
        let painter = ui.painter().clone();
        let (paper, bag) = if stacked {
            let inner = width - 2.0 - 44.0 * u;
            let paper = Rect::from_min_size(
                body + egui::vec2(22.0 * u + (inner - (doll_w + 28.0 * u + 2.0)) / 2.0, 18.0 * u),
                egui::vec2(doll_w + 28.0 * u + 2.0, paper_h),
            );
            let bag = Rect::from_min_size(
                egui::pos2(body.x + 22.0 * u + (inner - (grid_w + 28.0 * u + 2.0)) / 2.0, paper.bottom() + 22.0 * u),
                egui::vec2(grid_w + 28.0 * u + 2.0, bag_h),
            );
            (paper, bag)
        } else {
            let paper = Rect::from_min_size(body + egui::vec2(22.0 * u, 18.0 * u), egui::vec2(doll_w + 28.0 * u + 2.0, paper_h));
            (paper, Rect::from_min_size(egui::pos2(paper.right() + 22.0 * u, paper.top()), egui::vec2(grid_w + 28.0 * u + 2.0, bag_h)))
        };
        boxes.paint_box(&painter, "inv-paper", paper, &panel_style(u, true, false));
        boxes.paint_box(&painter, "inv-bag", bag, &panel_style(u, false, false));
        let mut stops = vec![];
        let mut hover_tip: Option<(Vec<(Tip, CardKind)>, Rect)> = None;
        // a card beside what the mouse is over or the pad's cursor is on; on touch only while a
        // finger rests on it (a long press): a tap's pointer stays where it was as a hover, and
        // the cursor stays on the item tapped, which stood a card over the backpack after a tap
        let touch_cards = isorun_ui::pointer::touch_mode(&ctx) && data.pad.is_none();
        let card =
            |hovered: bool, focused: bool, rect: Rect| if touch_cards { isorun_ui::pointer::card_shows(&ctx, hovered, rect) } else { hovered || focused };
        let pad = data.pad;
        let mut pointed = None;

        // the doll: the figure in the middle, the places around it
        let dl = paper.min + egui::vec2(15.0 * u, 15.0 * u);
        let fig = Rect::from_min_size(dl + egui::vec2(place + 8.0 * u, 0.0), egui::vec2(place * 2.4, 5.0 * place + 4.0 * 8.0 * u));
        boxes.paint_box(
            &painter,
            "inv-figure",
            fig,
            &BoxStyle {
                layers: vec![Layer::Radial(RadialSize::Ellipse(0.45, 0.4), (0.5, 0.55), vec![sa(rgba(216, 180, 95, 0.14)), s(TRANSPARENT, 0.7)])],
                ..Default::default()
            },
        );
        self.paint_figure(&painter, tiles, data.me, Rect::from_center_size(fig.center(), Vec2::splat(192.0 * u)), u, true);
        for p in DOLL_PLACES {
            let (c, r) = place_at(p);
            let cell_x = match c {
                0 => dl.x,
                1 => dl.x + place + 8.0 * u,
                _ => dl.x + place + 8.0 * u + place * 2.4 + 8.0 * u,
            };
            let col_w = if c == 1 { place * 2.4 } else { place };
            let srect = Rect::from_min_size(egui::pos2(cell_x + (col_w - place) / 2.0, dl.y + r as f32 * (place + 8.0 * u)), Vec2::splat(place));
            let resp = ui.interact(srect, ui.scope_id().with(("doll", p.id())), Sense::click());
            let focused = self.focus.as_deref() == Some(&format!("place:{}", p.id()))
                || doll.get(&p).is_some_and(|i| self.focus.as_deref() == Some(&format!("item:{}", i.slot())));
            let hot = resp.hovered() || focused;
            if resp.hovered() {
                pointed = Some(doll.get(&p).map_or_else(|| format!("place:{}", p.id()), |i| format!("item:{}", i.slot())));
            }
            match doll.get(&p) {
                Some(item) => {
                    self.item_socket(&painter, boxes, tiles, srect, item, true, hot, u);
                    stops.push(Stop2 { id: format!("item:{}", item.slot()), rect: srect, row: r, col: c });
                    if card(resp.hovered(), focused, srect) && self.menu.is_none() {
                        hover_tip = Some((self.item_cards(item, data, &doll, &on_doll), srect));
                    }
                    if isorun_ui::pointer::tap(&ctx, resp.clicked()) || (resp.secondary_clicked() && !resp.long_touched()) {
                        self.focus = Some(format!("item:{}", item.slot()));
                        self.open_menu(item, srect, data);
                    }
                    if resp.double_clicked() {
                        self.menu = None;
                        if let Some(a) = item_actions(item, data.worn.get(&item.letter).copied()).first() {
                            events.push(ScreenEvent::Act((*item).clone(), a.id));
                        }
                    }
                }
                None => {
                    boxes.paint_box(&painter, &format!("socket-doll-empty:{hot}"), srect, &socket_style(u, None, true, hot));
                    if let Some(id) = icons::named(tiles, place_silhouette(p)) {
                        let placed = tiles.rect(id).map(|rect| vec![Placed { rect, ox: 0.0, oy: 0.0 }]).unwrap_or_default();
                        // `canvas.silhouette`: grey, lighter, a little sepia, faint
                        let tint = Color32::from_rgb(0xd8, 0xc8, 0xa8).gamma_multiply(if hot { 0.3 } else { 0.16 });
                        icons::paint(tiles, &painter, &placed, Rect::from_center_size(srect.center(), Vec2::splat(56.0 * u)), tint);
                    }
                    stops.push(Stop2 { id: format!("place:{}", p.id()), rect: srect, row: r, col: c });
                    if card(resp.hovered(), focused, srect) && self.menu.is_none() {
                        let t = Tip {
                            title: place_label(p).into(),
                            sub: Some("Nothing here".into()),
                            hints: vec![KeyHint::keys(&["Click"]).pad(Glyph::A).label("Put something on")],
                            ..Default::default()
                        };
                        hover_tip = Some((vec![(t, CardKind::Small)], srect));
                    }
                    if isorun_ui::pointer::tap(&ctx, resp.clicked()) {
                        events.push(ScreenEvent::Fill(p, place_label(p)));
                    }
                }
            }
        }

        // the backpack: a grid of sockets, the empty ones faint
        let gl = bag.min + egui::vec2(15.0 * u, 15.0 * u);
        for i in 0..rows * cols {
            let (r, c) = (i / cols, i % cols);
            let srect = Rect::from_min_size(gl + egui::vec2(c as f32 * (sock + 6.0 * u), r as f32 * (sock + 6.0 * u)), Vec2::splat(sock));
            let Some(item) = pack.get(i) else {
                let st = socket_style(u, None, false, false);
                let mut st = st;
                st.opacity = 0.5;
                boxes.paint_box(&painter, "socket-bag-empty", srect, &st);
                continue;
            };
            let resp = ui.interact(srect, ui.scope_id().with(("bag", i)), Sense::click());
            let focused = self.focus.as_deref() == Some(&format!("item:{}", item.slot()));
            let hot = resp.hovered() || focused;
            if resp.hovered() {
                pointed = Some(format!("item:{}", item.slot()));
            }
            self.item_socket(&painter, boxes, tiles, srect, item, false, hot, u);
            // stacked, the backpack's rows come after the doll's six
            let (row, col) = if stacked { (6 + r, c) } else { (r, 4 + c) };
            stops.push(Stop2 { id: format!("item:{}", item.slot()), rect: srect, row, col });
            if card(resp.hovered(), focused, srect) && self.menu.is_none() {
                hover_tip = Some((self.item_cards(item, data, &doll, &on_doll), srect));
            }
            if isorun_ui::pointer::tap(&ctx, resp.clicked()) || (resp.secondary_clicked() && !resp.long_touched()) {
                self.focus = Some(format!("item:{}", item.slot()));
                self.open_menu(item, srect, data);
            }
            if resp.double_clicked() {
                self.menu = None;
                if let Some(a) = item_actions(item, data.worn.get(&item.letter).copied()).first() {
                    events.push(ScreenEvent::Act((*item).clone(), a.id));
                }
            }
        }

        // the summary under the panels
        // beside the doll, the summary and the footer stand under the backpack
        let sy = if side_by { bag.bottom() } else { paper.bottom().max(bag.bottom()) } + 12.0 * u + 4.0 * u;
        let mut x = if side_by { bag.left() + 6.0 * u } else { body.x + 22.0 * u + 6.0 * u };
        let p = data.player;
        for (k, v) in [("AC", p.ac), ("EV", p.ev), ("SH", p.sh)] {
            let vg = TextStyle::new(Face::Caps700, 19.0 * u, hex(0xefe4cc)).line(1.0).galley(&ctx, &v.to_string(), None);
            let kg = TextStyle::new(Face::Caps700, 9.5 * u, hex(0x857759)).spacing(0.14).line(1.0).galley(&ctx, k, None);
            let vw = vg.size().x;
            let kw = kg.size().x;
            painter.galley(egui::pos2(x, sy), vg, Color32::WHITE);
            painter.galley(egui::pos2(x + vw + 6.0 * u, sy + 19.0 * u - 9.5 * u - 1.0 * u), kg, Color32::WHITE);
            x += vw + 6.0 * u + kw + 22.0 * u;
        }
        let gg = TextStyle::new(Face::Caps700, 16.0 * u, GOLD_BRIGHT).line(1.0).galley(&ctx, &p.gold.to_string(), None);
        let coin = 16.0 * u * 0.8;
        paint_coin(&painter, boxes, egui::pos2(x, sy + 9.5 * u - coin / 2.0), coin);
        let gw = gg.size().x;
        painter.galley(egui::pos2(x + coin + 0.4 * 16.0 * u, sy + 9.5 * u - gg.size().y / 2.0), gg, Color32::WHITE);
        x += coin + 0.4 * 16.0 * u + gw + 22.0 * u;
        if data.checking {
            let cg = ctx.fonts_mut(|f| {
                f.layout_no_wrap("reading what is worn".into(), egui::FontId::new(12.0 * u, egui::FontFamily::Monospace), hex(0x857759).to_color32())
            });
            painter.galley(egui::pos2(x, sy + 9.5 * u - cg.size().y / 2.0), cg, Color32::WHITE);
        }
        let slots = TextStyle::new(Face::Caps700, 13.0 * u, hex(0x857759)).spacing(0.08).line(1.0).galley(
            &ctx,
            &format!("{} / {PACK_LETTERS}", data.items.len()),
            None,
        );
        painter.galley(egui::pos2(bag.right() - 6.0 * u - slots.size().x, sy + 9.5 * u - slots.size().y / 2.0), slots, Color32::WHITE);
        if pad.is_none() {
            footer(&painter, boxes, if side_by { bag.center().x } else { rect.center().x }, sy + summary_h - 4.0 * u + 6.0 * u, u);
        }

        // the cards beside what the pointer is over, or the menu of what was picked
        let tip_painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("hud-screen-tip")));
        if let Some((cards, anchor)) = hover_tip {
            // the cards keep a size that reads: on a small screen they may stand over the panel's edge
            let laid: Vec<_> = cards.iter().map(|(t, k)| layout_card(&ctx, t, *k, tu, pad)).collect();
            let w: f32 = laid.iter().map(|c| c.size.x).sum::<f32>() + 8.0 * u * (laid.len().saturating_sub(1)) as f32;
            let h = laid.iter().map(|c| c.size.y).fold(0.0, f32::max);
            let at = beside(if compact { screen } else { rect }, anchor, egui::vec2(w, h), 10.0 * u);
            let mut x = at.x;
            for c in laid {
                c.paint(&tip_painter, boxes, egui::pos2(x, at.y), pad.unwrap_or_default());
                x += c.size.x + 8.0 * u;
            }
        }
        if self.menu.is_some() {
            let host = if compact { screen } else { rect };
            if let Some(e) = self.paint_menu(ui, &tip_painter, boxes, host, tu) {
                events.push(e);
            }
            // a press outside the menu puts it away
            // where the press began, not the hover: a finger has no hover but on its own press frame
            let pressed_at = isorun_ui::pointer::any_pressed_at(ui.ctx()).map(|(p, _)| p);
            if let Some(p) = pressed_at
                .filter(|p| !self.menu_rect(&ctx, host, tu).is_some_and(|r| r.contains(*p)) && !self.menu.as_ref().is_some_and(|m| m.anchor.contains(*p)))
            {
                let _ = p;
                self.menu = None;
            }
        }
        self.focus_follows_pointer(&ctx, pad, pointed);
        self.stops = stops;
        events
    }

    fn item_cards(&self, item: &PackItem, data: &ScreenData, doll: &BTreeMap<DollPlace, &PackItem>, on_doll: &[&str]) -> Vec<(Tip, CardKind)> {
        let slot = equip_slot_of(&item.item);
        let is_worn = on_doll.contains(&item.letter.as_str());
        let replaced = if !is_worn { slot.map(|s| worn_in(doll, s)).unwrap_or_default() } else { vec![] };
        let mut cards: Vec<(Tip, CardKind)> =
            replaced.iter().map(|w| (item_tip(w, data.worn.get(&w.letter).copied(), None, Some("Equipped"), false), CardKind::Equipped)).collect();
        cards.push((
            item_tip(item, data.worn.get(&item.letter).copied(), replaced.first().copied(), is_worn.then_some("Equipped"), true),
            if is_worn { CardKind::Equipped } else { CardKind::Plain },
        ));
        cards
    }

    #[allow(clippy::too_many_arguments)]
    fn item_socket(&self, painter: &Painter, boxes: &mut BoxCache, tiles: &mut TileIcons, rect: Rect, item: &PackItem, doll: bool, hot: bool, u: f32) {
        let picked = self.menu.as_ref().is_some_and(|m| m.item.slot() == item.slot());
        let c = colour(item.col());
        boxes.paint_box(painter, &format!("socket:{doll}:{}:{}", item.col(), hot || picked), rect, &socket_style(u, Some(c), doll, hot || picked));
        if picked {
            painter.rect_stroke(rect.expand(1.0), 4.0 * u, egui::Stroke::new(1.0, GOLD_BRIGHT.to_color32()), egui::StrokeKind::Inside);
        }
        let placed = icons::layer_tiles(tiles, &item_tiles(item));
        let tint = if item.useless() { Color32::from_gray(130) } else { Color32::WHITE };
        icons::paint_with_shadow(tiles, painter, &placed, Rect::from_center_size(rect.center(), Vec2::splat(56.0 * u)), tint, 2.0 * u, 2.0 * u, Color32::BLACK);
        if let Some(q) = item.count().filter(|q| *q > 1) {
            let g = TextStyle::new(Face::MonoBold, 14.0 * u.max(SMALL_TEXT_UNIT), INK).galley(painter.ctx(), &q.to_string(), None);
            let at = egui::pos2(rect.right() - 1.0 - 5.0 * u - g.size().x, rect.bottom() - 1.0 - 3.0 * u - g.size().y);
            crate::paint::shadowed_galley(painter, at, g, INK.to_color32(), &[(0.0, 0.0, 2.0, Color32::BLACK), (0.0, 1.0, 1.0, Color32::BLACK)]);
        }
        let lg = TextStyle::new(Face::Mono, 11.0 * u.max(SMALL_TEXT_UNIT), hex(0x7d6b48)).galley(painter.ctx(), &item.letter, None);
        crate::paint::shadowed_galley(
            painter,
            rect.min + egui::vec2(1.0 + 5.0 * u, 1.0 + 3.0 * u),
            lg,
            hex(0x7d6b48).to_color32(),
            &[(0.0, 1.0, 1.0, Color32::BLACK)],
        );
    }

    /// The player's figure, large: the doll composed at 32 texels, grown smooth, as the web
    /// build grows it with xBR and scales the rest of the way smoothly.
    fn paint_figure(&mut self, painter: &Painter, tiles: &mut TileIcons, me: Option<&Billboard>, rect: Rect, u: f32, _shadow: bool) {
        let Some(me) = me else { return };
        let placed = icons::doll_tiles(tiles, me.tile, &me.layers);
        // the drop shadow under it: `drop-shadow(0 10u 10u rgba(0,0,0,.9))`
        for (dx, dy, a) in [(0.0, 10.0 * u, 0.35), (4.0 * u, 10.0 * u, 0.2), (-4.0 * u, 10.0 * u, 0.2), (0.0, 14.0 * u, 0.2), (0.0, 6.0 * u, 0.2)] {
            icons::paint(tiles, painter, &placed, rect.translate(egui::vec2(dx, dy)), Color32::BLACK.gamma_multiply(a));
        }
        icons::paint(tiles, painter, &placed, rect, Color32::WHITE);
        let _ = &self.figure;
    }

    fn menu_rect(&self, ctx: &egui::Context, host: Rect, u: f32) -> Option<Rect> {
        let m = self.menu.as_ref()?;
        let (size, _) = menu_layout(ctx, m, u);
        Some(Rect::from_min_size(beside(host, m.anchor, size, 10.0 * u), size))
    }

    fn paint_menu(&mut self, ui: &mut egui::Ui, painter: &Painter, boxes: &mut BoxCache, host: Rect, u: f32) -> Option<ScreenEvent> {
        let ctx = ui.ctx().clone();
        let rect = self.menu_rect(&ctx, host, u)?;
        let m = self.menu.as_mut()?;
        let (_, rows) = menu_layout(&ctx, m, u);
        boxes.paint_box(
            painter,
            &format!("inv-menu:{}x{}", rect.width() as i32, rect.height() as i32),
            rect,
            &BoxStyle {
                layers: vec![Layer::Linear(180.0, vec![sa(hex(0x1d1915)), sa(hex(0x0b0908))])],
                radius: [4.0 * u; 4],
                border: Border::all(1.0, style::BRONZE),
                insets: vec![inset(0.0, 0.0, 0.0, 1.0, hex(0x000000))],
                outers: vec![outer(0.0, 10.0 * u, 30.0 * u, 0.0, rgba(0, 0, 0, 0.9))],
                ..Default::default()
            },
        );
        let title = TextStyle::new(Face::Caps700, 14.0 * u, colour(m.item.col())).line(1.2).galley(&ctx, &clean_title(&m.item), Some(320.0 * u));
        let mut y = rect.top() + 1.0 + 6.0 * u;
        let inner_l = rect.left() + 1.0 + 6.0 * u;
        let inner_w = rect.width() - 2.0 - 12.0 * u;
        let th = title.size().y;
        painter.galley(egui::pos2(inner_l + 10.0 * u, y + 6.0 * u), title, Color32::WHITE);
        y += 6.0 * u + th + 8.0 * u;
        painter.rect_filled(Rect::from_min_size(egui::pos2(inner_l, y), egui::vec2(inner_w, 1.0)), 0.0, hex(0x3a2d16).to_color32());
        y += 1.0 + 4.0 * u;
        let mut picked = None;
        for (i, (g, keys)) in rows.into_iter().enumerate() {
            let rh = 7.0 * u * 2.0 + 15.0 * 1.1 * u;
            let r = Rect::from_min_size(egui::pos2(inner_l, y), egui::vec2(inner_w, rh));
            let resp = ui.interact(r, ui.scope_id().with(("inv-menu-row", i)), Sense::click());
            if resp.hovered() {
                m.focus = i;
            }
            if m.focus == i {
                crate::paint::gradient_rect(painter, r, false, &[(0.0, rgba(216, 180, 95, 0.3)), (1.0, rgba(216, 180, 95, 0.06))]);
            }
            let c = if i == 0 { GOLD_BRIGHT } else { hex(0xd9ccb0) };
            painter.galley_with_override_text_color(egui::pos2(r.left() + 10.0 * u, r.center().y - g.size().y / 2.0), g, c.to_color32());
            if !keys.is_empty() {
                let kc = KeycapSize::base(u);
                let w = keycaps_width(&ctx, &keys, kc, u);
                paint_keycaps(painter, boxes, egui::pos2(r.right() - 10.0 * u - w, r.center().y - kc.h / 2.0), &keys, kc, u);
            }
            if resp.clicked() {
                picked = Some(i);
            }
            y += rh;
        }
        let m = self.menu.take()?;
        if let Some(i) = picked {
            return Some(menu_event(&m, i));
        }
        self.menu = Some(m);
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn character(&mut self, ui: &mut egui::Ui, data: &ScreenData, tiles: &mut TileIcons, boxes: &mut BoxCache, u: f32, screen: Rect) -> Vec<ScreenEvent> {
        let mut events = vec![];
        let ctx = ui.ctx().clone();
        let p = data.player;
        let body = 15.0 * u;
        let ink = |c: C| TextStyle::new(Face::Mono, body, c).line(LINE);
        // the blocks' rows, laid out to measure
        let h3 = TextStyle::new(Face::Caps700, 14.0 * u, GOLD).spacing(0.14).upper().line(1.2);
        let mods = |n: i64| {
            if n > 0 {
                format!(" +{n}")
            } else if n < 0 {
                format!(" {n}")
            } else {
                String::new()
            }
        };
        let attr = |now: i64, max: Option<i64>| -> (String, bool) {
            match max {
                Some(m) if now < m => (format!("{now} ({m})"), true),
                _ => (now.to_string(), false),
            }
        };
        let place = if p.depth != 0 { format!("{}:{}", p.place, p.depth) } else { p.place.clone() };
        let hp_max = if p.real_hp_max > p.hp_max { format!("{} ({})", p.hp_max, p.real_hp_max) } else { p.hp_max.to_string() };
        enum Line {
            Head(&'static str),
            Stat(&'static str, String, bool, bool),
            Text(String, bool),
            Status(String, C, String),
        }
        let vitals = vec![
            Line::Head("Vitals"),
            Line::Stat("Health", p.hp.to_string(), false, false),
            Line::Stat("Max health", hp_max, false, false),
            Line::Stat("Magic", p.mp.to_string(), false, false),
            Line::Stat("Max magic", p.mp_max.to_string(), false, false),
            Line::Stat("Gold", p.gold.to_string(), false, true),
            Line::Stat("Place", place, false, false),
            Line::Stat("Turns", p.turn.to_string(), false, false),
        ];
        let (st_v, sd) = attr(p.str, Some(p.str_max));
        let (i, id) = attr(p.int, Some(p.int_max));
        let (d, dd) = attr(p.dex, Some(p.dex_max));
        let defence = vec![
            Line::Head("Defence"),
            Line::Stat("Armour", format!("{}{}", p.ac, mods(p.ac_mod)), false, false),
            Line::Stat("Evasion", format!("{}{}", p.ev, mods(p.ev_mod)), false, false),
            Line::Stat("Shield", format!("{}{}", p.sh, mods(p.sh_mod)), false, false),
            Line::Head("Attributes"),
            Line::Stat("Strength", st_v, sd, false),
            Line::Stat("Intelligence", i, id, false),
            Line::Stat("Dexterity", d, dd, false),
        ];
        let mut arms = vec![
            Line::Head("Arms"),
            Line::Text(data.weapon.clone(), false),
            Line::Text(if data.quiver.is_empty() { "Nothing quivered".into() } else { data.quiver.clone() }, false),
            Line::Head("Status"),
        ];
        let lights: Vec<_> = p.status.iter().filter(|s| s.light.as_ref().is_some_and(|l| !l.is_empty())).collect();
        if lights.is_empty() {
            arms.push(Line::Text("Nothing out of the ordinary.".into(), true));
        }
        for st in lights {
            arms.push(Line::Status(
                st.light.clone().unwrap_or_default(),
                colour(st.col.unwrap_or(7)),
                st.desc.clone().filter(|d| !d.is_empty()).or_else(|| st.text.clone()).unwrap_or_default(),
            ));
        }
        // the commands: a grid of three columns as wide as the widest button
        let cmds = commands();
        let kc = KeycapSize::base(u);
        let keys_of = |c: &Command| c.key.split(' ').map(String::from).collect::<Vec<_>>();
        let cmd_fixed = |c: &Command| 6.0 * u + 32.0 * u + 10.0 * u + 10.0 * u + keycaps_width(&ctx, &keys_of(c), kc, u) + 10.0 * u + 2.0;
        let cmd_w = cmds.iter().map(|c| cmd_fixed(c) + ink(INK).galley(&ctx, c.label, None).size().x).fold(0.0, f32::max);
        let cmd_h = 5.0 * u * 2.0 + 32.0 * u + 2.0;
        let grid_w = 3.0 * cmd_w + 2.0 * 6.0 * u;
        // a compact window (a phone) gives the sheet its whole width less the margins, and what does
        // not fit beside each other stands one under the other
        let compact = style::compact_window(screen.size());
        let avail = if compact { screen.shrink(4.0) } else { screen };
        let side = if compact { 12.0 * u } else { 24.0 * u };
        let natural_inner = grid_w.max(900.0 * u);
        let width = if compact { (natural_inner + 2.0 * side + 2.0).min(avail.width()).floor() } else { natural_inner + 48.0 * u + 2.0 };
        let inner_w = width - 2.0 - 2.0 * side;
        // the head: the figure smaller on a narrow screen
        let fig_size = if inner_w < 560.0 * u { 120.0 * u } else { 180.0 * u };
        let name_w = inner_w - fig_size - 22.0 * u;
        let name_g = TextStyle::new(Face::Caps700, 36.0 * u, GOLD_BRIGHT).spacing(0.04).line(1.0).galley(&ctx, &p.name, Some(name_w));
        let title_g = TextStyle::new(Face::Caps600, 16.0 * u, INK).line(1.3).galley(&ctx, p.title.trim_start_matches(',').trim(), Some(name_w));
        let species = if p.species_display_name.is_empty() { &p.species } else { &p.species_display_name };
        let species_g = ink(INK_DIM).galley(&ctx, &format!("Level {} {species}", p.xl), Some(name_w));
        let xpg = TextStyle::new(Face::Mono, body * 0.9, INK_DIM).galley(&ctx, &format!("{}% to level {}", p.progress, p.xl + 1), None);
        let name_lines_h = name_g.size().y + 4.0 * u + title_g.size().y + species_g.size().y + body * LINE + 8.0 * u + body * LINE;
        let head_h = fig_size.max(name_lines_h);
        // the three blocks: side by side where they fit, else vitals and defence over the arms,
        // else one under the other
        let gap = 14.0 * u;
        let rows_of_blocks: Vec<Vec<(&Vec<Line>, f32)>> = if inner_w >= 0.85 * 900.0 * u {
            let w = [1.0, 1.0, 1.6].map(|f| (inner_w - 2.0 * gap) * f / 3.6);
            vec![vec![(&vitals, w[0]), (&defence, w[1]), (&arms, w[2])]]
        } else if (inner_w - gap) / 2.0 >= 190.0 * u {
            let w = (inner_w - gap) / 2.0;
            vec![vec![(&vitals, w), (&defence, w)], vec![(&arms, inner_w)]]
        } else {
            vec![vec![(&vitals, inner_w)], vec![(&defence, inner_w)], vec![(&arms, inner_w)]]
        };
        let line_h = |l: &Line, w: f32| -> f32 {
            match l {
                Line::Head(_) => 14.0 * 1.2 * u,
                Line::Stat(..) => 3.0 * u * 2.0 + body * LINE + 1.0,
                Line::Text(t, _) => 2.0 * u * 2.0 + ink(INK).galley(&ctx, &formatted_string_to_text(t), Some(w)).size().y,
                Line::Status(_, _, d) => 3.0 * u * 2.0 + ink(INK_DIM).galley(&ctx, d, Some(w * 0.6)).size().y.max(body * LINE),
            }
        };
        let block_h = |lines: &[Line], w: f32| -> f32 {
            let mut h = 12.0 * u * 2.0 + 2.0;
            for (k, l) in lines.iter().enumerate() {
                if matches!(l, Line::Head(_)) {
                    h += if k > 0 { 14.0 * u } else { 0.0 } + 8.0 * u;
                }
                h += line_h(l, w - 28.0 * u);
            }
            h
        };
        let block_rows_h: Vec<f32> = rows_of_blocks.iter().map(|r| r.iter().map(|(l, w)| block_h(l, *w)).fold(0.0, f32::max)).collect();
        let blocks_h = block_rows_h.iter().sum::<f32>() + gap * (block_rows_h.len() as f32 - 1.0);
        // the commands' columns: three where the widest fits, two with their labels wrapping, else one
        let cmd_cols = (1..=3usize)
            .rev()
            .find(|&n| {
                let cw = (inner_w - (n - 1) as f32 * 6.0 * u) / n as f32;
                cw >= cmd_w || (n == 2 && cw >= 170.0 * u)
            })
            .unwrap_or(1);
        let cw = (inner_w - (cmd_cols - 1) as f32 * 6.0 * u) / cmd_cols as f32;
        let cmd_labels: Vec<_> = cmds.iter().map(|c| ink(INK).galley(&ctx, c.label, Some((cw - cmd_fixed(c)).max(40.0 * u)))).collect();
        let rows_n = cmds.len().div_ceil(cmd_cols);
        let cmd_row_h: Vec<f32> = (0..rows_n)
            .map(|r| (r * cmd_cols..((r + 1) * cmd_cols).min(cmds.len())).map(|k| cmd_labels[k].size().y + 10.0 * u + 2.0).fold(cmd_h, f32::max))
            .collect();
        let cmds_h = cmd_row_h.iter().sum::<f32>() + (rows_n - 1) as f32 * 6.0 * u;
        let content_h = 18.0 * u + head_h + 14.0 * u + blocks_h + 14.0 * u + 14.0 * 1.2 * u + 14.0 * u + cmds_h + 8.0 * u;
        let title_h = 12.0 * u + 28.0 * 1.1 * u + 10.0 * u + 1.0;
        let footer_h = if data.pad.is_none() { 24.0 * u } else { 0.0 };
        let natural_h = 1.0 + title_h + content_h + footer_h + 6.0 * u + 1.0;
        // taller than the screen: the panel takes the height and its body scrolls
        let max_h = if compact { avail.height() } else { screen.height() * 0.95 };
        let height = natural_h.min(max_h).floor();
        let top =
            if compact { avail.top() + ((avail.height() - height) / 2.0).clamp(0.0, avail.height() * 0.05) } else { screen.top() + screen.height() * 0.05 };
        let rect = Rect::from_min_size(egui::pos2((screen.center().x - width / 2.0).round(), top.round()), egui::vec2(width, height));
        self.panel = Some(rect);
        ui.interact(rect, ui.scope_id().with("char-panel"), Sense::click());
        let (bodyp, closed) = frame(ui, boxes, rect, "Character", u);
        if closed {
            events.push(ScreenEvent::Close);
        }
        let view = Rect::from_min_max(egui::pos2(rect.left() + 1.0, bodyp.y), egui::pos2(rect.right() - 1.0, rect.bottom() - 1.0 - 6.0 * u - footer_h));
        self.scroll = isorun_ui::scroll::scroll(ui, view, content_h, self.scroll);
        let painter = ui.painter().with_clip_rect(view.intersect(screen));
        let left = rect.left() + 1.0 + side;
        let mut y = bodyp.y - self.scroll + 18.0 * u;
        // the figure in its ring
        let fig = Rect::from_min_size(egui::pos2(left, y + (head_h - fig_size) / 2.0), Vec2::splat(fig_size));
        painter.circle_filled(fig.center(), fig_size / 2.0 + 4.0 * u, TRIM.to_color32());
        painter.circle_filled(fig.center(), fig_size / 2.0 + 3.0 * u, hex(0x120e08).to_color32());
        boxes.paint_box(
            &painter,
            &format!("char-figure:{}", fig_size as i32),
            fig,
            &BoxStyle {
                layers: vec![Layer::Radial(RadialSize::CircleFarthest, (0.5, 0.55), vec![sa(rgba(216, 180, 95, 0.2)), s(rgba(0, 0, 0, 0.5), 0.7)])],
                border: Border::all(1.0, hex(0x4b3b1d)),
                insets: vec![inset(0.0, 0.0, 20.0, 0.0, hex(0x000000))],
                ..Default::default()
            }
            .round(fig_size / 2.0),
        );
        self.paint_figure(&painter, tiles, data.me, Rect::from_center_size(fig.center(), Vec2::splat(fig_size * 160.0 / 180.0)), u, false);
        let nx = fig.right() + 22.0 * u;
        let mut ny = fig.center().y - name_lines_h / 2.0;
        let nh = name_g.size().y;
        crate::paint::shadowed_galley(&painter, egui::pos2(nx, ny), name_g, GOLD_BRIGHT.to_color32(), &[(0.0, 2.0, 6.0, Color32::BLACK)]);
        ny += nh + 4.0 * u;
        let th = title_g.size().y;
        painter.galley(egui::pos2(nx, ny), title_g, Color32::WHITE);
        ny += th;
        let sh = species_g.size().y;
        painter.galley(egui::pos2(nx, ny), species_g, Color32::WHITE);
        ny += sh;
        // the god and the piety
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = name_w;
        if p.god.is_empty() {
            job.append("No god", 0.0, ink(INK_DIM).format());
        } else {
            job.append(&format!("of {} ", p.god), 0.0, ink(if p.penance { DANGER } else { INK_DIM }).format());
            if p.god != "Gozag" {
                let stars = "★".repeat(p.piety_rank.max(0) as usize) + &"☆".repeat((6 - p.piety_rank).max(0) as usize);
                job.append(&stars, 0.0, ink(GOLD_BRIGHT).spacing(0.1).format());
            }
        }
        let god_g = ctx.fonts_mut(|f| f.layout_job(job));
        let gh = god_g.size().y;
        painter.galley(egui::pos2(nx, ny), god_g, Color32::WHITE);
        ny += gh + 8.0 * u;
        let track_w = (240.0 * u).min(name_w - xpg.size().x - 10.0 * u).max(60.0 * u);
        let track = Rect::from_min_size(egui::pos2(nx, ny + (body * LINE * 0.9 - 9.0 * u) / 2.0), egui::vec2(track_w, 9.0 * u));
        painter.rect_filled(track, 4.0 * u, hex(0x090705).to_color32());
        let fill = Rect::from_min_size(
            track.min + egui::vec2(1.0, 1.0),
            egui::vec2((track.width() - 2.0) * (p.progress.clamp(0, 100) as f32 / 100.0), track.height() - 2.0),
        );
        if fill.width() > 0.0 {
            crate::paint::gradient_rect(&painter, fill, true, &[(0.0, hex(0xf1d58e)), (0.6, hex(0xc08d34)), (1.0, hex(0x8a611c))]);
        }
        painter.rect_stroke(track, 4.0 * u, egui::Stroke::new(1.0, hex(0x3e301a).to_color32()), egui::StrokeKind::Inside);
        painter.galley(egui::pos2(track.right() + 10.0 * u, track.center().y - xpg.size().y / 2.0), xpg, Color32::WHITE);
        y += head_h + 14.0 * u;
        // the blocks
        for (row, row_h) in rows_of_blocks.iter().zip(&block_rows_h) {
            let mut bx = left;
            for (lines, bw) in row {
                let (lines, bw) = (*lines, *bw);
                let br = Rect::from_min_size(egui::pos2(bx, y), egui::vec2(bw, *row_h));
                boxes.paint_box(&painter, &format!("char-block:{}x{}", bw as i32, *row_h as i32), br, &panel_style(u, false, true));
                let il = br.left() + 1.0 + 14.0 * u;
                let iw = bw - 2.0 - 28.0 * u;
                let mut ly = br.top() + 1.0 + 12.0 * u;
                for (k, l) in lines.iter().enumerate() {
                    match l {
                        Line::Head(t) => {
                            if k > 0 {
                                ly += 14.0 * u;
                            }
                            let g = h3.galley(&ctx, t, None);
                            let hh = g.size().y;
                            painter.galley(egui::pos2(il, ly), g, Color32::WHITE);
                            ly += hh + 8.0 * u;
                        }
                        Line::Stat(key, v, drained, gold) => {
                            let rh = 3.0 * u * 2.0 + body * LINE;
                            let kg = ink(INK_DIM).galley(&ctx, key, None);
                            let vc = if *drained {
                                DANGER
                            } else if *gold {
                                GOLD_BRIGHT
                            } else {
                                INK
                            };
                            let vg = TextStyle::new(Face::MonoBold, body, vc).line(LINE).galley(&ctx, v, None);
                            painter.galley(egui::pos2(il, ly + 3.0 * u), kg, Color32::WHITE);
                            let vx = il + iw - vg.size().x;
                            if *gold {
                                let coin = body * 0.8;
                                paint_coin(&painter, boxes, egui::pos2(vx - coin - 0.35 * body, ly + 3.0 * u + (body * LINE - coin) / 2.0), coin);
                            }
                            painter.galley(egui::pos2(vx, ly + 3.0 * u), vg, Color32::WHITE);
                            // the dotted rule under the row
                            let ry = ly + rh;
                            let mut dx = il;
                            while dx < il + iw {
                                painter.rect_filled(Rect::from_min_size(egui::pos2(dx, ry), egui::vec2(1.0, 1.0)), 0.0, rgba(184, 145, 63, 0.15).to_color32());
                                dx += 3.0;
                            }
                            ly += rh + 1.0;
                        }
                        Line::Text(t, dim) => {
                            let mut job = isorun_ui::text::formatted(t, body);
                            if *dim {
                                job = egui::text::LayoutJob::simple(t.clone(), egui::FontId::monospace(body), INK_DIM.to_color32(), iw);
                            }
                            job.wrap.max_width = iw;
                            let g = ctx.fonts_mut(|f| f.layout_job(job));
                            let gh = g.size().y;
                            painter.galley(egui::pos2(il, ly + 2.0 * u), g, Color32::WHITE);
                            ly += gh + 4.0 * u;
                        }
                        Line::Status(light, c, desc) => {
                            let lg = TextStyle::new(Face::MonoBold, body, *c).line(LINE).galley(&ctx, light, None);
                            let lw = lg.size().x;
                            let dg = TextStyle::new(Face::Mono, body * 0.9, INK_DIM).line(LINE).galley(&ctx, desc, Some(iw - lw - 10.0 * u));
                            let rh = lg.size().y.max(dg.size().y);
                            painter.galley(egui::pos2(il, ly + 3.0 * u), lg, Color32::WHITE);
                            painter.galley(egui::pos2(il + lw + 10.0 * u, ly + 3.0 * u), dg, Color32::WHITE);
                            ly += rh + 6.0 * u;
                        }
                    }
                }
                bx += bw + gap;
            }
            y += row_h + gap;
        }
        let more = h3.galley(&ctx, "More about you", None);
        let mh = more.size().y;
        painter.galley(egui::pos2(left, y), more, Color32::WHITE);
        y += mh + 14.0 * u;
        // the commands
        let mut stops = vec![];
        let mut pointed = None;
        let mut ry = y;
        for (r, rh) in cmd_row_h.iter().enumerate() {
            for col in 0..cmd_cols {
                let k = r * cmd_cols + col;
                let Some(c) = cmds.get(k) else { break };
                let br = Rect::from_min_size(egui::pos2(left + col as f32 * (cw + 6.0 * u), ry), egui::vec2(cw, *rh));
                let id = format!("cmd:{}", c.key);
                // only the part in sight takes the pointer
                let seen = br.intersect(view);
                let resp = ui.interact(if seen.is_positive() { seen } else { Rect::NOTHING }, ui.scope_id().with(("char-cmd", k)), Sense::click());
                let hot = resp.hovered() || self.focus.as_deref() == Some(id.as_str());
                boxes.paint_box(
                    &painter,
                    &format!("char-command:{hot}"),
                    br,
                    &BoxStyle {
                        layers: vec![Layer::Linear(180.0, vec![sa(hex(0x2c241b)), sa(hex(0x130f0b))])],
                        radius: [4.0 * u; 4],
                        border: Border::all(1.0, if hot { GOLD_BRIGHT } else { hex(0x5a4724) }),
                        ..Default::default()
                    },
                );
                let tile = Rect::from_min_size(egui::pos2(br.left() + 1.0 + 6.0 * u, br.center().y - 16.0 * u), Vec2::splat(32.0 * u));
                if let Some(id) = icons::named(tiles, c.tile) {
                    let placed = tiles.rect(id).map(|rect| vec![Placed { rect, ox: 0.0, oy: 0.0 }]).unwrap_or_default();
                    icons::paint(tiles, &painter, &placed, tile, Color32::WHITE);
                }
                let lg = cmd_labels[k].clone();
                painter.galley(egui::pos2(tile.right() + 10.0 * u, br.center().y - lg.size().y / 2.0), lg, Color32::WHITE);
                let keys = keys_of(c);
                let kw = keycaps_width(&ctx, &keys, kc, u);
                paint_keycaps(&painter, boxes, egui::pos2(br.right() - 1.0 - 10.0 * u - kw, br.center().y - kc.h / 2.0), &keys, kc, u);
                if resp.clicked() {
                    events.push(ScreenEvent::Command(c.action.clone()));
                }
                if resp.hovered() {
                    pointed = Some(id.clone());
                    resp.on_hover_text(c.sub);
                }
                stops.push(Stop2 { id, rect: br, row: r, col });
            }
            ry += rh + 6.0 * u;
        }
        self.scroll = isorun_ui::scroll::bar(ui, ui.scope_id().with("char-bar"), view, content_h, self.scroll);
        if data.pad.is_none() {
            footer(ui.painter(), boxes, rect.center().x, rect.bottom() - 1.0 - 6.0 * u - footer_h, u);
        }
        self.focus_follows_pointer(&ctx, data.pad, pointed);
        // the cursor moved to a stop out of sight: scroll it in (scrollIntoView, nearest)
        if self.focus != self.scrolled_to {
            if let Some(st) = stops.iter().find(|s| Some(&s.id) == self.focus.as_ref()) {
                if st.rect.top() < view.top() {
                    self.scroll -= view.top() - st.rect.top() + 6.0 * u;
                } else if st.rect.bottom() > view.bottom() {
                    self.scroll += st.rect.bottom() - view.bottom() + 6.0 * u;
                }
                self.scroll = self.scroll.clamp(0.0, (content_h - view.height()).max(0.0));
            }
            self.scrolled_to = self.focus.clone();
        }
        self.stops = stops;
        let _ = y;
        events
    }
}

/// The layout of an item menu: its size, and its rows' labels and keycaps.
fn menu_layout(ctx: &egui::Context, m: &ItemMenu, u: f32) -> (Vec2, Vec<(Arc<egui::Galley>, Vec<String>)>) {
    let title = TextStyle::new(Face::Caps700, 14.0 * u, INK).line(1.2).galley(ctx, &clean_title(&m.item), Some(320.0 * u));
    let st = TextStyle::new(Face::Caps600, 15.0 * u, hex(0xd9ccb0)).spacing(0.04).line(1.1);
    let kc = KeycapSize::base(u);
    let rows: Vec<(Arc<egui::Galley>, Vec<String>)> = m.rows.iter().map(|(l, k, _)| (st.galley(ctx, l, None), k.clone())).collect();
    let row_w = rows.iter().map(|(g, k)| 20.0 * u + g.size().x + if k.is_empty() { 0.0 } else { 20.0 * u + keycaps_width(ctx, k, kc, u) }).fold(0.0, f32::max);
    let w = (title.size().x + 20.0 * u).max(row_w).max(220.0 * u - 12.0 * u - 2.0) + 12.0 * u + 2.0;
    let h = 2.0 + 12.0 * u + 6.0 * u + title.size().y + 8.0 * u + 1.0 + 4.0 * u + rows.len() as f32 * (14.0 * u + 15.0 * 1.1 * u);
    (egui::vec2(w, h), rows)
}

/// Where a floating card or menu goes beside `anchor`: to its right if it fits in `host`, else to
/// its left; its top at the anchor's, kept inside the host (screens.ts `place`).
fn beside(host: Rect, anchor: Rect, size: Vec2, gap: f32) -> Pos2 {
    let right = anchor.right() + gap;
    let left = if right + size.x <= host.right() { right } else { (anchor.left() - gap - size.x).max(host.left()) };
    let top = anchor.top().min(host.bottom() - size.y).max(host.top());
    egui::pos2(left, top)
}

fn paint_coin(painter: &Painter, boxes: &mut BoxCache, at: Pos2, size: f32) {
    let r = Rect::from_min_size(at, Vec2::splat(size));
    painter.circle_filled(r.center(), size / 2.0 + 1.0, hex(0x3b2804).to_color32());
    boxes.paint_box(
        painter,
        "coin",
        r,
        &BoxStyle {
            layers: vec![Layer::Radial(RadialSize::CircleFarthest, (0.35, 0.35), vec![sa(hex(0xfff2b0)), s(hex(0xe0aa2c), 0.55), sa(hex(0x7b5208))])],
            ..Default::default()
        }
        .round(size / 2.0),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gear::tests::{item, pack};

    #[test]
    fn cards_name_and_compare() {
        let items = pack(&[item(0, "a +2 war axe of flaming {rF+}", obj::WEAPONS, 9), item(1, "a +0 hand axe", obj::WEAPONS, 7)]);
        let t = item_tip(&items[0], None, Some(&items[1]), None, true);
        assert_eq!(t.title, "+2 war axe of flaming");
        assert_eq!(t.sub.as_deref(), Some("Weapon"));
        assert_eq!(t.tags, vec![TipTag { text: "rF+".into(), tone: Some(crate::tooltip::Tone::Better) }]);
        assert_eq!(t.hints[1].label.as_deref(), Some("Wield"));
        let w = item_tip(&items[1], Some(Worn::Weapon), None, Some("Equipped"), false);
        assert_eq!(w.sub.as_deref(), Some("Weapon · wielded"));
        assert!(w.hints.is_empty());
    }

    #[test]
    fn floating_things_keep_to_the_panel() {
        let host = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 600.0));
        let a = Rect::from_min_size(egui::pos2(900.0, 550.0), egui::vec2(50.0, 50.0));
        assert_eq!(beside(host, a, egui::vec2(200.0, 100.0), 10.0), egui::pos2(690.0, 500.0));
    }

    #[test]
    fn a_screen_opens_with_nothing_selected() {
        // given
        let items = pack(&[item(0, "a +0 ring mail", obj::ARMOUR, 7)]);
        let player = PlayerState::default();
        let worn = BTreeMap::new();
        let data = ScreenData {
            player: &player,
            items: &items,
            worn: &worn,
            me: None,
            checking: false,
            pad: Some(PadKind::Xbox),
            weapon: String::new(),
            quiver: String::new(),
        };
        let mut s = Screen::new(ScreenId::Inventory);
        let at = |x: f32| Rect::from_min_size(egui::pos2(x, 0.0), egui::vec2(50.0, 50.0));
        s.stops = vec![Stop2 { id: "place:helmet".into(), rect: at(0.0), row: 0, col: 0 }, Stop2 { id: "item:0".into(), rect: at(100.0), row: 0, col: 1 }];

        // when
        let selected = s.op(ScreenOp::Select, &data);

        // then
        assert!(selected.is_empty());
        assert_eq!(s.focused(), None);
        s.op(ScreenOp::Right, &data);
        assert_eq!(s.focused(), Some("place:helmet"));
    }

    #[test]
    fn the_pad_walks_and_acts_on_a_screen() {
        let items = pack(&[item(0, "a +0 hand axe", obj::WEAPONS, 7), item(1, "a potion of curing", obj::POTIONS, 2)]);
        let player = PlayerState::default();
        let worn = BTreeMap::new();
        let data = ScreenData {
            player: &player,
            items: &items,
            worn: &worn,
            me: None,
            checking: false,
            pad: Some(PadKind::Xbox),
            weapon: String::new(),
            quiver: String::new(),
        };
        let mut s = Screen::new(ScreenId::Inventory);
        let at = |x: f32, y: f32| Rect::from_min_size(egui::pos2(x, y), egui::vec2(50.0, 50.0));
        s.stops = vec![
            Stop2 { id: "place:helmet".into(), rect: at(0.0, 0.0), row: 0, col: 0 },
            Stop2 { id: "item:0".into(), rect: at(100.0, 0.0), row: 0, col: 4 },
            Stop2 { id: "item:1".into(), rect: at(160.0, 0.0), row: 0, col: 5 },
        ];
        s.focus = Some("item:0".into());
        s.op(ScreenOp::Right, &data);
        assert_eq!(s.focused(), Some("item:1"));
        s.op(ScreenOp::First, &data);
        assert_eq!(s.focused(), Some("place:helmet"));
        s.op(ScreenOp::PageNext, &data);
        assert_eq!(s.focused(), Some("item:1"));
        // X describes, Y takes the first action, both without a menu
        assert_eq!(s.op(ScreenOp::Describe, &data), vec![ScreenEvent::Act(items[1].clone(), ItemActionId::Describe)]);
        assert!(matches!(s.op(ScreenOp::Quick, &data).as_slice(), [ScreenEvent::Act(_, _)]));
        // A opens the item's menu, which then takes the moves; B puts it away, B again the screen
        assert!(s.op(ScreenOp::Select, &data).is_empty());
        assert!(s.menu.is_some());
        s.op(ScreenOp::Next, &data);
        assert_eq!(s.menu.as_ref().unwrap().focus, 1);
        assert!(s.op(ScreenOp::Cancel, &data).is_empty());
        assert!(s.menu.is_none());
        assert_eq!(s.op(ScreenOp::Cancel, &data), vec![ScreenEvent::Close]);
        // A on an empty place asks for what goes there
        s.op(ScreenOp::First, &data);
        assert!(matches!(s.op(ScreenOp::Select, &data).as_slice(), [ScreenEvent::Fill(DollPlace::Helmet, _)]));
        assert_eq!(s.pad_chips(&data).first().map(|c| c.1.as_str()), Some("Select"));
    }
}
