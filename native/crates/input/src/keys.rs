//! Keyboard -> WebTiles messages, reproducing the official client's rules
//! (webserver/static/scripts/client.js `handle_keydown` / `handle_keypress`
//! and key_conversion.js), plus detection of direction keys so the game can
//! rewrite them as facing-relative movement in command / targeting modes
//! (the port of `keys.ts`).
//!
//! The official client keys everything off the legacy `e.which` (keyCode),
//! with numpad digits normalised to 96..105 by `e.code`. [`KeyLike`] is the
//! subset of a DOM `KeyboardEvent` the encoder reads: on the web it is filled
//! from the browser's event, natively from egui's events ([`key_inputs`]),
//! and the messages come out identical.

use isorun_webtiles::{cm, Keys};
use serde_json::Value;

/// The subset of KeyboardEvent the encoder reads; plain values work for tests.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct KeyLike {
    /// The DOM `key`: the character typed ("a", "A", "!"), or the key's name ("ArrowUp", "Enter", " ").
    pub key: String,
    /// The DOM `code`: the physical key ("KeyA", "Numpad8"), or "" when unknown.
    pub code: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
    /// The browser's legacy keyCode, when it gave one.
    pub key_code: Option<u32>,
}

impl KeyLike {
    /// A key with no modifiers.
    pub fn new(key: &str, code: &str) -> Self {
        KeyLike { key: key.into(), code: code.into(), ..Default::default() }
    }

    pub fn plain(&self) -> bool {
        !self.ctrl && !self.alt && !self.meta && !self.shift
    }
}

/// Which set a direction key came from (servers.ts `DirSource`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyDirSource {
    Vim,
    Numpad,
    Arrows,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DirMod {
    None,
    Shift,
    Ctrl,
}

/// A direction key: the absolute compass direction it means (N=0, clockwise), with Shift (run)
/// or Ctrl (attack), and the set it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirKey {
    pub abs: u8,
    pub modifier: DirMod,
    pub source: KeyDirSource,
}

/// vim / numpad / arrow keys mapped to absolute compass directions (N=0).
fn vim(k: char) -> Option<u8> {
    Some(match k {
        'k' => 0,
        'u' => 1,
        'l' => 2,
        'n' => 3,
        'j' => 4,
        'b' => 5,
        'h' => 6,
        'y' => 7,
        _ => return None,
    })
}

fn numpad(code: &str) -> Option<u8> {
    Some(match code {
        "Numpad8" => 0,
        "Numpad9" => 1,
        "Numpad6" => 2,
        "Numpad3" => 3,
        "Numpad2" => 4,
        "Numpad1" => 5,
        "Numpad4" => 6,
        "Numpad7" => 7,
        _ => return None,
    })
}

fn arrows(key: &str) -> Option<u8> {
    Some(match key {
        "ArrowUp" => 0,
        "ArrowRight" => 2,
        "ArrowDown" => 4,
        "ArrowLeft" => 6,
        _ => return None,
    })
}

/// Returns the absolute direction a key means, which of the three key sets it
/// came from, or None if it is not a direction key. The set is reported for
/// what reads it (servers.ts `leftRightTurns` answers the same for all three).
/// Numpad keys are recognised by `code`, so NumLock does not
/// matter, and Ctrl / Shift ride along as the attack / run variants (Ctrl wins
/// when both are held, as in `mod`'s priority). Alt / Meta combos are not
/// directions and pass through to keydown_message untouched.
pub fn direction_key(e: &KeyLike) -> Option<DirKey> {
    if e.alt || e.meta {
        return None;
    }
    let modifier = if e.ctrl {
        DirMod::Ctrl
    } else if e.shift {
        DirMod::Shift
    } else {
        DirMod::None
    };
    if let Some(abs) = numpad(&e.code) {
        return Some(DirKey { abs, modifier, source: KeyDirSource::Numpad });
    }
    if let Some(abs) = arrows(&e.key) {
        return Some(DirKey { abs, modifier, source: KeyDirSource::Arrows });
    }
    let mut chars = e.key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        let k = c.to_lowercase().next().unwrap_or(c);
        if let Some(abs) = vim(k) {
            // A capital letter without Shift (CapsLock) is still the run variant,
            // as it would be in the official client where `K` is "run north".
            let upper = c != k;
            let modifier = if e.ctrl {
                DirMod::Ctrl
            } else if e.shift || upper {
                DirMod::Shift
            } else {
                DirMod::None
            };
            return Some(DirKey { abs, modifier, source: KeyDirSource::Vim });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// key_conversion.js tables, keyed by `which` (keyCode)

/// key_conversion.simple: unmodified keys handled on keydown.
fn simple(which: u32) -> Option<i32> {
    Some(match which {
        27 => 27,
        8 => 8,
        9 => 9,
        46 => Keys::CK_DELETE,
        45 => Keys::CK_INSERT,
        35 => Keys::CK_END,
        40 => Keys::CK_DOWN,
        34 => Keys::CK_PGDN,
        37 => Keys::CK_LEFT,
        12 => Keys::CK_CLEAR,
        39 => Keys::CK_RIGHT,
        36 => Keys::CK_HOME,
        38 => Keys::CK_UP,
        33 => Keys::CK_PGUP,
        _ => return None,
    })
}

/// The shift / ctrl / ctrlshift tables share one layout: the navigation keys in cio.h's order
/// from the table's CK_*_UP (UP DOWN LEFT RIGHT INSERT HOME END CLEAR PGUP PGDN), and the five
/// placeholders from its CK_*_ENTER (ENTER BKSP ESCAPE DELETE SPACE).
fn mod_table(which: u32, up: i32, enter: i32) -> Option<i32> {
    let (up_, down, left, right, insert, home, end, clear, pgup, pgdn) = (up, up + 1, up + 2, up + 3, up + 4, up + 5, up + 6, up + 7, up + 8, up + 9);
    Some(match which {
        // Numpad / Arrow keys
        45 => insert,
        35 => end,
        40 => down,
        34 => pgdn,
        37 => left,
        12 => clear,
        39 => right,
        36 => home,
        38 => up_,
        33 => pgup,
        // Numpad on mac / with numlock: converted to modified navigation keys
        97 => end,
        98 => down,
        99 => pgdn,
        100 => left,
        102 => right,
        103 => home,
        104 => up_,
        105 => pgup,
        13 => enter,
        8 => enter + 1,
        27 => enter + 2,
        46 => enter + 3,
        32 => enter + 4,
        _ => return None,
    })
}

/// key_conversion.shift (Shift+Tab is the one entry the others lack).
fn shift_table(which: u32) -> Option<i32> {
    if which == 9 {
        return Some(Keys::CK_SHIFT_TAB);
    }
    mod_table(which, Keys::CK_SHIFT_UP, Keys::CK_SHIFT_ENTER)
}

/// key_conversion.ctrl
fn ctrl_table(which: u32) -> Option<i32> {
    mod_table(which, Keys::CK_CTRL_UP, Keys::CK_CTRL_ENTER)
}

/// key_conversion.ctrlshift (no Tab entry: Ctrl+Shift+Tab stays with the browser).
fn ctrlshift_table(which: u32) -> Option<i32> {
    mod_table(which, Keys::CK_CTRL_SHIFT_UP, Keys::CK_CTRL_SHIFT_ENTER)
}

/// key_conversion.codes: unmodified keys by `e.code` (checked before SIMPLE).
/// F11 (fullscreen) is reserved for the browser. F12 focuses chat, as in the
/// official client; the game screen handles it before keys reach here.
fn codes(code: &str) -> Option<i32> {
    Some(match code {
        "Delete" => Keys::CK_DELETE,
        "Numpad0" => -1000,
        "Numpad1" => -1001,
        "Numpad2" => -1002,
        "Numpad3" => -1003,
        "Numpad4" => -1004,
        "Numpad5" => -1005,
        "Numpad6" => -1006,
        "Numpad7" => -1007,
        "Numpad8" => -1008,
        "Numpad9" => -1009,
        "NumpadEnter" => -1010,
        "NumpadDivide" => -1012,
        "NumpadMultiply" => -1015,
        "NumpadAdd" => -1016,
        "NumpadSubtract" => -1018,
        "NumpadDecimal" => -1019,
        "NumpadEqual" => -1021,
        "F1" => -265,
        "F2" => -266,
        "F3" => -267,
        "F4" => -268,
        "F5" => -269,
        "F6" => -270,
        "F7" => -271,
        "F8" => -272,
        "F9" => -273,
        "F10" => -274,
        "F13" => -277,
        "F14" => -278,
        "F15" => -279,
        "F16" => -280,
        "F17" => -281,
        "F18" => -282,
        "F19" => -283,
        _ => return None,
    })
}

/// key_conversion.captured_control_keys: Ctrl+these are sent as `which - 64`.
const CAPTURED_CTRL: &str = "OQFPWATXSGIDEHJKLYUBNCM1234567890";

// ---------------------------------------------------------------------------
// keyCode recovery

/// keyCode values for the named keys the tables above use.
fn key_which(key: &str) -> Option<u32> {
    Some(match key {
        "Backspace" => 8,
        "Tab" => 9,
        "Clear" => 12,
        "Enter" => 13,
        "Escape" => 27,
        " " => 32,
        "PageUp" => 33,
        "PageDown" => 34,
        "End" => 35,
        "Home" => 36,
        "ArrowLeft" => 37,
        "ArrowUp" => 38,
        "ArrowRight" => 39,
        "ArrowDown" => 40,
        "Insert" => 45,
        "Delete" => 46,
        _ => return None,
    })
}

fn single_char(s: &str) -> Option<char> {
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    }
}

/// The `e.which` the official client would see: the browser's keyCode when
/// present, otherwise derived from `key`; then the official numpad
/// normalisation (a digit keyCode with a Numpad code becomes 96..105).
pub fn which_of(e: &KeyLike) -> u32 {
    let mut which = e.key_code.unwrap_or(0);
    if which == 0 {
        if let Some(w) = key_which(&e.key) {
            which = w;
        } else if let Some(n) = e.key.strip_prefix('F').filter(|n| (1..=2).contains(&n.len()) && n.chars().all(|c| c.is_ascii_digit())) {
            which = 111 + n.parse::<u32>().unwrap_or(0);
        } else if let Some(c) = single_char(&e.key) {
            which = c.to_uppercase().next().unwrap_or(c) as u32;
        }
    }
    if (48..=57).contains(&which) {
        if let Some(d) = e.code.strip_prefix("Numpad").filter(|d| d.len() == 1 && d.chars().all(|c| c.is_ascii_digit())) {
            which = 96 + d.parse::<u32>().unwrap_or(0);
        }
    }
    which
}

/// Convert a keydown into the message the official client would send, or None
/// if the browser should keep the key. The official client sends printable
/// characters from the keypress that follows an unhandled keydown; we do not
/// listen for keypress, so that step is folded in here (`e.key` is the same
/// character keypress would report, CapsLock and layout included).
pub fn keydown_message(e: &KeyLike) -> Option<Value> {
    let which = which_of(e);
    if e.ctrl && !e.shift && !e.alt {
        if let Some(k) = ctrl_table(which) {
            return Some(cm::key(k));
        }
        if char::from_u32(which).is_some_and(|c| CAPTURED_CTRL.contains(c)) {
            return Some(cm::key(which as i32 - 64));
        }
        return None;
    }
    if !e.ctrl && e.shift && !e.alt {
        if let Some(k) = shift_table(which) {
            return Some(cm::key(k));
        }
    } else if e.ctrl && e.shift && !e.alt {
        return ctrlshift_table(which).map(cm::key);
    } else if !e.ctrl && !e.shift && e.alt {
        return if which < 32 { None } else { Some(cm::input_bytes(&[27, which.min(255) as u8])) };
    } else if !e.ctrl && !e.shift && !e.alt {
        if e.key == "F12" || e.code == "F12" {
            return None; // chat, handled by the game screen as in the official client
        }
        if let Some(k) = codes(&e.code) {
            return Some(cm::key(k));
        }
        if let Some(k) = simple(which) {
            return Some(cm::key(k));
        }
    }
    // keypress: skipped for Ctrl or Alt unless both are held (AltGr), and when
    // the browser would not fire it (Meta held, non-printable keys).
    if (e.ctrl || e.alt) && !(e.ctrl && e.alt) {
        return None;
    }
    if e.meta {
        return None;
    }
    if e.key == "Enter" {
        return Some(cm::input("\r"));
    }
    let c = single_char(&e.key)?;
    if c == '{' {
        return Some(cm::input_bytes(&[123]));
    }
    Some(cm::input(&e.key))
}

// ---------------------------------------------------------------------------
// egui events as KeyLike

/// One key from egui, as the DOM would deliver it: a key going down (with whether it is the
/// keyboard's auto-repeat) or coming up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyInput {
    Down { key: KeyLike, repeat: bool },
    Up { key: KeyLike },
}

/// The DOM `key` and `code` of an egui key (before the layout's own character, which a
/// following `Text` event carries). egui does not tell the numpad apart from the digit row,
/// so no key comes out with a `Numpad` code: the numpad's digits are digits.
pub fn dom_key(key: egui::Key, shift: bool) -> (String, String) {
    use egui::Key as K;
    let named = |k: &str| (k.to_string(), k.to_string());
    match key {
        K::ArrowUp => named("ArrowUp"),
        K::ArrowDown => named("ArrowDown"),
        K::ArrowLeft => named("ArrowLeft"),
        K::ArrowRight => named("ArrowRight"),
        K::Escape => named("Escape"),
        K::Tab => named("Tab"),
        K::Backspace => named("Backspace"),
        K::Enter => named("Enter"),
        K::Space => (" ".into(), "Space".into()),
        K::Insert => named("Insert"),
        K::Delete => named("Delete"),
        K::Home => named("Home"),
        K::End => named("End"),
        K::PageUp => named("PageUp"),
        K::PageDown => named("PageDown"),
        K::BrowserBack => named("BrowserBack"),
        _ => {
            let name = key.name();
            // F1..F35
            if let Some(n) = name.strip_prefix('F').filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) {
                return (format!("F{n}"), format!("F{n}"));
            }
            let sym = key.symbol_or_name();
            if let Some(c) = single_char(sym) {
                if c.is_ascii_alphabetic() {
                    let ch = if shift { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() };
                    return (ch.to_string(), format!("Key{}", c.to_ascii_uppercase()));
                }
                if c.is_ascii_digit() {
                    return (c.to_string(), format!("Digit{c}"));
                }
                return (c.to_string(), String::new());
            }
            (name.to_string(), String::new())
        }
    }
}

/// The DOM keyCode a browser reports for a physical key: the official client keys its tables
/// off it, so Shift+1 (`!`) is 49, never 33 (PageUp), and `'` is 222, never 39 (ArrowRight).
pub fn dom_key_code(key: egui::Key) -> Option<u32> {
    use egui::Key as K;
    let sym = key.symbol_or_name();
    if let Some(c) = single_char(sym) {
        if c.is_ascii_alphanumeric() {
            return Some(c.to_ascii_uppercase() as u32);
        }
    }
    Some(match key {
        K::Minus => 189,
        K::Equals | K::Plus => 187,
        K::OpenBracket => 219,
        K::CloseBracket => 221,
        K::Backslash | K::Pipe => 220,
        K::Semicolon | K::Colon => 186,
        K::Quote => 222,
        K::Comma => 188,
        K::Period => 190,
        K::Slash | K::Questionmark => 191,
        K::Backtick => 192,
        _ => match dom_key(key, false).0.as_str() {
            k => key_which(k).or_else(|| k.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()).map(|n| 111 + n))?,
        },
    })
}

/// The keyCode of a character no key event came with (an IME's, a layout's dead-key result):
/// the browser's "Process", which no table holds, so it is always typed.
const PROCESS_KEY_CODE: u32 = 229;

/// Whether a DOM key would be followed by a keypress carrying its character: one printable
/// character (or Space), with neither Ctrl nor Alt alone held (AltGr, both, still types).
fn types_text(key: &KeyLike) -> bool {
    single_char(&key.key).is_some() && !((key.ctrl || key.alt) && !(key.ctrl && key.alt)) && !key.meta
}

/// The keys of a frame's egui events, in order, as the DOM would deliver them.
///
/// egui sends a key as a `Key` event, then (for a key that types) a `Text` event with the
/// character the layout makes of it; the DOM gives both at once as `key`. So a `Key` that types
/// takes the `Text` after it as its `key`, and a `Text` with no `Key` before it (a character
/// egui has no key for: `é`, an IME's) is a key of its own. Ctrl+C and Ctrl+X reach egui as
/// `Copy` and `Cut`: crawl captures both, so they come back out as those keys. `modifiers` is
/// the frame's, for a lone `Text`.
pub fn key_inputs(events: &[egui::Event], modifiers: egui::Modifiers) -> Vec<KeyInput> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < events.len() {
        match &events[i] {
            egui::Event::Key { key, physical_key, pressed, repeat, modifiers: m } => {
                let (k, code) = dom_key(*key, m.shift);
                let key_code = physical_key.or(Some(*key)).and_then(dom_key_code);
                let mut like = KeyLike { key: k, code, ctrl: m.ctrl, shift: m.shift, alt: m.alt, meta: m.mac_cmd, key_code };
                if !pressed {
                    out.push(KeyInput::Up { key: like });
                    i += 1;
                    continue;
                }
                // a key that types (or one egui has no character for, `'`): the Text after it is its character
                let named = key_which(&like.key).is_some() || dom_key_code(*key).is_some_and(|c| (112..=135).contains(&c));
                if types_text(&like) || (!named && !((like.ctrl || like.alt) && !(like.ctrl && like.alt)) && !like.meta) {
                    if let Some(egui::Event::Text(t)) = events.get(i + 1) {
                        if let Some(c) = single_char(t) {
                            like.key = c.to_string();
                        }
                        i += 1;
                    }
                }
                out.push(KeyInput::Down { key: like, repeat: *repeat });
            }
            egui::Event::Text(t) => {
                for c in t.chars() {
                    let like = KeyLike { key: c.to_string(), code: String::new(), ctrl: false, shift: modifiers.shift, alt: modifiers.alt, meta: false, key_code: Some(PROCESS_KEY_CODE) };
                    out.push(KeyInput::Down { key: like, repeat: false });
                }
            }
            egui::Event::Copy => out.push(KeyInput::Down { key: KeyLike { key: "c".into(), code: "KeyC".into(), ctrl: true, key_code: Some(67), ..Default::default() }, repeat: false }),
            egui::Event::Cut => out.push(KeyInput::Down { key: KeyLike { key: "x".into(), code: "KeyX".into(), ctrl: true, key_code: Some(88), ..Default::default() }, repeat: false }),
            _ => {}
        }
        i += 1;
    }
    out
}
