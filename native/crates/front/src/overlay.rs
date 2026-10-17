//! The menus the game screen opens over the game (overlays.ts `showSystem`, `showSettings` and
//! `settings-panel.ts`): the Isorun menu (Start), the settings groups, a group's page, the gamepad
//! sheet and Tune the look; drawn as the game draws a menu: a title line, a letter per item, the
//! value on the right between the arrows that turn it, the hovered line lit. Left and right, A,
//! Enter or the letter change a value. The front end's settings pages read the same rows
//! (`settings.rs`), so the two never disagree.

use crate::lights_panel::{LightsEvent, LightsPanel};
use crate::paint::*;
use crate::settings::{self, Settings};
use egui::{pos2, vec2, Color32, Rect, Sense};

/// What the game is, for the Isorun menu: a player has the game's own commands, a spectator the
/// way back to the roster.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemOptions {
    pub spectating: bool,
    pub in_game: bool,
    /// Chat is offered (a server game; not on this device).
    pub chat: bool,
    pub can_fullscreen: bool,
    pub fullscreen: bool,
}

/// What a menu over the game asks of the game screen.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuAction {
    /// The menu closed: back to the dungeon.
    Resume,
    /// Send these keys to crawl (`` ` `` repeat, `?` help, `~` crawl's menu, `S` save and exit).
    Keys(String),
    Chat,
    /// Leave the game (a spectator: back to the roster).
    Leave,
    Fullscreen(bool),
    /// A setting changed: read them again (`settings::get`) and apply them.
    SettingsChanged,
    /// Tune the look moved a light: put it on the renderer (the Look is Custom now).
    Lights(isorun_render::IsoLights),
}

#[derive(Clone, Debug, PartialEq)]
enum Page {
    System,
    Settings,
    Group(String),
    Controls,
}

/// One row of a menu over the game.
struct Item {
    label: String,
    hint: String,
    /// A setting's key: the value shows, and left and right turn it.
    setting: Option<&'static str>,
    sep: bool,
    off: bool,
    act: Do,
}

#[derive(Clone, Debug, PartialEq)]
enum Do {
    Close,
    Then(MenuAction),
    Open(Page),
    Back,
    Setting,
    Reset(String),
    Tips,
    TuneLook,
}

/// The menus over the game. Keep one per game screen.
pub struct GameMenus {
    page: Option<Page>,
    opts: SystemOptions,
    cursor: usize,
    /// The row each page was left on, by its label: the same row again when it comes back.
    marks: std::collections::HashMap<String, String>,
    settings: Settings,
    lights: Option<LightsPanel>,
    tips_reset: bool,
    /// The settings were opened on their own (not from the Isorun menu): their Back closes them.
    settings_alone: bool,
}

impl Default for GameMenus {
    fn default() -> Self {
        GameMenus { page: None, opts: SystemOptions::default(), cursor: 0, marks: Default::default(), settings: settings::get(), lights: None, tips_reset: false, settings_alone: false }
    }
}

impl GameMenus {
    /// Open the Isorun menu (Start, or Escape in the game's own view).
    pub fn open_system(&mut self, opts: SystemOptions) {
        self.opts = opts;
        self.settings_alone = false;
        self.go(Page::System);
    }

    /// Open the settings over the game, on their own.
    pub fn open_settings(&mut self) {
        self.settings_alone = true;
        self.go(Page::Settings);
    }

    /// Open Tune the look: every light, colour and screen effect on sliders, shown at once over the game.
    pub fn tune_look(&mut self) {
        if self.lights.is_none() {
            let look = settings::str_of(&self.settings, "look").to_string();
            self.lights = Some(LightsPanel::new(if look == "custom" { "" } else { &look }));
        }
    }

    /// Whether a menu (or Tune the look) is up and takes the keys.
    pub fn is_open(&self) -> bool {
        self.page.is_some()
    }

    pub fn tuning(&self) -> bool {
        self.lights.is_some()
    }

    pub fn close(&mut self) {
        self.remember();
        self.page = None;
    }

    fn page_key(p: &Page) -> String {
        format!("{p:?}")
    }

    fn remember(&mut self) {
        if let Some(p) = &self.page {
            let items = self.items(p);
            if let Some(it) = items.get(self.cursor) {
                self.marks.insert(Self::page_key(p), it.label.clone());
            }
        }
    }

    fn go(&mut self, p: Page) {
        self.remember();
        self.settings = settings::get();
        let items = self.items(&p);
        let mark = self.marks.get(&Self::page_key(&p)).cloned();
        self.cursor = mark.and_then(|m| items.iter().position(|i| i.label == m)).unwrap_or(0);
        self.page = Some(p);
    }

    fn items(&self, p: &Page) -> Vec<Item> {
        let item = |label: &str, hint: &str, act: Do| Item { label: label.into(), hint: hint.into(), setting: None, sep: false, off: false, act };
        let mut out = Vec::new();
        match p {
            Page::System => {
                let playing = self.opts.in_game && !self.opts.spectating;
                out.push(item("Resume", "back to the dungeon, where you left it", Do::Close));
                if playing {
                    out.push(item("Repeat last command (`)", "do the last thing again", Do::Then(MenuAction::Keys("`".into()))));
                    out.push(item("Help (?)", "the manual, and what every key does", Do::Then(MenuAction::Keys("?".into()))));
                    // crawl binds CMD_GAME_MENU to `~` and F1; Escape does nothing in the main view
                    out.push(item("Crawl menu (F1)", "crawl's own menu: character dump, macros, lookup, quit", Do::Then(MenuAction::Keys("~".into()))));
                }
                if self.opts.in_game && self.opts.chat {
                    out.push(item("Chat (F12)", "talk to whoever is watching", Do::Then(MenuAction::Chat)));
                }
                let mut s = item("Settings", "graphics, controls and the gamepad buttons, the HUD, sound", Do::Open(Page::Settings));
                s.sep = true;
                out.push(s);
                if self.opts.can_fullscreen {
                    let full = self.opts.fullscreen;
                    out.push(item(if full { "Leave full screen" } else { "Full screen" }, if full { "back to a window; F11 or Escape does the same" } else { "fill the screen; F11 or Escape is the way back" }, Do::Then(MenuAction::Fullscreen(!full))));
                }
                // a player saves (crawl's S, which asks first); a spectator goes back to the roster
                let mut last = if playing {
                    item("Save and exit (S)", "the game keeps; come back to it whenever", Do::Then(MenuAction::Keys("S".into())))
                } else if self.opts.spectating {
                    item("Stop watching", "back to the list of games being played", Do::Then(MenuAction::Leave))
                } else {
                    item("Leave game", "back to the front door", Do::Then(MenuAction::Leave))
                };
                last.sep = true;
                out.push(last);
            }
            Page::Settings => {
                for (g, h) in settings::SETTING_GROUPS {
                    out.push(item(g, h, Do::Open(Page::Group(g.into()))));
                }
                let mut b = item("Back", "", Do::Back);
                b.sep = true;
                out.push(b);
            }
            Page::Group(g) => {
                for r in settings::group_rows(g) {
                    out.push(Item { label: r.label.into(), hint: r.hint.into(), setting: Some(r.key), sep: false, off: r.is_off(&self.settings), act: Do::Setting });
                }
                if g == "Controls" {
                    let mut t = item(if self.tips_reset { "Gamepad tips reset" } else { "Replay gamepad tips" }, "Forget which gamepad controls you have used and teach them again: switches Hints to Adaptive.", Do::Tips);
                    t.sep = true;
                    out.push(t);
                    out.push(item("Gamepad controls", "What every button on the pad does, on each layer.", Do::Open(Page::Controls)));
                }
                if g == "Graphics" {
                    let mut t = item("Tune the look", "Every light, colour and screen effect on sliders, shown at once over the game: the Look setting turns Custom.", Do::TuneLook);
                    t.sep = true;
                    out.push(t);
                }
                let mut reset = item("Reset to defaults", &format!("Put every {} setting back to its default.", g.to_lowercase()), Do::Reset(g.clone()));
                reset.off = settings::group_at_defaults(g, &self.settings);
                reset.sep = g != "Controls" && g != "Graphics";
                out.push(reset);
                out.push(item("Back", "Back to the settings.", Do::Back));
            }
            Page::Controls => out.push(item("Back", "Back to the controls.", Do::Back)),
        }
        out
    }

    fn title(&self) -> String {
        match &self.page {
            Some(Page::System) => "Isorun".into(),
            Some(Page::Settings) => "Settings".into(),
            Some(Page::Group(g)) => g.clone(),
            Some(Page::Controls) => "Gamepad controls".into(),
            None => String::new(),
        }
    }

    fn back(&mut self) -> Option<MenuAction> {
        match self.page.clone() {
            Some(Page::System) | None => {
                self.close();
                Some(MenuAction::Resume)
            }
            Some(Page::Settings) => {
                if self.settings_alone {
                    self.close();
                    Some(MenuAction::Resume)
                } else {
                    self.go(Page::System);
                    None
                }
            }
            Some(Page::Group(_)) => {
                self.go(Page::Settings);
                None
            }
            Some(Page::Controls) => {
                self.go(Page::Group("Controls".into()));
                None
            }
        }
    }

    fn choose(&mut self, i: usize, d: i32) -> Option<MenuAction> {
        let p = self.page.clone()?;
        let items = self.items(&p);
        let it = items.get(i)?;
        self.cursor = i;
        match it.act.clone() {
            Do::Close => {
                self.close();
                Some(MenuAction::Resume)
            }
            Do::Then(a) => {
                self.close();
                Some(a)
            }
            Do::Open(p) => {
                self.go(p);
                None
            }
            Do::Back => self.back(),
            Do::Setting => {
                let key = it.setting?;
                let row = settings::rows().into_iter().find(|r| r.key == key)?;
                settings::adjust(&row, &mut self.settings, d);
                settings::save_settings(&self.settings);
                Some(MenuAction::SettingsChanged)
            }
            Do::Reset(g) => {
                if settings::group_at_defaults(&g, &self.settings) {
                    return None;
                }
                settings::reset_group(&g, &mut self.settings);
                settings::save_settings(&self.settings);
                Some(MenuAction::SettingsChanged)
            }
            Do::Tips => {
                self.settings.insert("hints".into(), "adaptive".into());
                settings::save_settings(&self.settings);
                self.tips_reset = true;
                Some(MenuAction::SettingsChanged)
            }
            Do::TuneLook => {
                self.close();
                self.tune_look();
                None
            }
        }
    }

    /// The keys of this frame, when a menu is up: up and down move, left and right turn a setting,
    /// a letter or Enter chooses, Escape goes back. True when the keys were the menu's.
    pub fn keys(&mut self, ctx: &egui::Context) -> (bool, Vec<MenuAction>) {
        let mut out = Vec::new();
        let Some(p) = self.page.clone() else { return (false, out) };
        let n = self.items(&p).len();
        let events = ctx.input(|i| i.events.clone());
        for e in events {
            match e {
                egui::Event::Key { key, pressed: true, .. } => match key {
                    egui::Key::ArrowUp => self.cursor = (self.cursor + n - 1) % n.max(1),
                    egui::Key::ArrowDown | egui::Key::Tab => self.cursor = (self.cursor + 1) % n.max(1),
                    egui::Key::ArrowLeft | egui::Key::ArrowRight => {
                        let d = if key == egui::Key::ArrowLeft { -1 } else { 1 };
                        let items = self.items(&p);
                        if items.get(self.cursor).is_some_and(|i| i.setting.is_some()) {
                            out.extend(self.choose(self.cursor, d));
                        }
                    }
                    egui::Key::Enter | egui::Key::Space => out.extend(self.choose(self.cursor, 1)),
                    egui::Key::Escape => out.extend(self.back()),
                    _ => {}
                },
                egui::Event::Text(t) => {
                    for c in t.chars() {
                        if c.is_ascii_lowercase() {
                            let i = (c as u8 - b'a') as usize;
                            if i < n {
                                out.extend(self.choose(i, 1));
                            }
                        }
                    }
                }
                _ => {}
            }
            if self.page.is_none() {
                break;
            }
        }
        (true, out)
    }

    /// A pad's direction or button while a menu is up (the game maps its pad to these).
    pub fn pad(&mut self, ev: crate::front::PadEvent) -> Vec<MenuAction> {
        use crate::front::{PadButton, PadEvent};
        use crate::words::NavDir;
        let Some(p) = self.page.clone() else { return Vec::new() };
        let n = self.items(&p).len().max(1);
        match ev {
            PadEvent::Dir(NavDir::Up) => self.cursor = (self.cursor + n - 1) % n,
            PadEvent::Dir(NavDir::Down) => self.cursor = (self.cursor + 1) % n,
            PadEvent::Dir(d) => {
                if self.items(&p).get(self.cursor).is_some_and(|i| i.setting.is_some()) {
                    return self.choose(self.cursor, if d == NavDir::Left { -1 } else { 1 }).into_iter().collect();
                }
            }
            PadEvent::Press(PadButton::A) => return self.choose(self.cursor, 1).into_iter().collect(),
            PadEvent::Press(PadButton::B) => return self.back().into_iter().collect(),
            PadEvent::Press(PadButton::Start) | PadEvent::Press(PadButton::Select) => {
                self.close();
                return vec![MenuAction::Resume];
            }
            _ => {}
        }
        Vec::new()
    }

    /// Draw what is up over the game; what it asks of the game comes back.
    pub fn ui(&mut self, ctx: &egui::Context) -> Vec<MenuAction> {
        let mut out = Vec::new();
        if let Some(panel) = &mut self.lights {
            match panel.ui(ctx) {
                Some(LightsEvent::Close) => self.lights = None,
                Some(LightsEvent::Changed(l)) => {
                    if settings::str_of(&self.settings, "look") != "custom" {
                        self.settings.insert("look".into(), "custom".into());
                        settings::save_settings(&self.settings);
                    }
                    out.push(MenuAction::Lights(l));
                }
                None => {}
            }
        }
        let Some(page) = self.page.clone() else { return out };
        let screen = ctx.content_rect();
        let rem = 15.0 * settings::ui_scale(&self.settings);
        let size = 0.95 * rem;
        let bold = ctx.pixels_per_point() <= 1.5;
        let face = if bold { isorun_ui::fonts::bold() } else { egui::FontFamily::Monospace };
        let items = self.items(&page);
        let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::NULL.with("game-menus"));
        let painter = ctx.layer_painter(layer);
        // the pause menu takes the screen: the world dims behind it
        radial(&painter, screen, screen.center(), &[(0.0, rgba(3, 2, 6, 0.5)), (1.0, rgba(3, 2, 6, 0.86))]);
        let row_h = size * 1.17 + 0.44 * size;
        let value_w = if matches!(page, Page::Group(_)) { 10.0 * size + 4.0 * size } else { 0.0 };
        let label_w = items.iter().map(|i| galley(ctx, &i.label, &Style::new(size, face.clone(), C7), f32::INFINITY).size().x).fold(0.0, f32::max);
        let min_w = if matches!(page, Page::System | Page::Settings) { 22.0 * rem } else { 26.0 * rem };
        let width = (1.2 * size + 1.5 * size + label_w + 2.0 * size + value_w + 1.2 * size + 0.8 * rem).max(min_w).min(screen.width() * 0.94);
        let hint = items.get(self.cursor).map(|i| i.hint.clone()).unwrap_or_default();
        let hint_g = galley(ctx, &hint, &Style::new(0.82 * rem, face.clone(), INK_DIM), width - 1.6 * rem);
        let footer = match page {
            Page::Group(_) => "[<] [>] or a letter change a setting  [Esc] back",
            Page::System => "[Esc] resume",
            _ => "[Esc] back",
        };
        let seps = items.iter().filter(|i| i.sep).count() as f32 * 0.3 * size;
        let controls_h = if page == Page::Controls { 22.0 * rem } else { 0.0 };
        let title_h = size * 1.2 + rem;
        let body_h = items.len() as f32 * row_h + seps + 0.6 * rem + controls_h;
        let height = title_h + body_h + hint_g.size().y + 0.8 * rem + size * 1.2 + rem;
        let top = if screen.height() <= 780.0 { 20.0 } else { 50.0 };
        let rect = Rect::from_min_size(pos2(screen.center().x - width / 2.0, screen.top() + top), vec2(width, height));
        painter.rect_filled(rect.translate(vec2(0.0, 8.0)).expand(10.0), 6.0, rgba(0, 0, 0, 0.35));
        painter.rect_filled(rect, 6.0, rgba(10, 10, 14, 0.96));
        painter.rect_stroke(rect, 6.0, egui::Stroke::new(1.0, Color32::from_gray(0x55)), egui::StrokeKind::Inside);
        let title = galley(ctx, &self.title(), &Style::new(size, isorun_ui::fonts::bold(), Color32::WHITE), f32::INFINITY);
        painter.galley(pos2(rect.left() + 0.8 * rem, rect.top() + 0.5 * rem), title, Color32::WHITE);
        let line_y = rect.top() + title_h;
        painter.line_segment([pos2(rect.left(), line_y), pos2(rect.right(), line_y)], egui::Stroke::new(1.0, Color32::from_gray(0x33)));
        let mut y = line_y + 0.3 * rem;
        let s = self.settings.clone();
        let mut chosen = None;
        let mut turned = None;
        for (i, it) in items.iter().enumerate() {
            if it.sep {
                y += 0.3 * size;
            }
            let r = Rect::from_min_size(pos2(rect.left() + 0.4 * rem, y), vec2(width - 0.8 * rem, row_h));
            let resp = egui::Area::new(egui::Id::NULL.with(("game-menu-row", i))).order(egui::Order::Foreground).fixed_pos(r.min).show(ctx, |ui| ui.allocate_exact_size(r.size(), Sense::click()).1).inner;
            let focused = i == self.cursor;
            if focused {
                painter.rect_filled(r, 0.0, rgba(255, 212, 92, 0.2));
                painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, rgba(255, 212, 92, 0.6)), egui::StrokeKind::Inside);
            }
            let colour = if it.off { C8 } else if focused || resp.hovered() { Color32::WHITE } else { C7 };
            let k = galley(ctx, &((b'a' + i as u8) as char).to_string(), &Style::new(size, face.clone(), Color32::WHITE), f32::INFINITY);
            let mut x = r.left() + 0.6 * size;
            painter.galley(pos2(x + 1.2 * size - k.size().x, r.center().y - k.size().y / 2.0), k, Color32::WHITE);
            x += 1.2 * size + 0.5 * size;
            let dash = galley(ctx, "-", &Style::new(size, face.clone(), Color32::from_gray(0xaa)), f32::INFINITY);
            painter.galley(pos2(x, r.center().y - dash.size().y / 2.0), dash.clone(), Color32::from_gray(0xaa));
            x += dash.size().x + 0.5 * size;
            let label = galley(ctx, &it.label, &Style::new(size, face.clone(), colour), f32::INFINITY);
            painter.galley(pos2(x, r.center().y - label.size().y / 2.0), label, colour);
            if let Some(key) = it.setting {
                let row = settings::rows().into_iter().find(|rr| rr.key == key);
                let value = row.map(|rr| rr.value(&s)).unwrap_or_default();
                let vcol = if it.off { C8 } else if focused { Color32::WHITE } else if resp.hovered() { Color32::from_gray(0xdd) } else { C7 };
                let acol = if it.off { Color32::from_gray(0x44) } else if focused { ACCENT } else { Color32::from_gray(0x66) };
                let right = r.right() - 0.6 * size;
                let gt = galley(ctx, ">", &Style::new(size, face.clone(), acol), f32::INFINITY);
                let lt = galley(ctx, "<", &Style::new(size, face.clone(), acol), f32::INFINITY);
                let gt_r = Rect::from_min_size(pos2(right - gt.size().x - 0.7 * size, r.top()), vec2(gt.size().x + 0.7 * size, r.height()));
                let val_r = Rect::from_min_size(pos2(gt_r.left() - 0.6 * size - 10.0 * size, r.top()), vec2(10.0 * size, r.height()));
                let lt_r = Rect::from_min_size(pos2(val_r.left() - 0.6 * size - lt.size().x - 0.7 * size, r.top()), vec2(lt.size().x + 0.7 * size, r.height()));
                let v = galley(ctx, &value, &Style::new(size, face.clone(), vcol), f32::INFINITY);
                painter.galley(pos2(val_r.center().x - v.size().x / 2.0, r.center().y - v.size().y / 2.0), v, vcol);
                for (d, g, ar) in [(-1, lt, lt_r), (1, gt, gt_r)] {
                    let ares = egui::Area::new(egui::Id::NULL.with(("game-menu-arrow", i, d))).order(egui::Order::Foreground).fixed_pos(ar.min).show(ctx, |ui| ui.allocate_exact_size(ar.size(), Sense::click()).1).inner;
                    let c = if ares.hovered() && !it.off { Color32::WHITE } else { acol };
                    painter.galley(pos2(ar.center().x - g.size().x / 2.0, r.center().y - g.size().y / 2.0), g, c);
                    if ares.clicked() && !it.off {
                        turned = Some((i, d));
                    }
                }
            }
            if resp.clicked() && turned.is_none() {
                chosen = Some(i);
            }
            y += row_h;
        }
        if page == Page::Controls {
            let mut yy = y + 0.3 * rem;
            for side in crate::regions::CONTROLS_SHEET {
                for (button, tap, hold) in side.iter() {
                    let k = galley(ctx, button, &Style::new(size, isorun_ui::fonts::bold(), GOLD_BRIGHT), f32::INFINITY);
                    let text = if hold.is_empty() { tap.to_string() } else { format!("{tap}   hold: {hold}") };
                    let g = galley(ctx, &text, &Style::new(size, face.clone(), INK), width - 9.0 * rem);
                    painter.galley(pos2(rect.left() + 1.2 * rem, yy), k, GOLD_BRIGHT);
                    painter.galley(pos2(rect.left() + 7.0 * rem, yy), g.clone(), INK);
                    yy += g.size().y + 0.25 * rem;
                }
                yy += 0.5 * rem;
            }
        }
        y += 0.3 * rem + controls_h;
        painter.galley(pos2(rect.left() + 0.8 * rem, y), hint_g.clone(), INK_DIM);
        y += hint_g.size().y + 0.4 * rem;
        painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], egui::Stroke::new(1.0, Color32::from_gray(0x33)));
        let f = galley(ctx, footer, &Style::new(size, face.clone(), Color32::from_gray(0xaa)), f32::INFINITY);
        painter.galley(pos2(rect.left() + 0.8 * rem, y + 0.4 * rem), f, Color32::from_gray(0xaa));
        if let Some((i, d)) = turned {
            out.extend(self.choose(i, d));
        } else if let Some(i) = chosen {
            out.extend(self.choose(i, 1));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_isorun_menu() {
        let mut m = GameMenus::default();
        m.open_system(SystemOptions { in_game: true, chat: true, ..Default::default() });
        let labels: Vec<String> = m.items(&Page::System).iter().map(|i| i.label.clone()).collect();
        assert_eq!(labels, ["Resume", "Repeat last command (`)", "Help (?)", "Crawl menu (F1)", "Chat (F12)", "Settings", "Save and exit (S)"]);
        m.open_system(SystemOptions { in_game: true, spectating: true, ..Default::default() });
        let labels: Vec<String> = m.items(&Page::System).iter().map(|i| i.label.clone()).collect();
        assert_eq!(labels, ["Resume", "Settings", "Stop watching"]);
        // Settings, then back to the menu on the row that led away
        assert_eq!(m.choose(1, 1), None);
        assert_eq!(m.page, Some(Page::Settings));
        assert_eq!(m.back(), None);
        assert_eq!(m.page, Some(Page::System));
        assert_eq!(m.cursor, 1);
        assert_eq!(m.choose(0, 1), Some(MenuAction::Resume));
        assert!(!m.is_open());
    }
}
