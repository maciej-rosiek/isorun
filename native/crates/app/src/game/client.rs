//! The client's own lists over the game (the part of overlays.ts that is neither crawl's nor the
//! front end's): `showChoices` (a quick slot's pin, an empty place of the doll, the interaction
//! chooser), `showActionTabs` (an empty tab of X's actions, crawl's line saying so where its menu
//! would be) and `showPalette` (the command palette: the cmd-keys.h section for the mode,
//! filtered by typed text and by category). Drawn as the game draws a menu of its own: a title
//! (or the tabs in its place), the rows with their icons and keys, the line under the list that
//! says what the lit row does, the keyboard's footer.

use isorun_hud::quick_slots::TileLayer;
use isorun_input::action_tabs::{ActionTabId, ACTION_TABS};
use isorun_input::bindings::CommandCategory;
use isorun_input::router::OverlayOp;
use isorun_ui::theme::{GOLD_BRIGHT, INK, INK_DIM};
use isorun_ui::tiles::TileIcons;
use isorun_ui::widgets::{paint_panel, rgba, PanelLook};
use isorun_webtiles::{cm, Keys};
use serde_json::Value;

/// What a row does when it is fired.
#[derive(Clone, Debug, PartialEq)]
pub enum Run {
    /// the i-th of the HUD's choices (`Hud::choose`)
    HudChoice(usize),
    /// the interaction chooser's row: the feature (false) or the pile (true)
    Interact(bool),
    /// a command of the palette: the message the official client sends for its key
    Send(Value),
    /// an action through the runner (the Character tab's screens)
    Execute(isorun_input::Action),
    /// the Isorun menu's own rows
    Resume,
    Settings,
    Fullscreen(bool),
    Chat,
    Leave,
}

/// What the Isorun menu is opened for (overlays.ts `showSystem`).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct SystemOpts {
    pub spectating: bool,
    /// chat is offered (a server game)
    pub chat: bool,
    pub can_fullscreen: bool,
    pub fullscreen: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Row {
    pub label: String,
    pub sub: Option<String>,
    pub key: Option<String>,
    /// an icon by its tile name (gui, then main)
    pub tile: Option<&'static str>,
    pub tiles: Vec<TileLayer>,
    pub sep: bool,
    pub run: Option<Run>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Choices,
    /// X's actions: the tab up, and crawl's line (None while a turn is under way)
    Actions(ActionTabId, Option<String>),
    Palette { mode: CommandCategory, category: usize, filter: String },
    /// the Isorun menu (Start): the Character tab (0) and the System tab (1); a spectator has System alone
    System { tab: usize, opts: SystemOpts },
}

pub struct ClientMenu {
    pub kind: Kind,
    pub title: String,
    pub rows: Vec<Row>,
    pub focus: usize,
    /// the cursor's memory: the same row again when a list of this name comes back
    pub remember: String,
    /// the list came from the HUD, which hears of a close without a pick (`choices_closed`)
    pub from_hud: bool,
    /// the row each of the Isorun menu's tabs was left on
    pub tab_focus: [usize; 2],
}

/// What the player did on the list.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientEvent {
    Fire(Run),
    /// put away without a pick
    Closed { from_hud: bool },
    /// a tab of X's actions, turned to
    Tab(ActionTabId),
}

/// One command of data/commands.json (from the server's default key table, cmd-keys.h).
#[derive(Clone, Debug, serde::Deserialize)]
struct CatalogueCommand {
    id: String,
    label: String,
    category: String,
    mode: String,
    key: Option<String>,
    ck: Option<String>,
}

fn catalogue() -> &'static [CatalogueCommand] {
    static C: std::sync::OnceLock<Vec<CatalogueCommand>> = std::sync::OnceLock::new();
    C.get_or_init(|| serde_json::from_str(include_str!("../../../../../apps/isorun/data/commands.json")).unwrap_or_default())
}

fn mode_name(m: CommandCategory) -> &'static str {
    match m {
        CommandCategory::Command => "command",
        CommandCategory::Levelmap => "levelmap",
        CommandCategory::Targeting => "targeting",
        CommandCategory::Menu => "menu",
    }
}

/// The categories of one key-table section, catalogue order, `all` first.
fn categories_for(mode: &str) -> Vec<String> {
    let mut out = vec!["all".to_string()];
    for c in catalogue().iter().filter(|c| c.mode == mode) {
        if !out.contains(&c.category) {
            out.push(c.category.clone());
        }
    }
    out
}

/// The message the official client sends for a catalogue command's key.
fn command_message(c: &CatalogueCommand) -> Option<Value> {
    if let Some(ck) = &c.ck {
        return match ck.as_str() {
            "CK_HOME" => Some(cm::key(Keys::CK_HOME)),
            "CK_END" => Some(cm::key(Keys::CK_END)),
            "CK_UP" => Some(cm::key(Keys::CK_UP)),
            "CK_DOWN" => Some(cm::key(Keys::CK_DOWN)),
            "CK_LEFT" => Some(cm::key(Keys::CK_LEFT)),
            "CK_RIGHT" => Some(cm::key(Keys::CK_RIGHT)),
            "CK_PGUP" => Some(cm::key(Keys::CK_PGUP)),
            "CK_PGDN" => Some(cm::key(Keys::CK_PGDN)),
            _ => None,
        };
    }
    let key = c.key.as_ref()?;
    let mut chars = key.chars();
    let first = chars.next()?;
    // control characters, Esc, Enter, Tab go as keycodes; everything else is typed text
    if chars.next().is_none() && (first as u32) < 32 {
        Some(cm::key(first as i32))
    } else {
        Some(cm::input(key))
    }
}

fn key_name(c: &CatalogueCommand) -> String {
    if let Some(ck) = &c.ck {
        return ck.trim_start_matches("CK_").replace('_', " ").to_lowercase();
    }
    let Some(k) = &c.key else { return String::new() };
    match k.as_str() {
        "\t" => "Tab".into(),
        "\r" => "Enter".into(),
        "\u{1b}" => "Esc".into(),
        _ => {
            let code = k.chars().next().map(|c| c as u32).unwrap_or(32);
            if code < 32 {
                format!("Ctrl-{}", char::from_u32(64 + code).unwrap_or('?'))
            } else {
                k.clone()
            }
        }
    }
}

impl ClientMenu {
    pub fn choices(title: &str, rows: Vec<Row>, remember: &str, from_hud: bool) -> Self {
        ClientMenu { kind: Kind::Choices, title: title.into(), rows, focus: 0, remember: remember.into(), from_hud, tab_focus: [0; 2] }
    }

    pub fn actions(current: ActionTabId, text: Option<String>) -> Self {
        ClientMenu { kind: Kind::Actions(current, text), title: String::new(), rows: vec![], focus: 0, remember: String::new(), from_hud: false, tab_focus: [0; 2] }
    }

    pub fn palette(mode: CommandCategory) -> Self {
        let mut m = ClientMenu { kind: Kind::Palette { mode, category: 0, filter: String::new() }, title: String::new(), rows: vec![], focus: 0, remember: String::new(), from_hud: false, tab_focus: [0; 2] };
        m.title = match mode {
            CommandCategory::Command => "Commands",
            CommandCategory::Levelmap => "Level map commands",
            CommandCategory::Targeting => "Targeting commands",
            CommandCategory::Menu => "Menu commands",
        }
        .into();
        m.build_palette();
        m
    }

    /// The Isorun menu, drawn as the game draws a menu: a place of its own, so it opens again on the
    /// tab and row it was left on (`tab`, `tab_focus`). While playing it has two tabs: Character,
    /// the screens that review and manage the character, and System, the game's own options and the
    /// way out. A spectator has no character to read, so the System rows stand alone.
    pub fn system(opts: SystemOpts, tab: usize, tab_focus: [usize; 2]) -> Self {
        let tab = if opts.spectating { 1 } else { tab.min(1) };
        let mut m = ClientMenu { kind: Kind::System { tab, opts }, title: "Isorun".into(), rows: vec![], focus: 0, remember: "system".into(), from_hud: false, tab_focus };
        m.build_system();
        m
    }

    fn build_system(&mut self) {
        let Kind::System { tab, opts } = self.kind else { return };
        self.rows = if tab == 0 {
            isorun_input::command_menu::CHARACTER_COMMANDS
                .iter()
                .map(|c| Row { label: c.label.into(), sub: c.sub.map(Into::into), key: Some(c.key.into()), tile: c.tile, run: Some(Run::Execute(c.action())), ..Default::default() })
                .collect()
        } else {
            // three runs under rules: the game (back to it, its own commands), Isorun (the settings), and last the way out
            let mut rows = vec![Row { label: "Resume".into(), sub: Some("back to the dungeon, where you left it".into()), run: Some(Run::Resume), ..Default::default() }];
            let playing = !opts.spectating;
            if playing {
                let r = isorun_input::command_menu::REPEAT_COMMAND;
                let h = isorun_input::command_menu::HELP_COMMAND;
                rows.push(Row { label: format!("{} ({})", r.label, r.key), sub: Some("do the last thing again".into()), run: Some(Run::Send(cm::input(r.key))), ..Default::default() });
                rows.push(Row { label: format!("{} ({})", h.label, h.key), sub: Some("the manual, and what every key does".into()), run: Some(Run::Send(cm::input(h.key))), ..Default::default() });
                // crawl binds CMD_GAME_MENU to `~` and F1; Escape does nothing in the main view
                rows.push(Row { label: "Crawl menu (F1)".into(), sub: Some("crawl's own menu: character dump, macros, lookup, quit".into()), run: Some(Run::Send(cm::input("~"))), ..Default::default() });
            }
            if opts.chat {
                rows.push(Row { label: "Chat (F12)".into(), sub: Some("talk to whoever is watching".into()), run: Some(Run::Chat), ..Default::default() });
            }
            rows.push(Row { label: "Settings".into(), sub: Some("graphics, controls and the gamepad buttons, the HUD, sound".into()), sep: true, run: Some(Run::Settings), ..Default::default() });
            if opts.can_fullscreen {
                let (label, sub) = if opts.fullscreen { ("Leave full screen", "back to a window; F11 or Escape does the same") } else { ("Full screen", "fill the screen; F11 or Escape is the way back") };
                rows.push(Row { label: label.into(), sub: Some(sub.into()), run: Some(Run::Fullscreen(!opts.fullscreen)), ..Default::default() });
            }
            // a player saves (crawl's S, which asks first); a spectator goes back to the roster
            rows.push(if playing {
                Row { label: "Save and exit (S)".into(), sub: Some("the game keeps; come back to it whenever".into()), sep: true, run: Some(Run::Send(cm::input("S"))), ..Default::default() }
            } else {
                Row { label: "Stop watching".into(), sub: Some("back to the list of games being played".into()), sep: true, run: Some(Run::Leave), ..Default::default() }
            });
            for (i, r) in rows.iter_mut().enumerate() {
                r.key = Some(((b'a' + i as u8) as char).to_string());
            }
            rows
        };
        self.focus = self.tab_focus[tab].min(self.rows.len().saturating_sub(1));
    }

    /// The palette's rows for its section, category and filter (the pad's direct buttons left out).
    fn build_palette(&mut self) {
        let Kind::Palette { mode, category, filter } = &self.kind else { return };
        let mode = mode_name(*mode);
        let cats = categories_for(mode);
        let cat = cats.get(*category).cloned().unwrap_or_else(|| "all".into());
        let filter = filter.to_lowercase();
        self.rows = catalogue()
            .iter()
            .filter(|c| c.mode == mode && (mode != "command" || c.key.as_deref().is_none_or(|k| !isorun_input::command_menu::is_gamepad_command_key(k))))
            .filter(|c| cat == "all" || c.category == cat)
            .filter(|c| filter.is_empty() || format!("{} {} {}", c.label, c.category, c.id).to_lowercase().contains(&filter))
            .map(|c| Row { label: c.label.clone(), sub: Some(format!("{} · {}", c.category, key_name(c))), run: command_message(c).map(Run::Send), ..Default::default() })
            .collect();
        self.focus = 0;
    }

    /// The pad's buttons for the list (Overlays.padPrompts): the empty tab's two, or the lit row's A.
    pub fn pad_prompts(&self) -> Vec<isorun_input::bindings::BindingLabel> {
        use isorun_input::bindings::BindingLabel;
        use isorun_input::{Action, Button};
        match &self.kind {
            Kind::Actions(..) => vec![
                BindingLabel { button: Button::Y, teaching: false, label: "Swap weapons".into(), action: isorun_input::action_tabs::swap_weapons(), hold: None, contextual: true },
                BindingLabel { button: Button::LT, teaching: false, label: "Shout".into(), action: isorun_input::action_tabs::shout(), hold: None, contextual: true },
            ],
            _ if self.rows.is_empty() => vec![],
            _ => vec![BindingLabel { button: Button::A, teaching: false, label: "select".into(), action: Action::focus(isorun_input::focus::FocusOp::Select), hold: None, contextual: true }],
        }
    }

    /// A key or pad op on the list. Returns what came of it, and whether the list stays up.
    pub fn input(&mut self, op: OverlayOp) -> (Option<ClientEvent>, bool) {
        let n = self.rows.len();
        match op {
            OverlayOp::Next if n > 0 => self.focus = (self.focus + 1) % n,
            OverlayOp::Prev if n > 0 => self.focus = (self.focus + n - 1) % n,
            OverlayOp::PageNext | OverlayOp::Last if n > 0 => self.focus = n - 1,
            OverlayOp::PagePrev | OverlayOp::First => self.focus = 0,
            OverlayOp::Select | OverlayOp::Submit => {
                if let Some(run) = self.rows.get(self.focus).and_then(|r| r.run.clone()) {
                    return (Some(ClientEvent::Fire(run)), false);
                }
            }
            OverlayOp::Cancel | OverlayOp::Close => return (Some(ClientEvent::Closed { from_hud: self.from_hud }), false),
            OverlayOp::Left | OverlayOp::Right | OverlayOp::BumperPrev | OverlayOp::BumperNext => {
                let step = if matches!(op, OverlayOp::Left | OverlayOp::BumperPrev) { -1 } else { 1 };
                match &mut self.kind {
                    Kind::Actions(cur, _) => return (Some(ClientEvent::Tab(isorun_input::action_tabs::action_neighbour(*cur, step))), true),
                    Kind::Palette { mode, category, .. } => {
                        let k = categories_for(mode_name(*mode)).len() as i32;
                        *category = ((*category as i32 + step).rem_euclid(k.max(1))) as usize;
                        self.build_palette();
                    }
                    Kind::System { tab, opts } => {
                        if !opts.spectating {
                            let t = *tab;
                            self.tab_focus[t] = self.focus;
                            *tab = 1 - t;
                            self.build_system();
                        }
                    }
                    Kind::Choices => {}
                }
            }
            _ => {}
        }
        if let Kind::System { tab, .. } = self.kind {
            self.tab_focus[tab] = self.focus;
        }
        (None, true)
    }

    /// A typed character: the palette's filter, or a row's own key.
    pub fn hotkey(&mut self, key: &str) -> (Option<ClientEvent>, bool) {
        if let Kind::Palette { filter, .. } = &mut self.kind {
            if key.chars().count() == 1 && !key.starts_with("Ctrl-") && key != "Tab" {
                filter.push_str(key);
                self.build_palette();
                return (None, true);
            }
            if key == "Tab" {
                return self.input(OverlayOp::Right);
            }
            return (None, true);
        }
        if let Some(i) = self.rows.iter().position(|r| r.key.as_deref() == Some(key)) {
            self.focus = i;
            if let Some(run) = self.rows[i].run.clone() {
                return (Some(ClientEvent::Fire(run)), false);
            }
        }
        (None, true)
    }

    /// Backspace in the palette's filter.
    pub fn backspace(&mut self) {
        if let Kind::Palette { filter, .. } = &mut self.kind {
            filter.pop();
            self.build_palette();
        }
    }

    /// Draw the list; a click fires a row or turns a tab.
    pub fn ui(&mut self, ctx: &egui::Context, tiles: &mut TileIcons, scale: f32, keyboard: bool) -> (Option<ClientEvent>, bool) {
        let screen = ctx.content_rect();
        let rem = 15.0 * scale;
        let size = 0.95 * rem;
        let mono = egui::FontId::monospace(size);
        let small = egui::FontId::monospace(0.82 * rem);
        let wide = matches!(self.kind, Kind::Actions(..) | Kind::Palette { .. } | Kind::System { .. });
        let width = if wide { (48.0 * rem).min(screen.width() * 0.94) } else { (34.0 * rem).min(screen.width() * 0.92) };
        let row_h = (size * 1.6).max(32.0 * scale + 4.0);
        let has_icons = self.rows.iter().any(|r| r.tile.is_some() || !r.tiles.is_empty());
        let any_hint = self.rows.iter().any(|r| r.sub.is_some());
        let max_rows = ((screen.height() * 0.7 - 6.0 * rem) / row_h).floor().max(3.0) as usize;
        let shown = self.rows.len().min(max_rows);
        let first = if self.focus >= shown { self.focus + 1 - shown } else { 0 };
        // the tabs stand in for the title (overlays.ts installTabs, packTabs): a strip with the tabs' icons
        let tabbed = matches!(self.kind, Kind::Actions(..) | Kind::System { .. });
        let title_h = if tabbed { 32.0 * scale + 0.35 * 2.0 * rem + 4.0 } else { size * 1.3 + 0.8 * rem };
        let body_h = match &self.kind {
            Kind::Actions(..) => 3.6 * rem,
            Kind::Palette { .. } => shown as f32 * row_h + 2.4 * rem,
            Kind::Choices => shown as f32 * row_h + 0.9 * rem,
            // the panel stands as tall as the longer tab, so turning one never resizes it
            Kind::System { .. } => isorun_input::command_menu::CHARACTER_COMMANDS.len().min(max_rows) as f32 * row_h + 0.9 * rem,
        };
        let hint_h = if !any_hint { 0.0 } else { 1.4 * rem };
        let footer_h = if keyboard { size * 1.2 + 0.8 * rem } else { 0.0 };
        let height = title_h + body_h + hint_h + footer_h;
        let top = if screen.height() <= 780.0 { 20.0 } else { 50.0 };
        let rect = egui::Rect::from_min_size(egui::pos2(screen.center().x - width / 2.0, screen.top() + top), egui::vec2(width, height));
        let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("isorun-client-menu"));
        let painter = ctx.layer_painter(layer);
        // the pause menu takes the screen: the world dims behind it
        if matches!(self.kind, Kind::System { .. }) {
            isorun_front::paint::radial(&painter, screen, screen.center(), &[(0.0, rgba(3, 2, 6, 0.5)), (1.0, rgba(3, 2, 6, 0.86))]);
        }
        paint_panel(&painter, rect, &PanelLook::dock_popup());
        let mut out = None;
        let mut stays = true;
        let pointer = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.primary_clicked()));
        let clicked_at = |r: egui::Rect| pointer.1 && pointer.0.is_some_and(|p| r.contains(p));
        let hovered = |r: egui::Rect| pointer.0.is_some_and(|p| r.contains(p));
        let pad = 0.8 * rem;
        // the title, or the tabs in its place (X's actions, the palette's categories)
        let mut y = rect.top() + 0.4 * rem;
        let mut chip = None;
        match &self.kind {
            Kind::Actions(cur, _) => {
                let mut x = rect.left() + pad;
                for t in ACTION_TABS.iter() {
                    let on = t.id == *cur;
                    let colour = if on { egui::Color32::WHITE } else { INK_DIM };
                    let icon = 32.0 * scale;
                    let g = painter.layout_no_wrap(t.label.to_string(), mono.clone(), colour);
                    let r = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(icon + 0.35 * size + g.size().x, title_h - 0.6 * rem));
                    if let Some(id) = tiles.id_of(t.tile) {
                        tiles.paint_id(&painter, id, egui::Rect::from_min_size(egui::pos2(x, r.center().y - icon / 2.0), egui::vec2(icon, icon)));
                    }
                    painter.galley(egui::pos2(x + icon + 0.35 * size, r.center().y - g.size().y / 2.0), g, colour);
                    if on {
                        painter.line_segment([egui::pos2(r.left(), r.bottom()), egui::pos2(r.right(), r.bottom())], egui::Stroke::new(2.0, GOLD_BRIGHT));
                    }
                    if clicked_at(r) && !on {
                        out = Some(ClientEvent::Tab(t.id));
                    }
                    x = r.right() + 1.4 * size;
                }
            }
            Kind::Palette { mode, category, .. } => {
                let g = painter.layout_no_wrap(self.title.clone(), mono.clone(), egui::Color32::WHITE);
                painter.galley(egui::pos2(rect.left() + pad, y), g, egui::Color32::WHITE);
                let mut x = rect.left() + pad + 14.0 * size;
                for (i, c) in categories_for(mode_name(*mode)).iter().enumerate() {
                    let on = i == *category;
                    let g = painter.layout_no_wrap(c.clone(), small.clone(), if on { GOLD_BRIGHT } else { INK_DIM });
                    let r = egui::Rect::from_min_size(egui::pos2(x, y), g.size());
                    if x + g.size().x > rect.right() - pad {
                        break;
                    }
                    painter.galley(r.min, g, egui::Color32::WHITE);
                    if clicked_at(r) {
                        // the chips are a choice of their own: the list is rebuilt at once
                        chip = Some(i);
                    }
                    x = r.right() + 0.9 * size;
                }
            }
            Kind::System { tab, opts } => {
                let mut x = rect.left() + pad;
                let tabs: &[(&str, &str)] = if opts.spectating { &[("System", "CMD_GAME_MENU")] } else { &[("Character", "CMD_DISPLAY_CHARACTER_STATUS"), ("System", "CMD_GAME_MENU")] };
                for (i, (label, tile)) in tabs.iter().enumerate() {
                    let index = if opts.spectating { 1 } else { i };
                    let on = index == *tab;
                    let colour = if on { GOLD_BRIGHT } else { INK_DIM };
                    let icon = 32.0 * scale;
                    let g = painter.layout_no_wrap(label.to_string(), mono.clone(), colour);
                    let r = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(icon + 0.35 * size + g.size().x, title_h - 0.6 * rem));
                    if let Some(id) = tiles.id_of(tile) {
                        tiles.paint_id(&painter, id, egui::Rect::from_min_size(egui::pos2(x, r.center().y - icon / 2.0), egui::vec2(icon, icon)));
                    }
                    painter.galley(egui::pos2(x + icon + 0.35 * size, r.center().y - g.size().y / 2.0), g, colour);
                    if on {
                        painter.line_segment([egui::pos2(r.left(), r.bottom()), egui::pos2(r.right(), r.bottom())], egui::Stroke::new(2.0, GOLD_BRIGHT));
                    }
                    if clicked_at(r) && !on {
                        chip = Some(index);
                    }
                    x = r.right() + 1.4 * size;
                }
            }
            Kind::Choices => {
                let g = painter.layout_no_wrap(self.title.clone(), egui::FontId::new(size, isorun_ui::fonts::bold()), egui::Color32::WHITE);
                painter.galley(egui::pos2(rect.left() + pad, y), g, egui::Color32::WHITE);
            }
        }
        if let (Some(i), Kind::System { tab, .. }) = (chip, &mut self.kind) {
            self.tab_focus[*tab] = self.focus;
            *tab = i;
            self.build_system();
        }
        if let (Some(i), Kind::Palette { category, .. }) = (chip, &mut self.kind) {
            *category = i;
            self.build_palette();
        }
        y = rect.top() + title_h;
        painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], egui::Stroke::new(1.0, egui::Color32::from_gray(0x33)));
        y += 0.4 * rem;
        match &self.kind {
            Kind::Actions(_, text) => {
                let g = painter.layout(text.clone().unwrap_or_default(), mono.clone(), INK_DIM, width - 2.0 * pad);
                painter.galley(egui::pos2(rect.left() + pad, y + 0.8 * rem), g, INK_DIM);
            }
            kind => {
                if let Kind::Palette { filter, .. } = kind {
                    let shown_filter = if filter.is_empty() { "Filter commands…".to_string() } else { filter.clone() };
                    let g = painter.layout_no_wrap(shown_filter, mono.clone(), if filter.is_empty() { INK_DIM } else { INK });
                    let fr = egui::Rect::from_min_size(egui::pos2(rect.left() + pad, y), egui::vec2(width - 2.0 * pad, 1.5 * rem));
                    painter.rect_filled(fr, 3.0, rgba(0, 0, 0, 0.4));
                    painter.galley(egui::pos2(fr.left() + 0.4 * rem, fr.center().y - g.size().y / 2.0), g, INK);
                    y += 2.0 * rem;
                }
                for i in first..first + shown {
                    let row = &self.rows[i];
                    let r = egui::Rect::from_min_size(egui::pos2(rect.left() + 0.4 * rem, y), egui::vec2(width - 0.8 * rem, row_h));
                    let lit = i == self.focus;
                    if lit {
                        painter.rect_filled(r, 0.0, rgba(255, 212, 92, 0.2));
                        painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, rgba(255, 212, 92, 0.6)), egui::StrokeKind::Inside);
                    } else if hovered(r) {
                        painter.rect_filled(r, 0.0, rgba(255, 255, 255, 0.06));
                    }
                    if row.sep && i > first {
                        painter.line_segment([egui::pos2(r.left(), r.top()), egui::pos2(r.right(), r.top())], egui::Stroke::new(1.0, egui::Color32::from_gray(0x33)));
                    }
                    let mut x = r.left() + 0.6 * size;
                    if has_icons {
                        let icon = 32.0 * scale;
                        let cell = egui::Rect::from_min_size(egui::pos2(x, r.center().y - icon / 2.0), egui::vec2(icon, icon));
                        if !row.tiles.is_empty() {
                            for t in &row.tiles {
                                tiles.paint_ref(&painter, &isorun_webtiles::TileRef { t: t.t, tex: t.tex.unwrap_or(0), ymax: t.ymax }, cell);
                            }
                        } else if let Some(id) = row.tile.and_then(|n| tiles.id_of(n)) {
                            tiles.paint_id(&painter, id, cell);
                        }
                        x += icon + 0.5 * size;
                    }
                    let colour = if lit { egui::Color32::WHITE } else { egui::Color32::from_gray(0xc0) };
                    // the System tab's rows are lettered as the game's menus are: `a - Resume`
                    let lettered = matches!(self.kind, Kind::System { tab: 1, .. });
                    if lettered {
                        if let Some(k) = &row.key {
                            let g = painter.layout_no_wrap(format!("{k} - "), mono.clone(), egui::Color32::WHITE);
                            let w = g.size().x;
                            painter.galley(egui::pos2(x, r.center().y - g.size().y / 2.0), g, egui::Color32::WHITE);
                            x += w;
                        }
                    }
                    let g = painter.layout_no_wrap(row.label.clone(), mono.clone(), colour);
                    painter.galley(egui::pos2(x, r.center().y - g.size().y / 2.0), g, colour);
                    let right = match (&self.kind, &row.key, &row.sub) {
                        (Kind::Palette { .. }, _, Some(sub)) => Some(sub.clone()),
                        (Kind::System { tab: 1, .. }, _, _) => None,
                        (_, Some(k), _) => Some(k.clone()),
                        _ => None,
                    };
                    if let Some(k) = right {
                        let g = painter.layout_no_wrap(k, small.clone(), INK_DIM);
                        painter.galley(egui::pos2(r.right() - 0.6 * size - g.size().x, r.center().y - g.size().y / 2.0), g, INK_DIM);
                    }
                    if hovered(r) && ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                        self.focus = i;
                    }
                    if clicked_at(r) {
                        self.focus = i;
                        if let Some(run) = row.run.clone() {
                            out = Some(ClientEvent::Fire(run));
                            stays = false;
                        }
                    }
                    y += row_h;
                }
            }
        }
        // the line under the list: what the lit row does (the palette's rows carry theirs beside them)
        let mut fy = rect.top() + title_h + body_h;
        if any_hint && !matches!(self.kind, Kind::Palette { .. }) {
            let hint = self.rows.get(self.focus).and_then(|r| r.sub.clone()).unwrap_or_default();
            let g = painter.layout(hint, small.clone(), INK_DIM, width - 2.0 * pad);
            painter.galley(egui::pos2(rect.left() + pad, fy + 0.2 * rem), g, INK_DIM);
            fy += hint_h;
        }
        if keyboard {
            painter.line_segment([egui::pos2(rect.left(), fy), egui::pos2(rect.right(), fy)], egui::Stroke::new(1.0, egui::Color32::from_gray(0x33)));
            let tabs = !matches!(self.kind, Kind::Choices);
            // `.popup .more kbd`: the keys in caps, the words beside them
            let words = |t: &'static str| isorun_ui::widgets::KeyHint::Text(t, mono.clone(), egui::Color32::from_gray(0xaa));
            use isorun_ui::widgets::KeyHint::{Cap, Gap};
            let parts: Vec<isorun_ui::widgets::KeyHint> = match self.kind {
                Kind::Actions(..) => vec![Cap("←"), Gap(4.0), Cap("→"), words(" Tabs · "), Cap("Esc"), words(" Back")],
                _ if tabs => vec![Cap("←"), Gap(4.0), Cap("→"), words(" Tabs · "), Cap("Enter"), words(" Select · "), Cap("Esc"), words(" Back")],
                _ => vec![Cap("Enter"), words(" Select · "), Cap("Esc"), words(" Back")],
            };
            isorun_ui::widgets::key_hints(&painter, egui::pos2(rect.left() + pad, fy + 0.4 * rem), &parts, &isorun_ui::widgets::KeycapLook::footer(size));
        }
        // the panel takes its own clicks: none reach the view under it
        egui::Area::new(egui::Id::unique("isorun-client-menu-block")).order(egui::Order::Middle).fixed_pos(rect.min).show(ctx, |ui| {
            ui.allocate_exact_size(rect.size(), egui::Sense::click());
        });
        (out, stays)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_reads() {
        assert!(catalogue().len() > 50);
        let m = ClientMenu::palette(CommandCategory::Command);
        assert!(!m.rows.is_empty());
        // the pad's own buttons are left out of the command section
        assert!(!m.rows.iter().any(|r| r.label == "Autoexplore"));
    }

    #[test]
    fn keys_as_the_official_client_sends_them() {
        let c = |key: &str| CatalogueCommand { id: String::new(), label: String::new(), category: String::new(), mode: String::new(), key: Some(key.into()), ck: None };
        assert_eq!(command_message(&c("o")), Some(cm::input("o")));
        assert_eq!(command_message(&c("\u{10}")), Some(cm::key(16)));
        assert_eq!(key_name(&c("\u{10}")), "Ctrl-P");
    }
}
