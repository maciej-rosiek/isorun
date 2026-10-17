//! The prompt card (overlays.ts `updatePrompt`, `letterGrid`): a yes/no or choice prompt in the
//! middle of the view, the question and one chip per answer, each wearing what fires it on the
//! device that spoke last: after the keyboard or the mouse the hotkey as a key cap, after the pad
//! the button. A yes/no has no cursor (A is Yes, B is No, the keyboard's Enter crawl's default,
//! which its chip says); a choice prompt is a row the cursor walks; the letter picker a grid of
//! the inventory's letters. The stat gain is the level-up banner's cards instead.

use crate::focus::{Act, FocusOptions, Focusable};
use crate::frame::Screen;
use crate::menu::paint_focused;
use crate::prompt::{default_yesno, parse_prompt, prompt_lead, Letters, Mode, ParsedPrompt};
use crate::rich::{self, TextStyle};
use crate::{Device, OverlayCx, Overlays};
use egui::{Color32, Pos2, Rect, Vec2};
use isorun_ui::text::TERM_COLOURS;
use isorun_ui::widgets::{self, hex, rgba, KeycapLook, PanelLook};
use isorun_webtiles::cm;

/// A chip of the card.
#[derive(Clone, Debug)]
pub struct Chip {
    pub label: String,
    pub hotkey: String,
    pub colour: Option<u8>,
    pub held: Option<String>,
    pub from: bool,
}

/// The card as built for a prompt and a device.
#[derive(Clone, Debug)]
pub struct Card {
    pub text: Option<String>,
    pub lead: Option<String>,
    pub chips: Vec<Chip>,
    pub letters: Option<Letters>,
    pub yesno: bool,
    pub default: Option<String>,
    pub pad: bool,
}

/// What an answer sends: Tab and Enter are keys, not text (context.ts NAMED_KEYS).
pub fn answer(hotkey: &str) -> serde_json::Value {
    if hotkey == "\t" || hotkey == "\r" {
        cm::key(hotkey.chars().next().unwrap() as i32)
    } else {
        cm::input(hotkey)
    }
}

fn key_name(h: &str) -> String {
    match h {
        "\t" => "Tab".into(),
        "\r" => "Enter".into(),
        o => o.into(),
    }
}

/// The pad's face button an answer of a yes/no sits on: Yes on A, No on B, Always on X.
pub fn yesno_button(hotkey: &str) -> &'static str {
    match hotkey.to_ascii_uppercase().as_str() {
        "Y" => "A",
        "N" => "B",
        "A" => "X",
        _ => "A",
    }
}

impl Overlays {
    /// Rebuild the card when the prompt or the device changes (called every frame).
    pub(crate) fn update_prompt(&mut self, mode: Mode, cx: &mut OverlayCx) {
        let mut screen = String::new();
        let mut p: Option<ParsedPrompt> = None;
        if matches!(mode, Mode::Yesno | Mode::Prompt) && !cx.watching {
            let parsed = parse_prompt(cx.state).or_else(|| (mode == Mode::Yesno).then(default_yesno));
            if let Some(pp) = parsed.filter(|pp| !pp.options.is_empty()) {
                screen = format!(
                    "{:?}|{}|{}{}",
                    mode,
                    pp.text,
                    pp.letters.map(|l| format!("{l:?}|{}|", pp.from.clone().unwrap_or_default())).unwrap_or_default(),
                    pp.options.iter().map(|o| format!("{}={}{}", o.hotkey, o.label, o.held.as_ref().map(|h| format!(":{h}")).unwrap_or_default())).collect::<Vec<_>>().join(",")
                );
                p = Some(pp);
            }
        }
        let pad = cx.device == Device::Pad;
        let key = if screen.is_empty() { String::new() } else { format!("{screen}|{}", if pad { "pad" } else { "kbd" }) };
        if key == self.prompt_key {
            return;
        }
        let same_screen = !screen.is_empty() && screen == self.prompt_screen;
        self.prompt_key = key;
        self.prompt_screen = screen.clone();
        self.card = None;
        self.prompt_focusables.clear();
        let stat_gain = p.as_ref().is_some_and(|p| p.stat_gain);
        self.prompt_initial = Some(if p.as_ref().is_some_and(|p| !p.cancel) { -1 } else { 0 });
        if !same_screen {
            self.focus_gen += 1;
        }
        if !stat_gain {
            self.level_up.end_choice(self.now);
        }
        let Some(p) = p else { return };
        if stat_gain {
            self.prompt_focus_opts = FocusOptions { wrap: true, linear: true, ..Default::default() };
            self.prompt_focusables = self.level_up.choose(&p.options, pad, self.now);
            if same_screen {
                self.nav.set(self.prompt_focusables.clone(), &format!("prompt:{screen}"), self.prompt_focus_opts, self.prompt_initial, false);
            }
            return;
        }
        let lead = if p.yesno { None } else { prompt_lead(&p.text) };
        let chips: Vec<Chip> = p.options.iter().map(|o| Chip { label: o.label.clone(), hotkey: o.hotkey.clone(), colour: o.colour, held: o.held.clone(), from: p.from.as_deref() == Some(o.hotkey.as_str()) }).collect();
        if let Some(_kind) = p.letters {
            // four rows of thirteen: a-m, n-z, A-M, N-Z; the bumpers hop two rows between the cases
            const COLS: usize = 13;
            for (i, c) in chips.iter().enumerate() {
                self.prompt_focusables.push(Focusable { label: c.label.clone(), act: Act::send([answer(&c.hotkey)]), row: Some((i / COLS) as i32), col: Some((i % COLS) as i32), id: Some(format!("letter:{}", c.hotkey)), ..Default::default() });
            }
            let from = p.from.as_ref().and_then(|f| chips.iter().position(|c| &c.hotkey == f));
            self.prompt_initial = Some(from.unwrap_or(0) as i32);
            self.prompt_focus_opts = FocusOptions { wrap: true, page_rows: 2, ..Default::default() };
        } else {
            self.prompt_focus_opts = FocusOptions { wrap: true, linear: true, ..Default::default() };
            // a yes/no's answers are on A and B, not under a cursor
            if !p.yesno {
                for (i, c) in chips.iter().enumerate() {
                    self.prompt_focusables.push(Focusable { label: c.label.clone(), colour: c.colour, act: Act::send([answer(&c.hotkey)]), row: Some(0), col: Some(i as i32), id: Some(format!("chip:{i}")), ..Default::default() });
                }
            }
        }
        self.card = Some(Card { text: (lead.is_none() && !p.text.is_empty()).then(|| p.text.clone()), lead, chips, letters: p.letters, yesno: p.yesno, default: p.default.clone(), pad });
        // the same prompt drawn for another device: the cursor stays on the chip it was on
        if same_screen {
            self.nav.set(self.prompt_focusables.clone(), &format!("prompt:{screen}"), self.prompt_focus_opts, self.prompt_initial, false);
        }
    }

    /// Draw the card in the middle of the view.
    pub(crate) fn draw_card(&mut self, ctx: &egui::Context, cx: &mut OverlayCx, screen: &Screen) {
        let Some(card) = self.card.clone() else { return };
        // `.prompt-card`: the page's font (1rem), chips at 1.05em
        let font = screen.rem();
        let chip_font = 1.05 * font;
        let st = TextStyle::new(ctx, chip_font, hex(0xdddddd));
        let pad_t = 0.9 * font;
        let pad_x = 1.2 * font;
        let pad_b = 1.0 * font;
        let cap = KeycapLook::chip(chip_font);
        let gap = 0.6 * font;
        let lit = self.nav.current().map(|f| f.key().to_string());
        // the chips laid out first: their sizes make the card's
        struct ChipLay {
            size: Vec2,
            caps: Vec<String>,
            default: bool,
            label: std::sync::Arc<egui::Galley>,
        }
        let lays: Vec<ChipLay> = card
            .chips
            .iter()
            .map(|c| {
                let colour = if card.letters.is_some() {
                    if c.from {
                        widgets::palette::GOLD_BRIGHT.gamma_multiply(1.0).linear_multiply(1.0)
                    } else if c.held.is_some() {
                        Color32::WHITE
                    } else {
                        hex(0x888888)
                    }
                } else {
                    hex(0xdddddd)
                };
                let colour = if card.letters.is_some() && c.from { widgets::hex(0xffd45c) } else { colour };
                let label = rich::galley(ctx, rich::plain(&c.label, &st.clone().colour(colour).bold(), 0.0));
                let mut caps = Vec::new();
                // a label that already spells its key ("(S)trength") needs no cap beside it
                let spelled = c.label.contains(&format!("({})", c.hotkey));
                if card.pad {
                    if card.yesno {
                        caps.push(format!("({})", yesno_button(&c.hotkey)));
                    }
                } else if !spelled && card.letters.is_none() {
                    caps.push(key_name(&c.hotkey));
                }
                let default = !card.pad && card.default.as_deref() == Some(c.hotkey.as_str());
                if default {
                    caps.push("Enter".into());
                }
                let caps_w: f32 = caps.iter().map(|k| widgets::keycap_size(ctx, k, &cap).x).sum::<f32>() + caps.len() as f32 * 0.5 * chip_font + if default { 0.25 * cap.size } else { 0.0 };
                let (px_l, px_r, py) = if card.letters.is_some() { (0.0, 0.0, 0.3 * chip_font) } else { (0.6 * chip_font, 0.9 * chip_font, 0.35 * chip_font) };
                let inner_h = label.size().y.max(if caps.is_empty() { 0.0 } else { 1.5 * 0.85 * chip_font });
                let w = if card.letters.is_some() { 2.0 * font } else { px_l + caps_w + label.size().x + px_r + 2.0 };
                ChipLay { size: Vec2::new(w, inner_h + 2.0 * py + 2.0), caps, default, label }
            })
            .collect();
        let max_w = (0.94 * screen.w()).min(48.0 * font);
        let text_g = card.text.as_ref().map(|t| rich::galley(ctx, rich::plain(t, &TextStyle::new(ctx, 1.05 * font, Color32::WHITE), max_w - 2.0 * pad_x - 2.0)));
        let lead_g = card.lead.as_ref().filter(|l| !l.is_empty()).map(|l| rich::galley(ctx, rich::plain(l, &TextStyle::new(ctx, 1.05 * font, Color32::WHITE), 0.0)));
        // the chips' rows (flex-wrap, centred)
        let row_gap = 0.6 * font;
        let avail = max_w - 2.0 * pad_x - 2.0;
        let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
        let mut grid_cols = 0;
        if card.letters.is_some() {
            grid_cols = 13;
            for i in 0..lays.len() {
                if i % 13 == 0 && i > 0 {
                    rows.push(Vec::new());
                }
                rows.last_mut().unwrap().push(i);
            }
        } else {
            let mut x = lead_g.as_ref().map_or(0.0, |g| g.size().x + 0.2 * font + gap);
            for (i, l) in lays.iter().enumerate() {
                if x > 0.0 && x + l.size.x > avail {
                    rows.push(Vec::new());
                    x = 0.0;
                }
                rows.last_mut().unwrap().push(i);
                x += l.size.x + gap;
            }
        }
        let cell_gap = if grid_cols > 0 { 0.3 * font } else { gap };
        let row_w = |r: &Vec<usize>, first: bool| -> f32 {
            let w: f32 = r.iter().map(|&i| lays[i].size.x).sum::<f32>() + r.len().saturating_sub(1) as f32 * cell_gap;
            w + if first && grid_cols == 0 { lead_g.as_ref().map_or(0.0, |g| g.size().x + 0.2 * font + gap) } else { 0.0 }
        };
        let row_h = |r: &Vec<usize>| r.iter().map(|&i| lays[i].size.y).fold(0.0, f32::max);
        let chips_w = rows.iter().enumerate().map(|(k, r)| row_w(r, k == 0)).fold(0.0, f32::max);
        let chips_h: f32 = rows.iter().map(row_h).sum::<f32>() + rows.len().saturating_sub(1) as f32 * if grid_cols > 0 { 0.3 * font } else { row_gap };
        let slot_g = (card.letters == Some(Letters::Item)).then(|| {
            let i = self.nav.index().max(0) as usize;
            let c = card.chips.get(i).cloned();
            let mut job = rich::plain(&format!("{}\u{a0}", c.as_ref().map_or(String::new(), |c| c.hotkey.clone())), &TextStyle::new(ctx, font, Color32::WHITE).bold(), 0.0);
            let rest = match c.and_then(|c| c.held) {
                Some(h) => rich::formatted(&h, &TextStyle::new(ctx, font, hex(0xdddddd)), 0.0),
                None => rich::plain("empty", &TextStyle::new(ctx, font, hex(0x777777)), 0.0),
            };
            let off = job.text.len();
            for mut s in rest.sections {
                s.byte_range = s.byte_range.start + off..s.byte_range.end + off;
                job.sections.push(s);
            }
            job.text.push_str(&rest.text);
            rich::galley(ctx, job)
        });
        let text_h = text_g.as_ref().map_or(0.0, |g| g.size().y + 0.8 * font);
        let slot_h = slot_g.as_ref().map_or(0.0, |g| 0.7 * font + g.size().y.max(1.4 * font));
        let content_w = chips_w.max(text_g.as_ref().map_or(0.0, |g| g.size().x)).max(slot_g.as_ref().map_or(0.0, |g| g.size().x));
        let width = (content_w + 2.0 * pad_x + 2.0).clamp(22.0 * font, max_w).ceil();
        let height = (text_h + chips_h + slot_h + pad_t + pad_b + 2.0).ceil();
        let rect = Rect::from_center_size(screen.rect.center(), Vec2::new(width, height));
        let rect = Rect::from_min_size(Pos2::new(rect.min.x.round(), rect.min.y.round()), rect.size());
        let mut clicked = None;
        crate::frame::layer(ctx, egui::Id::unique("isorun-prompt-card"), rect, |ui, _| {
            let p = ui.painter().clone();
            widgets::paint_panel(&p, rect, &PanelLook::prompt_card());
            let left = rect.left() + 1.0 + pad_x;
            let mut y = rect.top() + 1.0 + pad_t;
            if let Some(g) = &text_g {
                p.galley(Pos2::new(left, y), g.clone(), Color32::WHITE);
                y += g.size().y + 0.8 * font;
            }
            let inner_w = width - 2.0 - 2.0 * pad_x;
            for (k, r) in rows.iter().enumerate() {
                let rh = row_h(r);
                let mut x = left + (inner_w - row_w(r, k == 0)) / 2.0;
                if k == 0 && grid_cols == 0 {
                    if let Some(g) = &lead_g {
                        p.galley(Pos2::new(x, y + (rh - g.size().y) / 2.0), g.clone(), Color32::WHITE);
                        x += g.size().x + 0.2 * font + gap;
                    }
                }
                for &i in r {
                    let l = &lays[i];
                    let c = &card.chips[i];
                    let cr = Rect::from_min_size(Pos2::new(x, y + (rh - l.size.y) / 2.0), l.size);
                    let resp = ui.interact(cr, egui::Id::unique(("isorun-chip", i)), egui::Sense::click());
                    let id = if card.letters.is_some() { format!("letter:{}", c.hotkey) } else { format!("chip:{i}") };
                    let focused = lit.as_deref() == Some(id.as_str());
                    let border = if card.letters.is_some() && c.from {
                        rgba(255, 212, 92, 0.6)
                    } else if card.letters.is_some() && c.held.is_some() {
                        hex(0x666666)
                    } else if resp.hovered() {
                        hex(0x777777)
                    } else {
                        hex(0x444444)
                    };
                    if focused {
                        paint_focused(&p, cr);
                    } else {
                        p.rect_stroke(cr, 4.0, egui::Stroke::new(1.0, border), egui::StrokeKind::Inside);
                    }
                    let cy = cr.center().y;
                    let mut cx0 = if card.letters.is_some() { cr.center().x - l.label.size().x / 2.0 } else { cr.left() + 1.0 + 0.6 * chip_font };
                    for (n, k) in l.caps.iter().enumerate() {
                        if n > 0 && l.default && n == l.caps.len() - 1 {
                            cx0 += 0.25 * cap.size;
                        }
                        let s = widgets::keycap_size(ctx, k, &cap);
                        widgets::keycap(&p, Pos2::new(cx0, cy - s.y / 2.0), k, &cap);
                        cx0 += s.x + 0.5 * chip_font;
                    }
                    p.galley(Pos2::new(cx0, cy - l.label.size().y / 2.0), l.label.clone(), Color32::WHITE);
                    if resp.clicked() {
                        clicked = Some(c.hotkey.clone());
                    }
                    x += l.size.x + cell_gap;
                }
                y += rh + if grid_cols > 0 { 0.3 * font } else { row_gap };
            }
            if let Some(g) = &slot_g {
                y += 0.7 * font - if grid_cols > 0 { 0.3 * font } else { row_gap };
                p.galley(Pos2::new(rect.center().x - g.size().x / 2.0, y), g.clone(), Color32::WHITE);
            }
        });
        let _ = TERM_COLOURS;
        if let Some(h) = clicked {
            cx.send(answer(&h));
        }
    }
}
