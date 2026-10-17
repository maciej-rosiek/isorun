//! The tooltip cards (tooltip.ts `tipCard`): a kicker, the title, a sub line, a rule, the body,
//! lines of label and value (better or worse against what is worn), tags, and the keys: keycaps
//! for the keyboard, the pad's glyph for the pad. And the keycaps themselves (`keycaps`), which
//! the dock and the screens wear beside their buttons.

use crate::glyphs::{glyph_size, paint_glyph, Glyph, PadKind};
use crate::paint::{hex, inset, outer, rgba, s, sa, BoxCache, BoxStyle, Layer, RadialSize, C, TRANSPARENT};
use crate::style::{Face, TextStyle, BETTER, BRONZE, BRONZE_BRIGHT, INK_DIM, WORSE};
use egui::{Color32, Painter, Pos2, Rect, Vec2};
use std::sync::Arc;

/// One key the thing is on: keycaps, the pad's button, whether the pad holds it, a label.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct KeyHint {
    pub keys: Vec<String>,
    pub pad: Option<Glyph>,
    pub pad_hold: bool,
    pub label: Option<String>,
}

impl KeyHint {
    pub fn keys(keys: &[&str]) -> KeyHint {
        KeyHint { keys: keys.iter().map(|k| k.to_string()).collect(), ..Default::default() }
    }
    pub fn pad(mut self, g: Glyph) -> Self {
        self.pad = Some(g);
        self
    }
    pub fn hold(mut self) -> Self {
        self.pad_hold = true;
        self
    }
    pub fn label(mut self, l: &str) -> Self {
        self.label = Some(l.into());
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tone {
    Better,
    Worse,
    Same,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TipLine {
    pub label: String,
    pub value: String,
    pub tone: Option<Tone>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TipTag {
    pub text: String,
    pub tone: Option<Tone>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Tip {
    pub title: String,
    pub colour: Option<C>,
    pub kicker: Option<String>,
    pub sub: Option<String>,
    pub body: Option<String>,
    pub lines: Vec<TipLine>,
    pub tags: Vec<TipTag>,
    pub hints: Vec<KeyHint>,
}

impl Tip {
    pub fn titled(title: impl Into<String>) -> Tip {
        Tip { title: title.into(), ..Default::default() }
    }
    pub fn body(mut self, b: impl Into<String>) -> Self {
        self.body = Some(b.into());
        self
    }
    pub fn sub(mut self, b: impl Into<String>) -> Self {
        self.sub = Some(b.into());
        self
    }
    pub fn kicker(mut self, b: impl Into<String>) -> Self {
        self.kicker = Some(b.into());
        self
    }
    pub fn hint(mut self, h: KeyHint) -> Self {
        self.hints.push(h);
        self
    }
}

/// The card's look: the plain one, the dimmer one of what is worn (`.equipped`), the narrow one
/// of an empty place (`.small`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardKind {
    Plain,
    Equipped,
    Small,
}

/// The sizes of a row of keycaps (`.keycap` and its variants beside the slots and the actions).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeycapSize {
    pub h: f32,
    pub min_w: f32,
    pub pad: f32,
    pub font: f32,
    pub wide_font: f32,
}

impl KeycapSize {
    /// `.keycap`: 20 high, 5 either side, 12 (10.5 for a word)
    pub fn base(u: f32) -> KeycapSize {
        KeycapSize { h: 20.0 * u, min_w: 20.0 * u, pad: 5.0 * u, font: 12.0 * u, wide_font: 10.5 * u }
    }
    /// `.slot-key .keycap`
    pub fn slot(u: f32) -> KeycapSize {
        KeycapSize { h: 17.0 * u, min_w: 17.0 * u, pad: 3.0 * u, font: 10.5 * u, wide_font: 10.5 * u }
    }
    /// `.action-key .keycap`
    pub fn action(u: f32) -> KeycapSize {
        KeycapSize { h: 18.0 * u, min_w: 18.0 * u, pad: 4.0 * u, font: 11.0 * u, wide_font: 9.5 * u }
    }
}

fn keycap_galley(ctx: &egui::Context, key: &str, size: KeycapSize) -> Arc<egui::Galley> {
    let wide = key.chars().count() > 1;
    TextStyle::new(Face::MonoBold, if wide { size.wide_font } else { size.font }, hex(0xefe4cc)).galley(ctx, key, None)
}

/// How wide a row of keycaps is.
pub fn keycaps_width(ctx: &egui::Context, keys: &[String], size: KeycapSize, u: f32) -> f32 {
    if keys.is_empty() {
        return 0.0;
    }
    let caps: f32 = keys.iter().map(|k| (keycap_galley(ctx, k, size).size().x + 2.0 * size.pad).max(size.min_w)).sum();
    caps + 3.0 * u * (keys.len() - 1) as f32
}

/// A row of keycaps with its top-left corner at `at`; returns its width.
pub fn paint_keycaps(painter: &Painter, boxes: &mut BoxCache, at: Pos2, keys: &[String], size: KeycapSize, u: f32) -> f32 {
    let ctx = painter.ctx();
    let mut x = at.x;
    for k in keys {
        let g = keycap_galley(ctx, k, size);
        let w = (g.size().x + 2.0 * size.pad).max(size.min_w);
        let rect = Rect::from_min_size(egui::pos2(x, at.y), egui::vec2(w, size.h));
        let style = BoxStyle {
            layers: vec![Layer::Linear(180.0, vec![sa(hex(0x3b342b)), sa(hex(0x1b1814))])],
            radius: [3.0 * u; 4],
            border: crate::paint::Border { w: [1.0, 1.0, 2.0 * u, 1.0], c: [hex(0x5d4f39); 4] },
            outers: vec![outer(0.0, 1.0, 0.0, 0.0, hex(0x000000))],
            ..Default::default()
        };
        boxes.paint_box(painter, "keycap", rect, &style);
        // the text centred in the box above the thicker bottom border
        let inner = Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.max.y - 2.0 * u + 1.0));
        painter.galley(inner.center() - g.size() / 2.0, g, Color32::WHITE);
        x += w + 3.0 * u;
    }
    (x - at.x - 3.0 * u).max(0.0)
}

/// A card laid out: its size, and how to paint it.
pub struct Card {
    pub size: Vec2,
    items: Vec<(Vec2, Item)>,
    kind: CardKind,
    u: f32,
}

enum Item {
    Text(Arc<egui::Galley>, Color32, bool),
    Rule(f32),
    KeysRule(f32),
    Keycaps(Vec<String>),
    Pad(Glyph, f32),
}

fn tone_colour(t: Option<Tone>, plain: C) -> Color32 {
    match t {
        Some(Tone::Better) => BETTER.to_color32(),
        Some(Tone::Worse) => WORSE.to_color32(),
        _ => plain.to_color32(),
    }
}

/// Lay a card out (tooltip.ts `tipCard`). `pad` is the pad in use, or None for the keyboard
/// and the mouse: what is `kbd-only` and `pad-only` shows by it.
pub fn layout_card(ctx: &egui::Context, tip: &Tip, kind: CardKind, u: f32, pad: Option<PadKind>) -> Card {
    let min_w = if kind == CardKind::Small { 0.0 } else { 200.0 * u };
    let max_w = 360.0 * u;
    let (px, py) = (16.0 * u, 12.0 * u);
    let body_size = 14.0 * u;
    let kicker_st = TextStyle::new(Face::Caps700, 11.0 * u, BRONZE).spacing(0.16).upper().line(1.0);
    let title_st = TextStyle::new(Face::Caps700, 19.0 * u, tip.colour.unwrap_or(BRONZE_BRIGHT)).line(1.2);
    let sub_st = TextStyle::new(Face::Caps600, 12.0 * u, hex(0x9a8c72)).spacing(0.06).line(1.3);
    let body_st = TextStyle::new(Face::Mono, body_size, hex(0xc9bea6)).line(1.4);
    let k_st = TextStyle::new(Face::Mono, body_size, hex(0x9a8c72)).line(1.4);
    let label_st = TextStyle::new(Face::Caps600, 12.0 * u, hex(0xc9bea6)).spacing(0.05).line(1.0);
    let hold_st = TextStyle::new(Face::Caps600, 10.0 * u, hex(0x9a8c72)).spacing(0.08).line(1.0);
    let inner_max = max_w - 2.0 * px;
    let kc = KeycapSize::base(u);

    // the hints that show on this device: a hint with no keycaps is the pad's only, one with no
    // pad the keyboard's only
    let hints: Vec<&KeyHint> = tip.hints.iter().filter(|h| if pad.is_some() { h.pad.is_some() } else { !h.keys.is_empty() }).collect();
    let hint_w = |h: &KeyHint| -> f32 {
        let mut parts = vec![];
        if pad.is_none() && !h.keys.is_empty() {
            parts.push(keycaps_width(ctx, &h.keys, kc, u));
        }
        if let (Some(_), Some(g)) = (pad, h.pad) {
            let mut w = glyph_size(g, 19.0 * u).x;
            if h.pad_hold {
                w += 3.0 * u + hold_st.galley(ctx, "hold", None).size().x;
            }
            parts.push(w);
        }
        if let Some(l) = &h.label {
            parts.push(label_st.galley(ctx, l, None).size().x);
        }
        parts.iter().sum::<f32>() + 6.0 * u * (parts.len().max(1) - 1) as f32
    };

    // the natural width: the widest thing on one line
    let mut natural: f32 = 0.0;
    if let Some(k) = &tip.kicker {
        natural = natural.max(kicker_st.galley(ctx, k, None).size().x);
    }
    natural = natural.max(title_st.galley(ctx, &tip.title, None).size().x);
    if let Some(sub) = &tip.sub {
        natural = natural.max(sub_st.galley(ctx, sub, None).size().x);
    }
    if let Some(b) = &tip.body {
        natural = natural.max(body_st.galley(ctx, b, None).size().x);
    }
    for l in &tip.lines {
        let v = TextStyle::new(Face::MonoBold, body_size, hex(0xefe4cc)).galley(ctx, &l.value, None).size().x;
        natural = natural.max(k_st.galley(ctx, &l.label, None).size().x + 18.0 * u + v);
    }
    if !tip.tags.is_empty() {
        let w: f32 = tip.tags.iter().map(|t| body_st.galley(ctx, &t.text, None).size().x).sum::<f32>() + 10.0 * u * (tip.tags.len() - 1) as f32;
        natural = natural.max(w);
    }
    if !hints.is_empty() {
        let w: f32 = hints.iter().map(|h| hint_w(h)).sum::<f32>() + 14.0 * u * (hints.len() - 1) as f32;
        natural = natural.max(w);
    }
    let inner_w = natural.min(inner_max).max(min_w - 2.0 * px).ceil();
    let width = inner_w + 2.0 * px;

    let mut items = Vec::new();
    let mut y = py;
    let text = |items: &mut Vec<(Vec2, Item)>, st: &TextStyle, t: &str, x: f32, y: &mut f32, wrap: Option<f32>, shadow: bool| {
        let g = st.galley(ctx, t, wrap);
        let h = g.size().y;
        let w = g.size().x;
        items.push((egui::vec2(x, *y), Item::Text(g, st.colour, shadow)));
        *y += h;
        w
    };
    if let Some(k) = &tip.kicker {
        text(&mut items, &kicker_st, k, px, &mut y, None, false);
        y += 3.0 * u;
    }
    text(&mut items, &title_st, &tip.title, px, &mut y, Some(inner_w), true);
    if let Some(sub) = &tip.sub {
        y += 2.0 * u;
        text(&mut items, &sub_st, sub, px, &mut y, Some(inner_w), false);
    }
    let sections = tip.body.is_some() || !tip.lines.is_empty() || !tip.tags.is_empty();
    if sections {
        y += 9.0 * u;
        items.push((egui::vec2(0.0, y), Item::Rule(width)));
        y += 1.0 + 9.0 * u;
    }
    if let Some(b) = &tip.body {
        text(&mut items, &body_st, b, px, &mut y, Some(inner_w), false);
    }
    if !tip.lines.is_empty() {
        if tip.body.is_some() {
            y += 8.0 * u;
        }
        for (i, l) in tip.lines.iter().enumerate() {
            if i > 0 {
                y += 3.0 * u;
            }
            let v_st = TextStyle::new(Face::MonoBold, body_size, if l.tone.is_some() && l.tone != Some(Tone::Same) { C::from_color32(tone_colour(l.tone, hex(0xefe4cc))) } else { hex(0xefe4cc) }).line(1.4);
            let vg = v_st.galley(ctx, &l.value, None);
            items.push((egui::vec2(px + inner_w - vg.size().x, y), Item::Text(vg, v_st.colour, false)));
            text(&mut items, &k_st, &l.label, px, &mut y, None, false);
        }
    }
    if !tip.tags.is_empty() {
        if tip.body.is_some() || !tip.lines.is_empty() {
            y += 8.0 * u;
        }
        let mut x = 0.0;
        let row_h = body_size * 1.4;
        for t in &tip.tags {
            let st = TextStyle::new(Face::Mono, body_size, match t.tone {
                Some(Tone::Better) => BETTER,
                Some(Tone::Worse) => WORSE,
                _ => hex(0x8fb3ff),
            })
            .line(1.4);
            let g = st.galley(ctx, &t.text, None);
            if x > 0.0 && x + g.size().x > inner_w {
                x = 0.0;
                y += row_h + 4.0 * u;
            }
            let w = g.size().x;
            items.push((egui::vec2(px + x, y), Item::Text(g, st.colour, false)));
            x += w + 10.0 * u;
        }
        y += row_h;
    }
    if !hints.is_empty() {
        y += 10.0 * u;
        items.push((egui::vec2(px, y), Item::KeysRule(inner_w)));
        y += 1.0 + 8.0 * u;
        let row_h = (19.0 * u).max(kc.h);
        let mut x = 0.0;
        for h in hints {
            let w = hint_w(h);
            if x > 0.0 && x + w > inner_w {
                x = 0.0;
                y += row_h + 6.0 * u;
            }
            let mut hx = px + x;
            let mid = y + row_h / 2.0;
            if pad.is_none() && !h.keys.is_empty() {
                items.push((egui::vec2(hx, mid - kc.h / 2.0), Item::Keycaps(h.keys.clone())));
                hx += keycaps_width(ctx, &h.keys, kc, u) + 6.0 * u;
            }
            if let (Some(_), Some(g)) = (pad, h.pad) {
                let gs = glyph_size(g, 19.0 * u);
                items.push((egui::vec2(hx, mid - gs.y / 2.0), Item::Pad(g, 19.0 * u)));
                hx += gs.x;
                if h.pad_hold {
                    hx += 3.0 * u;
                    let g = hold_st.galley(ctx, "hold", None);
                    let gw = g.size().x;
                    items.push((egui::vec2(hx, mid - g.size().y / 2.0), Item::Text(g, hold_st.colour, false)));
                    hx += gw;
                }
                hx += 6.0 * u;
            }
            if let Some(l) = &h.label {
                let g = label_st.galley(ctx, l, None);
                items.push((egui::vec2(hx, mid - g.size().y / 2.0), Item::Text(g, label_st.colour, false)));
            }
            x += w + 14.0 * u;
        }
        y += row_h;
    }
    y += py;
    let _ = INK_DIM;
    Card { size: egui::vec2(width, y.ceil()), items, kind, u }
}

/// `.tip-card`'s box
fn card_style(kind: CardKind, u: f32) -> BoxStyle {
    let mut st = BoxStyle {
        layers: vec![
            Layer::Radial(RadialSize::EllipseFarthest, (0.5, 0.0), vec![sa(rgba(150, 110, 50, 0.14)), s(TRANSPARENT, 0.65)]),
            Layer::Linear(180.0, vec![sa(hex(0x1b1714)), sa(hex(0x0b0908))]),
        ],
        radius: [3.0 * u; 4],
        border: crate::paint::Border::all(1.0, hex(0x5b4728)),
        insets: vec![inset(0.0, 0.0, 0.0, 1.0, hex(0x000000)), inset(0.0, 0.0, 0.0, 3.0 * u, rgba(184, 145, 63, 0.08))],
        outers: vec![outer(0.0, 8.0 * u, 28.0 * u, 0.0, rgba(0, 0, 0, 0.85))],
        ..Default::default()
    };
    if kind == CardKind::Equipped {
        st.border = crate::paint::Border::all(1.0, hex(0x3d3324));
    }
    st
}

impl Card {
    pub fn paint(&self, painter: &Painter, boxes: &mut BoxCache, at: Pos2, pad: PadKind) {
        let u = self.u;
        let rect = Rect::from_min_size(at, self.size);
        let key = if self.kind == CardKind::Equipped { "tip-card-equipped" } else { "tip-card" };
        boxes.paint_box(painter, key, rect, &card_style(self.kind, u));
        // `.equipped` is the same card a little darker (filter: brightness(0.92))
        let dim = if self.kind == CardKind::Equipped { 0.92 } else { 1.0 };
        for (off, item) in &self.items {
            let p = at + *off;
            match item {
                Item::Text(g, c, shadow) => {
                    let c = c.gamma_multiply(dim);
                    if *shadow {
                        crate::paint::shadowed_galley(painter, p, g.clone(), c, &[(0.0, 1.0, 2.0, Color32::BLACK)]);
                    } else {
                        painter.galley_with_override_text_color(p, g.clone(), c);
                    }
                }
                Item::Rule(w) => {
                    let r = Rect::from_min_size(p, egui::vec2(*w - 2.0 * 16.0 * u, 1.0)).translate(egui::vec2(16.0 * u, 0.0));
                    crate::paint::gradient_rect(painter, r, false, &[(0.0, TRANSPARENT), (0.2, hex(0x6b5530)), (0.8, hex(0x6b5530)), (1.0, TRANSPARENT)]);
                }
                Item::KeysRule(w) => {
                    painter.rect_filled(Rect::from_min_size(p, egui::vec2(*w, 1.0)), 0.0, hex(0x2c2419).to_color32());
                }
                Item::Keycaps(keys) => {
                    paint_keycaps(painter, boxes, p, keys, KeycapSize::base(u), u);
                }
                Item::Pad(g, em) => paint_glyph(painter, *g, pad, p, *em, Color32::WHITE),
            }
        }
    }
}
