//! A server text prompt's field (overlays.ts `renderTextInput`, `wireInput`, following the
//! official textinput.js): Enter submits with a CR, Escape sends ESC, Up and Down recall the
//! prompt's history, and the prompt's tag decides which characters are hotkeys rather than text.

use crate::frame::{self, Screen};
use crate::rich::{self, TextStyle};
use crate::{OverlayCx, Overlays};
use egui::{Color32, Pos2, Rect, Vec2};
use isorun_ui::widgets::{self, hex};
use isorun_webtiles::{cm, TextInputState};

/// Official textinput.js keeps the last ten entries per historyId.
pub const HISTORY_SIZE: usize = 10;

#[derive(Clone, Debug)]
pub struct Field {
    /// the init_input it answers
    pub source: TextInputState,
    pub text: String,
    /// select_prefill: the first key replaces the prefill
    pub selected: bool,
    pub history: Option<Vec<String>>,
    pub history_pos: usize,
}

/// What a key typed into the field does.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldKey {
    Char(char),
    Backspace,
    Enter,
    Escape,
    Up,
    Down,
}

impl Field {
    pub fn new(ti: TextInputState, history: Option<Vec<String>>) -> Self {
        let text = ti.text.clone();
        let selected = ti.select_prefill == Some(true) || ti.extra.get("select").is_some_and(isorun_webtiles::truthy);
        let history_pos = history.as_ref().map_or(0, |h| h.len());
        Field { source: ti, text, selected, history, history_pos }
    }

    /// The prompt's own keys (textinput.js): true when the character is a hotkey, sent here.
    fn special(&mut self, ch: char, out: &mut Vec<serde_json::Value>) -> bool {
        let tag = self.source.tag.as_deref().unwrap_or("");
        let seed = self.source.kind.as_deref() == Some("seed-selection");
        match tag {
            "stash_search" if ch == '?' && self.text.is_empty() => {
                out.push(cm::key(63));
                true
            }
            "repeat" if !ch.is_ascii_digit() => {
                self.submit(ch as i32, out);
                true
            }
            "travel_depth" if "<>?$^-p".contains(ch) => {
                self.submit(ch as i32, out);
                true
            }
            "skill_target" if ch == '-' => {
                self.submit(ch as i32, out);
                true
            }
            _ if seed => {
                // '-' clears, '?' helps, 'd' picks the daily seed; anything but a digit is refused
                if ch == '-' || ch == '?' || ch == 'd' {
                    out.push(cm::key(ch as i32));
                    return true;
                }
                !ch.is_ascii_digit()
            }
            _ => false,
        }
    }

    /// Send the text: ctrl-u and ctrl-k wipe any prefill on the crawl side ('repeat' has none).
    pub fn submit(&mut self, final_char: i32, out: &mut Vec<serde_json::Value>) {
        if let Some(h) = self.history.as_mut() {
            if !self.text.is_empty() {
                h.retain(|x| x != &self.text);
                h.push(self.text.clone());
                if h.len() > HISTORY_SIZE {
                    h.remove(0);
                }
            }
        }
        if self.source.tag.as_deref() != Some("repeat") {
            out.push(cm::key(21));
            out.push(cm::key(11));
        }
        out.push(cm::text_input(&format!("{}{}", self.text, char::from_u32(final_char as u32).unwrap_or('\r'))));
    }

    /// A key typed while the field is up; the messages it sends go to `out`.
    pub fn key(&mut self, k: FieldKey, out: &mut Vec<serde_json::Value>) {
        match k {
            FieldKey::Escape => out.push(cm::key(27)),
            FieldKey::Enter => self.submit(13, out),
            FieldKey::Up | FieldKey::Down => {
                if let Some(h) = &self.history {
                    if h.is_empty() {
                        return;
                    }
                    let n = h.len() as i64;
                    let mut pos = self.history_pos as i64 + if k == FieldKey::Up { -1 } else { 1 };
                    if pos < 0 {
                        pos = n - 1;
                    }
                    if pos >= n {
                        pos = 0;
                    }
                    self.history_pos = pos as usize;
                    self.text = h[pos as usize].clone();
                    self.selected = false;
                }
            }
            FieldKey::Backspace => {
                if self.selected {
                    self.text.clear();
                } else {
                    self.text.pop();
                }
                self.selected = false;
            }
            FieldKey::Char(c) => {
                if self.special(c, out) {
                    return;
                }
                if self.selected {
                    self.text.clear();
                    self.selected = false;
                }
                let max = self.source.maxlen.unwrap_or(i64::MAX).max(0) as usize;
                if self.text.chars().count() < max {
                    self.text.push(c);
                }
            }
        }
    }
}

impl Overlays {
    /// The field's own popup, low in the view (`bottom: 9rem`, at least 20rem wide).
    pub(crate) fn draw_text_input(&mut self, ctx: &egui::Context, _cx: &mut OverlayCx, screen: &Screen, dock: bool) {
        let Some(f) = self.field.as_ref() else { return };
        let font = screen.popup_font();
        let st = TextStyle::new(ctx, font, hex(0xdddddd));
        let pad_y = 0.5 * screen.rem();
        let pad_x = 0.8 * screen.rem();
        let prompt = f.source.prompt.clone().filter(|p| !p.is_empty()).unwrap_or_else(|| "Input here (ESC to cancel): ".into());
        let head = rich::galley(ctx, rich::formatted(&prompt, &st, 0.0));
        // `<input size=N>`: N average characters, 2px of padding and border each side
        let size = f.source.size.unwrap_or(20).max(1) as f32;
        // Chrome's width for `size`: N average characters, plus the font's widest glyph's excess over
        // the average (about an em for DejaVu Sans Mono), plus its padding and border
        let field_w = (size * widgets::ch(font) + font).round() + 8.0;
        let field_h = widgets::normal_line_height(font) + 7.0;
        let width = (head.size().x.max(field_w) + 2.0 * pad_x + 2.0).max(20.0 * screen.rem()).min(screen.popup_max_w()).ceil();
        let height = (2.0 * pad_y + head.size().y + 1.0 + 2.0 * pad_y + field_h + 2.0).ceil();
        let x = screen.rect.center().x - width / 2.0;
        let y = screen.rect.bottom() - 9.0 * screen.rem() - height;
        let rect = Rect::from_min_size(Pos2::new(x.round(), y.round()), Vec2::new(width, height));
        let text = f.text.clone();
        let selected = f.selected;
        let blink = (ctx.input(|i| i.time) * 2.0) as i64 % 2 == 0;
        frame::layer(ctx, egui::Id::unique("isorun-text-input"), rect, |ui, _| {
            let p = ui.painter().clone();
            widgets::paint_panel(&p, rect, &frame::look(dock));
            let left = rect.left() + 1.0 + pad_x;
            let mut y = rect.top() + 1.0 + pad_y;
            p.galley(Pos2::new(left, y), head.clone(), st.colour);
            y += head.size().y + pad_y;
            widgets::rule(&p, rect.left() + 1.0..=rect.right() - 1.0, y, frame::rule_colour(dock));
            y += 1.0 + pad_y;
            // the browser's field in the dark scheme, focused: #3b3b3b inside a white ring
            let fr = Rect::from_min_size(Pos2::new(left, y), Vec2::new(field_w, field_h));
            p.rect_filled(fr, 2.0, hex(0x3b3b3b));
            p.rect_stroke(fr, 2.0, egui::Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Inside);
            let g = rich::galley(ui.ctx(), rich::plain(&text, &st.clone().colour(Color32::WHITE), 0.0));
            let tp = Pos2::new(fr.left() + 4.0, fr.center().y - g.size().y / 2.0);
            if selected && !text.is_empty() {
                p.rect_filled(Rect::from_min_size(tp, g.size()), 0.0, Color32::from_rgb(0x3d, 0x5a, 0x80));
            }
            p.with_clip_rect(fr.shrink(2.0)).galley(tp, g.clone(), Color32::WHITE);
            if blink {
                p.rect_filled(Rect::from_min_size(Pos2::new(tp.x + g.size().x, tp.y + 1.0), Vec2::new(1.0, g.size().y - 2.0)), 0.0, Color32::WHITE);
            }
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

    /// Submit the field's text, keeping it in its history (textinput.js).
    pub(crate) fn field_key(&mut self, k: FieldKey, cx: &mut OverlayCx) {
        let Some(f) = self.field.as_mut() else { return };
        let mut out = Vec::new();
        f.key(k, &mut out);
        if let (Some(id), Some(h)) = (f.source.history_id.clone(), f.history.clone()) {
            self.histories.insert(id, h);
        }
        for m in out {
            cx.send(m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(tag: &str) -> Field {
        Field::new(TextInputState { tag: Some(tag.into()), ..Default::default() }, Some(Vec::new()))
    }

    #[test]
    fn typing_and_submitting() {
        let mut f = field("");
        let mut out = Vec::new();
        for c in "quit".chars() {
            f.key(FieldKey::Char(c), &mut out);
        }
        assert!(out.is_empty());
        f.key(FieldKey::Enter, &mut out);
        assert_eq!(out, vec![cm::key(21), cm::key(11), cm::text_input("quit\r")]);
        assert_eq!(f.history.as_deref(), Some(&["quit".to_string()][..]));
        f.text.clear();
        f.history_pos = 1;
        f.key(FieldKey::Up, &mut out);
        assert_eq!(f.text, "quit");
    }

    #[test]
    fn prompt_hotkeys() {
        let mut out = Vec::new();
        let mut t = field("travel_depth");
        t.key(FieldKey::Char('>'), &mut out);
        assert_eq!(out, vec![cm::key(21), cm::key(11), cm::text_input(">")]);
        out.clear();
        let mut r = field("repeat");
        r.key(FieldKey::Char('5'), &mut out);
        r.key(FieldKey::Char('x'), &mut out);
        assert_eq!(out, vec![cm::text_input("5x")]);
        out.clear();
        let mut s = field("stash_search");
        s.key(FieldKey::Char('?'), &mut out);
        assert_eq!(out, vec![cm::key(63)]);
        let mut seed = Field::new(TextInputState { kind: Some("seed-selection".into()), ..Default::default() }, None);
        out.clear();
        seed.key(FieldKey::Char('a'), &mut out);
        seed.key(FieldKey::Char('7'), &mut out);
        assert_eq!(seed.text, "7");
        assert!(out.is_empty());
    }

    #[test]
    fn prefill_selected() {
        let mut f = Field::new(TextInputState { text: "old".into(), select_prefill: Some(true), ..Default::default() }, None);
        let mut out = Vec::new();
        f.key(FieldKey::Char('n'), &mut out);
        assert_eq!(f.text, "n");
    }
}
