//! The HUD as the game screen drives it: the dock or the classic panes over the view, the screens
//! of the dock, the damage numbers and the chat, laid out on the console grid (game.ts
//! `relayout`), with what the dock knows of the character (game.ts `updateDock`, `slotSources`,
//! `loadMemory`, `learnFromMenus`, `probeGear`, `dockButton`, `showInventory`, `itemAct`,
//! `fillPlace`, `trackResume`).

use crate::classic::{Classic, HoldProgress, InspectLine, PadPrompt, PaneInput, PaneLayout};
use crate::dock::{paint_tip, Dock, DockButtonId, DockEvent, DockFrame, DockView, NavOp};
use crate::dock_actions::{dock_actions, hostiles_in_view, underfoot, DockContext};
use crate::dock_memory::{load_dock_memory, save_dock_memory, DockMemory};
use crate::gear::{equip_slot_of, item_action_keys, item_title, pack_items, worn_from_menu, worn_now, DollPlace, EquipSlot, ItemActionId, KeyStep, PackItem};
use crate::glyphs::PadKind;
use crate::grid::console::{classic_split, fit, game_split, level_map_split, CellRect, GameLayout, Grid, STAT_WIDTH};
use crate::pack_tabs::is_pack;
use crate::paint::BoxCache;
use crate::quick_slots::{abilities_from_menu, quick_slots, slot_choices, spells_from_menu, QuickSlot, SlotRef, SlotSources, TileLayer};
use crate::screens::{Screen, ScreenData, ScreenEvent, ScreenId, ScreenOp};
use crate::{chat::Chat, damage_numbers::DamageNumbers, style};
use crate::{Action, Device, HudCommand, Mode};
use egui::{Pos2, Rect};
use isorun_client::servers::Settings;
use isorun_scene::{Dir8, Scene};
use isorun_ui::tiles::TileIcons;
use isorun_webtiles::{action_panel_items, cm, floor_items_label, formatted_string_to_text, GameState, MouseMode, Phase, UiState};

/// How long reading the pack behind the inventory waits for crawl (`probeGear`).
const GEAR_PROBE: f64 = 3.0;
/// How long a screen put away for an action waits to come back, and how long an action that
/// opened nothing is given (`trackResume`).
const RESUME: f64 = 20.0;
const RESUME_QUIET: f64 = 0.6;
/// a phone's message pane: this many rows, landscape and portrait
const COMPACT_MESSAGE_ROWS: (i32, i32) = (2, 4);
/// the most the view's centre is lifted over the compact dock, of the view's height
const MAX_CENTRE_LIFT: f32 = 0.15;
/// how much less the centre is lifted on a phone held upright, of the view's height: a tall
/// screen has room over the dock to spare, and the player stands better a little lower on it
const PORTRAIT_LIFT_DROP: f32 = 0.09;
/// DejaVu Sans Mono's advance per point of size
const MONO_ADVANCE: f32 = 0.602;

/// What the HUD is drawn from this frame and where its asks go.
pub struct HudCx<'a> {
    pub state: &'a GameState,
    pub scene: &'a Scene,
    pub tiles: &'a mut TileIcons,
    pub settings: &'a Settings,
    /// seconds on a monotonic clock
    pub now: f64,
    /// what crawl is doing (the game screen's context); `guess_mode` reads it off the state
    pub mode: Mode,
    pub device: Device,
    pub pad: PadKind,
    /// a coarse pointer: no hover cards, the touch layout's corners
    pub touch: bool,
    /// the camera's facing, for the monster list's arrows
    pub facing: Dir8,
    /// the level map is open
    pub map_view: bool,
    /// the dock's actions are chosen from this; None reads it off the scene
    pub dock_context: Option<DockContext>,
    /// the pad's prompts for the corner, and its menu buttons for the bottom corners
    pub prompts: &'a [PadPrompt],
    pub menu_prompts: &'a [PadPrompt],
    pub holding: Option<HoldProgress>,
    /// a panel of the client's own is up, its rect: the prompts stand under it
    pub panel: Option<Rect>,
    /// the card about the cell under the pointer
    pub inspect: Option<(&'a [InspectLine], &'a [String])>,
    /// the chat box is on: not on a game on this device, with no one to talk to (game.ts `chatOn`)
    pub chat: bool,
    /// a client overlay of the game screen's is up (game.ts `overlays.hasClientOverlay`)
    pub client_overlay: bool,
    /// where on the screen a cell's actor stands, for the damage numbers
    pub screen_of: &'a dyn Fn(i32, i32) -> Option<Pos2>,
    /// messages for crawl (`cm::*`, `key_message`)
    pub out: &'a mut Vec<serde_json::Value>,
    /// asks of the game screen
    pub commands: &'a mut Vec<HudCommand>,
}

/// The screen as the HUD lays it out (game.ts `relayout`).
#[derive(Clone, Copy, Debug)]
pub struct HudLayout {
    pub grid: Grid,
    pub cells: GameLayout,
    /// where the view is drawn: the whole grid (the level map keeps to its clear part)
    pub view: Rect,
    /// the part of the view no pane covers
    pub clear: Rect,
    /// the dock is up, and its height with the row of actions over it
    pub dock: bool,
    pub dock_height: f32,
    /// `IsoRender3d.setCentreLift`: how far the view's centre is lifted over the compact dock
    pub centre_lift: f32,
    /// the minimap's slot (the 2D renderer draws there); empty with no sidebar
    pub minimap: Rect,
    /// the sidebar's column, whose foot the chat stands at
    pub sidebar: Rect,
    pub panes: PaneLayout,
}

/// What the HUD did this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct HudOutput {
    /// the HUD took this frame's keys (a screen is up, or `i` and `@` opened one): none go to crawl
    pub consumed_keys: bool,
    /// a screen of the dock is up
    pub screen_open: bool,
    /// crawl's pack is up only to be read: the game screen does not draw it (`hideMenu`)
    pub hide_menu: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Probe {
    Open(f64),
    Closing(f64),
}

/// A choice the HUD offers through the game screen's chooser (Overlays.showChoices): the quick
/// slot's pin, an empty place of the doll's item.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub label: String,
    pub sub: Option<String>,
    pub tile: Option<&'static str>,
    pub tiles: Vec<TileLayer>,
    pub sep: bool,
}

/// What a list of choices is for, so `choose` knows what a pick does.
#[derive(Clone, Debug, PartialEq)]
enum ChoiceFor {
    Slot(usize, Vec<Option<SlotRef>>),
    Fill(Vec<PackItem>),
}

pub struct Hud {
    pub dock: Dock,
    pub classic: Classic,
    pub damage: DamageNumbers,
    pub chat: Chat,
    boxes: BoxCache,
    screen: Option<Screen>,
    memory: DockMemory,
    character: String,
    memory_rev: u64,
    fresh: bool,
    fresh_rev: u64,
    menus_read_rev: u64,
    probe: Option<Probe>,
    resume: Option<Resume>,
    slots: Vec<Option<QuickSlot>>,
    view: DockView,
    layout: Option<HudLayout>,
    choices: Option<ChoiceFor>,
    fonts_checked: bool,
    /// the pad's operations on the screen up, carried out in the next `ui`
    screen_ops: Vec<ScreenOp>,
    /// the screens read the keyboard themselves; a game screen with a router turns it off and
    /// hands them `screen_op`s
    pub screens_read_keys: bool,
}

/// A screen put away for crawl to take an action, to come back once crawl is done with it
/// (game.ts `trackResume`).
#[derive(Clone, Debug)]
struct Resume {
    id: ScreenId,
    at: f64,
    /// crawl has left command mode for it (a menu, a prompt, a description)
    left: bool,
    /// for an item's action: what must hold for the inventory to come back (None: it comes back)
    watch: Option<ResumeWatch>,
}

/// Whether an item's action from the inventory brings the inventory back once crawl is done:
/// putting on and taking off (wear, wield, put on; take off, unwield, remove) and reading the
/// description leave the player among their things. Using the item up or starting something in
/// the dungeon (read, quaff, eat, evoke, zap, throw or fire, all `Use`) and dropping it do not:
/// the screen stays away, and whatever crawl asks next (an aim, a scroll's menu) is shown.
pub fn resumes_after(id: ItemActionId) -> bool {
    matches!(id, ItemActionId::Equip | ItemActionId::Unequip | ItemActionId::Describe)
}

/// What an item's action that brings the inventory back is watched for: it does not come back
/// after one that led to an aim, a level change or a fight, nor after a description from which
/// the item was used (a key of its own: the description's `r`, `q`, `d`), seen as crawl opening
/// a menu or a prompt from it or as the item gone or fewer.
#[derive(Clone, Debug, PartialEq)]
pub struct ResumeWatch {
    pub id: ItemActionId,
    pub letter: String,
    /// the item's count at the letter (None: no item there)
    pub count: Option<i64>,
    pub hp: i64,
    pub place: (String, i64),
    pub hostiles: usize,
}

impl ResumeWatch {
    pub fn of(id: ItemActionId, letter: String, st: &GameState, scene: &Scene) -> ResumeWatch {
        let count = pack_items(&st.player.inv).iter().find(|i| i.letter == letter).map(|i| i.count().unwrap_or(1));
        ResumeWatch { id, letter, count, hp: st.player.hp, place: (st.player.place.clone(), st.player.depth), hostiles: hostiles_in_view(scene) }
    }

    /// Whether crawl's mode while the screen is away keeps it coming back. An aim never does. A
    /// description shows as a popup; a menu or a prompt over it is something done from it. Putting
    /// on and taking off may go through crawl's own menus and prompts (the equip menu, which ring
    /// to take off, a confirmation), which are part of the action.
    pub fn allows(&self, mode: Mode) -> bool {
        match mode {
            Mode::Command | Mode::Popup => true,
            Mode::Targeting | Mode::LevelMap | Mode::Other => false,
            Mode::Menu | Mode::Prompt => self.id != ItemActionId::Describe,
        }
    }

    /// Whether the inventory comes back, `now` taken when crawl is done: on the same level, no
    /// fight begun (a hostile more in view, or hit points lost with one there), and after a
    /// description the item still there as it was.
    pub fn still(&self, now: &ResumeWatch) -> bool {
        let fight = now.hostiles > self.hostiles || (now.hostiles > 0 && now.hp < self.hp);
        let used = self.id == ItemActionId::Describe && now.count != self.count;
        now.place == self.place && !fight && !used
    }
}

impl Default for Hud {
    fn default() -> Self {
        Self::new()
    }
}

/// What crawl is doing, read off the state, for a game screen without a context of its own
/// (context.ts `deriveMode`, in short).
pub fn guess_mode(st: &GameState) -> Mode {
    if st.ui_state == UiState::VIEW_MAP {
        return Mode::LevelMap;
    }
    if !st.menus.is_empty() {
        return Mode::Menu;
    }
    if !st.ui.is_empty() {
        return Mode::Popup;
    }
    match st.input_mode {
        MouseMode::COMMAND => Mode::Command,
        MouseMode::TARGET | MouseMode::TARGET_DIR | MouseMode::TARGET_PATH => Mode::Targeting,
        MouseMode::PROMPT | MouseMode::YESNO => Mode::Prompt,
        _ => Mode::Other,
    }
}

impl Hud {
    pub fn new() -> Hud {
        Hud {
            dock: Dock::new(),
            classic: Classic::new(),
            damage: DamageNumbers::default(),
            chat: Chat::new(),
            boxes: BoxCache::default(),
            screen: None,
            memory: DockMemory::default(),
            character: String::new(),
            memory_rev: 0,
            fresh: false,
            fresh_rev: u64::MAX,
            menus_read_rev: u64::MAX,
            probe: None,
            resume: None,
            slots: vec![],
            view: DockView::default(),
            layout: None,
            choices: None,
            fonts_checked: false,
            screen_ops: vec![],
            screens_read_keys: true,
        }
    }

    /// The layout of the screen: the grid, the panes on it, the dock (game.ts `relayout`). Call
    /// before drawing the view, so it knows its rect and its centre's lift.
    pub fn layout(&mut self, ctx: &egui::Context, screen: Rect, st: &GameState, settings: &Settings, map_view: bool) -> HudLayout {
        let o = &st.options;
        let opt = |k: &str| o.get(k).and_then(|v| v.as_bool()) == Some(true);
        let hide_sidebar = map_view && opt("tile_level_map_hide_sidebar");
        let hide_messages = map_view && opt("tile_level_map_hide_messages");
        // no character yet: the crt layer covers the sidebar while the new-game chooser is up
        let choosing = st.ui.last().is_some_and(|p| p.kind.starts_with("newgame"));
        let compact = style::compact_window(screen.size());
        let dock_on = settings.hud == "dock" && st.player.received && matches!(st.phase, Phase::Playing | Phase::Watching) && !map_view && !choosing;
        let hide_stats = hide_sidebar || st.ui_state == UiState::CRT || !st.player.received || dock_on || (map_view && compact);
        let grid = fit(screen.width(), screen.height(), (16.0 * settings.ui_scale).round(), MONO_ADVANCE);
        let mut grid = grid;
        grid.ox += screen.left();
        grid.oy += screen.top();
        let frame = DockFrame::new(screen, settings.ui_scale, false);
        let dock_height = if dock_on { self.dock.height(ctx, &frame, &self.view) } else { 0.0 };
        let pane = st.messages.pane_height as i32;
        let msg_rows = if compact { pane.min(if style::landscape_window(screen.size()) { COMPACT_MESSAGE_ROWS.0 } else { COMPACT_MESSAGE_ROWS.1 }) } else { pane };
        let cells = if settings.hud == "dock" { game_split(&grid, msg_rows, STAT_WIDTH, if dock_on { (dock_height / grid.ch).ceil() as i32 } else { 0 }) } else { classic_split(&grid, msg_rows, STAT_WIDTH) };
        let view_cells = if map_view { level_map_split(&grid, &cells, hide_sidebar, hide_messages) } else { cells.view };
        let view = grid.rect(view_cells);
        let clear = grid.rect(cells.clear);
        let centre_lift = if compact && !map_view { centre_lift(view.height(), clear.height(), style::landscape_window(screen.size())) } else { 0.0 };
        let free = if map_view { view_cells } else { cells.clear };
        let column = CellRect { h: cells.sidebar.h, ..cells.sidebar };
        let prompts = if dock_on && cells.sidebar.w > 0 { column } else { free };
        let panes = self.classic.layout(grid, cells, free, hide_stats, hide_sidebar, hide_messages || choosing, prompts, settings.minimap_tiles, settings.minimap_cell);
        let l = HudLayout { grid, cells, view, clear, dock: dock_on, dock_height, centre_lift, minimap: panes.minimap, sidebar: grid.rect(cells.sidebar), panes };
        self.layout = Some(l);
        l
    }

    /// A short line at the top of the screen for three seconds (hud.ts `status`).
    pub fn status(&mut self, text: impl Into<String>, now: f64) {
        self.classic.status(text, now);
    }

    /// Which screen is up.
    pub fn screen_open(&self) -> Option<ScreenId> {
        self.screen.as_ref().map(|s| s.id)
    }

    /// The panel of the screen that is up, as last drawn: a tap outside it puts the screen away.
    pub fn screen_panel(&self) -> Option<Rect> {
        self.screen.as_ref().and_then(|s| s.panel())
    }

    /// One of the pad's operations on the screen that is up (game.ts `pad` on a client overlay,
    /// the router's overlay ops): carried out in the next `ui`, where the screen's data is.
    /// Cancel with no item menu open puts the screen away at once.
    pub fn screen_op(&mut self, op: ScreenOp) {
        let Some(s) = self.screen.as_ref() else { return };
        if op == ScreenOp::Cancel && !s.menu_open() && self.screen_ops.is_empty() {
            self.close_screen();
            return;
        }
        self.screen_ops.push(op);
    }

    pub fn close_screen(&mut self) {
        self.screen = None;
    }

    /// What the dock knows of the character: its pins, spells, abilities and what it wears.
    pub fn memory(&self) -> &DockMemory {
        &self.memory
    }

    /// Draw the HUD over the view and take its input. Call `layout` first in the frame.
    pub fn ui(&mut self, ui: &mut egui::Ui, cx: &mut HudCx) -> HudOutput {
        let ctx = ui.ctx().clone();
        if !self.fonts_checked {
            style::ensure_fonts(&ctx);
            self.fonts_checked = true;
        }
        self.boxes.begin_frame();
        let st = cx.state;
        let screen = isorun_ui::safe::rect(&ctx);
        let Some(layout) = self.layout else { return HudOutput::default() };
        let watching = st.watching.is_some();
        let mut output = HudOutput::default();
        if !watching {
            self.load_memory(st);
            self.learn_from_menus(st);
        }
        self.track_probe(cx);
        self.track_resume(cx);

        // the classic panes, under the dock
        let pane_input = PaneInput {
            state: st,
            scene: cx.scene,
            facing: cx.facing,
            mode: cx.mode,
            device: cx.device,
            pad: cx.pad,
            spectating: watching,
            dock: cx.settings.hud == "dock",
            hints: cx.settings.hints != "off",
            prompts: cx.prompts,
            menu_prompts: cx.menu_prompts,
            holding: cx.holding,
            panel: cx.panel,
            inspect: cx.inspect,
            now: cx.now,
        };
        self.classic.ui(ui, &pane_input, cx.tiles, &mut self.boxes, cx.out, cx.commands);

        // the damage numbers over the actors
        let u = style::hud_unit(screen.size(), cx.settings.ui_scale);
        if st.player.received {
            self.damage.health(st.player.hp, st.player.hp_max, cx.scene.player, cx.now, cx.settings.damage_numbers);
        }
        let numbers = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::unique("hud-damage")));
        self.damage.paint(&numbers, cx.now, u, cx.screen_of);

        // the dock
        if layout.dock {
            self.update_view(cx);
            let frame = DockFrame::new(screen, cx.settings.ui_scale, cx.touch);
            let (events, tip) = self.dock.ui(ui, &frame, &self.view, cx.tiles, &mut self.boxes, cx.now);
            if let Some(t) = tip.filter(|_| self.screen.is_none()) {
                paint_tip(&ctx, &mut self.boxes, screen, &t, frame.u, (cx.device == Device::Pad).then_some(cx.pad));
            }
            for e in events {
                self.dock_event(e, cx);
            }
            // `i` and `@` open the dock's screens in command mode
            if cx.mode == Mode::Command && self.screen.is_none() && !cx.client_overlay && !self.chat.capturing(&ctx) {
                let typed: Vec<String> = ui.input(|i| i.events.iter().filter_map(|e| if let egui::Event::Text(t) = e { Some(t.clone()) } else { None }).collect());
                let mods = ui.input(|i| i.modifiers.ctrl || i.modifiers.alt || i.modifiers.command);
                if !mods {
                    if typed.iter().any(|t| t == "i") {
                        output.consumed_keys = true;
                        self.dock_button(DockButtonId::Inventory, cx);
                    } else if typed.iter().any(|t| t == "@") {
                        output.consumed_keys = true;
                        self.dock_button(DockButtonId::Character, cx);
                    }
                }
            }
        } else {
            self.dock.focus_off();
        }

        // the chat at the foot of the sidebar's column
        let in_game = cx.chat && matches!(st.phase, Phase::Playing | Phase::Watching);
        self.chat.ui(&ctx, st, layout.sidebar, in_game, st.lobby.username.is_some(), cx.out);
        if self.chat.capturing(&ctx) {
            output.consumed_keys = true;
        }

        // a screen of the dock over everything
        if let Some(mut screen_up) = self.screen.take() {
            let p = &st.player;
            let items = pack_items(&p.inv);
            let worn = worn_now(p, &items, &self.memory.worn);
            let me = crate::classic::player_billboard(cx.scene);
            let data = ScreenData { player: p, items: &items, worn: &worn, me, checking: self.probe.is_some(), pad: (cx.device == Device::Pad).then_some(cx.pad), weapon: crate::gear::weapon_line(p), quiver: p.quiver_desc.clone() };
            screen_up.own_keys = self.screens_read_keys;
            let mut events = vec![];
            for op in std::mem::take(&mut self.screen_ops) {
                events.extend(screen_up.op(op, &data));
            }
            events.extend(screen_up.ui(&ctx, &data, cx.tiles, &mut self.boxes, u));
            self.screen = Some(screen_up);
            for e in events {
                self.screen_event(e, cx);
            }
            output.consumed_keys = true;
        }
        if self.screen.is_none() {
            self.screen_ops.clear();
        }
        output.screen_open = self.screen.is_some();
        output.hide_menu = self.probe.is_some();
        output
    }

    /// The dock's view of the player, the actions and the slots (game.ts `updateDock`).
    fn update_view(&mut self, cx: &HudCx) {
        let st = cx.state;
        let watching = st.watching.is_some();
        self.slots = if watching { vec![] } else { quick_slots(&self.memory.pins, &self.slot_sources(st)) };
        let mut v = DockView::from_player(&st.player);
        if cx.mode == Mode::Command && !watching {
            let label = floor_items_label(st);
            let dctx = cx.dock_context.clone().unwrap_or_else(|| DockContext { under: underfoot(cx.scene, label.as_deref()), hostiles_in_view: hostiles_in_view(cx.scene) });
            v.actions = dock_actions(&dctx, cx.scene);
        }
        v.slots = self.slots.clone();
        v.spectating = watching;
        v.device = cx.device;
        v.pad = cx.pad;
        self.view = v;
    }

    /// What the quick slots can hold now (game.ts `slotSources`).
    fn slot_sources<'a>(&'a self, st: &GameState) -> SlotSources<'a> {
        let p = &st.player;
        let items = pack_items(&p.inv);
        let quiver_available = p.extra.get("quiver_available").is_some_and(isorun_webtiles::truthy);
        let quiver = if quiver_available { formatted_string_to_text(&p.quiver_desc) } else { String::new() };
        let consumables = action_panel_items(st).iter().filter_map(|c| items.iter().find(|i| i.item.slot == c.item.slot).cloned()).collect();
        let quiver_item = items.iter().find(|i| i.item.slot == p.quiver_item).cloned();
        SlotSources { spells: &self.memory.spells, abilities: &self.memory.abilities, consumables, items, quiver, quiver_item }
    }

    /// The character's dock memory, loaded once its name is known and again for another
    /// character; a new game (crawl's "Welcome," not "Welcome back,") starts empty.
    fn load_memory(&mut self, st: &GameState) {
        let p = &st.player;
        if !p.received || p.name.is_empty() {
            return;
        }
        let id = format!("{}|{}", p.name, p.species);
        if id != self.character {
            self.character = id.clone();
            self.memory = load_dock_memory(&id, p.turn);
            self.memory_rev += 1;
            self.fresh = false;
        }
        if self.fresh || p.turn > 0 || st.rev.messages == self.fresh_rev {
            return;
        }
        self.fresh_rev = st.rev.messages;
        let welcome = format!("Welcome, {} ", p.name);
        if !st.messages.lines.iter().any(|l| formatted_string_to_text(&l.text).starts_with(&welcome)) {
            return;
        }
        self.fresh = true;
        self.memory = DockMemory::default();
        self.remember(st);
    }

    fn remember(&mut self, st: &GameState) {
        self.memory.turn = st.player.turn;
        self.memory_rev += 1;
        if !self.character.is_empty() {
            save_dock_memory(&self.character, &self.memory);
        }
    }

    /// Crawl's menus as they come up: the spell and ability lists name what the quick slots can
    /// hold, and the pack's Gear page says what is worn.
    fn learn_from_menus(&mut self, st: &GameState) {
        if st.rev.ui == self.menus_read_rev {
            return;
        }
        self.menus_read_rev = st.rev.ui;
        let Some(menu) = st.menus.last() else { return };
        if self.character.is_empty() {
            return;
        }
        let mut changed = false;
        if let Some(s) = spells_from_menu(menu).filter(|s| *s != self.memory.spells) {
            self.memory.spells = s;
            changed = true;
        }
        if let Some(a) = abilities_from_menu(menu).filter(|a| *a != self.memory.abilities) {
            self.memory.abilities = a;
            changed = true;
        }
        if is_pack(menu) && formatted_string_to_text(&menu.title.text).starts_with("Gear") {
            let worn = worn_from_menu(&menu.items);
            if worn != self.memory.worn {
                self.memory.worn = worn;
                changed = true;
            }
        }
        if changed {
            self.remember(st);
        }
    }

    /// Read what is worn off crawl's own pack: `i`, read the Gear page when it is up, Escape.
    /// The pack is not drawn meanwhile (`hide_menu`).
    fn probe_gear(&mut self, cx: &mut HudCx) {
        if self.probe.is_some() || cx.state.watching.is_some() || guess_mode(cx.state) != Mode::Command {
            return;
        }
        self.probe = Some(Probe::Open(cx.now));
        cx.out.push(cm::input("i"));
    }

    fn track_probe(&mut self, cx: &mut HudCx) {
        let st = cx.state;
        let menu = st.menus.last();
        let Some(probe) = self.probe else { return };
        let mode = guess_mode(st);
        let t = match probe {
            Probe::Open(t) | Probe::Closing(t) => t,
        };
        if cx.now - t > GEAR_PROBE {
            self.probe = None;
        } else if let Probe::Open(t) = probe {
            if menu.is_some_and(is_pack) {
                self.menus_read_rev = u64::MAX;
                self.learn_from_menus(st);
                self.probe = Some(Probe::Closing(t));
                cx.out.push(cm::key(27));
            } else if mode != Mode::Command {
                self.probe = None;
            }
        } else if mode == Mode::Command && st.menus.is_empty() {
            self.probe = None;
        }
    }

    /// A screen put away for crawl to take an action comes back once crawl is done with it.
    /// An item's action brings the inventory back only while what crawl does for it stays what
    /// the action is (`ResumeWatch`).
    fn track_resume(&mut self, cx: &mut HudCx) {
        let Some(r) = self.resume.as_mut() else { return };
        if cx.now - r.at > RESUME || cx.client_overlay {
            self.resume = None;
            return;
        }
        let mode = guess_mode(cx.state);
        if mode != Mode::Command {
            r.left = true;
            if r.watch.as_ref().is_some_and(|w| !w.allows(mode)) {
                self.resume = None;
            }
            return;
        }
        if !r.left && cx.now - r.at < RESUME_QUIET {
            return;
        }
        let r = self.resume.take().expect("resume");
        if let Some(w) = &r.watch {
            let now = ResumeWatch::of(w.id, w.letter.clone(), cx.state, cx.scene);
            if !w.still(&now) {
                return;
            }
        }
        self.open_screen(r.id, cx);
    }

    fn open_screen(&mut self, id: ScreenId, cx: &mut HudCx) {
        if cx.state.watching.is_some() {
            return;
        }
        if cx.client_overlay {
            cx.commands.push(HudCommand::CloseClientOverlay);
        }
        if self.screen.as_ref().map(|s| s.id) != Some(id) {
            self.screen = Some(Screen::new(id));
        }
        if id == ScreenId::Inventory {
            self.probe_gear(cx);
        }
    }

    /// A dock button (game.ts `dockButton`).
    pub fn dock_button(&mut self, id: DockButtonId, cx: &mut HudCx) {
        self.dock.focus_off();
        if cx.state.watching.is_some() {
            return;
        }
        match id {
            DockButtonId::Inventory | DockButtonId::Character => {
                let sid = if id == DockButtonId::Inventory { ScreenId::Inventory } else { ScreenId::Character };
                if matches!(cx.mode, Mode::Menu | Mode::Popup) {
                    self.resume = Some(Resume { id: sid, at: cx.now, left: true, watch: None });
                    cx.commands.push(HudCommand::Run(Action::Keys { label: "Cancel".into(), seq: vec![KeyStep::key(27)] }));
                    return;
                }
                self.open_screen(sid, cx);
            }
            DockButtonId::Menu => {
                if cx.client_overlay {
                    cx.commands.push(HudCommand::CloseClientOverlay);
                }
                cx.commands.push(HudCommand::SystemMenu);
            }
            _ => {
                if !matches!(cx.mode, Mode::Command | Mode::Menu | Mode::Popup) {
                    return;
                }
                if cx.client_overlay {
                    cx.commands.push(HudCommand::CloseClientOverlay);
                }
                match id {
                    DockButtonId::Spells => cx.commands.push(HudCommand::ActionTab("spells")),
                    DockButtonId::Abilities => cx.commands.push(HudCommand::ActionTab("abilities")),
                    _ => {
                        let mut seq = if cx.mode == Mode::Command { vec![] } else { vec![KeyStep::key(27)] };
                        let (label, key) = if id == DockButtonId::Skills { ("Skills", "m") } else { ("Level map", "X") };
                        seq.push(KeyStep::text(key));
                        cx.commands.push(HudCommand::Run(Action::Keys { label: label.into(), seq }));
                    }
                }
            }
        }
    }

    fn dock_event(&mut self, e: DockEvent, cx: &mut HudCx) {
        match e {
            DockEvent::Button(id) => self.dock_button(id, cx),
            DockEvent::Action(a) => self.dock_command(cx, Some(HudCommand::Run(a))),
            DockEvent::Slot(i) => self.use_slot(i, cx),
            DockEvent::SlotAssign(i) => self.assign_slot(i, cx),
        }
    }

    /// A dock action: only where crawl takes commands, with no screen of ours over the dungeon.
    fn dock_command(&mut self, cx: &mut HudCx, c: Option<HudCommand>) {
        self.dock.focus_off();
        if cx.state.watching.is_some() || cx.mode != Mode::Command {
            return;
        }
        if cx.client_overlay {
            cx.commands.push(HudCommand::CloseClientOverlay);
        }
        self.screen = None;
        if let Some(c) = c {
            cx.commands.push(c);
        }
    }

    fn use_slot(&mut self, i: usize, cx: &mut HudCx) {
        let Some(slot) = self.slots.get(i).cloned().flatten() else { return self.assign_slot(i, cx) };
        let a = match &slot.r {
            SlotRef::Fire => Action::Fire,
            SlotRef::Item(s) => Action::Send(cm::inv_item_action(*s)),
            SlotRef::Spell(l) => Action::Keys { label: "Cast".into(), seq: vec![KeyStep::text("z"), KeyStep::text(l).awaiting()] },
            SlotRef::Ability(l) => Action::Keys { label: "Ability".into(), seq: vec![KeyStep::text("a"), KeyStep::text(l).awaiting()] },
        };
        self.dock_command(cx, Some(HudCommand::Run(a)));
    }

    /// A spell, an ability or an item onto the first quick slot the player has not pinned.
    fn pin_to_free_slot(&mut self, r: SlotRef, name: Option<String>, cx: &mut HudCx) {
        let mem = &mut self.memory;
        let same = mem.pins.iter().position(|p| p.as_ref() == Some(&r));
        let Some(at) = same.or_else(|| mem.pins.iter().position(|p| p.is_none())) else {
            self.classic.status("Every quick slot is taken: right click one to change it", cx.now);
            return;
        };
        mem.pins[at] = Some(r.clone());
        let known = match &r {
            SlotRef::Spell(l) => mem.spells.iter().find(|k| &k.letter == l).map(|k| k.name.clone()),
            SlotRef::Ability(l) => mem.abilities.iter().find(|k| &k.letter == l).map(|k| k.name.clone()),
            _ => None,
        };
        self.remember(cx.state);
        let name = name.or(known).unwrap_or_else(|| "It".into());
        self.classic.status(format!("{name} is on quick slot {}", (at + 1) % 10), cx.now);
    }

    /// Put something else on a quick slot: what it can hold, as a list, with "fill by itself"
    /// first (game.ts `assignSlot`). The list goes to the game screen's chooser; its pick comes
    /// back through `choose`.
    fn assign_slot(&mut self, i: usize, cx: &mut HudCx) {
        self.dock.focus_off();
        if cx.state.watching.is_some() {
            return;
        }
        let choices = slot_choices(&self.slot_sources(cx.state));
        let mut rows = vec![Choice { label: "Fill by itself".into(), sub: Some("the quiver, then your spells, abilities and consumables, in order".into()), tile: None, tiles: vec![], sep: false }];
        let mut refs = vec![None];
        for c in choices {
            rows.push(Choice { label: formatted_string_to_text(&c.label), sub: Some(c.group.into()), tile: None, tiles: c.tiles.clone(), sep: false });
            refs.push(Some(c.r));
        }
        rows.push(Choice { label: "Open the spell list".into(), sub: Some("the slots learn your spells from it".into()), tile: Some("CMD_CAST_SPELL"), tiles: vec![], sep: true });
        rows.push(Choice { label: "Open the abilities".into(), sub: Some("the slots learn your abilities from it".into()), tile: Some("CMD_USE_ABILITY"), tiles: vec![], sep: false });
        self.choices = Some(ChoiceFor::Slot(i, refs));
        cx.commands.push(HudCommand::Choices { title: format!("Quick slot {}", (i + 1) % 10), choices: rows, remember: "quick-slot" });
    }

    /// The row picked from the last list of choices the HUD asked for (`HudCommand::Choices`).
    pub fn choose(&mut self, row: usize, cx: &mut HudCx) {
        match self.choices.take() {
            Some(ChoiceFor::Slot(i, refs)) => {
                if row < refs.len() {
                    let r = refs[row].clone();
                    if let Some(r) = &r {
                        for p in self.memory.pins.iter_mut() {
                            if p.as_ref() == Some(r) {
                                *p = None;
                            }
                        }
                    }
                    self.memory.pins[i] = r;
                    self.remember(cx.state);
                } else if row == refs.len() {
                    self.dock_button(DockButtonId::Spells, cx);
                } else {
                    self.dock_button(DockButtonId::Abilities, cx);
                }
            }
            Some(ChoiceFor::Fill(items)) => {
                if let Some(item) = items.get(row).cloned() {
                    self.item_act(item, ItemActionId::Equip, cx);
                }
            }
            None => {}
        }
    }

    /// The chooser was put away without a pick: an empty place's list goes back to the inventory.
    pub fn choices_closed(&mut self, cx: &mut HudCx) {
        if let Some(ChoiceFor::Fill(_)) = self.choices.take() {
            self.open_screen(ScreenId::Inventory, cx);
        }
    }

    fn screen_event(&mut self, e: ScreenEvent, cx: &mut HudCx) {
        match e {
            ScreenEvent::Close => self.screen = None,
            ScreenEvent::Act(item, a) => self.item_act(item, a, cx),
            ScreenEvent::Pin(item) => {
                let name = item_title(item.name());
                self.pin_to_free_slot(SlotRef::Item(item.item.slot), Some(name), cx);
            }
            ScreenEvent::Fill(place, label) => self.fill_place(place, label, cx),
            ScreenEvent::Command(a) => {
                if cx.mode != Mode::Command {
                    return;
                }
                self.screen = None;
                self.resume = Some(Resume { id: ScreenId::Character, at: cx.now, left: false, watch: None });
                cx.commands.push(HudCommand::Run(a));
            }
        }
    }

    /// An empty place of the doll: what in the pack goes there, as a list to put one on; the
    /// quiver is crawl's own Quiver tab (game.ts `fillPlace`).
    fn fill_place(&mut self, place: DollPlace, label: &str, cx: &mut HudCx) {
        if place == DollPlace::Quiver {
            self.screen = None;
            cx.commands.push(HudCommand::ActionTab("quiver"));
            return;
        }
        let p = &cx.state.player;
        let items = pack_items(&p.inv);
        let worn = worn_now(p, &items, &self.memory.worn);
        let slot = place.slot();
        let fits: Vec<PackItem> = items.into_iter().filter(|i| equip_slot_of(&i.item) == slot && !worn.contains_key(&i.letter)).collect();
        if fits.is_empty() {
            self.classic.status(format!("Nothing in the pack goes on: {label}"), cx.now);
            return;
        }
        let rows = fits.iter().map(|i| Choice { label: format!("{} - {}", i.letter, item_title(i.name())), sub: None, tile: None, tiles: crate::quick_slots::item_tiles(i), sep: false }).collect();
        self.choices = Some(ChoiceFor::Fill(fits));
        self.screen = None;
        cx.commands.push(HudCommand::Choices { title: format!("{label}: put on"), choices: rows, remember: "fill-place" });
        let _ = EquipSlot::Ring;
    }

    /// An item's action from the inventory: the screen goes while crawl takes it, and comes back
    /// after for those that leave the player in the inventory (`resumes_after`; game.ts `itemAct`
    /// brought it back after every one).
    fn item_act(&mut self, item: PackItem, id: ItemActionId, cx: &mut HudCx) {
        if cx.state.watching.is_some() || self.probe.is_some() || guess_mode(cx.state) != Mode::Command {
            return;
        }
        self.screen = None;
        self.resume = resumes_after(id).then(|| Resume { id: ScreenId::Inventory, at: cx.now, left: false, watch: Some(ResumeWatch::of(id, item.letter.clone(), cx.state, cx.scene)) });
        match id {
            ItemActionId::Use => cx.commands.push(HudCommand::Run(Action::Send(cm::inv_item_action(item.item.slot)))),
            ItemActionId::Describe => cx.commands.push(HudCommand::Run(Action::Send(cm::inv_item_describe(item.item.slot)))),
            _ => {
                let Some((describe_first, seq)) = item_action_keys(id, &item) else { return };
                if describe_first {
                    cx.commands.push(HudCommand::Run(Action::SendThen(cm::inv_item_describe(item.item.slot), seq)));
                } else {
                    cx.commands.push(HudCommand::Run(Action::Keys { label: format!("{id:?}"), seq }));
                }
            }
        }
    }

    /// The pad's cursor onto the dock (L3), or a move of it; see `Dock::nav`.
    pub fn dock_focus(&mut self) {
        let v = self.view.clone();
        self.dock.focus_on(&v);
    }

    pub fn dock_nav(&mut self, op: NavOp, cx: &mut HudCx) {
        let v = self.view.clone();
        if let Some(e) = self.dock.nav(op, &v) {
            self.dock_event(e, cx);
        }
    }
}

/// How far the view's centre is lifted over a compact dock's panes: to the middle of the room above
/// them, but by no more than [`MAX_CENTRE_LIFT`] of the view, and upright by [`PORTRAIT_LIFT_DROP`]
/// of it less (the player stands lower on a tall phone, still over the dock).
pub fn centre_lift(view_h: f32, clear_h: f32, landscape: bool) -> f32 {
    let lift = ((view_h - clear_h) / 2.0).min(view_h * MAX_CENTRE_LIFT);
    if landscape {
        lift
    } else {
        (lift - view_h * PORTRAIT_LIFT_DROP).max(0.0)
    }
}

#[cfg(test)]
mod centre_lift_tests {
    use super::centre_lift;

    #[test]
    fn upright_the_centre_is_lifted_less_and_never_below_the_middle() {
        // a 915px tall phone with 600px clear over the dock: 137px lifted on its side, 55 upright
        assert_eq!(centre_lift(915.0, 600.0, true).round(), 137.0);
        assert_eq!(centre_lift(915.0, 600.0, false).round(), 55.0);
        // little dock: little lift, and none upright
        assert_eq!(centre_lift(915.0, 875.0, true), 20.0);
        assert_eq!(centre_lift(915.0, 875.0, false), 0.0);
    }
}

#[cfg(test)]
mod resume_tests {
    use super::{resumes_after, ResumeWatch};
    use crate::gear::ItemActionId;
    use crate::Mode;

    fn watch(id: ItemActionId) -> ResumeWatch {
        ResumeWatch { id, letter: "c".into(), count: Some(2), hp: 20, place: ("D".into(), 1), hostiles: 0 }
    }

    #[test]
    fn only_putting_on_taking_off_and_describing_come_back() {
        assert!(resumes_after(ItemActionId::Equip));
        assert!(resumes_after(ItemActionId::Unequip));
        assert!(resumes_after(ItemActionId::Describe));
        // read, quaff, eat, evoke, zap, throw, fire
        assert!(!resumes_after(ItemActionId::Use));
        assert!(!resumes_after(ItemActionId::Drop));
    }

    #[test]
    fn an_aim_a_menu_from_a_description_or_a_level_ends_it() {
        let wear = watch(ItemActionId::Equip);
        let describe = watch(ItemActionId::Describe);
        assert!(wear.allows(Mode::Popup) && wear.allows(Mode::Menu) && wear.allows(Mode::Prompt));
        assert!(!wear.allows(Mode::Targeting) && !wear.allows(Mode::LevelMap));
        assert!(describe.allows(Mode::Popup));
        assert!(!describe.allows(Mode::Menu) && !describe.allows(Mode::Prompt) && !describe.allows(Mode::Targeting));
        assert!(wear.still(&wear));
        assert!(!wear.still(&ResumeWatch { place: ("D".into(), 2), ..wear.clone() }));
    }

    #[test]
    fn a_fight_or_the_item_used_from_its_description_ends_it() {
        let wear = watch(ItemActionId::Equip);
        assert!(!wear.still(&ResumeWatch { hostiles: 1, ..wear.clone() }));
        let among = ResumeWatch { hostiles: 1, ..wear.clone() };
        assert!(!among.still(&ResumeWatch { hp: 15, ..among.clone() }));
        // poison ticking with nothing in view is no fight
        assert!(wear.still(&ResumeWatch { hp: 19, ..wear.clone() }));
        let describe = watch(ItemActionId::Describe);
        assert!(describe.still(&describe));
        assert!(!describe.still(&ResumeWatch { count: Some(1), ..describe.clone() }));
        assert!(!describe.still(&ResumeWatch { count: None, ..describe.clone() }));
        // wearing armour changes no count
        assert!(wear.still(&ResumeWatch { count: Some(1), ..wear.clone() }));
    }
}
