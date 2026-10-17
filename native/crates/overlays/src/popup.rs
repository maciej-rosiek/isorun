//! crawl's popups (overlays.ts `renderPopup`): what each type shows, built into a model of
//! sections (the header with its tiles, the scrolling body, the actions line, a footer, a more
//! line) with the stops the cursor walks; and the drawing of that model.

use crate::focus::{Act, Focusable};
use crate::frame::{self, Screen};
use crate::menu::{self, paint_focused};
use crate::menu_nav;
use crate::rich::{self, TextStyle};
use crate::tiles::{self, Tile};
use crate::{OverlayCx, Overlays};
use egui::{Color32, Pos2, Rect, Vec2};
use isorun_ui::text::TERM_COLOURS;
use isorun_ui::tiles::TileIcons;
use isorun_ui::widgets::{self, hex, rgba};
use isorun_webtiles::{cm, formatted_string_to_text, js_string, Keys, Popup};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{Map, Value};

/// One entry of a describe popup's `actions` string ("(w)ield", "or (d)rop."): clickify_action
/// in the official ui-layouts.js. A trailing period and an "or " prefix are decoration, the key is
/// the parenthesised character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionWord {
    pub prefix: String,
    pub suffix: String,
    pub text: String,
    pub key: Option<String>,
}

pub fn parse_action_word(w: &str) -> ActionWord {
    let mut w = w.to_string();
    let mut suffix = String::new();
    let mut prefix = String::new();
    if w.ends_with('.') {
        suffix = ".".into();
        w.pop();
    }
    if let Some(rest) = w.strip_prefix("or ") {
        prefix = "or ".into();
        w = rest.to_string();
    }
    static KEY: Lazy<Regex> = Lazy::new(|| Regex::new(r"\((.)\)").unwrap());
    let key = KEY.captures(&w).map(|c| c[1].to_string());
    ActionWord { prefix, suffix, text: w, key }
}

/// The `actions` string, joined with ", " on the server (describe.cc _actions_desc): each word
/// keeps the text the line prints, parentheses and all ("(d)rop", not "Drop"), so the pad's chip
/// and the screen say the same thing.
pub fn parse_actions(text: &str) -> Vec<(String, String)> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split(", ")
        .map(parse_action_word)
        .filter_map(|a| {
            let k = a.key?;
            let label = a.text.trim().to_string();
            Some((k.clone(), if label.is_empty() { k } else { label }))
        })
        .collect()
}

/// What a formatted scroller does with keys it never prints, as a describe popup's actions line
/// would spell them: the dungeon overview's (dgn-overview.cc `_process_command`), and the help's
/// menu (command.cc `help_popup`: every "<w>k</w>: Section" line of the text it was pushed with).
pub fn scroller_actions(tag: &str, text: &str, pushed: &str) -> String {
    static OVERVIEW: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\s|<[^>]+>)*Dungeon Overview and Level Annotations").unwrap());
    static HELP_LINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<w>(.)</w>: ([^\n<]+)").unwrap());
    if OVERVIEW.is_match(text) {
        return "(G) Travel, (_) Altar, ($) Shops, (!) Annotate".into();
    }
    if tag == "help" || text.contains("Dungeon Crawl Help") {
        return HELP_LINE.captures_iter(pushed).map(|c| format!("({}) {}", &c[1], c[2].trim())).collect::<Vec<_>>().join(", ");
    }
    String::new()
}

/// A god's powers list (ui-layouts.js describe_god): the server's block of lines less its heading
/// and trailing blank, each split into the power and its cost; a power the player cannot use yet
/// comes through darkgrey.
pub fn god_powers(list: &str) -> Vec<(String, String, bool)> {
    static LINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(<[a-z]*>)?(.*\.) *(\(.*\))?$").unwrap());
    let lines: Vec<&str> = list.split('\n').collect();
    if lines.len() < 4 {
        return Vec::new();
    }
    lines[3..lines.len() - 1]
        .iter()
        .filter_map(|l| {
            let m = LINE.captures(l)?;
            Some((m[2].to_string(), m.get(3).map_or(String::new(), |x| x.as_str().to_string()), m.get(1).map(|x| x.as_str()) == Some("<darkgrey>")))
        })
        .collect()
}

/// A spell's description, as the official format_spell_html does: the `Level:` line is one field
/// per column, so its spaces are held together rather than left to wrap.
pub fn split_spell_desc(txt: &str) -> Option<(String, String, String)> {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)^(.*\n\n)(Level: [^\n]+)(\n\n.*)$").unwrap());
    let m = RE.captures(txt)?;
    Some((m[1].to_string(), m[2].to_string(), m[3].to_string()))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Spell {
    pub letter: String,
    pub title: String,
    pub effect: String,
    pub range: String,
    pub colour: Option<u8>,
    pub tile: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpellBook {
    pub label: String,
    pub spells: Vec<Spell>,
}

fn spellbooks(v: Option<&Value>) -> Vec<SpellBook> {
    v.and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .map(|b| SpellBook {
            label: b.get("label").map(js_string).unwrap_or_default(),
            spells: b
                .get("spells")
                .and_then(|s| s.as_array())
                .into_iter()
                .flatten()
                .map(|s| Spell {
                    letter: s.get("letter").map(js_string).unwrap_or_default(),
                    title: s.get("title").map(js_string).unwrap_or_default(),
                    effect: s.get("effect").map(js_string).unwrap_or_default(),
                    range: s.get("range_string").or(s.get("range")).map(js_string).unwrap_or_default(),
                    colour: s.get("colour").and_then(|c| c.as_u64()).map(|c| c as u8),
                    tile: s.get("tile").and_then(|t| t.as_i64()),
                })
                .collect(),
        })
        .collect()
}

/// A part of a popup's scrolling body.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// a formatted string as one run of pre-wrapped text
    Formatted(String),
    /// fmtBody: paragraphs (split at blank lines) each a `<pre>` with a 0.6em foot
    Paras(String),
    /// a line held together (the spell's `Level:` line)
    Nowrap(String),
    /// server HTML (the seed screen's body)
    Html(String),
    Spellset { books: Vec<SpellBook>, colour: bool, first_row: i32 },
    /// a god's Title / Favour table (`.god-favour`), each value in the god's colour
    GodFavour { rows: Vec<(String, String)>, colour: u8 },
    GodPowers { powers: Vec<(String, String, bool)>, colour: u8 },
    /// `.tbl`: white-space pre
    Table(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Section {
    Header { tiles: Vec<Tile>, tile_px: f32, title: String, colour: Option<u8>, formatted: bool },
    Body(Vec<Block>),
    Actions(String),
    /// a footer: its formatted text, and the stop it is when it switches panes
    Footer { text: String, stop: Option<String> },
    More(String),
}

#[derive(Clone, Debug, Default)]
pub struct PopupModel {
    pub sections: Vec<Section>,
    pub focusables: Vec<Focusable>,
    /// where the cursor starts: crawl's first verb, or nothing (-1)
    pub initial: i32,
    pub actions: Vec<(String, String, Vec<Value>)>,
    /// describe popups keep the official client's 80-column measure (82ch)
    pub narrow: bool,
    /// the character overview dims the world behind it
    pub dim: bool,
}

fn s(d: &Map<String, Value>, k: &str) -> String {
    match d.get(k) {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

struct Builder {
    m: PopupModel,
    row: i32,
}

impl Builder {
    /// The actions line: clickable, and offered to the pad in order; `lit`: the cursor starts on its first verb.
    fn actions(&mut self, text: &str, lit: bool) {
        if text.is_empty() {
            return;
        }
        let parsed = parse_actions(text);
        if lit && self.m.initial < 0 && !parsed.is_empty() {
            self.m.initial = self.m.focusables.len() as i32;
        }
        let r = self.row;
        self.row += 1;
        for (i, (key, label)) in parsed.iter().enumerate() {
            let send = vec![cm::input(key)];
            self.m.focusables.push(Focusable { label: label.clone(), act: Act::Send(send.clone()), row: Some(r), col: Some(i as i32), id: Some(format!("act:{key}")), ..Default::default() });
            self.m.actions.push((key.clone(), label.clone(), send));
        }
        self.m.sections.push(Section::Actions(text.to_string()));
    }

    fn header(&mut self, title: &str, tiles: Vec<Tile>, colour: Option<u8>) {
        if title.is_empty() && tiles.is_empty() {
            return;
        }
        self.m.sections.push(Section::Header { tiles, tile_px: 64.0, title: title.to_string(), colour, formatted: true });
    }

    /// A describe popup's scrolling body: the spell list where the server put its placeholder.
    fn describe_body(&mut self, text: &str, books: Vec<SpellBook>, colour: bool) {
        let parts: Vec<&str> = text.split("SPELLSET_PLACEHOLDER").collect();
        let mut blocks = vec![Block::Paras(parts[0].to_string())];
        if !books.is_empty() {
            let first_row = self.row;
            for b in &books {
                for (i, sp) in b.spells.iter().enumerate() {
                    if i % 2 == 0 {
                        self.row += 1;
                    }
                    let send = vec![cm::text_input(&sp.letter)];
                    self.m.focusables.push(Focusable {
                        label: formatted_string_to_text(&sp.title),
                        act: Act::Send(send.clone()),
                        row: Some(self.row - 1),
                        col: Some((i % 2) as i32),
                        id: Some(format!("spell:{}", sp.letter)),
                        ..Default::default()
                    });
                }
            }
            for b in &books {
                for sp in &b.spells {
                    self.m.actions.push((sp.letter.clone(), formatted_string_to_text(&sp.title), vec![cm::text_input(&sp.letter)]));
                }
            }
            blocks.push(Block::Spellset { books, colour, first_row });
        }
        if parts.len() > 1 {
            blocks.push(Block::Paras(parts[1..].concat()));
        }
        self.m.sections.push(Section::Body(blocks));
    }

    /// The pane switch is one row of the popup, and the row is the whole list ("Description | Quote").
    fn pane_action(&mut self, names: &[&str], text: String) {
        if names.len() < 2 {
            self.m.sections.push(Section::Footer { text, stop: None });
            return;
        }
        let label = names.join(" | ");
        let send = vec![cm::input("!")];
        self.m.actions.push(("!".into(), label.clone(), send.clone()));
        self.m.focusables.push(Focusable { label, act: Act::Send(send), row: Some(self.row), id: Some("pane".into()), ..Default::default() });
        self.row += 1;
        self.m.sections.push(Section::Footer { text, stop: Some("pane".into()) });
    }
}

/// What a popup shows and offers.
pub fn model(p: &Popup, icons: &TileIcons) -> PopupModel {
    let d = &p.data;
    let mut b = Builder { m: PopupModel { initial: -1, ..Default::default() }, row: 0 };
    let state_str = |k: &str| p.state.get(k).and_then(|v| v.as_str()).map(|s| s.to_string());
    match p.kind.as_str() {
        "formatted-scroller" => {
            // the character overview (`%`) is "resists": the world dims behind it
            b.m.dim = s(d, "tag") == "resists";
            b.header(&s(d, "title"), Vec::new(), None);
            let text = state_str("text").unwrap_or_else(|| s(d, "text"));
            b.m.sections.push(Section::Body(vec![Block::Formatted(text.clone())]));
            if !s(d, "more").is_empty() {
                b.m.sections.push(Section::More(s(d, "more")));
            }
            // the keys a scroller answers to but never prints: the reader's to go to, nothing lit at first
            b.actions(&scroller_actions(&s(d, "tag"), &text, &s(d, "text")), false);
        }
        "describe-spell" => {
            b.m.narrow = true;
            b.header(&s(d, "title"), Tile::list(d.get("tile")), None);
            let desc = if s(d, "desc").is_empty() { s(d, "body") } else { s(d, "desc") };
            let blocks = match split_spell_desc(&desc) {
                Some((a, level, c)) => vec![Block::Paras(a), Block::Nowrap(level), Block::Paras(c)],
                None => vec![Block::Paras(desc)],
            };
            b.m.sections.push(Section::Body(blocks));
            if d.get("can_mem").is_some_and(isorun_webtiles::truthy) {
                b.actions("(M)emorise this spell.", true);
            }
        }
        "describe-item" | "describe-cards" | "describe-generic" => {
            b.m.narrow = true;
            let tiles = if d.get("tiles").is_some_and(|t| t.is_array()) { Tile::list(d.get("tiles")) } else { Tile::list(d.get("tile")) };
            b.header(&s(d, "title"), tiles, None);
            let body = s(d, "body") + if p.kind == "describe-generic" { s(d, "footer") } else { String::new() }.as_str();
            b.describe_body(&body, spellbooks(d.get("spellset")), true);
            b.actions(&s(d, "actions"), true);
        }
        "describe-feature-wide" => {
            b.m.narrow = true;
            for f in d.get("feats").and_then(|v| v.as_array()).into_iter().flatten() {
                let fo = f.as_object().cloned().unwrap_or_default();
                let title = s(&fo, "title");
                b.m.sections.push(Section::Header { tiles: Tile::list(fo.get("tile")), tile_px: 64.0, title: title.clone(), colour: None, formatted: false });
                let body = s(&fo, "body");
                if body != title {
                    let quote = s(&fo, "quote");
                    b.m.sections.push(Section::Body(vec![Block::Formatted(body + if quote.is_empty() { String::new() } else { format!("\n\n{quote}") }.as_str())]));
                }
            }
            b.actions(&s(d, "actions"), true);
        }
        "describe-monster" | "describe-god" => {
            b.m.narrow = true;
            let god = p.kind == "describe-god";
            let colour = d.get("colour").and_then(|c| c.as_u64()).map(|c| c as u8);
            if god {
                let name = if s(d, "name").is_empty() { s(d, "title") } else { s(d, "name") };
                b.header(&name, Tile::list(d.get("tile")), colour);
            } else {
                b.header(&s(d, "title"), tiles::monster_tiles(icons, d), None);
            }
            let pane = p.state.get("pane").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
            let (panes, names): (Vec<String>, Vec<&str>) = if god {
                let mut panes = vec![String::new(), s(d, "powers"), s(d, "wrath")];
                let mut names = vec!["Overview", "Powers", "Wrath"];
                if !s(d, "extra").is_empty() {
                    panes.push(s(d, "extra"));
                    names.push("Extra");
                }
                (panes, names)
            } else {
                // the status and quote panes exist only when non-empty
                let mut panes = vec![s(d, "body")];
                let mut names = vec!["Description"];
                if !s(d, "status").is_empty() {
                    panes.push(s(d, "status"));
                    names.push("Status");
                }
                if !s(d, "quote").is_empty() {
                    panes.push(s(d, "quote"));
                    names.push("Quote");
                }
                (panes, names)
            };
            let cur = pane.min(panes.len() - 1);
            let prompt = state_str("prompt").map(|p| format!("\n{p}")).unwrap_or_default();
            if god {
                let c = colour.unwrap_or(7);
                let blocks = match cur {
                    1 => {
                        let mut v = vec![Block::Paras(s(d, "powers"))];
                        if !s(d, "info_table").is_empty() {
                            v.push(Block::Table(s(d, "info_table")));
                        }
                        v
                    }
                    2 => vec![Block::Paras(s(d, "wrath"))],
                    3 => vec![Block::Table(s(d, "extra"))],
                    _ => vec![
                        Block::Paras(s(d, "description")),
                        Block::GodFavour { rows: vec![("Title".into(), s(d, "title")), ("Favour".into(), s(d, "favour"))], colour: c },
                        Block::GodPowers { powers: god_powers(&s(d, "powers_list")), colour: c },
                    ],
                };
                b.m.sections.push(Section::Body(blocks));
                // "!/^: Overview | Powers | Wrath", and at an altar "J/Enter: join religion"
                let panes_text = names.iter().enumerate().map(|(i, n)| if i == cur { format!("<white>{n}</white>") } else { n.to_string() }).collect::<Vec<_>>().join(" | ");
                let mut text = format!("<cyan>!</cyan>/<cyan>^</cyan>:\u{a0}{panes_text}");
                let r = b.row;
                b.row += 1;
                b.m.focusables.push(Focusable { label: names.join(" | "), act: Act::send([cm::input("!")]), row: Some(r), col: Some(0), id: Some("panes".into()), ..Default::default() });
                if d.get("is_altar").is_some_and(isorun_webtiles::truthy) {
                    text.push_str(&format!("\u{a0}\u{a0}<cyan>J</cyan>/<cyan>Enter</cyan>: join religion{}", s(d, "service_fee")));
                    b.m.initial = b.m.focusables.len() as i32;
                    b.m.focusables.push(Focusable { label: "Join religion".into(), act: Act::send([cm::key(Keys::ENTER)]), row: Some(r), col: Some(1), id: Some("join".into()), ..Default::default() });
                }
                text.push_str(&prompt);
                b.m.sections.push(Section::Footer { text, stop: None });
                b.actions(&s(d, "actions"), true);
            } else {
                b.describe_body(&panes[cur], if cur == 0 { spellbooks(d.get("spellset")) } else { Vec::new() }, false);
                // the pane names sit behind the key that cycles them, in cyan
                let names_text = names.iter().enumerate().map(|(i, n)| if i == cur { format!("<white>{n}</white>") } else { n.to_string() }).collect::<Vec<_>>().join(" | ");
                b.pane_action(&names, format!("[<cyan>!</cyan>]:\u{a0}{names_text}{prompt}"));
                b.actions(&s(d, "actions"), true);
            }
        }
        "version" | "game-over" => {
            b.header(&s(d, "title"), Tile::list(d.get("tile")), None);
            let body = [s(d, "body"), s(d, "information")].into_iter().find(|t| !t.is_empty()).unwrap_or_default();
            b.m.sections.push(Section::Body(vec![Block::Paras(body)]));
            if !s(d, "features").is_empty() {
                b.m.sections.push(Section::Body(vec![Block::Paras(s(d, "features"))]));
            }
        }
        "progress-bar" => {
            let status = state_str("status").unwrap_or_else(|| s(d, "status"));
            let bar = state_str("bar_text").unwrap_or_else(|| s(d, "bar_text"));
            b.header(&s(d, "title"), Vec::new(), None);
            b.m.sections.push(Section::Body(vec![Block::Formatted(format!("{bar}\n{}", status.replace('<', "<<")))]));
        }
        "seed-selection" => {
            b.header(&s(d, "title"), Vec::new(), None);
            b.m.sections.push(Section::Body(vec![Block::Html(s(d, "body"))]));
            let r = b.row;
            b.row += 1;
            for (i, (id, label, msg)) in [("seed", "Type seed", None), ("clear", "Clear", Some(45)), ("daily", "Today's daily seed", Some(100))].into_iter().enumerate() {
                let act = msg.map_or(Act::Keyboard, |k| Act::send([cm::key(k)]));
                b.m.focusables.push(Focusable { label: label.into(), act, row: Some(r), col: Some(i as i32), id: Some(id.into()), ..Default::default() });
            }
            b.m.actions.push(("-".into(), "Clear".into(), vec![cm::key(45)]));
            b.m.actions.push(("d".into(), "Today's daily seed".into(), vec![cm::key(100)]));
            b.m.actions.push(("?".into(), "Help".into(), vec![cm::key(63)]));
            b.m.sections.push(Section::Footer { text: "Enter seed: [-] Clear  [d] Today's daily seed".into(), stop: None });
            b.m.sections.push(Section::Footer { text: rich::html_spans(&s(d, "footer")).into_iter().map(|s| s.text).collect(), stop: None });
        }
        "msgwin-get-line" => b.header(&s(d, "prompt"), Vec::new(), None),
        "newgame-random-combo" => {
            b.header(&s(d, "prompt"), tiles::doll(d.get("doll")), None);
            if let Some(Section::Header { tile_px, .. }) = b.m.sections.last_mut() {
                *tile_px = 64.0;
            }
            b.m.sections.push(Section::Body(vec![Block::Formatted("Do you want to play this combination?".into())]));
            // newgame.cc _prompt_choose_random: Y plays it, n rolls again, q quits
            b.actions("(Y)es, (n)o, (q)uit", true);
        }
        _ => {
            let title = if s(d, "title").is_empty() { p.kind.clone() } else { s(d, "title") };
            b.header(&title, Vec::new(), None);
            let body = [s(d, "body"), s(d, "text")].into_iter().find(|t| !t.is_empty()).unwrap_or_else(|| Value::Object(d.clone()).to_string().chars().take(500).collect());
            b.m.sections.push(Section::Body(vec![Block::Paras(body)]));
        }
    }
    // the more line a popup prints is walkable too: its switches are rows of the cursor's ring
    if let Some(more) = b.m.sections.iter().find_map(|s| if let Section::More(t) = s { Some(t.clone()) } else { None }) {
        let r = b.row;
        let sws = menu::switches(&formatted_string_to_text(&more));
        for sw in &sws {
            let mut send = Vec::new();
            let code = menu_nav::more_switch_keycode(&sw.key);
            if code != 0 {
                send.push(cm::key(code));
            } else if sw.key.chars().count() == 1 {
                send.push(cm::input(&sw.key));
            }
            b.m.focusables.push(Focusable { label: sw.label.clone(), act: Act::Send(send.clone()), row: Some(r + sw.line as i32), col: Some(sw.col as i32), id: Some(format!("more:{}", sw.key)), ..Default::default() });
            b.m.actions.push((sw.key.clone(), sw.label.clone(), send));
        }
        b.row += sws.iter().map(|s| s.line as i32).max().unwrap_or(0) + 1;
    }
    b.m
}

/// The `show_dialog` html's choices: every `[data-key]` element sends that key as input bytes
/// (client.js handle_dialog: the launcher's `read` waits on stdin, not crawl's socket).
pub fn dialog_focus(html: &str) -> Vec<Focusable> {
    dialog_buttons(html)
        .into_iter()
        .enumerate()
        .map(|(i, (label, key))| {
            let act = key.map_or(Act::None, |k| Act::send([cm::input_bytes(&[k as u8])]));
            let cancel = matches!(label.to_ascii_lowercase().as_str(), "no" | "cancel");
            Focusable { label, act, row: Some(0), col: Some(i as i32), cancel, ..Default::default() }
        })
        .collect()
}

/// The buttons of a dialog's html: their words and the key each sends.
pub fn dialog_buttons(html: &str) -> Vec<(String, Option<char>)> {
    static EL: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?is)<(button|a|input)\b([^>]*)>(?:(.*?)</(?:button|a)>)?"#).unwrap());
    static KEY: Lazy<Regex> = Lazy::new(|| Regex::new(r#"data-key\s*=\s*["']([^"']*)["']"#).unwrap());
    static VALUE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"value\s*=\s*["']([^"']*)["']"#).unwrap());
    EL.captures_iter(html)
        .filter(|c| &c[1] != "input" || c[2].contains("button") || c[2].contains("submit") || KEY.is_match(&c[2]))
        .map(|c| {
            let key = KEY.captures(&c[2]).and_then(|k| k[1].chars().next());
            let text: String = c.get(3).map(|t| rich::html_spans(t.as_str()).into_iter().map(|s| s.text).collect()).unwrap_or_default();
            let label = VALUE.captures(&c[2]).map(|v| v[1].to_string()).filter(|v| !v.is_empty()).unwrap_or(text);
            (if label.trim().is_empty() { "OK".to_string() } else { label.trim().to_string() }, key)
        })
        .collect()
}

// ------------------------------------------------------------------------------------ drawing

/// A laid-out block of the body.
enum Laid {
    Galley { at: f32, g: std::sync::Arc<egui::Galley> },
    Spells { at: f32, rows: Vec<SpellRow>, label: Option<(f32, std::sync::Arc<egui::Galley>)> },
    Gap,
}

struct SpellRow {
    y: f32,
    col: usize,
    tile: Option<Tile>,
    title: std::sync::Arc<egui::Galley>,
    effect: std::sync::Arc<egui::Galley>,
    range: std::sync::Arc<egui::Galley>,
    id: String,
    h: f32,
}

struct BodyLayout {
    items: Vec<Laid>,
    height: f32,
}

fn lay_body(ctx: &egui::Context, blocks: &[Block], st: &TextStyle, width: f32, icons: &mut TileIcons) -> BodyLayout {
    let font = st.size;
    let mut y = 0.0;
    let mut items = Vec::new();
    for b in blocks {
        match b {
            Block::Formatted(t) => {
                let g = rich::galley(ctx, rich::formatted(t, st, width));
                items.push(Laid::Galley { at: y, g: g.clone() });
                y += g.size().y;
            }
            Block::Html(h) => {
                let g = rich::galley(ctx, rich::html(h, st, width));
                items.push(Laid::Galley { at: y, g: g.clone() });
                y += g.size().y;
            }
            Block::Nowrap(t) => {
                let g = rich::galley(ctx, rich::formatted(t, st, 0.0));
                items.push(Laid::Galley { at: y, g: g.clone() });
                y += g.size().y;
            }
            Block::Table(t) => {
                let g = rich::galley(ctx, rich::formatted(t, st, 0.0));
                items.push(Laid::Galley { at: y, g: g.clone() });
                y += g.size().y;
            }
            Block::Paras(t) => {
                // fmtBody: a `<pre>` per paragraph, an empty one dropped; one with no text but its tags is a
                // line box with nothing in it, which has no height, and only its 0.6em foot shows
                for para in t.split("\n\n") {
                    if para.is_empty() {
                        continue;
                    }
                    if !formatted_string_to_text(para).is_empty() {
                        let g = rich::galley(ctx, rich::formatted(para, st, width));
                        items.push(Laid::Galley { at: y, g: g.clone() });
                        y += g.size().y;
                    }
                    y += 0.6 * font;
                }
            }
            Block::Spellset { books, colour, .. } => {
                // `.spellset { margin: 0.5em 0 }`; entries two to a line, each half the width
                y += 0.5 * font;
                for book in books {
                    let label = (!book.label.trim().is_empty()).then(|| {
                        let g = rich::galley(ctx, rich::formatted(&book.label, st, width));
                        let at = y;
                        y += g.size().y;
                        (at, g)
                    });
                    let half = width / 2.0;
                    let gap = 0.3 * font;
                    let mut rows = Vec::new();
                    let gui = icons.gamedata().and_then(|g| g.gd.enums.get("texture", "GUI"));
                    for (i, sp) in book.spells.iter().enumerate() {
                        let colour = if *colour { sp.colour.map(|c| TERM_COLOURS[c as usize & 15]).unwrap_or(st.colour) } else { st.colour };
                        let row_st = st.clone().colour(colour);
                        let tile = match (sp.tile, gui) {
                            (Some(t), Some(g)) => Some(Tile { t, tex: Some(g), ..Default::default() }),
                            _ => None,
                        };
                        let effect = rich::galley(ctx, rich::formatted(&sp.effect, &row_st, 0.0));
                        let range = rich::galley(ctx, rich::formatted(&sp.range, &row_st, 0.0));
                        let fixed = if tile.is_some() { 32.0 + gap } else { 0.0 } + if sp.effect.is_empty() { 0.0 } else { effect.size().x + gap } + if sp.range.is_empty() { 0.0 } else { range.size().x + gap };
                        let mut tj = rich::formatted(&format!("{} - {}", sp.letter, sp.title), &row_st, 0.0);
                        tj.wrap = egui::text::TextWrapping::truncate_at_width((half - fixed).max(10.0));
                        let title = rich::galley(ctx, tj);
                        let h = 1.3f32.max(1.0) * font + 0.1 * font;
                        let h = h.max(if tile.is_some() { 32.0 + 0.1 * font } else { 0.0 });
                        rows.push(SpellRow { y: y + (i / 2) as f32 * h, col: i % 2, tile, title, effect, range, id: format!("spell:{}", sp.letter), h });
                    }
                    let n = book.spells.len().div_ceil(2);
                    let h = rows.first().map_or(0.0, |r| r.h);
                    let at = y;
                    y += n as f32 * h;
                    items.push(Laid::Spells { at, rows, label });
                }
                y += 0.5 * font;
            }
            Block::GodFavour { rows, colour } => {
                y += font;
                for (label, value) in rows {
                    let mut j = rich::plain(&format!("{label}\u{a0}\u{a0}-\u{a0}\u{a0}"), st, 0.0);
                    let vj = rich::formatted(value, &st.clone().colour(TERM_COLOURS[*colour as usize & 15]), 0.0);
                    j.sections.extend(vj.sections.into_iter().map(|mut s| {
                        let off = j.text.len();
                        s.byte_range = s.byte_range.start + off..s.byte_range.end + off;
                        s
                    }));
                    j.text.push_str(&vj.text);
                    let g = rich::galley(ctx, j);
                    items.push(Laid::Galley { at: y, g: g.clone() });
                    y += g.size().y;
                }
                y += font;
                items.push(Laid::Gap);
            }
            Block::GodPowers { powers, colour } => {
                let mut lines: Vec<(String, String, Color32)> = vec![("Granted powers".into(), "(Cost)".into(), st.colour)];
                lines.extend(powers.iter().map(|(p, c, dim)| (p.clone(), c.clone(), TERM_COLOURS[if *dim { 8 } else { *colour as usize & 15 }])));
                for (power, cost, c) in lines {
                    let g = rich::galley(ctx, rich::runs(&[(&power, c)], st, (width - 8.0 * font).max(10.0)));
                    let cg = rich::galley(ctx, rich::runs(&[(&cost, c)], st, 0.0));
                    items.push(Laid::Galley { at: y, g: g.clone() });
                    items.push(Laid::Galley { at: -(y + 1.0), g: cg });
                    y += g.size().y;
                }
                y += font;
            }
        }
    }
    BodyLayout { items, height: y }
}

/// The natural width of a body (its longest line).
fn body_natural(ctx: &egui::Context, blocks: &[Block], st: &TextStyle) -> f32 {
    let mut w: f32 = 0.0;
    for b in blocks {
        let n = match b {
            Block::Formatted(t) | Block::Nowrap(t) | Block::Table(t) => rich::natural_width(ctx, rich::formatted(t, st, 0.0)),
            Block::Paras(t) => rich::natural_width(ctx, rich::formatted(&t.replace("\n\n", "\n"), st, 0.0)),
            Block::Html(h) => rich::natural_width(ctx, rich::html(h, st, 0.0)),
            Block::Spellset { books, .. } => books
                .iter()
                .flat_map(|b| b.spells.iter())
                .map(|sp| 2.0 * (32.0 + rich::natural_width(ctx, rich::formatted(&format!("{} - {}  {}  {}", sp.letter, sp.title, sp.effect, sp.range), st, 0.0))))
                .fold(0.0, f32::max),
            Block::GodFavour { rows, .. } => rows.iter().map(|(a, b)| rich::natural_width(ctx, rich::formatted(&format!("{a}    {b}"), st, 0.0))).fold(0.0, f32::max),
            Block::GodPowers { powers, .. } => powers.iter().map(|(a, b, _)| rich::natural_width(ctx, rich::plain(&format!("{a} {b}"), st, 0.0)) + st.size).fold(0.0, f32::max),
        };
        w = w.max(n);
    }
    w
}

/// The actions line as runs: decoration in the line's grey, the words in white; with the char
/// range of each word, for its stop.
fn actions_job(text: &str, st: &TextStyle, focused: Option<&str>, wrap: f32) -> (egui::text::LayoutJob, Vec<(String, std::ops::Range<usize>)>) {
    let grey = hex(0xaaaaaa);
    let mut parts: Vec<(String, Color32, Option<String>)> = Vec::new();
    for (i, w) in text.split(", ").enumerate() {
        if i > 0 {
            parts.push((", ".into(), grey, None));
        }
        let a = parse_action_word(w);
        if !a.prefix.is_empty() {
            parts.push((a.prefix.clone(), grey, None));
        }
        parts.push((a.text.clone(), if a.key.is_some() { Color32::WHITE } else { grey }, a.key.clone()));
        if !a.suffix.is_empty() {
            parts.push((a.suffix.clone(), grey, None));
        }
    }
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = if wrap > 0.0 { wrap } else { f32::INFINITY };
    let mut ranges = Vec::new();
    let mut chars = 0;
    let mut pad_next = false;
    for (t, c, key) in parts {
        let lit = key.as_deref().is_some_and(|k| Some(k) == focused);
        // `.popup .actions span[data-hotkey].focused { padding: 0 0.2em }`
        let lead = if lit || pad_next { 0.2 * st.size } else { 0.0 };
        pad_next = lit;
        let fmt = egui::TextFormat { font_id: st.font(), color: c, line_height: Some(st.line_height), ..Default::default() };
        job.append(&t, lead, fmt);
        let n = t.chars().count();
        if let Some(k) = key {
            ranges.push((k, chars..chars + n));
        }
        chars += n;
    }
    (job, ranges)
}

/// Whether a block's text stands in columns: a run of three spaces or more between words.
fn columned(b: &Block) -> bool {
    let t = match b {
        Block::Formatted(t) | Block::Paras(t) | Block::Html(t) | Block::Nowrap(t) | Block::Table(t) => formatted_string_to_text(t),
        _ => return false,
    };
    t.lines().any(|l| {
        let l = l.trim();
        l.contains("   ")
    })
}

impl Overlays {
    /// Draw a popup of crawl's; `top` is the one the keys go to.
    pub(crate) fn draw_popup(&mut self, ctx: &egui::Context, cx: &mut OverlayCx, p: &Popup, top: bool, depth: usize, screen: &Screen) {
        if p.kind == "newgame-choice" {
            return self.draw_newgame(ctx, cx, p, top, screen);
        }
        let m = model(p, cx.tiles);
        let font = screen.popup_font();
        // text laid out in columns (the overview's gods, the help's index, a character dump's
        // tables) wraps into a muddle on a phone: there it is set smaller, to fit the width if it
        // can, but no smaller than 8px or two thirds of the size; prose wraps as it is
        let font = if screen.compact() && m.sections.iter().any(|s| matches!(s, Section::Body(b) if b.iter().any(columned))) {
            let base = TextStyle::new(ctx, font, hex(0xdddddd));
            let natural = m.sections.iter().map(|s| if let Section::Body(b) = s { body_natural(ctx, b, &base) } else { 0.0 }).fold(0.0, f32::max);
            let room = screen.popup_max_w() - 2.0 - 2.0 * 0.8 * screen.rem() - frame::SCROLLBAR_W;
            if natural > room { (font * room / natural).max((2.0 / 3.0 * font).max(8.0)).min(font) } else { font }
        } else {
            font
        };
        let st = TextStyle::new(ctx, font, hex(0xdddddd));
        let grey = st.clone().colour(hex(0xaaaaaa));
        let pad_y = 0.5 * screen.rem();
        let pad_x = 0.8 * screen.rem();
        let foot_pad_y = 0.4 * screen.rem();
        let rule = frame::rule_colour(cx.dock);
        let focused = |nav: &crate::focus::FocusNav, id: &str| top && nav.is_current(id);
        let cur_act = if top { self.nav.current().and_then(|f| f.id.as_deref()).and_then(|id| id.strip_prefix("act:")).map(|s| s.to_string()) } else { None };
        // the natural width of each section
        let mut natural: f32 = 0.0;
        for s in &m.sections {
            let w = match s {
                Section::Header { tiles, tile_px, title, formatted, .. } => {
                    let tj = if *formatted { rich::formatted(title, &st, 0.0) } else { rich::plain(title, &st, 0.0) };
                    rich::natural_width(ctx, tj) + if tiles.is_empty() { 0.0 } else { tile_px + 0.6 * font }
                }
                Section::Body(blocks) => body_natural(ctx, blocks, &st),
                Section::Actions(t) => rich::natural_width(ctx, actions_job(t, &st, cur_act.as_deref(), 0.0).0),
                Section::Footer { text, .. } | Section::More(text) => rich::natural_width(ctx, rich::formatted(text, &grey, 0.0)),
            };
            natural = natural.max(w);
        }
        let max_w = if m.narrow { screen.popup_max_w().min(82.0 * widgets::ch(font)) } else { screen.popup_max_w() };
        let mut words = 0usize;
        let mut tile_min: f32 = 0.0;
        for s in &m.sections {
            match s {
                Section::Header { tiles, tile_px, title, .. } => {
                    words = words.max(frame::longest_word(&formatted_string_to_text(title)));
                    if !tiles.is_empty() {
                        tile_min = tile_min.max(tile_px + 0.6 * font + frame::longest_word(&formatted_string_to_text(title)) as f32 * widgets::ch(font));
                    }
                }
                Section::Body(blocks) => {
                    for b in blocks {
                        let t = match b {
                            Block::Formatted(t) | Block::Paras(t) | Block::Html(t) => formatted_string_to_text(t),
                            Block::Nowrap(t) | Block::Table(t) => formatted_string_to_text(t).replace(' ', "\u{a0}"),
                            _ => String::new(),
                        };
                        words = words.max(frame::longest_word(&t));
                    }
                }
                Section::Actions(t) | Section::Footer { text: t, .. } | Section::More(t) => words = words.max(frame::longest_word(&formatted_string_to_text(t))),
            }
        }
        let min_content = (words as f32 * widgets::ch(font)).max(tile_min);
        let width = frame::shrink_to_fit(screen, natural + 2.0 * pad_x + 2.0, min_content + 2.0 * pad_x + 2.0, screen.popup_min_w(), max_w);
        let inner = width - 2.0 - 2.0 * pad_x;
        // lay every section out at that width
        enum L {
            Header(Vec<Tile>, f32, std::sync::Arc<egui::Galley>),
            Body(usize, BodyLayout),
            Actions(std::sync::Arc<egui::Galley>, Vec<(String, std::ops::Range<usize>)>),
            Foot(std::sync::Arc<egui::Galley>, Option<String>, String),
        }
        let mut laid = Vec::new();
        let mut heights = Vec::new();
        for (i, s) in m.sections.iter().enumerate() {
            match s {
                Section::Header { tiles, tile_px, title, colour, formatted } => {
                    let hst = colour.map_or(st.clone(), |c| st.clone().colour(TERM_COLOURS[c as usize & 15]));
                    let tw = if tiles.is_empty() { 0.0 } else { tile_px + 0.6 * font };
                    let g = rich::galley(ctx, if *formatted { rich::formatted(title, &hst, inner - tw) } else { rich::plain(title, &hst, inner - tw) });
                    heights.push(2.0 * pad_y + g.size().y.max(if tiles.is_empty() { 0.0 } else { *tile_px }) + 1.0);
                    laid.push(L::Header(tiles.clone(), *tile_px, g));
                }
                Section::Body(blocks) => {
                    let b = lay_body(ctx, blocks, &st, inner, cx.tiles);
                    heights.push(b.height + 2.0 * pad_y);
                    laid.push(L::Body(i, b));
                }
                Section::Actions(t) => {
                    let (j, r) = actions_job(t, &st, cur_act.as_deref(), inner);
                    let g = rich::galley(ctx, j);
                    heights.push(g.size().y + 2.0 * foot_pad_y + 1.0);
                    laid.push(L::Actions(g, r));
                }
                Section::Footer { text, stop } => {
                    let lit = stop.as_deref().is_some_and(|s| focused(&self.nav, s));
                    let g = rich::galley(ctx, rich::formatted(text, &grey, inner));
                    heights.push(g.size().y + if lit { 0.0 } else { 2.0 * foot_pad_y } + 1.0);
                    laid.push(L::Foot(g, stop.clone(), String::new()));
                }
                Section::More(text) => {
                    let g = rich::galley(ctx, rich::formatted(text, &grey, inner));
                    heights.push(g.size().y + 2.0 * foot_pad_y + 1.0);
                    laid.push(L::Foot(g, None, formatted_string_to_text(text)));
                }
            }
        }
        // the scrolling body is the tallest; it gives way when the popup would pass its max height
        let body_ix = laid.iter().enumerate().filter_map(|(k, l)| if let L::Body(_, b) = l { Some((k, b.height)) } else { None }).max_by(|a, b| a.1.total_cmp(&b.1)).map(|x| x.0);
        let total: f32 = heights.iter().sum::<f32>() + 2.0;
        let max_h = screen.popup_max_h();
        let mut content_h = 0.0;
        let mut client_h = 0.0;
        if let Some(k) = body_ix {
            content_h = heights[k];
            if total > max_h {
                heights[k] = (heights[k] - (total - max_h)).max(0.0).floor();
            }
            client_h = heights[k];
        }
        let overflow = content_h > client_h + 1.0;
        let height: f32 = heights.iter().sum::<f32>() + 2.0;
        let rect = frame::place(screen, Vec2::new(width, height), depth, p.ui_centred == Some(true), cx.touch && top);
        let key = p.stack_order;
        let max_scroll = (content_h - client_h).max(0.0);
        // the server's scroll line, when a new one arrived and it was not an echo of ours
        if top && p.scroll_seq != 0 && self.popup_scroll_applied.get(&key) != Some(&p.scroll_seq) {
            self.popup_scroll_applied.insert(key, p.scroll_seq);
            if let Some(line) = p.scroll {
                if !p.scroll_from_webtiles || cx.watching {
                    let s = if line == 2147483647 { max_scroll } else { line as f32 * st.line_height };
                    self.popup_scroll.insert(key, s);
                }
            }
        }
        let scroll = self.popup_scroll.get(&key).copied().unwrap_or(0.0).clamp(0.0, max_scroll);
        if top {
            self.popup_geom = crate::PopupGeom { stack: key, client_h, scroll_h: content_h, line_h: st.line_height };
        }
        let dock = cx.dock;
        let mut new_scroll = scroll;
        let mut clicked: Vec<Vec<Value>> = Vec::new();
        let mut close = false;
        let nav_ids: Option<String> = if top { self.nav.current().map(|f| f.key().to_string()) } else { None };
        let is_lit = |id: &str| nav_ids.as_deref() == Some(id);
        let id = egui::Id::unique(("isorun-popup", key));
        let model_focus = m.focusables.clone();
        frame::layer(ctx, id, rect, |ui, _| {
            let pt = ui.painter().clone();
            widgets::paint_panel(&pt, rect, &frame::look(dock));
            let left = rect.left() + 1.0 + pad_x;
            let mut y = rect.top() + 1.0;
            for (k, l) in laid.iter().enumerate() {
                let h = heights[k];
                match l {
                    L::Header(tiles, tile_px, g) => {
                        let cy = y + h / 2.0 - 0.5;
                        let mut x = left;
                        if !tiles.is_empty() {
                            tiles::paint(cx.tiles, &pt, tiles, tiles::square(x, cy, *tile_px));
                            x += tile_px + 0.6 * font;
                        }
                        pt.galley(Pos2::new(x, cy - g.size().y / 2.0), g.clone(), st.colour);
                        widgets::rule(&pt, rect.left() + 1.0..=rect.right() - 1.0, y + h - 1.0, rule);
                    }
                    L::Body(_, b) => {
                        let view = Rect::from_min_size(Pos2::new(rect.left() + 1.0, y), Vec2::new(width - 2.0, h));
                        let scrolled = Some(k) == body_ix;
                        let off = if scrolled { new_scroll } else { 0.0 };
                        if scrolled && overflow {
                            new_scroll = frame::wheel(ui, view, content_h, new_scroll);
                        }
                        let bp = pt.with_clip_rect(view);
                        let top_y = y + pad_y - off;
                        for item in &b.items {
                            match item {
                                Laid::Galley { at, g } if *at >= 0.0 => bp.galley(Pos2::new(left, top_y + at), g.clone(), st.colour),
                                Laid::Galley { at, g } => bp.galley(Pos2::new(left + inner - g.size().x, top_y + (-at - 1.0)), g.clone(), st.colour),
                                Laid::Spells { at, rows, label } => {
                                    if let Some((ly, g)) = label {
                                        bp.galley(Pos2::new(left, top_y + ly), g.clone(), st.colour);
                                    }
                                    let _ = at;
                                    let half = inner / 2.0;
                                    for r in rows {
                                        let rr = Rect::from_min_size(Pos2::new(left + r.col as f32 * half, top_y + r.y), Vec2::new(half, r.h));
                                        if is_lit(&r.id) {
                                            paint_focused(&bp, rr);
                                        }
                                        let mut x = rr.left();
                                        if let Some(t) = &r.tile {
                                            tiles::paint(cx.tiles, &bp, std::slice::from_ref(t), tiles::square(x, rr.center().y, 32.0));
                                            x += 32.0 + 0.3 * font;
                                        }
                                        bp.galley(Pos2::new(x, rr.center().y - r.title.size().y / 2.0), r.title.clone(), st.colour);
                                        let mut rx = rr.right();
                                        for g in [&r.range, &r.effect] {
                                            if g.size().x > 0.0 {
                                                rx -= g.size().x;
                                                bp.galley(Pos2::new(rx, rr.center().y - g.size().y / 2.0), (*g).clone(), st.colour);
                                                rx -= 0.3 * font;
                                            }
                                        }
                                        if view.contains(rr.center()) && ui.interact(rr, id.with(&r.id), egui::Sense::click()).clicked() {
                                            if let Some(f) = model_focus.iter().find(|f| f.key() == r.id) {
                                                if let Act::Send(v) = &f.act {
                                                    clicked.push(v.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                                Laid::Gap => {}
                            }
                        }
                        if scrolled && overflow {
                            new_scroll = frame::scrollbar(ui, id.with("bar"), view, content_h, new_scroll);
                        }
                        if k + 1 < laid.len() && !matches!(laid[k + 1], L::Actions(..) | L::Foot(..)) {
                            // bodies one after another (the feature popup) are separated by their headers' rules
                        }
                    }
                    L::Actions(g, ranges) => {
                        widgets::rule(&pt, rect.left() + 1.0..=rect.right() - 1.0, y, rule);
                        let at = Pos2::new(left, y + 1.0 + foot_pad_y);
                        for (key, r) in ranges {
                            let a = g.pos_from_cursor(egui::text::CCursor::new(r.start));
                            let b = g.pos_from_cursor(egui::text::CCursor::new(r.end));
                            let mut cr = Rect::from_min_max(a.min, Pos2::new(b.min.x, a.max.y)).translate(at.to_vec2());
                            let lit = is_lit(&format!("act:{key}"));
                            if lit {
                                cr = cr.expand2(Vec2::new(0.2 * font, 0.0));
                                paint_focused(&pt, cr);
                            }
                            if ui.interact(cr, id.with(("act", key)), egui::Sense::click()).clicked() {
                                clicked.push(vec![cm::input(key)]);
                            }
                        }
                        pt.galley(at, g.clone(), hex(0xaaaaaa));
                    }
                    L::Foot(g, stop, plain) => {
                        widgets::rule(&pt, rect.left() + 1.0..=rect.right() - 1.0, y, rule);
                        let lit = stop.as_deref().is_some_and(&is_lit);
                        let r = Rect::from_min_max(Pos2::new(rect.left() + 1.0, y + 1.0), Pos2::new(rect.right() - 1.0, y + h));
                        let at = if lit { Pos2::new(r.left() + 0.2 * font, r.top()) } else { Pos2::new(left, y + 1.0 + foot_pad_y) };
                        if lit {
                            paint_focused(&pt, r);
                        }
                        if !plain.is_empty() {
                            // a more line's switches: walkable stops of their own
                            for sw in menu::switches(plain) {
                                if let Some(cr) = menu::char_rect(g, plain, sw.line, sw.col, sw.len) {
                                    let cr = cr.translate(at.to_vec2());
                                    if is_lit(&format!("more:{}", sw.key)) {
                                        menu::paint_hover(&pt, cr, dock);
                                    }
                                    if ui.interact(cr, id.with(("more", sw.col, sw.line)), egui::Sense::click()).clicked() {
                                        let code = menu_nav::more_switch_keycode(&sw.key);
                                        clicked.push(vec![if code != 0 { cm::key(code) } else { cm::input(&sw.key) }]);
                                    }
                                }
                            }
                        }
                        pt.galley(at, g.clone(), hex(0xaaaaaa));
                        if stop.is_some() && ui.interact(r, id.with("pane"), egui::Sense::click()).clicked() {
                            clicked.push(vec![cm::input("!")]);
                        }
                    }
                }
                y += h;
            }
            if cx.touch && top && !cx.watching {
                close = frame::touch_close(ui, id.with("close"), rect);
            }
        });
        if (new_scroll - scroll).abs() > 0.1 && top && p.kind == "formatted-scroller" && self.scroller_report_at.is_none() {
            // ui-layouts.js scroller_onscroll: the server is told where it is, 100 ms after the wheel stops
            self.scroller_report_at = Some(self.now + 0.1);
        }
        self.popup_scroll.insert(key, new_scroll);
        if m.dim && top {
            // drawn under the popup by `ui` (it knows before the layers are made)
        }
        if !top {
            return;
        }
        for msgs in clicked {
            for msg in msgs {
                cx.send(msg);
            }
        }
        if close {
            self.back(cx);
        }
    }

    /// ui-layouts.js update_server_scroll: a formatted scroller's top line, so the server keeps our place.
    pub(crate) fn report_scroller(&mut self, cx: &mut OverlayCx) {
        self.scroller_report_at = None;
        let Some(p) = cx.state.ui.last() else { return };
        if p.kind != "formatted-scroller" || cx.watching {
            return;
        }
        let g = &self.popup_geom;
        let s = self.popup_scroll.get(&p.stack_order).copied().unwrap_or(0.0);
        let line = (s / g.line_h.max(1.0)).round() as i64;
        cx.send(cm::formatted_scroller_scroll(line));
    }

    /// ui-layouts.js scroller_handle_key: a line, a page or an end; false when the top popup has no
    /// scrolling body. A page is the body's height in whole lines less one, so the last line read
    /// stays in view.
    pub(crate) fn popup_scroll_by(&mut self, key: menu_nav::ScrollKey, cx: &mut OverlayCx) -> bool {
        let Some(p) = cx.state.ui.last() else { return false };
        let g = self.popup_geom.clone();
        if g.stack != p.stack_order || g.scroll_h <= 0.0 {
            return false;
        }
        let lh = g.line_h.max(1.0);
        let page = ((g.client_h / lh).floor() - 1.0).max(1.0) * lh;
        let max = (g.scroll_h - g.client_h).max(0.0);
        let s = self.popup_scroll.get(&p.stack_order).copied().unwrap_or(0.0);
        use menu_nav::ScrollKey::*;
        let n = match key {
            LineUp => s - lh,
            LineDown => s + lh,
            PageUp => s - page,
            PageDown => s + page,
            Home => 0.0,
            End => max,
        };
        self.popup_scroll.insert(p.stack_order, n.clamp(0.0, max));
        // a key reports at once
        self.report_scroller(cx);
        true
    }

    /// The top popup's text is taller than its box.
    pub(crate) fn popup_overflows(&self) -> bool {
        self.popup_geom.scroll_h > self.popup_geom.client_h + 1.0
    }

    /// A dialog's html (`show_dialog`): its text and its buttons.
    pub(crate) fn draw_dialog(&mut self, ctx: &egui::Context, cx: &mut OverlayCx, html: &str, screen: &Screen) {
        let font = screen.popup_font();
        let st = TextStyle::new(ctx, font, hex(0xdddddd));
        static TAGS: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<(button|a)\b[^>]*>.*?</(button|a)>|<input\b[^>]*>").unwrap());
        let text = TAGS.replace_all(html, "");
        let width = if screen.compact() { screen.popup_max_w() } else { (screen.popup_min_w().max(400.0)).min(screen.popup_max_w()) };
        let pad = screen.rem();
        let g = rich::galley(ctx, rich::html(&text, &st, width - 2.0 * pad));
        let buttons = dialog_buttons(html);
        let bh = widgets::normal_line_height(font) + 0.4 * font + 2.0;
        let height = g.size().y + 2.0 * pad + if buttons.is_empty() { 0.0 } else { bh + 0.3 * font };
        let rect = frame::place(screen, Vec2::new(width, height), 0, false, false);
        let dock = cx.dock;
        let mut clicked = None;
        let lit = self.nav.index();
        frame::layer(ctx, egui::Id::unique("isorun-dialog"), rect, |ui, _| {
            let p = ui.painter().clone();
            widgets::paint_panel(&p, rect, &frame::look(dock));
            p.galley(rect.min + Vec2::splat(pad), g.clone(), st.colour);
            let mut x = rect.left() + pad;
            let y = rect.top() + pad + g.size().y + 0.3 * font;
            for (i, (label, _)) in buttons.iter().enumerate() {
                let lg = rich::galley(ui.ctx(), rich::plain(label, &st, 0.0));
                let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(lg.size().x + 2.4 * font + 2.0, bh));
                if widgets::button(ui, r, egui::Id::unique(("isorun-dialog-btn", i)), label, widgets::ButtonLook::Plain).clicked() {
                    clicked = Some(i);
                }
                if i as i32 == lit {
                    paint_focused(ui.painter(), r);
                }
                x = r.right() + 0.3 * font;
            }
        });
        if let Some(i) = clicked {
            self.nav.focus(i as i32);
            if let Some(k) = buttons[i].1 {
                cx.send(cm::input_bytes(&[k as u8]));
            }
        }
    }
}

/// `rgba` re-exported for the other drawing modules.
pub(crate) fn dim_colour() -> Color32 {
    rgba(3, 2, 6, 0.55)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_words() {
        assert_eq!(parse_action_word("or (d)rop."), ActionWord { prefix: "or ".into(), suffix: ".".into(), text: "(d)rop".into(), key: Some("d".into()) });
        assert_eq!(parse_actions("(w)ield, (q)uiver, or (d)rop."), vec![("w".into(), "(w)ield".into()), ("q".into(), "(q)uiver".into()), ("d".into(), "(d)rop".into())]);
        assert_eq!(parse_actions("(<)go up."), vec![("<".into(), "(<)go up".into())]);
        assert!(parse_actions("").is_empty());
        assert!(parse_actions("nothing here").is_empty());
    }

    #[test]
    fn scroller_keys() {
        assert_eq!(scroller_actions("", "<lightgrey>    <white>Dungeon Overview and Level Annotations</white>", ""), "(G) Travel, (_) Altar, ($) Shops, (!) Annotate");
        assert_eq!(scroller_actions("help", "x", "<w>?</w>: List of commands\n<w>^</w>: Quickstart Guide\n"), "(?) List of commands, (^) Quickstart Guide");
        assert_eq!(scroller_actions("resists", "Player the Fighter", ""), "");
    }

    #[test]
    fn god_power_lines() {
        let list = "<white>Granted powers:</white>\n\n\nYou can call upon Trog to rage. (Piety)\n<darkgrey>You can call upon Trog for help.\n";
        let p = god_powers(list);
        assert_eq!(p[0], ("You can call upon Trog to rage.".into(), "(Piety)".into(), false));
        assert!(p[1].2);
    }

    #[test]
    fn spell_level_line() {
        let (a, l, c) = split_spell_desc("Hurls a dart.\n\nLevel: 1        School: Conjuration\n\nFail: 3%").unwrap();
        assert_eq!((a.as_str(), l.as_str(), c.as_str()), ("Hurls a dart.\n\n", "Level: 1        School: Conjuration", "\n\nFail: 3%"));
        assert!(split_spell_desc("no level").is_none());
    }

    #[test]
    fn dialogs() {
        let b = dialog_buttons("<p>Transfer your save?</p><input type='button' data-key='y' value='Yes'><input type='button' data-key='n' value='No'>");
        assert_eq!(b, vec![("Yes".into(), Some('y')), ("No".into(), Some('n'))]);
        let f = dialog_focus("<button data-key='n'>No</button>");
        assert!(f[0].cancel);
    }

    #[test]
    fn describe_model() {
        let mut p = Popup { kind: "describe-item".into(), ..Default::default() };
        p.data.insert("title".into(), "a +0 dagger".into());
        p.data.insert("body".into(), "A dagger.\n\nIt is sharp.".into());
        p.data.insert("actions".into(), "(w)ield, (q)uiver, or (d)rop.".into());
        let m = model(&p, &TileIcons::default());
        assert_eq!(m.initial, 0);
        assert_eq!(m.focusables.iter().map(|f| f.key().to_string()).collect::<Vec<_>>(), ["act:w", "act:q", "act:d"]);
        assert_eq!(m.actions.len(), 3);
        assert!(matches!(m.sections[0], Section::Header { .. }));
        // a monster's panes are one stop, and the actions after
        let mut mon = Popup { kind: "describe-monster".into(), ..Default::default() };
        mon.data.insert("title".into(), "a goblin".into());
        mon.data.insert("body".into(), "A goblin.".into());
        mon.data.insert("quote".into(), "Quote.".into());
        let m = model(&mon, &TileIcons::default());
        assert_eq!(m.focusables[0].label, "Description | Quote");
        assert_eq!(m.initial, -1);
    }
}
