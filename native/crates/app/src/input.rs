//! Keys to crawl: the keyboard as the web client's `keys.ts` sends it, with the arrows turned to
//! the screen (up is the way the camera faces, `runner.ts` `absolute`).

use isorun_scene::{rotate_dir, Dir8, DIR8_DX, DIR8_DY};
use isorun_webtiles::{ctrl, Keys};

/// A direction on the screen: up is the camera's facing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    N = 0,
    NE = 1,
    E = 2,
    SE = 3,
    S = 4,
    SW = 5,
    W = 6,
    NW = 7,
}

impl Dir {
    /// The compass direction on the map for the camera's facing.
    pub fn rotated(self, facing: Dir8) -> Dir8 {
        rotate_dir(facing, self as i32)
    }
}

pub trait Delta {
    fn delta(self) -> (i32, i32);
}

impl Delta for Dir8 {
    fn delta(self) -> (i32, i32) {
        (DIR8_DX[self as usize & 7], DIR8_DY[self as usize & 7])
    }
}

/// crawl's movement letters by compass direction, clockwise from north.
const MOVE_KEYS: [char; 8] = ['k', 'u', 'l', 'n', 'j', 'b', 'h', 'y'];

/// The key a step in a compass direction sends; `run` sends the capital (Shift-move).
pub fn move_key(d: Dir8, run: bool) -> i32 {
    let c = MOVE_KEYS[d as usize & 7];
    (if run { c.to_ascii_uppercase() } else { c }) as i32
}

fn screen_dir(key: egui::Key) -> Option<Dir> {
    Some(match key {
        egui::Key::ArrowUp => Dir::N,
        egui::Key::ArrowDown => Dir::S,
        egui::Key::ArrowLeft => Dir::W,
        egui::Key::ArrowRight => Dir::E,
        egui::Key::Home => Dir::NW,
        egui::Key::PageUp => Dir::NE,
        egui::Key::End => Dir::SW,
        egui::Key::PageDown => Dir::SE,
        _ => return None,
    })
}

/// The keys typed this frame as crawl's key codes, in order. Arrows (and Home, End, Page Up and
/// Page Down, the keypad's diagonals) step relative to the screen; `facing` is the camera's.
pub fn crawl_keys(ui: &egui::Ui, facing: Dir8) -> Vec<i32> {
    let mut out = Vec::new();
    ui.input(|i| {
        for e in &i.events {
            match e {
                egui::Event::Text(t) => {
                    for c in t.chars() {
                        out.push(c as i32);
                    }
                }
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    if let Some(d) = screen_dir(*key) {
                        out.push(move_key(d.rotated(facing), modifiers.shift));
                        continue;
                    }
                    let code = match key {
                        egui::Key::Enter => Keys::ENTER,
                        egui::Key::Escape => Keys::ESC,
                        egui::Key::Tab if modifiers.shift => Keys::CK_SHIFT_TAB,
                        egui::Key::Tab => Keys::TAB,
                        egui::Key::Backspace => Keys::BACKSPACE,
                        egui::Key::Delete => Keys::CK_DELETE,
                        egui::Key::Insert => Keys::CK_INSERT,
                        egui::Key::F1 => Keys::f(1),
                        egui::Key::F2 => Keys::f(2),
                        egui::Key::F3 => Keys::f(3),
                        egui::Key::F4 => Keys::f(4),
                        egui::Key::F5 => Keys::f(5),
                        egui::Key::F6 => Keys::f(6),
                        egui::Key::F7 => Keys::f(7),
                        egui::Key::F8 => Keys::f(8),
                        egui::Key::F9 => Keys::f(9),
                        egui::Key::F10 => Keys::f(10),
                        _ if modifiers.ctrl || modifiers.command => match key.symbol_or_name().chars().next() {
                            Some(c) if c.is_ascii_alphabetic() && key.symbol_or_name().len() == 1 => ctrl(c),
                            _ => continue,
                        },
                        _ => continue,
                    };
                    out.push(code);
                }
                _ => {}
            }
        }
    });
    out
}
