//! Keys while crawl has something up (game.ts `onKeyDown` for the overlays' modes, bindings.ts
//! `screenKey`): on a screen the keyboard is a pad — Enter is A, Escape is B, Space is A (but
//! pages text to read), the arrows are the d-pad and PageUp / PageDown the bumpers — so a key and
//! the button it stands for go down one path (`focus_op`, `menu_op`). What none of that takes goes
//! to crawl as the official client sends it (keys.ts `keydownMessage`).

use crate::card::answer;
use crate::focus::{FocusOp, Focusable};
use crate::menu::MenuOp;
use crate::menu_nav::{scroll_key_intent, KeyName, NavKey, ScrollKey};
use crate::prompt::{default_yesno, derive_mode, newgame_back, parse_prompt, Mode};
use crate::text_field::FieldKey;
use crate::{OverlayCx, Overlays};
use egui::{Event, Key, Modifiers};
use isorun_webtiles::{cm, Keys, MenuFlag};
use serde_json::Value;

/// The pad's face buttons and bumpers a key stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Button {
    A,
    B,
    Start,
    Lb,
    Rb,
}

/// The message the official client sends for a key no screen took (keys.ts `keydownMessage`).
pub fn raw_key(key: Key, m: Modifiers) -> Option<Value> {
    let ck = |plain: i32, shift: i32, ctrl: i32| {
        if m.ctrl && !m.shift {
            ctrl
        } else if m.shift && !m.ctrl {
            shift
        } else {
            plain
        }
    };
    let code = match key {
        Key::Escape => 27,
        Key::Backspace => 8,
        Key::Tab => {
            if m.shift {
                Keys::CK_SHIFT_TAB
            } else {
                9
            }
        }
        Key::Delete => ck(Keys::CK_DELETE, Keys::CK_SHIFT_DELETE, Keys::CK_CTRL_DELETE),
        Key::Insert => ck(Keys::CK_INSERT, Keys::CK_SHIFT_INSERT, Keys::CK_CTRL_INSERT),
        Key::End => ck(Keys::CK_END, Keys::CK_SHIFT_END, Keys::CK_CTRL_END),
        Key::Home => ck(Keys::CK_HOME, Keys::CK_SHIFT_HOME, Keys::CK_CTRL_HOME),
        Key::PageUp => ck(Keys::CK_PGUP, Keys::CK_SHIFT_PGUP, Keys::CK_CTRL_PGUP),
        Key::PageDown => ck(Keys::CK_PGDN, Keys::CK_SHIFT_PGDN, Keys::CK_CTRL_PGDN),
        Key::ArrowUp => ck(Keys::CK_UP, Keys::CK_SHIFT_UP, Keys::CK_CTRL_UP),
        Key::ArrowDown => ck(Keys::CK_DOWN, Keys::CK_SHIFT_DOWN, Keys::CK_CTRL_DOWN),
        Key::ArrowLeft => ck(Keys::CK_LEFT, Keys::CK_SHIFT_LEFT, Keys::CK_CTRL_LEFT),
        Key::ArrowRight => ck(Keys::CK_RIGHT, Keys::CK_SHIFT_RIGHT, Keys::CK_CTRL_RIGHT),
        Key::Enter if m.shift => Keys::CK_SHIFT_ENTER,
        Key::Enter if m.ctrl => Keys::CK_CTRL_ENTER,
        Key::Enter => return Some(cm::input("\r")),
        Key::F1 => Keys::f(1),
        Key::F2 => Keys::f(2),
        Key::F3 => Keys::f(3),
        Key::F4 => Keys::f(4),
        Key::F5 => Keys::f(5),
        Key::F6 => Keys::f(6),
        Key::F7 => Keys::f(7),
        Key::F8 => Keys::f(8),
        Key::F9 => Keys::f(9),
        Key::F10 => Keys::f(10),
        _ if (m.ctrl || m.command) && !m.alt => {
            // key_conversion.captured_control_keys: Ctrl+these are sent as `which - 64`
            let name = key.symbol_or_name();
            let c = name.chars().next()?;
            if name.chars().count() == 1 && "OQFPWATXSGIDEHJKLYUBNCM1234567890".contains(c.to_ascii_uppercase()) {
                c.to_ascii_uppercase() as i32 - 64
            } else {
                return None;
            }
        }
        _ => return None,
    };
    Some(cm::key(code))
}

/// A typed character as the official client sends it (`{` would open a JSON message: bytes).
pub fn raw_char(c: char) -> Value {
    if c == '{' {
        cm::input_bytes(&[123])
    } else {
        cm::input(&c.to_string())
    }
}

fn nav_key(key: Key, m: Modifiers) -> KeyName {
    let _ = m;
    match key {
        Key::PageUp => KeyName::PageUp,
        Key::PageDown => KeyName::PageDown,
        Key::Home => KeyName::Home,
        Key::End => KeyName::End,
        Key::ArrowUp => KeyName::Up,
        Key::ArrowDown => KeyName::Down,
        Key::ArrowLeft => KeyName::Left,
        Key::ArrowRight => KeyName::Right,
        Key::Enter => KeyName::Enter,
        Key::Escape => KeyName::Escape,
        Key::Space => KeyName::Char(' '),
        _ => KeyName::Other,
    }
}

impl Overlays {
    /// Every key of this frame (egui's events): true when the overlays took the keys, so the game
    /// sends none of them. Up or not, a key answers the level-up banner.
    pub fn input(&mut self, ctx: &egui::Context, cx: &mut OverlayCx) -> bool {
        let events = ctx.input(|i| i.events.clone());
        let mut took = false;
        for e in &events {
            took |= self.key(e, cx);
        }
        took || Overlays::up(cx.state) && !cx.watching
    }

    /// One key. True when it was the overlays' (sent or acted on here).
    pub fn key(&mut self, ev: &Event, cx: &mut OverlayCx) -> bool {
        self.sync(cx, None);
        let is_key = matches!(ev, Event::Key { pressed: true, .. } | Event::Text(_));
        if is_key {
            self.level_up.answer(self.now);
        }
        if cx.watching || !Overlays::up(cx.state) {
            return false;
        }
        let mode = derive_mode(cx.state);
        // a server text prompt: the field takes every key
        if mode == Mode::Text && self.field.is_some() {
            match ev {
                Event::Key { key, pressed: true, modifiers, .. } => {
                    let k = match key {
                        Key::Escape => Some(FieldKey::Escape),
                        Key::Enter => Some(FieldKey::Enter),
                        Key::Backspace => Some(FieldKey::Backspace),
                        Key::ArrowUp => Some(FieldKey::Up),
                        Key::ArrowDown => Some(FieldKey::Down),
                        _ => None,
                    };
                    if let Some(k) = k {
                        self.field_key(k, cx);
                    } else if modifiers.ctrl && *key == Key::U {
                        if let Some(f) = self.field.as_mut() {
                            f.text.clear();
                        }
                    }
                }
                Event::Text(t) => {
                    for c in t.chars() {
                        self.field_key(FieldKey::Char(c), cx);
                    }
                }
                Event::Paste(t) => {
                    for c in t.chars().filter(|c| !c.is_control()) {
                        self.field_key(FieldKey::Char(c), cx);
                    }
                }
                _ => return false,
            }
            return true;
        }
        // a menu whose title is the filter's field (menu.js title_prompt)
        if let Some(menu) = cx.state.menus.last().filter(|m| m.title_prompt.as_ref().is_some_and(|t| !t.raw)) {
            match ev {
                Event::Key { key: Key::Enter, pressed: true, .. } => {
                    let t = std::mem::take(&mut self.menu_filter);
                    cx.send(cm::text_input(&format!("{t}\r")));
                }
                Event::Key { key: Key::Escape, pressed: true, .. } => {
                    self.menu_filter.clear();
                    cx.send(cm::key(27));
                }
                Event::Key { key: Key::Backspace, pressed: true, .. } => {
                    self.menu_filter.pop();
                }
                Event::Text(t) => {
                    for c in t.chars() {
                        // macro_mapping: '?' is help, sent as a key even inside the prompt (menu.js:714)
                        if menu.tag == "macro_mapping" && c == '?' {
                            cx.send(cm::key(63));
                        } else {
                            self.menu_filter.push(c);
                        }
                    }
                }
                _ => return false,
            }
            return true;
        }
        match ev {
            Event::Key { key, pressed: true, modifiers, .. } => {
                let m = *modifiers;
                if m.is_none() {
                    if let Some(done) = self.screen_key(*key, mode, cx) {
                        return done;
                    }
                }
                let nk = NavKey { key: nav_key(*key, m), shift: m.shift, ctrl: m.ctrl || m.command, alt: m.alt };
                if self.page_key(nk, mode, cx) {
                    return true;
                }
                if *key == Key::Space && m.is_none() {
                    cx.send(cm::input(" "));
                    return true;
                }
                if let Some(msg) = raw_key(*key, m) {
                    cx.send(msg);
                }
                true
            }
            Event::Text(t) => {
                for c in t.chars() {
                    // a space came as its key, before its text
                    if c == ' ' {
                        continue;
                    }
                    let nk = NavKey::plain(KeyName::Char(c));
                    if !self.page_key(nk, mode, cx) {
                        cx.send(raw_char(c));
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// The paging keys of a menu (menu.js) or a popup's scroller (ui-layouts.js).
    fn page_key(&mut self, nk: NavKey, mode: Mode, cx: &mut OverlayCx) -> bool {
        self.pointer_live = false;
        if mode == Mode::Menu && cx.state.menus.last().is_some_and(|m| m.kind.as_deref() != Some("crt")) {
            return self.menu_key(cx, nk);
        }
        if matches!(mode, Mode::Popup | Mode::Ended) {
            if let Some(p) = cx.state.ui.last() {
                if let Some(intent) = scroll_key_intent(&p.kind, nk) {
                    return self.popup_scroll_by(intent, cx);
                }
            }
        }
        false
    }

    /// The top popup scrolls its own text: space and the paging keys are the scroller's there.
    pub fn popup_scrolls(&self, cx: &OverlayCx) -> bool {
        cx.state.ui.last().is_some_and(|p| crate::menu_nav::is_scroller(&p.kind))
    }

    /// The top overlay has more than it shows, so the bumpers have a page to turn.
    pub fn pageable(&self, cx: &OverlayCx) -> bool {
        let mode = derive_mode(cx.state);
        if mode == Mode::Menu && cx.state.menus.last().is_some_and(|m| m.kind.as_deref() != Some("crt")) {
            return self.menu_geom.scroll_h > self.menu_geom.client_h + 1.0;
        }
        if matches!(mode, Mode::Popup | Mode::Ended) && self.popup_overflows() {
            return true;
        }
        let rows = self.nav.options().page_rows;
        rows > 0 && self.nav.count() as i32 > rows
    }

    /// bindings.ts `screenKey` and the button's action: None leaves the key to the rest.
    fn screen_key(&mut self, key: Key, mode: Mode, cx: &mut OverlayCx) -> Option<bool> {
        let crt_menu = cx.state.menus.last().is_some_and(|m| m.kind.as_deref() == Some("crt"));
        let focus = mode.is_focus(crt_menu);
        let cursor = focus || mode == Mode::Menu;
        // a yes/no's Enter and Space are its default answer: they go to crawl as they are
        if !cursor || (mode == Mode::Yesno && matches!(key, Key::Enter | Key::Space)) {
            return None;
        }
        let scrolls = self.popup_scrolls(cx);
        let multi = cx.state.menus.last().is_some_and(|m| m.flags & MenuFlag::MULTISELECT != 0);
        let button = match key {
            Key::Enter => Some(if mode == Mode::Menu && multi { Button::Start } else { Button::A }),
            Key::Escape => Some(Button::B),
            Key::Space if !scrolls => Some(Button::A),
            Key::PageUp if focus && !scrolls => Some(Button::Lb),
            Key::PageDown if focus && !scrolls => Some(Button::Rb),
            Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight => {
                self.direction(key, mode, crt_menu, cx);
                return Some(true);
            }
            _ => None,
        }?;
        self.button(button, mode, crt_menu, cx);
        Some(true)
    }

    fn direction(&mut self, key: Key, mode: Mode, crt_menu: bool, cx: &mut OverlayCx) {
        if mode == Mode::Menu && !crt_menu {
            let op = match key {
                Key::ArrowUp => MenuOp::Prev,
                Key::ArrowDown => MenuOp::Next,
                Key::ArrowLeft => MenuOp::Left,
                _ => MenuOp::Right,
            };
            return self.menu_op(op, cx);
        }
        let op = match key {
            Key::ArrowUp => FocusOp::Prev,
            Key::ArrowDown => FocusOp::Next,
            Key::ArrowLeft => FocusOp::Left,
            _ => FocusOp::Right,
        };
        self.focus_op(op, cx);
    }

    /// What a button does on this screen (bindings.ts `bindingTable`, for the overlays' modes).
    fn button(&mut self, b: Button, mode: Mode, crt_menu: bool, cx: &mut OverlayCx) {
        let menu = cx.state.menus.last();
        if mode == Mode::Menu && !crt_menu {
            match b {
                Button::A => self.menu_op(MenuOp::Select, cx),
                Button::B => self.menu_op(MenuOp::Cancel, cx),
                // Start is Enter on a menu where rows are marked: it takes what is marked, once something is
                Button::Start => {
                    let marked = menu.is_some_and(|m| m.items.iter().flatten().any(|it| marked_row(&it.text)));
                    if marked {
                        cx.send(cm::key(Keys::ENTER));
                    }
                }
                Button::Lb | Button::Rb => {}
            }
            return;
        }
        match mode {
            Mode::Yesno => {
                // a yes/no: Yes on A, No on B, whatever crawl's default
                let p = parse_prompt(cx.state).unwrap_or_else(default_yesno);
                let want = if b == Button::A { "Y" } else { "N" };
                if let Some(o) = p.options.iter().find(|o| o.hotkey.eq_ignore_ascii_case(want)) {
                    cx.send(answer(&o.hotkey));
                }
            }
            Mode::Newgame if b == Button::B => {
                // a step back where crawl's Escape would abandon the choices so far
                let back = cx.state.ui.last().and_then(|p| newgame_back(&p.data)).unwrap_or(27);
                cx.send(cm::key(back));
            }
            Mode::Prompt if b == Button::B && parse_prompt(cx.state).is_some_and(|p| !p.cancel) => {
                // the stat gain: no way back
            }
            Mode::Prompt if b == Button::A && parse_prompt(cx.state).is_some_and(|p| p.stat_gain) && self.nav.current().is_none() => {
                // and no answer until the cursor lights one
            }
            _ => match b {
                Button::A => self.focus_op(FocusOp::Select, cx),
                Button::B => self.focus_op(FocusOp::Cancel, cx),
                Button::Lb if self.pageable(cx) => self.focus_op(FocusOp::PagePrev, cx),
                Button::Rb if self.pageable(cx) => self.focus_op(FocusOp::PageNext, cx),
                _ => {}
            },
        }
    }

    /// A focus operation, from the pad or from a key that stands for a pad button: one path for
    /// both. With nothing lit the op falls back to the key the official client would send.
    pub fn focus_op(&mut self, op: FocusOp, cx: &mut OverlayCx) {
        self.sync(cx, None);
        self.pointer_live = false;
        let mode = derive_mode(cx.state);
        if mode == Mode::Menu && cx.state.menus.last().is_some_and(|m| m.kind.as_deref() != Some("crt")) {
            let m = match op {
                FocusOp::Next => MenuOp::Next,
                FocusOp::Prev => MenuOp::Prev,
                FocusOp::Left => MenuOp::Left,
                FocusOp::Right => MenuOp::Right,
                FocusOp::Select => MenuOp::Select,
                FocusOp::AltSelect => MenuOp::AltSelect,
                FocusOp::Cancel => MenuOp::Cancel,
                FocusOp::PageNext => MenuOp::PageNext,
                FocusOp::PagePrev => MenuOp::PagePrev,
                FocusOp::First => MenuOp::First,
                FocusOp::Last => MenuOp::Last,
            };
            return self.menu_op(m, cx);
        }
        // text to read with nothing lit: up, down and the bumpers read it
        let reading = mode == Mode::Popup && self.popup_scrolls(cx) && self.nav.current().is_none() && matches!(op, FocusOp::Next | FocusOp::Prev | FocusOp::PageNext | FocusOp::PagePrev);
        let moved = !reading && self.nav.op(op);
        self.run_fired(cx);
        if !moved {
            self.focus_fallback(mode, op, cx);
        }
    }

    fn focus_fallback(&mut self, mode: Mode, op: FocusOp, cx: &mut OverlayCx) {
        // a yes/no has no cursor: nothing moves
        if mode == Mode::Yesno {
            return;
        }
        // a popup that scrolls on the client takes the moves itself
        if matches!(mode, Mode::Popup | Mode::Ended) && self.popup_scrolls(cx) {
            let k = match op {
                FocusOp::Prev => Some(ScrollKey::LineUp),
                FocusOp::Next => Some(ScrollKey::LineDown),
                FocusOp::PagePrev => Some(ScrollKey::PageUp),
                FocusOp::PageNext => Some(ScrollKey::PageDown),
                FocusOp::First => Some(ScrollKey::Home),
                FocusOp::Last => Some(ScrollKey::End),
                _ => None,
            };
            if let Some(k) = k {
                if self.popup_scroll_by(k, cx) {
                    return;
                }
            }
        }
        let closes = matches!(mode, Mode::Popup | Mode::Dialog);
        let code = match op {
            FocusOp::Select => Some(if closes { Keys::ESC } else { Keys::ENTER }),
            FocusOp::AltSelect => None,
            FocusOp::Cancel => Some(Keys::ESC),
            FocusOp::Prev => Some(Keys::CK_UP),
            FocusOp::Next => Some(Keys::CK_DOWN),
            FocusOp::Left => Some(Keys::CK_LEFT),
            FocusOp::Right => Some(Keys::CK_RIGHT),
            FocusOp::PageNext => Some(Keys::CK_PGDN),
            FocusOp::PagePrev => Some(Keys::CK_PGUP),
            FocusOp::First => Some(Keys::CK_HOME),
            FocusOp::Last => Some(Keys::CK_END),
        };
        if let Some(c) = code {
            cx.send(cm::key(c));
        }
    }

    /// The phone's Back, a popup's close button: what the game screen says, else Escape.
    pub fn back(&mut self, cx: &mut OverlayCx) {
        match cx.back.as_mut() {
            Some(f) => f(),
            None => cx.send(cm::key(27)),
        }
    }

    /// The focusables of the top overlay (for a game screen that draws its own hints).
    pub fn stops(&self) -> &[Focusable] {
        self.nav.items()
    }
}

/// A marked row says so between its letter and its text: `a + club`, `a # 3 darts`.
fn marked_row(text: &str) -> bool {
    let t = isorun_webtiles::formatted_string_to_text(text);
    let c: Vec<char> = t.trim_start().chars().collect();
    c.len() > 3 && !c[0].is_whitespace() && c[1] == ' ' && (c[2] == '+' || c[2] == '#') && c[3] == ' '
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_keys_as_the_official_client() {
        assert_eq!(raw_key(Key::Escape, Modifiers::NONE), Some(cm::key(27)));
        assert_eq!(raw_key(Key::Enter, Modifiers::NONE), Some(cm::input("\r")));
        assert_eq!(raw_key(Key::ArrowUp, Modifiers::SHIFT), Some(cm::key(Keys::CK_SHIFT_UP)));
        assert_eq!(raw_key(Key::F, Modifiers::CTRL), Some(cm::key(6)));
        assert_eq!(raw_key(Key::Z, Modifiers::CTRL), None);
        assert_eq!(raw_char('{'), cm::input_bytes(&[123]));
        assert_eq!(raw_char('a'), cm::input("a"));
    }

    #[test]
    fn marked_rows() {
        assert!(marked_row(" a + club"));
        assert!(marked_row(" b # 3 darts"));
        assert!(!marked_row(" a - club"));
    }
}
