//! What the client sends: the special keycodes ([`Keys`]), [`ctrl`], the client message builders
//! ([`cm`]), [`key_message`], and the constant tables the server's numbers mean ([`MouseMode`],
//! [`MenuFlag`], [`UiState`], [`MapFeature`]). Every server message is `{ "msg": ..., ...fields }`;
//! the reducer reads them as `serde_json::Value`.

use serde_json::{json, Value};

use crate::wire::{js_string, Obj};

/// Key codes (cio.h via key_conversion.js). The `CK_*` codes count up from -255 in cio.h's
/// order, with a placeholder for tab after `CK_PGDN` and five (enter, backspace, escape, delete,
/// space: only offsets there) before `CK_SHIFT_ENTER`.
pub struct Keys;

impl Keys {
    pub const ESC: i32 = 27;
    pub const ENTER: i32 = 13;
    pub const TAB: i32 = 9;
    pub const BACKSPACE: i32 = 8;
    pub const SPACE: i32 = 32;

    pub const CK_DELETE: i32 = -255;
    pub const CK_UP: i32 = Self::CK_DELETE + 1;
    pub const CK_DOWN: i32 = Self::CK_UP + 1;
    pub const CK_LEFT: i32 = Self::CK_DOWN + 1;
    pub const CK_RIGHT: i32 = Self::CK_LEFT + 1;
    pub const CK_INSERT: i32 = Self::CK_RIGHT + 1;
    pub const CK_HOME: i32 = Self::CK_INSERT + 1;
    pub const CK_END: i32 = Self::CK_HOME + 1;
    pub const CK_CLEAR: i32 = Self::CK_END + 1;
    pub const CK_PGUP: i32 = Self::CK_CLEAR + 1;
    pub const CK_PGDN: i32 = Self::CK_PGUP + 1;
    // + 1: the tab placeholder
    pub const CK_SHIFT_UP: i32 = Self::CK_PGDN + 2;
    pub const CK_SHIFT_DOWN: i32 = Self::CK_SHIFT_UP + 1;
    pub const CK_SHIFT_LEFT: i32 = Self::CK_SHIFT_DOWN + 1;
    pub const CK_SHIFT_RIGHT: i32 = Self::CK_SHIFT_LEFT + 1;
    pub const CK_SHIFT_INSERT: i32 = Self::CK_SHIFT_RIGHT + 1;
    pub const CK_SHIFT_HOME: i32 = Self::CK_SHIFT_INSERT + 1;
    pub const CK_SHIFT_END: i32 = Self::CK_SHIFT_HOME + 1;
    pub const CK_SHIFT_CLEAR: i32 = Self::CK_SHIFT_END + 1;
    pub const CK_SHIFT_PGUP: i32 = Self::CK_SHIFT_CLEAR + 1;
    pub const CK_SHIFT_PGDN: i32 = Self::CK_SHIFT_PGUP + 1;
    pub const CK_SHIFT_TAB: i32 = Self::CK_SHIFT_PGDN + 1;
    pub const CK_CTRL_UP: i32 = Self::CK_SHIFT_TAB + 1;
    pub const CK_CTRL_DOWN: i32 = Self::CK_CTRL_UP + 1;
    pub const CK_CTRL_LEFT: i32 = Self::CK_CTRL_DOWN + 1;
    pub const CK_CTRL_RIGHT: i32 = Self::CK_CTRL_LEFT + 1;
    pub const CK_CTRL_INSERT: i32 = Self::CK_CTRL_RIGHT + 1;
    pub const CK_CTRL_HOME: i32 = Self::CK_CTRL_INSERT + 1;
    pub const CK_CTRL_END: i32 = Self::CK_CTRL_HOME + 1;
    pub const CK_CTRL_CLEAR: i32 = Self::CK_CTRL_END + 1;
    pub const CK_CTRL_PGUP: i32 = Self::CK_CTRL_CLEAR + 1;
    pub const CK_CTRL_PGDN: i32 = Self::CK_CTRL_PGUP + 1;
    pub const CK_CTRL_TAB: i32 = Self::CK_CTRL_PGDN + 1;
    pub const CK_CTRL_SHIFT_UP: i32 = Self::CK_CTRL_TAB + 1;
    pub const CK_CTRL_SHIFT_DOWN: i32 = Self::CK_CTRL_SHIFT_UP + 1;
    pub const CK_CTRL_SHIFT_LEFT: i32 = Self::CK_CTRL_SHIFT_DOWN + 1;
    pub const CK_CTRL_SHIFT_RIGHT: i32 = Self::CK_CTRL_SHIFT_LEFT + 1;
    pub const CK_CTRL_SHIFT_INSERT: i32 = Self::CK_CTRL_SHIFT_RIGHT + 1;
    pub const CK_CTRL_SHIFT_HOME: i32 = Self::CK_CTRL_SHIFT_INSERT + 1;
    pub const CK_CTRL_SHIFT_END: i32 = Self::CK_CTRL_SHIFT_HOME + 1;
    pub const CK_CTRL_SHIFT_CLEAR: i32 = Self::CK_CTRL_SHIFT_END + 1;
    pub const CK_CTRL_SHIFT_PGUP: i32 = Self::CK_CTRL_SHIFT_CLEAR + 1;
    pub const CK_CTRL_SHIFT_PGDN: i32 = Self::CK_CTRL_SHIFT_PGUP + 1;
    pub const CK_CTRL_SHIFT_TAB: i32 = Self::CK_CTRL_SHIFT_PGDN + 1;
    // + 5: the ENTER, BKSP, ESCAPE, DELETE and SPACE placeholders (cio.h)
    pub const CK_SHIFT_ENTER: i32 = Self::CK_CTRL_SHIFT_TAB + 6;
    pub const CK_SHIFT_BKSP: i32 = Self::CK_SHIFT_ENTER + 1;
    pub const CK_SHIFT_ESCAPE: i32 = Self::CK_SHIFT_BKSP + 1;
    pub const CK_SHIFT_DELETE: i32 = Self::CK_SHIFT_ESCAPE + 1;
    pub const CK_SHIFT_SPACE: i32 = Self::CK_SHIFT_DELETE + 1;
    pub const CK_CTRL_ENTER: i32 = Self::CK_SHIFT_SPACE + 1;
    pub const CK_CTRL_BKSP: i32 = Self::CK_CTRL_ENTER + 1;
    pub const CK_CTRL_ESCAPE: i32 = Self::CK_CTRL_BKSP + 1;
    pub const CK_CTRL_DELETE: i32 = Self::CK_CTRL_ESCAPE + 1;
    pub const CK_CTRL_SPACE: i32 = Self::CK_CTRL_DELETE + 1;
    pub const CK_CTRL_SHIFT_ENTER: i32 = Self::CK_CTRL_SPACE + 1;
    pub const CK_CTRL_SHIFT_BKSP: i32 = Self::CK_CTRL_SHIFT_ENTER + 1;
    pub const CK_CTRL_SHIFT_ESCAPE: i32 = Self::CK_CTRL_SHIFT_BKSP + 1;
    pub const CK_CTRL_SHIFT_DELETE: i32 = Self::CK_CTRL_SHIFT_ESCAPE + 1;
    pub const CK_CTRL_SHIFT_SPACE: i32 = Self::CK_CTRL_SHIFT_DELETE + 1;

    /// The keypad's digit `n`.
    pub const fn numpad(n: i32) -> i32 {
        -1000 - n
    }

    /// F1..F19 as key_conversion.js sends them (-265..-283); F11 / F12 are left to the browser.
    pub const fn f(n: i32) -> i32 {
        -264 - n
    }
}

/// Ctrl-A .. Ctrl-Z.
pub fn ctrl(letter: char) -> i32 {
    letter.to_uppercase().next().unwrap_or(letter) as i32 - 64
}

/// The client messages.
///
/// The official client serializes through `comm.js send_message`, which sets `data["msg"] =
/// msg` *after* the caller's fields, so `msg` is the last key on the wire. Only `key` and byte
/// `input` escape it: client.js hand-builds those two as strings with `msg` first. The order
/// carries no meaning to any parser, but it is what the server is sent, so it is what these
/// build (serde_json keeps insertion order here).
pub mod cm {
    use super::*;

    pub fn login(username: &str, password: &str) -> Value {
        json!({ "username": username, "password": password, "msg": "login" })
    }
    pub fn token_login(cookie: &str) -> Value {
        json!({ "cookie": cookie, "msg": "token_login" })
    }
    pub fn set_login_cookie() -> Value {
        json!({ "msg": "set_login_cookie" })
    }
    pub fn forget_login_cookie(cookie: &str) -> Value {
        json!({ "cookie": cookie, "msg": "forget_login_cookie" })
    }
    pub fn register(username: &str, password: &str, email: &str) -> Value {
        json!({ "username": username, "password": password, "email": email, "msg": "register" })
    }
    pub fn play(game_id: &str) -> Value {
        json!({ "game_id": game_id, "msg": "play" })
    }
    pub fn watch(username: &str) -> Value {
        json!({ "username": username, "msg": "watch" })
    }
    pub fn go_lobby() -> Value {
        json!({ "msg": "go_lobby" })
    }
    pub fn force_terminate(answer: bool) -> Value {
        json!({ "answer": answer, "msg": "force_terminate" })
    }
    pub fn chat(text: &str) -> Value {
        json!({ "text": text, "msg": "chat_msg" })
    }
    pub fn pong() -> Value {
        json!({ "msg": "pong" })
    }
    /// Lobby: ask for the rc file of a game (answered by `rcfile_contents`).
    pub fn get_rc(game_id: &str) -> Value {
        json!({ "game_id": game_id, "msg": "get_rc" })
    }
    /// Lobby: save the rc file of a game.
    pub fn set_rc(game_id: &str, contents: &str) -> Value {
        json!({ "game_id": game_id, "contents": contents, "msg": "set_rc" })
    }
    /// Any key while the `stale_processes` notice shows: keep the stale games alive.
    pub fn stop_stale_process_purge() -> Value {
        json!({ "msg": "stop_stale_process_purge" })
    }
    /// Admin panel: a server-wide announcement.
    pub fn admin_announce(text: &str) -> Value {
        json!({ "text": text, "msg": "admin_announce" })
    }
    /// Consumables action panel (action_panel.js): use the item in `slot`.
    pub fn inv_item_action(slot: i64) -> Value {
        json!({ "slot": slot, "msg": "inv_item_action" })
    }
    /// Consumables action panel: describe the item in `slot`.
    pub fn inv_item_describe(slot: i64) -> Value {
        json!({ "slot": slot, "msg": "inv_item_describe" })
    }
    /// Consumables action panel: open the game menu.
    pub fn main_menu_action() -> Value {
        json!({ "msg": "main_menu_action" })
    }
    /// options.js send_option: an rc line the client changed (the action panel's own
    /// settings); the value is written as JavaScript's `String()` writes it.
    pub fn set_option(name: &str, value: &Value) -> Value {
        json!({ "line": format!("{name} = {}", js_string(value)), "msg": "set_option" })
    }
    /// Printable text, written to the input buffer atomically.
    pub fn input(text: &str) -> Value {
        json!({ "text": text, "msg": "input" })
    }
    /// Raw bytes into the input buffer (used for `{` and alt sequences); client.js
    /// `send_bytes`, so `msg` leads.
    pub fn input_bytes(data: &[u8]) -> Value {
        json!({ "msg": "input", "data": data })
    }
    /// Control characters and CK_* codes; client.js `send_keycode`, so `msg` leads.
    pub fn key(keycode: i32) -> Value {
        json!({ "msg": "key", "keycode": keycode })
    }
    /// Text-with-terminator used by line readers and menu filters.
    pub fn text_input(text: &str) -> Value {
        json!({ "text": text, "msg": "text_input" })
    }
    pub fn click_cell(x: i32, y: i32, button: i32) -> Value {
        json!({ "x": x, "y": y, "button": button, "msg": "click_cell" })
    }
    /// Pointer over a cell while targeting: the server moves the targeting cursor there.
    pub fn target_cursor(x: i32, y: i32) -> Value {
        json!({ "x": x, "y": y, "msg": "target_cursor" })
    }
    /// `mouse` is false in the TS default.
    pub fn menu_hover(hover: i64, mouse: bool) -> Value {
        json!({ "hover": hover, "mouse": mouse, "msg": "menu_hover" })
    }
    pub fn menu_scroll(first: i64, last: i64, hover: i64) -> Value {
        json!({ "first": first, "last": last, "hover": hover, "msg": "menu_scroll" })
    }
    pub fn formatted_scroller_scroll(scroll: i64) -> Value {
        json!({ "scroll": scroll, "msg": "formatted_scroller_scroll" })
    }
    /// The state's own fields, then `msg` (a `msg` among them keeps its place and is overwritten).
    pub fn ui_state_sync(state: &Obj) -> Value {
        let mut o = state.clone();
        o.insert("msg".into(), "ui_state_sync".into());
        Value::Object(o)
    }
    pub fn outer_menu_focus(hotkey: i64, menu_id: &str) -> Value {
        json!({ "hotkey": hotkey, "menu_id": menu_id, "msg": "outer_menu_focus" })
    }
}

/// The rule for turning a key press into the right message: printable ASCII as `input` (but `{`
/// as bytes), other code points below 0x10000 as `input`, the rest as `key`. A lone surrogate,
/// which JavaScript would send as text, goes as a `key` here.
pub fn key_message(keycode: i32) -> Value {
    if (32..127).contains(&keycode) && keycode != 123 {
        return cm::input(&char::from(keycode as u8).to_string());
    }
    if keycode == 123 {
        return cm::input_bytes(&[123]);
    }
    if keycode > 127 && keycode < 0x10000 {
        if let Some(c) = char::from_u32(keycode as u32) {
            return cm::input(&c.to_string());
        }
    }
    cm::key(keycode)
}

/// Mouse modes (tileweb.h): the server's `input_mode`.
pub struct MouseMode;

impl MouseMode {
    pub const NORMAL: i64 = 0;
    pub const COMMAND: i64 = 1;
    pub const TARGET: i64 = 2;
    pub const TARGET_DIR: i64 = 3;
    pub const TARGET_PATH: i64 = 4;
    pub const MORE: i64 = 5;
    pub const MACRO: i64 = 6;
    pub const PROMPT: i64 = 7;
    pub const YESNO: i64 = 8;
}

/// A menu's `flags` (menu.h).
pub struct MenuFlag;

impl MenuFlag {
    pub const NOSELECT: i64 = 0x0001;
    pub const SINGLESELECT: i64 = 0x0002;
    pub const MULTISELECT: i64 = 0x0004;
    pub const SELECT_QTY: i64 = 0x0008;
    pub const ANYPRINTABLE: i64 = 0x0010;
    pub const SELECT_BY_PAGE: i64 = 0x0020;
    pub const INIT_HOVER: i64 = 0x0040;
    pub const WRAP: i64 = 0x0080;
    pub const ALLOW_FILTER: i64 = 0x0100;
    pub const ALLOW_FORMATTING: i64 = 0x0200;
    pub const SHOW_PAGENUMBERS: i64 = 0x0400;
    pub const START_AT_END: i64 = 0x1000;
    pub const PRESELECTED: i64 = 0x2000;
    pub const ARROWS_SELECT: i64 = 0x40000;
    /// The inventory a page per category (drop): a category's own key selects it, `,` does not.
    pub const PAGED_INVENTORY: i64 = 0x200000;
}

/// The server's `ui_state`.
pub struct UiState;

impl UiState {
    pub const NORMAL: i64 = 0;
    pub const CRT: i64 = 1;
    pub const VIEW_MAP: i64 = 2;
}

/// Minimap feature categories (map-feature.h), as published by enums.js: a cell's `mf`.
pub struct MapFeature;

impl MapFeature {
    pub const UNSEEN: i64 = 0;
    pub const FLOOR: i64 = 1;
    pub const WALL: i64 = 2;
    pub const MAP_FLOOR: i64 = 3;
    pub const MAP_WALL: i64 = 4;
    pub const DOOR: i64 = 5;
    pub const ITEM: i64 = 6;
    pub const MONS_FRIENDLY: i64 = 7;
    pub const MONS_PEACEFUL: i64 = 8;
    pub const MONS_NEUTRAL: i64 = 9;
    pub const MONS_HOSTILE: i64 = 10;
    pub const MONS_NO_EXP: i64 = 11;
    pub const STAIR_UP: i64 = 12;
    pub const STAIR_DOWN: i64 = 13;
    pub const STAIR_BRANCH: i64 = 14;
    pub const FEATURE: i64 = 15;
    pub const WATER: i64 = 16;
    pub const LAVA: i64 = 17;
    pub const TRAP: i64 = 18;
    pub const EXCL_ROOT: i64 = 19;
    pub const EXCL: i64 = 20;
    pub const PLAYER: i64 = 21;
    pub const DEEP_WATER: i64 = 22;
    pub const PORTAL: i64 = 23;
    pub const TRANSPORTER: i64 = 24;
    pub const TRANSPORTER_LANDING: i64 = 25;
    pub const EXPLORE_HORIZON: i64 = 26;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keycodes_count_as_cio_h_does() {
        // the values protocol.ts computes
        let all = [
            Keys::CK_DELETE,
            Keys::CK_UP,
            Keys::CK_DOWN,
            Keys::CK_LEFT,
            Keys::CK_RIGHT,
            Keys::CK_INSERT,
            Keys::CK_HOME,
            Keys::CK_END,
            Keys::CK_CLEAR,
            Keys::CK_PGUP,
            Keys::CK_PGDN,
            Keys::CK_SHIFT_UP,
            Keys::CK_SHIFT_DOWN,
            Keys::CK_SHIFT_LEFT,
            Keys::CK_SHIFT_RIGHT,
            Keys::CK_SHIFT_INSERT,
            Keys::CK_SHIFT_HOME,
            Keys::CK_SHIFT_END,
            Keys::CK_SHIFT_CLEAR,
            Keys::CK_SHIFT_PGUP,
            Keys::CK_SHIFT_PGDN,
            Keys::CK_SHIFT_TAB,
            Keys::CK_CTRL_UP,
            Keys::CK_CTRL_DOWN,
            Keys::CK_CTRL_LEFT,
            Keys::CK_CTRL_RIGHT,
            Keys::CK_CTRL_INSERT,
            Keys::CK_CTRL_HOME,
            Keys::CK_CTRL_END,
            Keys::CK_CTRL_CLEAR,
            Keys::CK_CTRL_PGUP,
            Keys::CK_CTRL_PGDN,
            Keys::CK_CTRL_TAB,
            Keys::CK_CTRL_SHIFT_UP,
            Keys::CK_CTRL_SHIFT_DOWN,
            Keys::CK_CTRL_SHIFT_LEFT,
            Keys::CK_CTRL_SHIFT_RIGHT,
            Keys::CK_CTRL_SHIFT_INSERT,
            Keys::CK_CTRL_SHIFT_HOME,
            Keys::CK_CTRL_SHIFT_END,
            Keys::CK_CTRL_SHIFT_CLEAR,
            Keys::CK_CTRL_SHIFT_PGUP,
            Keys::CK_CTRL_SHIFT_PGDN,
            Keys::CK_CTRL_SHIFT_TAB,
            Keys::CK_SHIFT_ENTER,
            Keys::CK_SHIFT_BKSP,
            Keys::CK_SHIFT_ESCAPE,
            Keys::CK_SHIFT_DELETE,
            Keys::CK_SHIFT_SPACE,
            Keys::CK_CTRL_ENTER,
            Keys::CK_CTRL_BKSP,
            Keys::CK_CTRL_ESCAPE,
            Keys::CK_CTRL_DELETE,
            Keys::CK_CTRL_SPACE,
            Keys::CK_CTRL_SHIFT_ENTER,
            Keys::CK_CTRL_SHIFT_BKSP,
            Keys::CK_CTRL_SHIFT_ESCAPE,
            Keys::CK_CTRL_SHIFT_DELETE,
            Keys::CK_CTRL_SHIFT_SPACE,
        ];
        let expected: Vec<i32> = (-255..=-245).chain(-243..=-211).chain(-205..=-191).collect();
        assert_eq!(all.to_vec(), expected);
        assert_eq!(Keys::f(1), -265);
        assert_eq!(Keys::numpad(5), -1005);
        assert_eq!(ctrl('a'), 1);
        assert_eq!(ctrl('Z'), 26);
    }
}
