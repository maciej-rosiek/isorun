//! The screens crawl puts over the game (the port of overlays.ts): its menus, its popups (the
//! describe screens, the help and the other scrollers, character creation, the version and game
//! over screens), the CRT screens (the skills screen), its prompts (the yes/no and choice card, the
//! letter picker, the text field) and the level-up banner. See README.md for the API.

pub mod crt_scrape;
pub mod focus;
pub mod menu_nav;
pub mod prompt;
pub mod rich;

mod card;
mod crt;
mod frame;
mod input;
mod level_up;
mod menu;
mod newgame;
mod osk;
mod popup;
mod text_field;
mod tiles;

pub use focus::{Act, FocusInfo, FocusOp, Focusable};
pub use level_up::LevelUp;
pub use menu::MenuOp;
pub use prompt::{derive_mode, parse_prompt, Mode, ParsedPrompt};

use focus::{FocusNav, FocusOptions};
use isorun_ui::tiles::TileIcons;
use isorun_webtiles::{GameState, MenuState};
use serde_json::Value;
use std::collections::HashMap;

/// Who spoke last: the overlays that read both ways (a chip's key cap or the pad's glyph) show the one that fits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Device {
    #[default]
    Keyboard,
    Pad,
    Pointer,
}

/// What the game screen lends the overlays for one frame.
pub struct OverlayCx<'a> {
    pub state: &'a GameState,
    /// crawl's tiles, for the item icons in menus and the describe headers
    pub tiles: &'a mut TileIcons,
    /// the client messages the overlays send, in order (`isorun_webtiles::cm`, `key_message`)
    pub out: &'a mut Vec<Value>,
    /// a spectator sends nothing and answers no prompt
    pub watching: bool,
    pub device: Device,
    /// a coarse pointer (a touch screen): every popup gets its round close button
    pub touch: bool,
    /// the dock HUD is on (the default): crawl's popups wear its dark stone and gold trim
    pub dock: bool,
    /// the web's UI scale (`--ui-scale`): every size follows it
    pub ui_scale: f32,
    /// a menu of crawl's the client opened only to read it (game.ts `probeGear`): not drawn
    pub hide_menu: Option<&'a dyn Fn(&MenuState) -> bool>,
    /// what the phone's Back and a popup's close button do (game.ts `GameScreen.back`); None sends Escape
    pub back: Option<&'a mut dyn FnMut()>,
    /// the height a bar at the foot of the screen takes (the touch bar): the popups stand clear of it
    pub bottom: f32,
    /// crawl's text prompt is answered by the game itself (the Find screen answers crawl's stash
    /// search): its field is not drawn
    pub hide_text: bool,
}

impl<'a> OverlayCx<'a> {
    pub fn new(state: &'a GameState, tiles: &'a mut TileIcons, out: &'a mut Vec<Value>) -> Self {
        OverlayCx {
            state,
            tiles,
            out,
            watching: false,
            device: Device::Keyboard,
            touch: false,
            dock: true,
            ui_scale: 1.0,
            hide_menu: None,
            back: None,
            bottom: 0.0,
            hide_text: false,
        }
    }
    fn send(&mut self, m: Value) {
        if !self.watching {
            self.out.push(m);
        }
    }
    fn hidden(&self, m: &MenuState) -> bool {
        self.hide_menu.is_some_and(|f| f(m))
    }
}

/// A more-line switch made walkable: its key, its words, and where it is in the line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MoreSwitchAt {
    pub key: String,
    pub label: String,
    /// the line of the more text it is on, the character it starts at there, and its length
    pub line: usize,
    pub col: usize,
    pub len: usize,
}

/// The top menu's measured layout, from the last frame: what the paging needs.
#[derive(Clone, Debug, Default)]
pub(crate) struct MenuGeom {
    pub stack: i64,
    /// each row's top and height in the body's content, padding included
    pub rows: Vec<(f32, f32)>,
    pub client_h: f32,
    pub scroll_h: f32,
    pub inset: f32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PopupGeom {
    pub stack: i64,
    pub client_h: f32,
    pub scroll_h: f32,
    pub line_h: f32,
}

/// The screens crawl puts over the game.
pub struct Overlays {
    pub(crate) last_ui_rev: u64,
    pub(crate) nav: FocusNav,
    pub(crate) focusables: Vec<Focusable>,
    focus_screen: String,
    focus_opts: FocusOptions,
    focus_initial: Option<i32>,
    focus_force: bool,
    focus_gen: u64,
    focus_synced: u64,
    // the prompt card
    pub(crate) prompt_key: String,
    prompt_screen: String,
    pub(crate) prompt_focusables: Vec<Focusable>,
    prompt_initial: Option<i32>,
    prompt_focus_opts: FocusOptions,
    pub(crate) card: Option<card::Card>,
    /// newgame: the server's `button_focus` at the last build
    newgame_focus: String,
    /// the new-game stop last drawn lit: a move to another brings it into view
    pub(crate) newgame_lit: String,
    /// the columns character creation's grids were last drawn in (main, sub): the cursor's grid
    pub(crate) newgame_cols: (i32, i32),
    // menus
    /// the hover each menu (by its stack place) was moved to here, and the server's hover then:
    /// ours holds until the server moves its own
    pub(crate) hover: HashMap<i64, (i64, i64)>,
    pub(crate) scroll: HashMap<i64, f32>,
    pub(crate) menu_geom: MenuGeom,
    pub(crate) menu_footer: Vec<MoreSwitchAt>,
    pub(crate) footer_index: i32,
    footer_of: i64,
    game_menu_for: i64,
    game_menu_last: i64,
    /// a `menu_scroll` waiting for its 100 ms (menu.js update_server_scroll)
    pub(crate) scroll_sync_at: Option<f64>,
    /// a pending scroll of the top menu, applied when it is next drawn (in content px)
    pub(crate) pending_menu_scroll: Option<f32>,
    // popups
    pub(crate) popup_scroll: HashMap<i64, f32>,
    popup_scroll_applied: HashMap<i64, u64>,
    pub(crate) popup_geom: PopupGeom,
    pub(crate) scroller_report_at: Option<f64>,
    pub(crate) actions: Vec<(String, String, Vec<Value>)>,
    popup_ids: HashMap<i64, u64>,
    next_popup_id: u64,
    // the text field of a server prompt
    pub(crate) field: Option<text_field::Field>,
    /// the text of a menu's filter field (menu.js title_prompt)
    pub(crate) menu_filter: String,
    histories: HashMap<String, Vec<String>>,
    /// the level-up banner and the stat-gain cards
    pub level_up: LevelUp,
    /// the mouse's position still counts: a key or a pad button puts it to sleep until it moves
    pub(crate) pointer_live: bool,
    pointer_at: Option<egui::Pos2>,
    /// the pointer moved this frame: only a move takes the cursor (a row scrolled under a still mouse does not)
    pub(crate) pointer_moved: bool,
    pub(crate) now: f64,
    /// the pad's on-screen keyboard over the server's text field (osk.ts)
    pub(crate) osk: isorun_input::osk::Osk,
    /// who spoke last, as of the last sync: the pad taking over brings the keyboard
    pub(crate) device: Device,
}

impl Default for Overlays {
    fn default() -> Self {
        Self::new()
    }
}

/// The game menu (main.cc `GameMenu`, F1 and `~`): it opens again on the row it was left on.
pub(crate) const GAME_MENU_TAG: &str = "game_menu";

/// LB / RB on a new-game grid move the cursor this many rows.
const NEWGAME_PAGE_ROWS: i32 = 5;

impl Overlays {
    pub fn new() -> Self {
        Overlays {
            last_ui_rev: u64::MAX,
            nav: FocusNav::new(),
            focusables: Vec::new(),
            focus_screen: String::new(),
            focus_opts: FocusOptions::default(),
            focus_initial: None,
            focus_force: false,
            focus_gen: 0,
            focus_synced: u64::MAX,
            prompt_key: String::new(),
            prompt_screen: String::new(),
            prompt_focusables: Vec::new(),
            prompt_initial: None,
            prompt_focus_opts: FocusOptions::default(),
            card: None,
            newgame_focus: String::new(),
            newgame_lit: String::new(),
            newgame_cols: (i32::MAX, i32::MAX),
            hover: HashMap::new(),
            scroll: HashMap::new(),
            menu_geom: MenuGeom::default(),
            menu_footer: Vec::new(),
            footer_index: -1,
            footer_of: -1,
            game_menu_for: -1,
            game_menu_last: -1,
            scroll_sync_at: None,
            pending_menu_scroll: None,
            popup_scroll: HashMap::new(),
            popup_scroll_applied: HashMap::new(),
            popup_geom: PopupGeom::default(),
            scroller_report_at: None,
            actions: Vec::new(),
            popup_ids: HashMap::new(),
            next_popup_id: 0,
            field: None,
            menu_filter: String::new(),
            histories: HashMap::new(),
            level_up: LevelUp::default(),
            pointer_live: true,
            pointer_at: None,
            pointer_moved: false,
            now: 0.0,
            osk: Default::default(),
            device: Device::Keyboard,
        }
    }

    /// The mode the screen is in (context.ts `deriveMode`).
    pub fn mode(state: &GameState) -> Mode {
        derive_mode(state)
    }

    /// Whether anything of crawl's is over the game now: the keys are the overlays' while it is.
    pub fn up(state: &GameState) -> bool {
        matches!(derive_mode(state), Mode::Menu | Mode::Popup | Mode::Newgame | Mode::Crt | Mode::Yesno | Mode::Prompt | Mode::Dialog | Mode::Text)
    }

    /// The cursor as the bindings see it, in a mode that has one.
    pub fn focus_info(&self, state: &GameState) -> Option<FocusInfo> {
        let mode = derive_mode(state);
        mode.is_focus(state.menus.last().is_some_and(|m| m.kind.as_deref() == Some("crt"))).then(|| self.nav.info())
    }

    /// Actions the top popup offers, in pad order (A, X, Y, LT, RT): its key and its words.
    pub fn popup_actions(&self) -> Vec<(String, String)> {
        self.actions.iter().map(|a| (a.0.clone(), a.1.clone())).collect()
    }

    /// Fire the i-th popup action; false when there is none.
    pub fn trigger_popup_action(&mut self, i: usize, cx: &mut OverlayCx) -> bool {
        let Some(a) = self.actions.get(i).cloned() else { return false };
        for m in a.2 {
            cx.send(m);
        }
        true
    }

    /// Feed the message lines that arrived (the level-up banner reads "You have reached level N").
    pub fn messages(&mut self, lines: &[isorun_webtiles::GameMessage]) {
        self.level_up.messages(lines, self.now);
    }

    // ------------------------------------------------------------------ the focus layer

    /// Each popup's own number, for the cursor's screen.
    fn popup_id(&mut self, stack: i64) -> u64 {
        if let Some(&id) = self.popup_ids.get(&stack) {
            return id;
        }
        self.next_popup_id += 1;
        self.popup_ids.insert(stack, self.next_popup_id);
        self.next_popup_id
    }

    /// A renderer's focusables for the top overlay; the last registration of a build wins.
    fn register_focus(&mut self, screen: String, items: Vec<Focusable>, opts: FocusOptions, initial: Option<i32>, force: bool) {
        self.focusables = items;
        self.focus_screen = screen;
        self.focus_opts = opts;
        self.focus_initial = initial;
        self.focus_force = force;
        self.focus_gen += 1;
    }

    /// The state's ui changed: rebuild what the top overlay offers the cursor (overlays.ts `update`).
    fn rebuild(&mut self, cx: &mut OverlayCx) {
        let st = cx.state;
        self.actions.clear();
        self.focusables.clear();
        self.focus_screen.clear();
        self.focus_opts = FocusOptions::default();
        self.focus_initial = None;
        self.focus_force = false;
        self.focus_gen += 1;
        // forget what belongs to overlays that are gone
        let live: Vec<i64> = st.menus.iter().map(|m| m.stack_order).chain(st.ui.iter().map(|p| p.stack_order)).collect();
        self.hover.retain(|k, _| live.contains(k));
        self.scroll.retain(|k, _| live.contains(k));
        self.popup_scroll.retain(|k, _| live.contains(k));
        self.popup_scroll_applied.retain(|k, _| live.contains(k));
        self.popup_ids.retain(|k, _| live.contains(k));
        // the menus first, then the popups: the focus set is the last to register
        let visible: Vec<&MenuState> = st.menus.iter().filter(|m| !(m.tag == "prompt" && !cx.watching) && !cx.hidden(m)).collect();
        if let Some(top) = visible.last() {
            if top.kind.as_deref() == Some("crt") {
                let (screen, items, opts) = crt::focus(st, &top.tag);
                self.register_focus(screen, items, opts, None, false);
            } else {
                self.seat_menu(top, cx);
            }
        }
        if st.menus.last().map(|m| m.tag.as_str()) != Some(GAME_MENU_TAG) {
            self.game_menu_for = -1;
        }
        if let Some(p) = st.ui.last() {
            let id = self.popup_id(p.stack_order);
            if p.kind == "newgame-choice" {
                let (screen, items, initial, server_focus, from_client) = newgame::focus(p, self.newgame_cols);
                let force = !from_client && server_focus != self.newgame_focus;
                self.newgame_focus = server_focus;
                self.register_focus(screen, items, FocusOptions { page_rows: NEWGAME_PAGE_ROWS, ..Default::default() }, Some(initial), force);
            } else {
                let model = popup::model(p, cx.tiles);
                self.actions = model.actions.clone();
                self.register_focus(format!("popup:{}:{}", p.kind, id), model.focusables.clone(), FocusOptions::default(), Some(model.initial), false);
            }
        } else if st.ui_state == isorun_webtiles::UiState::CRT && st.menus.is_empty() {
            let (screen, items, opts) = crt::focus(st, "crt");
            self.register_focus(screen, items, opts, None, false);
        }
        if let Some(html) = &st.dialog {
            let items = popup::dialog_focus(html);
            self.register_focus("dialog".into(), items, FocusOptions { wrap: true, linear: true, ..Default::default() }, None, false);
        }
        // the server's text prompt: a field of ours, fresh when the prompt is
        let osk_was_up = self.osk.visible() && self.field.is_some();
        match &st.text_input {
            Some(ti) if !cx.watching => {
                if self.field.as_ref().is_none_or(|f| f.source != *ti) {
                    let history = ti.history_id.as_ref().map(|h| self.histories.entry(h.clone()).or_default().clone());
                    self.field = Some(text_field::Field::new(ti.clone(), history));
                }
                // a prompt comes with the on-screen keyboard up when the pad spoke last, so it has
                // its keys at once; the keyboard follows the field across rebuilds. After a physical
                // keyboard or the mouse the field stands alone and takes the typing; X still brings
                // the keyboard up (`osk_op`). The new game's seed field waits to be asked.
                let prompt = ti.kind.as_deref() != Some("seed-selection") && cx.device == Device::Pad;
                if osk_was_up || prompt {
                    self.osk.attach(osk::target(ti));
                }
            }
            _ => {
                self.field = None;
                self.osk.detach();
            }
        }
    }

    /// The cursor onto a menu just built (overlays.ts `seatGameMenu`, `seatArrowsHover`).
    fn seat_menu(&mut self, menu: &MenuState, cx: &mut OverlayCx) {
        if self.footer_of != menu.stack_order {
            self.footer_of = menu.stack_order;
            self.footer_index = -1;
        }
        if menu.tag == GAME_MENU_TAG {
            if self.game_menu_for != menu.stack_order {
                self.game_menu_for = menu.stack_order;
                if self.game_menu_last >= 0 {
                    self.set_hover(menu, self.game_menu_last, false, true, cx);
                }
            }
            return;
        }
        // An arrows-select menu up with nothing hovered: the cursor goes on its first row, so A and
        // Enter take the lit row. A page turned in the pack keeps the hover's index from the page
        // before, which can fall on a section header: the cursor goes on the first row after it.
        if menu.flags & isorun_webtiles::MenuFlag::ARROWS_SELECT == 0 || cx.watching {
            return;
        }
        let hov = menu::hoverable(menu);
        let at = self.hovered(menu);
        if at >= 0 && hov.contains(&at) {
            return;
        }
        let row = if at < 0 { hov.first().copied() } else { hov.iter().copied().find(|&i| i > at).or(hov.last().copied()) };
        if let Some(r) = row {
            self.set_hover(menu, r, false, false, cx);
        }
    }

    /// Seat the cursor on the current focus set after a rebuild.
    fn sync_focus(&mut self, mode: Mode, crt_menu: bool) {
        if !mode.is_focus(crt_menu) {
            if self.nav.count() > 0 {
                self.nav.clear();
            }
            self.focus_synced = self.focus_gen;
            return;
        }
        if self.focus_synced == self.focus_gen {
            return;
        }
        self.focus_synced = self.focus_gen;
        if matches!(mode, Mode::Yesno | Mode::Prompt) {
            self.nav.set(self.prompt_focusables.clone(), &format!("prompt:{}", self.prompt_screen), self.prompt_focus_opts, self.prompt_initial, false);
        } else {
            self.nav.set(self.focusables.clone(), &self.focus_screen.clone(), self.focus_opts, self.focus_initial, self.focus_force);
        }
    }

    /// Bring the models up to the state: called by `ui` and by the input calls.
    fn sync(&mut self, cx: &mut OverlayCx, ctx: Option<&egui::Context>) {
        if let Some(ctx) = ctx {
            self.now = ctx.input(|i| i.time);
        }
        let st = cx.state;
        if cx.device != self.device {
            self.device = cx.device;
            self.device_changed();
        }
        if st.rev.ui != self.last_ui_rev {
            self.last_ui_rev = st.rev.ui;
            self.rebuild(cx);
        }
        // the player's numbers first: the stat-gain cards opened by the prompt read them
        self.level_up.sync(&st.player, st.messages.more, self.now);
        let mode = derive_mode(st);
        self.update_prompt(mode, cx);
        let crt_menu = st.menus.last().is_some_and(|m| m.kind.as_deref() == Some("crt"));
        self.sync_focus(mode, crt_menu);
        self.run_timers(cx);
    }

    /// The debounced reports: the menu's page (100 ms, menu.js) and a scroller's line (100 ms after the wheel stops).
    fn run_timers(&mut self, cx: &mut OverlayCx) {
        if self.scroll_sync_at.is_some_and(|t| self.now >= t) {
            self.scroll_sync_at = None;
            if let Some(menu) = cx.state.menus.last() {
                let vis = self.menu_visible();
                let hov = self.hovered(menu);
                cx.send(isorun_webtiles::cm::menu_scroll(vis.first, vis.last, hov));
            }
        }
        if self.scroller_report_at.is_some_and(|t| self.now >= t) {
            self.report_scroller(cx);
        }
    }

    /// Carry out what the cursor fired.
    pub(crate) fn run_fired(&mut self, cx: &mut OverlayCx) {
        for act in self.nav.take_fired() {
            match act {
                Act::Send(msgs) => {
                    for m in msgs {
                        cx.send(m);
                    }
                }
                Act::Keyboard | Act::None => {}
            }
        }
    }

    /// The hover a menu has now: ours, until the server moves its own.
    pub(crate) fn hovered(&self, menu: &MenuState) -> i64 {
        match self.hover.get(&menu.stack_order) {
            Some(&(ours, server)) if server == menu.last_hovered => ours,
            _ => menu.last_hovered,
        }
    }

    /// The application's pointer moved or clicked: the hover under it counts again.
    pub(crate) fn pointer_wakes(&mut self, ctx: &egui::Context) {
        let (pos, pressed) = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.any_pressed()));
        self.pointer_moved = false;
        if let Some(p) = pos {
            let moved = self.pointer_at.is_some_and(|a| a != p);
            if self.pointer_at.is_none() {
                self.pointer_at = Some(p);
            }
            if pressed || moved {
                self.pointer_at = Some(p);
                self.pointer_live = true;
                self.pointer_moved = moved;
            }
        }
    }
}

impl Overlays {
    /// Draw whatever crawl has up, and answer the mouse on it. Call once a frame, after the view.
    pub fn ui(&mut self, ctx: &egui::Context, cx: &mut OverlayCx) {
        self.sync(cx, Some(ctx));
        self.pointer_wakes(ctx);
        let screen = frame::Screen::above(ctx, cx.ui_scale, cx.bottom);
        let st = cx.state;
        // the character overview stands on the world dimmed, as WebTiles' popups on its overlay
        if st.ui.last().is_some_and(|p| !p.hidden && p.kind == "formatted-scroller" && p.data.get("tag").and_then(|t| t.as_str()) == Some("resists")) {
            frame::backdrop(ctx, egui::Id::unique("isorun-dim"), popup::dim_colour());
        }
        let mut depth = 0;
        let n = st.menus.len();
        for (i, m) in st.menus.iter().enumerate() {
            // crawl's own Yes/No menu (tag `prompt`): the card asks it; a spectator sees the menu as it is
            if (m.tag == "prompt" && !cx.watching) || cx.hidden(m) {
                continue;
            }
            let top = i + 1 == n;
            if !m.hidden {
                if m.kind.as_deref() == Some("crt") {
                    self.draw_crt(ctx, cx, &m.tag, top, depth, &screen, m.stack_order);
                } else {
                    self.draw_menu(ctx, cx, m, top, depth, &screen);
                }
            }
            depth += 1;
        }
        let n = st.ui.len();
        for (i, p) in st.ui.iter().enumerate() {
            if !p.hidden {
                self.draw_popup(ctx, cx, p, i + 1 == n, depth, &screen);
            }
            depth += 1;
        }
        if st.ui_state == isorun_webtiles::UiState::CRT && st.menus.is_empty() && st.ui.is_empty() {
            self.draw_crt(ctx, cx, "crt", true, 0, &screen, -1);
        }
        if let Some(html) = st.dialog.clone() {
            self.draw_dialog(ctx, cx, &html, &screen);
        }
        if !cx.hide_text && self.field.as_ref().is_some_and(|f| f.source.kind.as_deref() != Some("seed-selection")) {
            self.draw_text_input(ctx, cx, &screen, cx.dock);
        }
        if matches!(derive_mode(st), Mode::Yesno | Mode::Prompt) {
            self.draw_card(ctx, cx, &screen);
        }
        if !cx.hide_text {
            self.draw_osk(ctx, cx);
        }
        let lit = self.nav.current().map(|f| f.key().to_string());
        let mut out = Vec::new();
        if self.level_up.draw(ctx, &screen, cx.tiles, lit.as_deref(), &mut out, self.now) {
            for m in out {
                cx.send(m);
            }
        }
        if self.scroll_sync_at.is_some() || self.scroller_report_at.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}
