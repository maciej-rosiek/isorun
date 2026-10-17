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
use isorun_input::keys::{key_inputs, keydown_message, KeyInput, KeyLike};
use isorun_webtiles::{cm, GameState, Keys, MenuFlag, MenuState};

/// The pad's face buttons and bumpers a key stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Button {
    A,
    B,
    Start,
    Lb,
    Rb,
}

/// A `KeyLike` as the paging keys read it (menu.js, ui-layouts.js).
fn nav_key(k: &KeyLike) -> NavKey {
    let key = match k.key.as_str() {
        "PageUp" => KeyName::PageUp,
        "PageDown" => KeyName::PageDown,
        "Home" => KeyName::Home,
        "End" => KeyName::End,
        "ArrowUp" => KeyName::Up,
        "ArrowDown" => KeyName::Down,
        "ArrowLeft" => KeyName::Left,
        "ArrowRight" => KeyName::Right,
        "Enter" => KeyName::Enter,
        "Escape" => KeyName::Escape,
        "+" if k.code == "NumpadAdd" => KeyName::NumpadAdd,
        "-" if k.code == "NumpadSubtract" => KeyName::NumpadSubtract,
        s => {
            let mut c = s.chars();
            match (c.next(), c.next()) {
                (Some(ch), None) => KeyName::Char(ch),
                _ => KeyName::Other,
            }
        }
    };
    NavKey { key, shift: k.shift, ctrl: k.ctrl || k.meta, alt: k.alt }
}

/// The character a key types, if it types one (no Ctrl, Alt or Meta).
fn typed(k: &KeyLike) -> Option<char> {
    if k.ctrl || k.alt || k.meta {
        return None;
    }
    let mut c = k.key.chars();
    match (c.next(), c.next()) {
        (Some(ch), None) if !ch.is_control() => Some(ch),
        _ => None,
    }
}

impl Overlays {
    /// Every key of this frame, for a screen with no router of its own (the examples): the
    /// whole of `key` for each. True when the overlays took the keys. Up or not, a key answers
    /// the level-up banner. The game screen does not call this: its router decides (game.ts
    /// `onKeyDown`) and hands the overlays `text_key`, `key_like` and the pad's ops.
    pub fn input(&mut self, ctx: &egui::Context, cx: &mut OverlayCx) -> bool {
        let (events, modifiers) = ctx.input(|i| (i.events.clone(), i.modifiers));
        let mut took = false;
        for k in key_inputs(&events, modifiers) {
            if let KeyInput::Down { key, .. } = k {
                took |= self.key(&key, cx);
            }
        }
        for e in &events {
            if let egui::Event::Paste(t) = e {
                took |= self.paste(t, cx);
            }
        }
        took || Overlays::up(cx.state) && !cx.watching
    }

    /// One key with nothing deciding for the overlays: the field's, the keyboard as a pad
    /// (bindings.ts `screenKey`), the paging keys, else to crawl as the official client sends it
    /// (keys.ts `keydownMessage`). True when it was the overlays'.
    pub fn key(&mut self, key: &KeyLike, cx: &mut OverlayCx) -> bool {
        self.sync(cx, None);
        self.level_up.answer(self.now);
        if cx.watching || !Overlays::up(cx.state) {
            return false;
        }
        if self.text_entry(cx.state) {
            return self.text_key(key, cx);
        }
        let mode = derive_mode(cx.state);
        if key.plain() {
            if let Some(done) = self.screen_key(&key.key, mode, cx) {
                return done;
            }
        }
        if self.key_like(key, cx) {
            return true;
        }
        if key.key == " " && key.plain() {
            cx.send(cm::input(" "));
            return true;
        }
        if let Some(msg) = keydown_message(key) {
            cx.send(msg);
        }
        true
    }

    /// A text field of crawl's has the keys: the server's text prompt, or a menu whose title is
    /// the filter's field (menu.js title_prompt). The router leaves every key to it then, as the
    /// web's focused `<input>` takes them (game.ts `isTextEntry`).
    pub fn text_entry(&self, state: &GameState) -> bool {
        (derive_mode(state) == Mode::Text && self.field.is_some() && state.text_input.is_some()) || Self::filter_menu(state).is_some()
    }

    fn filter_menu(state: &GameState) -> Option<&MenuState> {
        state.menus.last().filter(|m| m.title_prompt.as_ref().is_some_and(|t| !t.raw))
    }

    /// A key for the text field that has the keys (`text_entry`): true when it took it.
    pub fn text_key(&mut self, key: &KeyLike, cx: &mut OverlayCx) -> bool {
        self.sync(cx, None);
        if cx.watching {
            return false;
        }
        // a server text prompt: the field takes every key
        if derive_mode(cx.state) == Mode::Text && self.field.is_some() {
            let k = match key.key.as_str() {
                "Escape" => Some(FieldKey::Escape),
                "Enter" => Some(FieldKey::Enter),
                "Backspace" => Some(FieldKey::Backspace),
                "ArrowUp" => Some(FieldKey::Up),
                "ArrowDown" => Some(FieldKey::Down),
                _ => typed(key).map(FieldKey::Char),
            };
            if let Some(k) = k {
                self.field_key(k, cx);
            } else if key.ctrl && key.key.eq_ignore_ascii_case("u") {
                if let Some(f) = self.field.as_mut() {
                    f.text.clear();
                }
            }
            return true;
        }
        // a menu whose title is the filter's field (menu.js title_prompt)
        let Some(menu) = Self::filter_menu(cx.state) else { return false };
        match key.key.as_str() {
            "Enter" => {
                let t = std::mem::take(&mut self.menu_filter);
                cx.send(cm::text_input(&format!("{t}\r")));
            }
            "Escape" => {
                self.menu_filter.clear();
                cx.send(cm::key(27));
            }
            "Backspace" => {
                self.menu_filter.pop();
            }
            _ => match typed(key) {
                // macro_mapping: '?' is help, sent as a key even inside the prompt (menu.js:714)
                Some('?') if menu.tag == "macro_mapping" => cx.send(cm::key(63)),
                Some(c) => self.menu_filter.push(c),
                None => return false,
            },
        }
        true
    }

    /// Pasted text into the field that has the keys: true when one took it.
    pub fn paste(&mut self, text: &str, cx: &mut OverlayCx) -> bool {
        if !self.text_entry(cx.state) || cx.watching {
            return false;
        }
        for c in text.chars().filter(|c| !c.is_control()) {
            let k = KeyLike { key: c.to_string(), ..Default::default() };
            self.text_key(&k, cx);
        }
        true
    }

    /// The top menu's or popup's own keys on the client (overlays.ts `menuKey`, `popupKey`): the
    /// paging keys and characters move a menu's hover and page it, and scroll a popup's text.
    /// The router calls it where game.ts does; true when taken.
    pub fn key_like(&mut self, key: &KeyLike, cx: &mut OverlayCx) -> bool {
        self.sync(cx, None);
        if cx.watching {
            return false;
        }
        let mode = derive_mode(cx.state);
        self.page_key(nav_key(key), mode, cx)
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
    fn screen_key(&mut self, key: &str, mode: Mode, cx: &mut OverlayCx) -> Option<bool> {
        let crt_menu = cx.state.menus.last().is_some_and(|m| m.kind.as_deref() == Some("crt"));
        let focus = mode.is_focus(crt_menu);
        let cursor = focus || mode == Mode::Menu;
        // a yes/no's Enter and Space are its default answer: they go to crawl as they are
        if !cursor || (mode == Mode::Yesno && matches!(key, "Enter" | " ")) {
            return None;
        }
        let scrolls = self.popup_scrolls(cx);
        let multi = cx.state.menus.last().is_some_and(|m| m.flags & MenuFlag::MULTISELECT != 0);
        let button = match key {
            "Enter" => Some(if mode == Mode::Menu && multi { Button::Start } else { Button::A }),
            "Escape" => Some(Button::B),
            " " if !scrolls => Some(Button::A),
            "PageUp" if focus && !scrolls => Some(Button::Lb),
            "PageDown" if focus && !scrolls => Some(Button::Rb),
            "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" => {
                self.direction(key, mode, crt_menu, cx);
                return Some(true);
            }
            _ => None,
        }?;
        self.button(button, mode, crt_menu, cx);
        Some(true)
    }

    fn direction(&mut self, key: &str, mode: Mode, crt_menu: bool, cx: &mut OverlayCx) {
        if mode == Mode::Menu && !crt_menu {
            let op = match key {
                "ArrowUp" => MenuOp::Prev,
                "ArrowDown" => MenuOp::Next,
                "ArrowLeft" => MenuOp::Left,
                _ => MenuOp::Right,
            };
            return self.menu_op(op, cx);
        }
        let op = match key {
            "ArrowUp" => FocusOp::Prev,
            "ArrowDown" => FocusOp::Next,
            "ArrowLeft" => FocusOp::Left,
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
    fn keys_as_the_paging_reads_them() {
        let k = |key: &str| KeyLike { key: key.into(), ..Default::default() };
        assert_eq!(nav_key(&k("PageDown")).key, KeyName::PageDown);
        assert_eq!(nav_key(&k(">")).key, KeyName::Char('>'));
        assert_eq!(nav_key(&k("F5")).key, KeyName::Other);
        assert_eq!(typed(&k("a")), Some('a'));
        assert_eq!(typed(&k("Enter")), None);
        assert_eq!(typed(&KeyLike { key: "u".into(), ctrl: true, ..Default::default() }), None);
    }

    #[test]
    fn marked_rows() {
        assert!(marked_row(" a + club"));
        assert!(marked_row(" b # 3 darts"));
        assert!(!marked_row(" a - club"));
    }
}
