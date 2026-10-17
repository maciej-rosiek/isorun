//! The WebTiles chat box (chat.ts, chat.js, client.html): the spectator count caption, the
//! spectator list, the message history, an input line. It keeps the official behaviour: the
//! caption toggles the body, unread lines are counted while the body is closed ("3 new messages
//! (Press F12)"), `(-)` hides the whole box to a `+`, Enter sends, up and down walk the sent-line
//! history, Esc or F12 close it. The server's `toggle_chat` and `super_hide_chat` are read from
//! state. A server's html is kept to its text: no server's markup runs here.

use crate::paint::{hex, rgba};
use egui::{Color32, Rect};
use isorun_webtiles::{cm, ChatLine, GameState};
use once_cell::sync::Lazy;
use regex::Regex;

/// chat.js: how many sent lines the up/down history keeps.
const HISTORY_LIMIT: usize = 25;

/// One line of the history as drawn: who said it and what, or the server's own words.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatText {
    pub sender: String,
    pub text: String,
    pub automated: bool,
}

static TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]*>").unwrap());

/// A server's html as plain text (chat.ts `serverHtml`, kept to its text): tags dropped, the
/// entities html escapes with read back.
pub fn server_text(html: &str) -> String {
    let t = TAG.replace_all(html, "");
    t.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&#x27;", "'").replace("&nbsp;", " ").replace("&amp;", "&")
}

/// A chat line's sender and message, read off the server's spans (`chat_sender`, `chat_msg`).
pub fn chat_text(line: &ChatLine) -> ChatText {
    static MSG: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?s)<span class="chat_msg[^"]*">(.*)</span>"#).unwrap());
    let text = MSG.captures(&line.html).map(|c| server_text(&c[1])).unwrap_or_else(|| {
        let all = server_text(&line.html);
        all.strip_prefix(&format!("{}: ", line.sender)).map(String::from).unwrap_or(all)
    });
    ChatText { sender: line.sender.clone(), text, automated: line.automated }
}

/// The urls in a line (chat.js `linkify`), as (start, end) byte ranges; a closing bracket or
/// punctuation right after a url is prose, not the url.
pub fn links(text: &str) -> Vec<(usize, usize)> {
    static URL: Lazy<Regex> = Lazy::new(|| Regex::new(r"https?://[^\s<]+").unwrap());
    URL.find_iter(text)
        .map(|m| {
            let mut end = m.end();
            while end > m.start() && matches!(text.as_bytes()[end - 1], b'.' | b',' | b';' | b':' | b'!' | b'?' | b')' | b']') {
                end -= 1;
            }
            (m.start(), end)
        })
        .collect()
}

#[derive(Default)]
pub struct Chat {
    open: bool,
    hidden: bool,
    new_messages: usize,
    seen: usize,
    sent: Vec<String>,
    unsent: String,
    history_pos: Option<usize>,
    input: String,
    focus_input: bool,
}

impl Chat {
    pub fn new() -> Chat {
        Chat::default()
    }

    /// F12: the box visible, the caret in its line (chat.js `focus`).
    pub fn focus(&mut self) {
        self.hidden = false;
        self.open = true;
        self.new_messages = 0;
        self.focus_input = true;
    }

    /// Whether the line has the keyboard: game keys stay out of it.
    pub fn capturing(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|m| m.focused() == Some(egui::Id::unique("hud-chat-input")))
    }

    /// Draw the box at the foot of the sidebar's column (`column`), when the state says chat is
    /// on; lines typed go out as `chat` messages.
    pub fn ui(&mut self, ctx: &egui::Context, st: &GameState, column: Rect, in_game: bool, logged_in: bool, out: &mut Vec<serde_json::Value>) {
        let super_hidden = st.chat_super_hidden;
        let show = in_game && st.chat_visible && !self.hidden && !super_hidden;
        let tab = in_game && self.hidden && !super_hidden;
        // new lines while the body is closed are counted
        if st.chat.len() < self.seen {
            self.seen = 0;
            self.new_messages = 0;
        }
        for l in &st.chat[self.seen..] {
            if !self.open && !l.meta {
                self.new_messages += 1;
            }
        }
        self.seen = st.chat.len();
        if tab {
            egui::Area::new(egui::Id::unique("hud-chat-hidden")).order(egui::Order::Foreground).anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-(ctx.content_rect().right() - column.right()), -(ctx.content_rect().bottom() - column.bottom()))).show(ctx, |ui| {
                egui::Frame::new().fill(rgba(0, 0, 0, 0.7).to_color32()).corner_radius(4).inner_margin(egui::Margin::symmetric(8, 0)).show(ui, |ui| {
                    if ui.add(egui::Label::new(egui::RichText::new("+").color(hex(0x99ccff).to_color32())).sense(egui::Sense::click())).clicked() {
                        self.hidden = false;
                    }
                });
            });
        }
        if !show {
            return;
        }
        let font = 15.0 * 0.8;
        egui::Area::new(egui::Id::unique("hud-chat")).order(egui::Order::Foreground).fixed_pos(egui::pos2(column.left(), column.bottom())).pivot(egui::Align2::LEFT_BOTTOM).show(ctx, |ui| {
            ui.set_width(column.width());
            egui::Frame::new().fill(rgba(0, 0, 0, 0.7).to_color32()).stroke(egui::Stroke::new(1.0, rgba(255, 255, 255, 0.15).to_color32())).corner_radius(4).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
                ui.set_width(column.width() - 18.0);
                ui.style_mut().override_font_id = Some(egui::FontId::monospace(font));
                ui.horizontal(|ui| {
                    let n = st.spectators.as_ref().map_or(0, |s| s.count);
                    let caption = format!("{n} {}", if n == 1 { "spectator" } else { "spectators" });
                    let count = if self.open {
                        "(Esc: close)".to_string()
                    } else {
                        format!("{} new messages{}", self.new_messages, if self.new_messages > 0 { " (Press F12)" } else { "" })
                    };
                    let count_colour = if self.new_messages > 0 && !self.open { hex(0xffff55) } else { hex(0xdddddd) };
                    let r = ui.add(egui::Label::new(egui::RichText::new(caption).color(hex(0xdddddd).to_color32())).sense(egui::Sense::click()));
                    let r2 = ui.add(egui::Label::new(egui::RichText::new(count).color(count_colour.to_color32())).sense(egui::Sense::click()));
                    if r.clicked() || r2.clicked() {
                        self.open = !self.open;
                        self.new_messages = 0;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Label::new(egui::RichText::new("(-)").color(hex(0x99ccff).to_color32())).sense(egui::Sense::click())).clicked() {
                            self.hidden = true;
                        }
                    });
                });
                if !self.open {
                    return;
                }
                let names = st.spectators.as_ref().map(|s| server_text(&s.names)).unwrap_or_default();
                ui.label(egui::RichText::new(if names.is_empty() { " ".into() } else { names }).color(hex(0xaaaaaa).to_color32()));
                egui::ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.28).stick_to_bottom(true).show(ui, |ui| {
                    for l in &st.chat {
                        let t = chat_text(l);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            if !t.sender.is_empty() {
                                ui.label(egui::RichText::new(format!("{}: ", t.sender)).strong().color(Color32::WHITE));
                            }
                            let colour = if t.automated { hex(0xff55ff) } else { hex(0xdddddd) };
                            let mut at = 0;
                            for (a, b) in links(&t.text) {
                                ui.label(egui::RichText::new(&t.text[at..a]).color(colour.to_color32()));
                                ui.hyperlink_to(egui::RichText::new(&t.text[a..b]).color(hex(0x99ccff).to_color32()), &t.text[a..b]);
                                at = b;
                            }
                            ui.label(egui::RichText::new(&t.text[at..]).color(colour.to_color32()));
                        });
                    }
                });
                if !logged_in {
                    ui.label(egui::RichText::new("Log in to chat").color(hex(0xaaaaaa).to_color32()));
                    return;
                }
                let edit = egui::TextEdit::singleline(&mut self.input).id(egui::Id::unique("hud-chat-input")).desired_width(f32::INFINITY);
                let r = ui.add(edit);
                if self.focus_input {
                    r.request_focus();
                    self.focus_input = false;
                }
                if r.has_focus() {
                    let (_enter, up, down, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::F12)));
                    if up && self.history_pos.map_or(0, |p| p + 1) < self.sent.len().min(HISTORY_LIMIT) {
                        // keep the unsent line so going past the start of the history brings it back
                        if self.history_pos.is_none() {
                            self.unsent = self.input.clone();
                        }
                        let p = self.history_pos.map_or(0, |p| p + 1);
                        self.history_pos = Some(p);
                        self.input = self.sent[p].clone();
                    }
                    if down {
                        match self.history_pos {
                            Some(0) => {
                                self.input = std::mem::take(&mut self.unsent);
                                self.history_pos = None;
                            }
                            Some(p) => {
                                self.history_pos = Some(p - 1);
                                self.input = self.sent[p - 1].clone();
                            }
                            None => {}
                        }
                    }
                    if esc {
                        self.open = false;
                        r.surrender_focus();
                    }
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.input.is_empty() {
                    out.push(cm::chat(&self.input));
                    self.sent.insert(0, std::mem::take(&mut self.input));
                    self.sent.truncate(HISTORY_LIMIT);
                    self.history_pos = None;
                    self.unsent.clear();
                    r.request_focus();
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_html_is_text() {
        let l = ChatLine { sender: "ada".into(), html: r#"<span class="chat_sender">ada</span>: <span class="chat_msg">hi &lt;b&gt; <script>x</script>see https://x.org/a.</span>"#.into(), meta: false, automated: false };
        let t = chat_text(&l);
        assert_eq!(t.text, "hi <b> xsee https://x.org/a.");
        let ls = links(&t.text);
        assert_eq!(&t.text[ls[0].0..ls[0].1], "https://x.org/a");
    }
}
