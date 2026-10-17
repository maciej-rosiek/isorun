//! The port of `apps/isorun/test/keys.test.ts`: the official tables, transcribed from
//! crawl-ref/source/webserver/static/scripts/key_conversion.js (keyed by e.which).

use isorun_input::keys::{direction_key, key_inputs, keydown_message, which_of, DirKey, DirMod, KeyDirSource, KeyInput, KeyLike};
use isorun_webtiles::{cm, Keys as K};

type Table = &'static [(u32, i32)];

const SIMPLE: Table = &[(27, 27), (8, 8), (9, 9), (46, K::CK_DELETE), (45, K::CK_INSERT), (35, K::CK_END), (40, K::CK_DOWN), (34, K::CK_PGDN), (37, K::CK_LEFT), (12, K::CK_CLEAR), (39, K::CK_RIGHT), (36, K::CK_HOME), (38, K::CK_UP), (33, K::CK_PGUP)];
const SHIFT: Table = &[
    (9, K::CK_SHIFT_TAB),
    (45, K::CK_SHIFT_INSERT),
    (35, K::CK_SHIFT_END),
    (40, K::CK_SHIFT_DOWN),
    (34, K::CK_SHIFT_PGDN),
    (37, K::CK_SHIFT_LEFT),
    (12, K::CK_SHIFT_CLEAR),
    (39, K::CK_SHIFT_RIGHT),
    (36, K::CK_SHIFT_HOME),
    (38, K::CK_SHIFT_UP),
    (33, K::CK_SHIFT_PGUP),
    (97, K::CK_SHIFT_END),
    (98, K::CK_SHIFT_DOWN),
    (99, K::CK_SHIFT_PGDN),
    (100, K::CK_SHIFT_LEFT),
    (102, K::CK_SHIFT_RIGHT),
    (103, K::CK_SHIFT_HOME),
    (104, K::CK_SHIFT_UP),
    (105, K::CK_SHIFT_PGUP),
    (13, K::CK_SHIFT_ENTER),
    (8, K::CK_SHIFT_BKSP),
    (27, K::CK_SHIFT_ESCAPE),
    (46, K::CK_SHIFT_DELETE),
    (32, K::CK_SHIFT_SPACE),
];
const CTRL: Table = &[
    (45, K::CK_CTRL_INSERT),
    (35, K::CK_CTRL_END),
    (40, K::CK_CTRL_DOWN),
    (34, K::CK_CTRL_PGDN),
    (37, K::CK_CTRL_LEFT),
    (12, K::CK_CTRL_CLEAR),
    (39, K::CK_CTRL_RIGHT),
    (36, K::CK_CTRL_HOME),
    (38, K::CK_CTRL_UP),
    (33, K::CK_CTRL_PGUP),
    (97, K::CK_CTRL_END),
    (98, K::CK_CTRL_DOWN),
    (99, K::CK_CTRL_PGDN),
    (100, K::CK_CTRL_LEFT),
    (102, K::CK_CTRL_RIGHT),
    (103, K::CK_CTRL_HOME),
    (104, K::CK_CTRL_UP),
    (105, K::CK_CTRL_PGUP),
    (13, K::CK_CTRL_ENTER),
    (8, K::CK_CTRL_BKSP),
    (27, K::CK_CTRL_ESCAPE),
    (46, K::CK_CTRL_DELETE),
    (32, K::CK_CTRL_SPACE),
];
const CTRLSHIFT: Table = &[
    (45, K::CK_CTRL_SHIFT_INSERT),
    (35, K::CK_CTRL_SHIFT_END),
    (40, K::CK_CTRL_SHIFT_DOWN),
    (34, K::CK_CTRL_SHIFT_PGDN),
    (37, K::CK_CTRL_SHIFT_LEFT),
    (12, K::CK_CTRL_SHIFT_CLEAR),
    (39, K::CK_CTRL_SHIFT_RIGHT),
    (36, K::CK_CTRL_SHIFT_HOME),
    (38, K::CK_CTRL_SHIFT_UP),
    (33, K::CK_CTRL_SHIFT_PGUP),
    (97, K::CK_CTRL_SHIFT_END),
    (98, K::CK_CTRL_SHIFT_DOWN),
    (99, K::CK_CTRL_SHIFT_PGDN),
    (100, K::CK_CTRL_SHIFT_LEFT),
    (102, K::CK_CTRL_SHIFT_RIGHT),
    (103, K::CK_CTRL_SHIFT_HOME),
    (104, K::CK_CTRL_SHIFT_UP),
    (105, K::CK_CTRL_SHIFT_PGUP),
    (13, K::CK_CTRL_SHIFT_ENTER),
    (8, K::CK_CTRL_SHIFT_BKSP),
    (27, K::CK_CTRL_SHIFT_ESCAPE),
    (46, K::CK_CTRL_SHIFT_DELETE),
    (32, K::CK_CTRL_SHIFT_SPACE),
];
const CODES: &[(&str, i32)] = &[
    ("Delete", K::CK_DELETE),
    ("Numpad0", -1000),
    ("Numpad1", -1001),
    ("Numpad2", -1002),
    ("Numpad3", -1003),
    ("Numpad4", -1004),
    ("Numpad5", -1005),
    ("Numpad6", -1006),
    ("Numpad7", -1007),
    ("Numpad8", -1008),
    ("Numpad9", -1009),
    ("NumpadEnter", -1010),
    ("NumpadDivide", -1012),
    ("NumpadMultiply", -1015),
    ("NumpadAdd", -1016),
    ("NumpadSubtract", -1018),
    ("NumpadDecimal", -1019),
    ("NumpadEqual", -1021),
    ("F1", -265),
    ("F2", -266),
    ("F3", -267),
    ("F4", -268),
    ("F5", -269),
    ("F6", -270),
    ("F7", -271),
    ("F8", -272),
    ("F9", -273),
    ("F10", -274),
    ("F13", -277),
    ("F14", -278),
    ("F15", -279),
    ("F16", -280),
    ("F17", -281),
    ("F18", -282),
    ("F19", -283),
];
const CAPTURED: &str = "OQFPWATXSGIDEHJKLYUBNCM1234567890";

#[derive(Default, Clone, Copy)]
struct Mods {
    ctrl: bool,
    shift: bool,
    alt: bool,
    meta: bool,
}
const NONE: Mods = Mods { ctrl: false, shift: false, alt: false, meta: false };
const SH: Mods = Mods { shift: true, ..NONE };
const CT: Mods = Mods { ctrl: true, ..NONE };
const CS: Mods = Mods { ctrl: true, shift: true, ..NONE };
const AL: Mods = Mods { alt: true, ..NONE };

fn ev(key: &str, code: &str, m: Mods, key_code: Option<u32>) -> KeyLike {
    KeyLike { key: key.into(), code: code.into(), ctrl: m.ctrl, shift: m.shift, alt: m.alt, meta: m.meta, key_code }
}

/// `key` / `code` a browser reports for a given keyCode (numpad digits: numlock on).
fn which_key(which: u32) -> Option<(&'static str, &'static str)> {
    Some(match which {
        8 => ("Backspace", "Backspace"),
        9 => ("Tab", "Tab"),
        12 => ("Clear", "NumLock"),
        13 => ("Enter", "Enter"),
        27 => ("Escape", "Escape"),
        32 => (" ", "Space"),
        33 => ("PageUp", "PageUp"),
        34 => ("PageDown", "PageDown"),
        35 => ("End", "End"),
        36 => ("Home", "Home"),
        37 => ("ArrowLeft", "ArrowLeft"),
        38 => ("ArrowUp", "ArrowUp"),
        39 => ("ArrowRight", "ArrowRight"),
        40 => ("ArrowDown", "ArrowDown"),
        45 => ("Insert", "Insert"),
        46 => ("Delete", "Delete"),
        97 => ("1", "Numpad1"),
        98 => ("2", "Numpad2"),
        99 => ("3", "Numpad3"),
        100 => ("4", "Numpad4"),
        102 => ("6", "Numpad6"),
        103 => ("7", "Numpad7"),
        104 => ("8", "Numpad8"),
        105 => ("9", "Numpad9"),
        _ => return None,
    })
}

/// An event as the browser would deliver it for keyCode `which`, with and without keyCode.
fn ev_for(which: u32, m: Mods, with_key_code: bool) -> KeyLike {
    let (key, code) = which_key(which).map(|(k, c)| (k.to_string(), c.to_string())).unwrap_or_else(|| (char::from_u32(which).unwrap().to_lowercase().to_string(), String::new()));
    // a browser reports the digit keyCode for numpad digits; the official client rewrites it to 96..105 from `code`
    let reported = if (97..=105).contains(&which) { which - 48 } else { which };
    ev(&key, &code, m, with_key_code.then_some(reported))
}

fn table(t: Table, m: Mods) {
    for &(which, expected) in t {
        assert_eq!(keydown_message(&ev_for(which, m, true)), Some(cm::key(expected)), "which {which}");
        assert_eq!(keydown_message(&ev_for(which, m, false)), Some(cm::key(expected)), "which {which} without keyCode");
    }
}

#[test]
fn keys_layout_matches_cio_h() {
    assert_eq!(K::CK_DELETE, -255);
    assert_eq!(K::CK_UP, -254);
    assert_eq!(K::CK_SHIFT_UP, K::CK_UP + 11);
    assert_eq!(K::CK_CTRL_UP, K::CK_SHIFT_UP + 11);
    assert_eq!(K::CK_CTRL_SHIFT_UP, K::CK_CTRL_UP + 11);
    assert_eq!(K::CK_CTRL_SHIFT_TAB, K::CK_CTRL_SHIFT_UP + 10);
    assert_eq!(K::CK_SHIFT_ENTER, K::CK_CTRL_SHIFT_TAB + 6);
    assert_eq!(K::CK_CTRL_ENTER, K::CK_SHIFT_ENTER + 5);
    assert_eq!(K::CK_CTRL_SHIFT_ENTER, K::CK_CTRL_ENTER + 5);
    assert_eq!(K::CK_CTRL_SHIFT_SPACE, K::CK_CTRL_SHIFT_ENTER + 4);
}

#[test]
fn key_conversion_simple() {
    table(SIMPLE, NONE);
}

#[test]
fn key_conversion_shift() {
    table(SHIFT, SH);
}

#[test]
fn key_conversion_ctrl() {
    table(CTRL, CT);
}

#[test]
fn key_conversion_ctrlshift() {
    table(CTRLSHIFT, CS);
}

#[test]
fn key_conversion_codes() {
    for &(code, expected) in CODES {
        // key is what a numlock-on / plain keyboard reports
        let key = if code.starts_with("Numpad") && code.ends_with(|c: char| c.is_ascii_digit()) { &code[code.len() - 1..] } else { code };
        assert_eq!(keydown_message(&ev(key, code, NONE, None)), Some(cm::key(expected)), "{code}");
    }
}

#[test]
fn numpad_digits_map_by_code_whether_numlock_is_on_or_off() {
    assert_eq!(keydown_message(&ev("ArrowUp", "Numpad8", NONE, Some(38))), Some(cm::key(-1008)));
    assert_eq!(keydown_message(&ev("8", "Numpad8", NONE, Some(104))), Some(cm::key(-1008)));
}

#[test]
fn f11_and_f12_stay_with_the_browser() {
    assert_eq!(keydown_message(&ev("F11", "F11", NONE, Some(122))), None);
    assert_eq!(keydown_message(&ev("F12", "F12", NONE, Some(123))), None);
}

#[test]
fn f20_is_not_mapped() {
    assert_eq!(keydown_message(&ev("F20", "F20", NONE, None)), None);
}

#[test]
fn captured_control_keys() {
    for c in CAPTURED.chars() {
        let code = if c.is_ascii_digit() { format!("Digit{c}") } else { format!("Key{c}") };
        let e = ev(&c.to_lowercase().to_string(), &code, CT, Some(c as u32));
        assert_eq!(keydown_message(&e), Some(cm::key(c as i32 - 64)), "Ctrl+{c}");
        assert_eq!(keydown_message(&KeyLike { key_code: None, ..e }), Some(cm::key(c as i32 - 64)), "Ctrl+{c} without keyCode");
    }
}

#[test]
fn ctrl_r_v_z_are_left_to_the_browser() {
    for c in ['R', 'V', 'Z'] {
        assert_eq!(keydown_message(&ev(&c.to_lowercase().to_string(), &format!("Key{c}"), CT, None)), None);
    }
}

#[test]
fn ctrl_shift_letter_ctrl_tab_and_ctrl_shift_tab_send_nothing() {
    assert_eq!(keydown_message(&ev("A", "KeyA", CS, None)), None);
    assert_eq!(keydown_message(&ev("Tab", "Tab", CT, None)), None);
    assert_eq!(keydown_message(&ev("Tab", "Tab", CS, None)), None);
}

#[test]
fn ctrl_numpad_digit_with_numlock_on_is_the_ctrl_navigation_key() {
    assert_eq!(keydown_message(&ev("8", "Numpad8", CT, Some(56))), Some(cm::key(K::CK_CTRL_UP)));
    assert_eq!(keydown_message(&ev("0", "Numpad0", CT, Some(48))), None);
    assert_eq!(keydown_message(&ev("5", "Numpad5", CT, Some(53))), None);
}

#[test]
fn ctrl_punctuation_is_not_captured() {
    assert_eq!(keydown_message(&ev("-", "Minus", CT, Some(189))), None);
    assert_eq!(keydown_message(&ev("=", "Equal", CT, Some(187))), None);
}

#[test]
fn alt_sends_esc_and_which_for_letters() {
    assert_eq!(keydown_message(&ev("a", "KeyA", AL, None)), Some(cm::input_bytes(&[27, 65])));
    assert_eq!(keydown_message(&ev("a", "KeyA", AL, Some(65))), Some(cm::input_bytes(&[27, 65])));
}

#[test]
fn alt_sends_esc_and_which_for_any_key_with_which_from_32() {
    assert_eq!(keydown_message(&ev("ArrowUp", "ArrowUp", AL, None)), Some(cm::input_bytes(&[27, 38])));
    assert_eq!(keydown_message(&ev(" ", "Space", AL, None)), Some(cm::input_bytes(&[27, 32])));
    assert_eq!(keydown_message(&ev("F1", "F1", AL, None)), Some(cm::input_bytes(&[27, 112])));
    assert_eq!(keydown_message(&ev("1", "Digit1", AL, None)), Some(cm::input_bytes(&[27, 49])));
}

#[test]
fn alt_sends_nothing_below_32_or_with_shift_or_ctrl() {
    assert_eq!(keydown_message(&ev("Tab", "Tab", AL, None)), None);
    assert_eq!(keydown_message(&ev("Escape", "Escape", AL, None)), None);
    assert_eq!(keydown_message(&ev("Enter", "Enter", AL, None)), None);
    assert_eq!(keydown_message(&ev("A", "KeyA", Mods { alt: true, shift: true, ..NONE }, None)), None);
    assert_eq!(keydown_message(&ev("ArrowUp", "ArrowUp", Mods { alt: true, ctrl: true, ..NONE }, None)), None);
}

#[test]
fn keypress_sends_the_character_as_typed() {
    assert_eq!(keydown_message(&ev("a", "KeyA", NONE, None)), Some(cm::input("a")));
    assert_eq!(keydown_message(&ev("A", "KeyA", SH, None)), Some(cm::input("A")));
    assert_eq!(keydown_message(&ev("A", "KeyA", NONE, None)), Some(cm::input("A")));
    assert_eq!(keydown_message(&ev("!", "Digit1", SH, Some(49))), Some(cm::input("!")));
    assert_eq!(keydown_message(&ev("é", "Semicolon", NONE, Some(186))), Some(cm::input("é")));
    assert_eq!(keydown_message(&ev(" ", "Space", NONE, None)), Some(cm::input(" ")));
}

#[test]
fn keypress_sends_brace_as_raw_bytes() {
    assert_eq!(keydown_message(&ev("{", "BracketLeft", SH, Some(219))), Some(cm::input_bytes(&[123])));
}

#[test]
fn enter_comes_from_keypress_as_cr() {
    assert_eq!(keydown_message(&ev("Enter", "Enter", NONE, None)), Some(cm::input("\r")));
}

#[test]
fn altgr_characters_go_through() {
    assert_eq!(keydown_message(&ev("@", "KeyQ", Mods { ctrl: true, alt: true, ..NONE }, None)), Some(cm::input("@")));
}

#[test]
fn modifier_only_dead_and_meta_keys_send_nothing() {
    assert_eq!(keydown_message(&ev("Shift", "ShiftLeft", SH, None)), None);
    assert_eq!(keydown_message(&ev("Dead", "Quote", NONE, None)), None);
    assert_eq!(keydown_message(&ev("a", "KeyA", Mods { meta: true, ..NONE }, None)), None);
}

#[test]
fn meta_does_not_block_keydown_table_keys() {
    assert_eq!(keydown_message(&ev("ArrowUp", "ArrowUp", Mods { meta: true, ..NONE }, None)), Some(cm::key(K::CK_UP)));
}

#[test]
fn shift_f_keys_send_nothing_on_keydown() {
    assert_eq!(keydown_message(&ev("F1", "F1", SH, None)), None);
    assert_eq!(keydown_message(&ev("F5", "F5", SH, None)), None);
}

#[test]
fn which_of_prefers_the_browser_keycode_and_normalises_numpad_digits() {
    assert_eq!(which_of(&ev(";", "Semicolon", NONE, Some(186))), 186);
    assert_eq!(which_of(&ev("8", "Numpad8", NONE, Some(56))), 104);
    assert_eq!(which_of(&ev("8", "Digit8", NONE, Some(56))), 56);
    assert_eq!(which_of(&ev("ArrowUp", "Numpad8", NONE, Some(38))), 38);
}

fn dk(abs: u8, modifier: DirMod, source: KeyDirSource) -> Option<DirKey> {
    Some(DirKey { abs, modifier, source })
}

#[test]
fn direction_key_reads_numpad_by_code() {
    assert_eq!(direction_key(&ev("8", "Numpad8", NONE, None)), dk(0, DirMod::None, KeyDirSource::Numpad));
    assert_eq!(direction_key(&ev("ArrowUp", "Numpad8", NONE, None)), dk(0, DirMod::None, KeyDirSource::Numpad));
    assert_eq!(direction_key(&ev("8", "Numpad8", SH, None)), dk(0, DirMod::Shift, KeyDirSource::Numpad));
    assert_eq!(direction_key(&ev("8", "Numpad8", CT, None)), dk(0, DirMod::Ctrl, KeyDirSource::Numpad));
    assert_eq!(direction_key(&ev("3", "Numpad3", CT, None)), dk(3, DirMod::Ctrl, KeyDirSource::Numpad));
    assert_eq!(direction_key(&ev("5", "Numpad5", NONE, None)), None);
}

#[test]
fn direction_key_reads_arrows_and_vim_keys() {
    assert_eq!(direction_key(&ev("ArrowLeft", "ArrowLeft", CT, None)), dk(6, DirMod::Ctrl, KeyDirSource::Arrows));
    assert_eq!(direction_key(&ev("y", "KeyY", NONE, None)), dk(7, DirMod::None, KeyDirSource::Vim));
    assert_eq!(direction_key(&ev("J", "KeyJ", SH, None)), dk(4, DirMod::Shift, KeyDirSource::Vim));
    // CapsLock still runs
    assert_eq!(direction_key(&ev("J", "KeyJ", NONE, None)), dk(4, DirMod::Shift, KeyDirSource::Vim));
    assert_eq!(direction_key(&ev("k", "KeyK", CT, None)), dk(0, DirMod::Ctrl, KeyDirSource::Vim));
}

#[test]
fn direction_key_tells_the_three_key_sets_apart() {
    assert_eq!(direction_key(&ev("ArrowLeft", "Numpad4", NONE, None)).map(|d| d.source), Some(KeyDirSource::Numpad));
    assert_eq!(direction_key(&ev("ArrowLeft", "ArrowLeft", NONE, None)).map(|d| d.source), Some(KeyDirSource::Arrows));
    assert_eq!(direction_key(&ev("h", "KeyH", NONE, None)).map(|d| d.source), Some(KeyDirSource::Vim));
}

#[test]
fn direction_key_ignores_alt_meta_combos_and_non_direction_keys() {
    assert_eq!(direction_key(&ev("k", "KeyK", AL, None)), None);
    assert_eq!(direction_key(&ev("ArrowUp", "ArrowUp", Mods { meta: true, ..NONE }, None)), None);
    assert_eq!(direction_key(&ev(".", "Period", NONE, None)), None);
}

// ---------------------------------------------------------------------------
// egui's events as the DOM would deliver them (native only)

fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers }
}

fn down(inputs: &[KeyInput]) -> Vec<KeyLike> {
    inputs
        .iter()
        .filter_map(|i| match i {
            KeyInput::Down { key, .. } => Some(key.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn egui_a_key_and_its_text_are_one_key_typing_the_layouts_character() {
    let shift = egui::Modifiers { shift: true, ..Default::default() };
    let events = vec![key_event(egui::Key::A, shift), egui::Event::Text("A".into()), key_event(egui::Key::Num1, shift), egui::Event::Text("!".into())];
    let keys = down(&key_inputs(&events, shift));
    assert_eq!(keys.len(), 2);
    assert_eq!(keydown_message(&keys[0]), Some(cm::input("A")));
    assert_eq!(keydown_message(&keys[1]), Some(cm::input("!")));
}

#[test]
fn egui_named_keys_and_ctrl_letters_come_out_as_the_official_client_sends_them() {
    let none = egui::Modifiers::default();
    let ctrl = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
    let events = vec![key_event(egui::Key::Escape, none), key_event(egui::Key::Tab, none), key_event(egui::Key::F, ctrl), key_event(egui::Key::Enter, none), key_event(egui::Key::F1, none)];
    let msgs: Vec<_> = down(&key_inputs(&events, none)).iter().map(keydown_message).collect();
    assert_eq!(msgs, vec![Some(cm::key(27)), Some(cm::key(9)), Some(cm::key(6)), Some(cm::input("\r")), Some(cm::key(-265))]);
}

#[test]
fn egui_text_with_no_key_is_a_key_of_its_own_and_copy_is_ctrl_c() {
    let none = egui::Modifiers::default();
    let events = vec![egui::Event::Text("é".into()), egui::Event::Copy];
    let msgs: Vec<_> = down(&key_inputs(&events, none)).iter().map(keydown_message).collect();
    assert_eq!(msgs, vec![Some(cm::input("é")), Some(cm::key(3))]);
}

#[test]
fn egui_vim_keys_and_arrows_are_direction_keys() {
    let none = egui::Modifiers::default();
    let events = vec![key_event(egui::Key::K, none), egui::Event::Text("k".into()), key_event(egui::Key::ArrowLeft, none)];
    let dirs: Vec<_> = down(&key_inputs(&events, none)).iter().map(|k| direction_key(k).map(|d| d.abs)).collect();
    assert_eq!(dirs, vec![Some(0), Some(6)]);
}

#[test]
fn egui_punctuation_keeps_the_browsers_keycode_so_no_table_mistakes_it_for_a_navigation_key() {
    let none = egui::Modifiers::default();
    let shift = egui::Modifiers { shift: true, ..Default::default() };
    let events = vec![
        egui::Event::Key { key: egui::Key::Quote, physical_key: Some(egui::Key::Quote), pressed: true, repeat: false, modifiers: none },
        egui::Event::Text("'".into()),
        egui::Event::Key { key: egui::Key::OpenBracket, physical_key: Some(egui::Key::Num9), pressed: true, repeat: false, modifiers: shift },
        egui::Event::Text("(".into()),
        egui::Event::Text("\"".into()),
    ];
    let msgs: Vec<_> = down(&key_inputs(&events, none)).iter().map(keydown_message).collect();
    assert_eq!(msgs, vec![Some(cm::input("'")), Some(cm::input("(")), Some(cm::input("\""))]);
}

#[test]
fn egui_on_the_web_a_character_before_its_key_is_one_key() {
    // eframe's web runner pushes the Text before the Key while the canvas has the keyboard
    isorun_input::keys::WEB_ORDER.with(|w| w.set(true));
    let shift = egui::Modifiers { shift: true, ..Default::default() };
    let none = egui::Modifiers::default();
    let events = vec![egui::Event::Text("2".into()), key_event(egui::Key::Num2, none), egui::Event::Text(">".into()), key_event(egui::Key::Period, shift), key_event(egui::Key::Escape, none)];
    let msgs: Vec<_> = down(&key_inputs(&events, none)).iter().map(keydown_message).collect();
    isorun_input::keys::WEB_ORDER.with(|w| w.set(false));
    assert_eq!(msgs, vec![Some(cm::input("2")), Some(cm::input(">")), Some(cm::key(27))]);
}
