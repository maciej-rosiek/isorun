//! The on-screen keyboard (the port of `osk.ts`): a pad-driven keyboard that types into a text
//! field. [`Osk`] keeps the cursor over its keys and turns the pad's ops into [`OskEdit`]s for
//! the field it serves; [`OskText`] is such a field (a string and a caret) for whoever has none
//! of its own; [`Osk::show`] draws it as styles.css `.osk` does.

use crate::bindings::OskOp;
use isorun_scene::Dir8;

const OSK_ROWS: [&str; 6] = ["1234567890-=", "qwertyuiop[]", "asdfghjkl;'\\", "zxcvbnm,./`", "!@#$%^&*()_+", "{}|:\"<>?~ "];

/// One button on the keyboard's hint line: the button and what it does here.
#[derive(Clone, Debug, PartialEq)]
pub struct OskPrompt {
    pub button: crate::gamepad::Button,
    pub label: String,
    /// the chip that submits: a target may move it to another button (`OskTarget.submit_button`)
    pub submit: bool,
}

/// The hint line's buttons, in the dialogs' order: what types, then the edits, then submit.
/// Y's label is the keyboard's to name (Done, Send, Submit).
pub fn osk_prompts(submit: &str) -> Vec<OskPrompt> {
    use crate::gamepad::Button::*;
    vec![
        OskPrompt { button: A, label: "Type".into(), submit: false },
        OskPrompt { button: X, label: "Backspace".into(), submit: false },
        OskPrompt { button: RB, label: "Space".into(), submit: false },
        OskPrompt { button: LB, label: "Shift".into(), submit: false },
        OskPrompt { button: Y, label: submit.into(), submit: true },
    ]
}

/// A key of the prompt's own, over the letters: a travel depth's Up and Down.
#[derive(Clone, Debug, PartialEq)]
pub struct OskExtra {
    pub ch: char,
    pub label: String,
}

/// What the keyboard is typing into, as far as the keyboard needs to know.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct OskTarget {
    /// keys of the prompt's own, over the letters as the keyboard's first row
    pub extras: Vec<OskExtra>,
    /// the pad button that submits, when it is not Y: the skill target prompt confirms on Start only
    pub submit_button: Option<crate::gamepad::Button>,
}

/// What the keyboard asks of its field.
#[derive(Clone, Debug, PartialEq)]
pub enum OskEdit {
    /// Type this character at the caret (the field may take it as a hotkey of its own instead: `special`).
    Type(char),
    Backspace,
    Submit,
    Cancel,
}

#[derive(Clone, Debug, Default)]
pub struct Osk {
    row: usize,
    col: usize,
    shift: bool,
    target: Option<OskTarget>,
}

impl Osk {
    pub fn visible(&self) -> bool {
        self.target.is_some()
    }

    /// Show the keyboard for `target`. A keyboard newly shown starts on its first key; one
    /// following its field across a rebuild keeps its place, within the rows it has now (a
    /// prompt's extras row comes and goes).
    pub fn attach(&mut self, target: OskTarget) {
        if self.target.is_none() {
            self.row = 0;
            self.col = 0;
            self.shift = false;
        }
        self.target = Some(target);
        let rows = self.rows();
        if self.row >= rows.len() {
            self.row = 0;
        }
        self.col = self.col.min(rows[self.row].chars().count() - 1);
    }

    /// Hide the keyboard and forget the target.
    pub fn detach(&mut self) {
        self.target = None;
    }

    /// The rows the cursor walks: the target's extras first when it has them, then the letters.
    fn rows(&self) -> Vec<String> {
        let mut rows = Vec::new();
        if let Some(t) = self.target.as_ref().filter(|t| !t.extras.is_empty()) {
            rows.push(t.extras.iter().map(|e| e.ch).collect());
        }
        rows.extend(OSK_ROWS.iter().map(|r| r.to_string()));
        rows
    }

    fn has_extras(&self) -> bool {
        self.target.as_ref().is_some_and(|t| !t.extras.is_empty())
    }

    /// The key the cursor is on, as it would type.
    pub fn focused(&self) -> Option<char> {
        self.target.as_ref()?;
        let rows = self.rows();
        let ch = rows.get(self.row)?.chars().nth(self.col)?;
        // an extra is sent as it is; a letter follows the shift
        Some(if self.has_extras() && self.row == 0 {
            ch
        } else if self.shift {
            ch.to_ascii_uppercase()
        } else {
            ch
        })
    }

    pub fn shifted(&self) -> bool {
        self.shift
    }

    /// Run one keyboard operation: None when nothing is attached or the op edits nothing.
    pub fn op(&mut self, op: OskOp, dir: Option<Dir8>) -> Option<OskEdit> {
        self.target.as_ref()?;
        match op {
            OskOp::Move => {
                let dir = dir? as usize & 7;
                let rows = self.rows();
                let dy = [-1, -1, 0, 1, 1, 1, 0, -1][dir];
                let dx = [0, 1, 1, 1, 0, -1, -1, -1][dir];
                let n = rows.len() as i32;
                self.row = (self.row as i32 + dy).rem_euclid(n) as usize;
                let len = rows[self.row].chars().count() as i32;
                // left and right wrap along the row; up and down keep the column, clamped to
                // a shorter row's end (the extras row is four keys wide, the letters twelve)
                if dx != 0 {
                    self.col = (self.col as i32 + dx).rem_euclid(len) as usize;
                } else {
                    self.col = self.col.min(len as usize - 1);
                }
                None
            }
            OskOp::Type => self.focused().map(OskEdit::Type),
            OskOp::Backspace => Some(OskEdit::Backspace),
            OskOp::Space => Some(OskEdit::Type(' ')),
            OskOp::Shift => {
                self.shift = !self.shift;
                None
            }
            OskOp::Submit => Some(OskEdit::Submit),
            OskOp::Cancel => Some(OskEdit::Cancel),
        }
    }

    /// Draw the keyboard over the bottom of `screen`, with `preview` (the field's text, its
    /// password dots, the line being typed on) over the keys and the hint line under them.
    /// Returns what a click on a key asked.
    pub fn show(&self, ctx: &egui::Context, preview: &str, prompts: &[OskPrompt]) -> Option<OskEdit> {
        let target = self.target.as_ref()?;
        let screen = isorun_ui::safe::rect(ctx);
        let em = 15.0;
        let key = 2.1 * em;
        let gap = 0.2 * em;
        let rows = self.rows();
        let mut clicked = None;
        let id = egui::Id::unique("isorun-osk");
        let size = ctx.memory(|m| m.area_rect(id)).map_or(egui::vec2(12.0 * (key + gap) + 20.0, 300.0), |r| r.size());
        // left: 50%, bottom: 12%, translateX(-50%)
        let pos = egui::pos2(screen.center().x - size.x / 2.0, screen.bottom() - screen.height() * 0.12 - size.y);
        let gold = egui::Color32::from_rgb(0xd8, 0xb4, 0x5f);
        egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            let bg = ui.painter().add(egui::Shape::Noop);
            let inner = egui::Frame::new().inner_margin(0.6 * em).show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                let font = egui::FontId::monospace(em);
                // the preview
                let w = 12.0 * key + 11.0 * gap;
                let (r, _) = ui.allocate_exact_size(egui::vec2(w, 1.6 * em + 0.6 * em), egui::Sense::hover());
                ui.painter().rect_filled(r, 0.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 9));
                ui.painter().hline(r.x_range(), r.bottom(), egui::Stroke::new(1.0, gold));
                ui.painter().text(
                    egui::pos2(r.left() + 0.5 * em, r.center().y),
                    egui::Align2::LEFT_CENTER,
                    if preview.is_empty() { " " } else { preview },
                    font.clone(),
                    egui::Color32::WHITE,
                );
                ui.add_space(0.3 * em);
                let mut draw_key = |ui: &mut egui::Ui, r: usize, c: usize, label: String, width: f32, ch: char| {
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, key), egui::Sense::click());
                    let focused = r == self.row && c == self.col;
                    let p = ui.painter();
                    if focused {
                        p.add(
                            egui::epaint::Shadow { offset: [0, 0], blur: 12, spread: 0, color: egui::Color32::from_rgba_unmultiplied(216, 180, 95, 89) }
                                .as_shape(rect, 2.0),
                        );
                        p.rect_filled(rect, 2.0, egui::Color32::from_rgba_unmultiplied(216, 180, 95, 71));
                        p.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, gold), egui::StrokeKind::Inside);
                    } else {
                        p.rect_filled(rect, 2.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 13));
                    }
                    let ink = if focused { egui::Color32::WHITE } else { isorun_ui::theme::INK };
                    p.text(rect.center(), egui::Align2::CENTER_CENTER, label, font.clone(), ink);
                    if resp.clicked() {
                        clicked = Some(OskEdit::Type(ch));
                    }
                };
                if !target.extras.is_empty() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.4 * em;
                        for (c, e) in target.extras.iter().enumerate() {
                            let label = format!("{} {}", e.ch, e.label);
                            let width = ui.fonts_mut(|f| f.layout_no_wrap(label.clone(), font.clone(), egui::Color32::WHITE).size().x) + 1.4 * em;
                            draw_key(ui, 0, c, label, width, e.ch);
                        }
                    });
                }
                let first = if target.extras.is_empty() { 0 } else { 1 };
                for (r0, row) in OSK_ROWS.iter().enumerate() {
                    ui.horizontal(|ui| {
                        for (c, ch) in row.chars().enumerate() {
                            let ch = if self.shift { ch.to_ascii_uppercase() } else { ch };
                            let label = if ch == ' ' { "␣".to_string() } else { ch.to_string() };
                            draw_key(ui, r0 + first, c, label, key, ch);
                        }
                    });
                }
                // the hint line: the buttons and what they do, the keys at the end
                let mut line = String::new();
                for p in prompts {
                    let button = if p.submit { target.submit_button.unwrap_or(p.button) } else { p.button };
                    line.push_str(&format!("{} {} · ", button.name(), p.label));
                }
                line.push_str("Enter / Esc");
                ui.add_space(0.3 * em);
                ui.label(egui::RichText::new(line).font(egui::FontId::monospace(0.8 * em)).color(isorun_ui::theme::INK_DIM));
                let _ = rows.len();
            });
            let rect = inner.response.rect;
            let shapes = vec![
                egui::epaint::Shadow { offset: [0, 24], blur: 70, spread: 0, color: egui::Color32::from_black_alpha(179) }.as_shape(rect, 2.0).into(),
                {
                    let mut mesh = egui::Mesh::default();
                    let (top, bottom) = (egui::Color32::from_rgba_unmultiplied(14, 12, 20, 247), egui::Color32::from_rgba_unmultiplied(7, 6, 11, 247));
                    mesh.colored_vertex(rect.left_top(), top);
                    mesh.colored_vertex(rect.right_top(), top);
                    mesh.colored_vertex(rect.right_bottom(), bottom);
                    mesh.colored_vertex(rect.left_bottom(), bottom);
                    mesh.add_triangle(0, 1, 2);
                    mesh.add_triangle(0, 2, 3);
                    egui::Shape::mesh(mesh)
                },
                egui::Shape::rect_stroke(rect, 2.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(216, 180, 95, 82)), egui::StrokeKind::Inside),
            ];
            ui.painter().set(bg, egui::Shape::Vec(shapes));
        });
        clicked
    }
}

/// A text field the keyboard can type into: a string and a caret (a char index), with a
/// longest length and a password's dots.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OskText {
    pub text: String,
    pub caret: usize,
    pub max_len: Option<usize>,
    pub password: bool,
}

impl OskText {
    pub fn new(text: &str) -> Self {
        OskText { text: text.into(), caret: text.chars().count(), max_len: None, password: false }
    }

    /// Apply an edit; Submit and Cancel are the owner's, and come back.
    pub fn apply(&mut self, edit: &OskEdit) -> Option<OskEdit> {
        match edit {
            OskEdit::Type(ch) => {
                if self.max_len.is_some_and(|m| self.text.chars().count() >= m) {
                    return None;
                }
                let at = self.byte(self.caret);
                self.text.insert(at, *ch);
                self.caret += 1;
                None
            }
            OskEdit::Backspace => {
                if self.caret > 0 {
                    let (a, b) = (self.byte(self.caret - 1), self.byte(self.caret));
                    self.text.replace_range(a..b, "");
                    self.caret -= 1;
                }
                None
            }
            other => Some(other.clone()),
        }
    }

    fn byte(&self, chars: usize) -> usize {
        self.text.char_indices().nth(chars).map_or(self.text.len(), |(i, _)| i)
    }

    /// What the keyboard's preview shows: dots for a password, the line being typed on for a box of lines.
    pub fn preview(&self) -> String {
        if self.password {
            return "•".repeat(self.text.chars().count());
        }
        let at = self.byte(self.caret);
        let start = self.text[..at].rfind('\n').map_or(0, |i| i + 1);
        let end = self.text[at..].find('\n').map_or(self.text.len(), |i| i + at);
        self.text[start..end].to_string()
    }
}
