//! The web build's shared look, painted with egui: the panel frame of its popups (border,
//! insets, drop shadow, gradients), the key caps (`<kbd>`), the card with a title over a rule, and
//! its buttons. Every size here is in CSS px, which are egui points; the callers scale them.
//!
//! CSS gradients have no egui primitive: a gradient is a small texture made once per gradient
//! (cached on the context) and painted into a rounded rect, so the corners clip it as CSS does.

use egui::epaint::{CornerRadius, RectShape, Shadow, StrokeKind};
use egui::{Color32, FontFamily, FontId, Painter, Pos2, Rect, Stroke, TextureId, Vec2};
use std::sync::Arc;

/// `rgba(r, g, b, a)` as CSS writes it (alpha 0..1, unmultiplied).
pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0).round() as u8)
}

/// `#rrggbb`.
pub const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// The dock's palette (dock.css `:root`).
pub mod palette {
    use super::hex;
    use egui::Color32;
    pub const TRIM: Color32 = hex(0xb8913f);
    pub const TRIM_DARK: Color32 = hex(0x4a3815);
    pub const IRON_DEEP: Color32 = hex(0x080707);
    pub const IRON: Color32 = hex(0x121110);
    pub const IRON_RAISED: Color32 = hex(0x1a1816);
    pub const IRON_LINE: Color32 = hex(0x2c2824);
    pub const BRONZE: Color32 = hex(0x9c7a45);
    pub const BRONZE_DIM: Color32 = hex(0x4d3d24);
    pub const BRONZE_BRIGHT: Color32 = hex(0xe8c98a);
    /// the rules inside a dock popup (`#3a2d16`)
    pub const RULE: Color32 = hex(0x3a2d16);
    pub const GOLD: Color32 = hex(0xd8b45f);
    pub const GOLD_BRIGHT: Color32 = hex(0xf3d98a);
    pub const INK: Color32 = hex(0xe9e2d0);
    pub const INK_DIM: Color32 = hex(0x9d9483);
}

// ---------------------------------------------------------------------------------------- text

/// The weight of every word that is not bold on purpose (styles.css `--text-weight`): a screen at
/// a device pixel or less per CSS px draws a regular stroke as a single grey pixel, so the text
/// goes bold there; a high-density screen draws the regular weight crisp.
pub fn text_family(ctx: &egui::Context) -> FontFamily {
    if ctx.pixels_per_point() <= 1.5 {
        crate::fonts::bold()
    } else {
        FontFamily::Monospace
    }
}

/// DejaVu Sans Mono at `size` CSS px, in the page's weight.
pub fn mono(ctx: &egui::Context, size: f32) -> FontId {
    FontId::new(size, text_family(ctx))
}

/// DejaVu Sans Mono Bold, whatever the screen.
pub fn mono_bold(size: f32) -> FontId {
    FontId::new(size, crate::fonts::bold())
}

/// The line box of `line-height: normal` for DejaVu Sans Mono, as Chrome makes it: the font's
/// ascent and descent (1901 and 483 of 2048) each rounded to whole pixels.
pub fn normal_line_height(size: f32) -> f32 {
    (size * 1901.0 / 2048.0).round() + (size * 483.0 / 2048.0).round()
}

/// The advance of one DejaVu Sans Mono character (`1ch`): 1233 of 2048 units.
pub fn ch(size: f32) -> f32 {
    size * 1233.0 / 2048.0
}

/// Text in a font that has no bold face of its own (Cinzel, Grenze), drawn bold as the browser
/// does it (`font-weight: 700` on a regular face): the glyphs a fraction of a pixel wider.
pub fn galley_bold(painter: &Painter, pos: Pos2, galley: Arc<egui::Galley>, colour: Color32) {
    painter.galley_with_override_text_color(pos + Vec2::new(0.6, 0.0), galley.clone(), colour);
    painter.galley_with_override_text_color(pos, galley, colour);
}

// ----------------------------------------------------------------------------------- gradients

/// A gradient stop: where (0..1) and the colour there.
pub type Stop = (f32, Color32);

fn lerp_colour(stops: &[Stop], t: f32) -> [f32; 4] {
    let un = |c: Color32| {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        [r as f32, g as f32, b as f32, a as f32]
    };
    if stops.is_empty() {
        return [0.0; 4];
    }
    if t <= stops[0].0 {
        return un(stops[0].1);
    }
    for w in stops.windows(2) {
        let (a, b) = (w[0], w[1]);
        if t <= b.0 {
            let k = if b.0 > a.0 { (t - a.0) / (b.0 - a.0) } else { 1.0 };
            let (ca, cb) = (un(a.1), un(b.1));
            // CSS interpolates in premultiplied space: a stop fading to transparent keeps its hue
            let pa = [ca[0] * ca[3], ca[1] * ca[3], ca[2] * ca[3], ca[3]];
            let pb = [cb[0] * cb[3], cb[1] * cb[3], cb[2] * cb[3], cb[3]];
            let p: Vec<f32> = (0..4).map(|i| pa[i] + (pb[i] - pa[i]) * k).collect();
            let alpha = p[3];
            return if alpha > 0.0 { [p[0] / alpha, p[1] / alpha, p[2] / alpha, alpha] } else { [0.0; 4] };
        }
    }
    un(stops.last().unwrap().1)
}

fn cached_texture(ctx: &egui::Context, key: String, make: impl FnOnce() -> egui::ColorImage) -> TextureId {
    let id = egui::Id::unique(("isorun-gradient", key.clone()));
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return t.id();
    }
    let tex = ctx.load_texture(format!("gradient:{key}"), make(), egui::TextureOptions::LINEAR);
    let tid = tex.id();
    ctx.data_mut(|d| d.insert_temp(id, tex));
    tid
}

fn image(w: usize, h: usize, px: impl Fn(usize, usize) -> [f32; 4]) -> egui::ColorImage {
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let c = px(x, y);
            rgba.extend(c.iter().map(|v| v.round().clamp(0.0, 255.0) as u8));
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba)
}

/// A gradient across a box, `angle` in CSS degrees (180: top to bottom, 90: left to right).
pub fn linear_gradient(ctx: &egui::Context, angle: u32, stops: &[Stop]) -> TextureId {
    let key = format!("lin{angle}{stops:?}");
    let stops = stops.to_vec();
    cached_texture(ctx, key, move || {
        let n = 128;
        match angle % 360 {
            90 => image(n, 1, |x, _| lerp_colour(&stops, (x as f32 + 0.5) / n as f32)),
            270 => image(n, 1, |x, _| lerp_colour(&stops, 1.0 - (x as f32 + 0.5) / n as f32)),
            0 => image(1, n, |_, y| lerp_colour(&stops, 1.0 - (y as f32 + 0.5) / n as f32)),
            _ => image(1, n, |_, y| lerp_colour(&stops, (y as f32 + 0.5) / n as f32)),
        }
    })
}

/// `radial-gradient(ellipse at cx cy, ...)` sized farthest-corner, as a texture laid over the box:
/// `aspect` is the box's width over its height (the ellipse follows the box).
pub fn radial_gradient(ctx: &egui::Context, centre: (f32, f32), aspect: f32, stops: &[Stop]) -> TextureId {
    let aspect = (aspect * 8.0).round() / 8.0;
    let key = format!("rad{centre:?}{aspect}{stops:?}");
    let stops = stops.to_vec();
    cached_texture(ctx, key, move || {
        let (w, h) = (64usize, 64usize);
        // farthest-corner for an ellipse: the closest-side ellipse's ratio, scaled to reach the far corner
        let (cx, cy) = centre;
        let fx = cx.max(1.0 - cx) * aspect;
        let fy = cy.max(1.0 - cy);
        let ratio = fx / fy.max(1e-3);
        let ry = (fy * fy + (fx / ratio).powi(2)).sqrt();
        let rx = ry * ratio;
        image(w, h, |x, y| {
            let u = ((x as f32 + 0.5) / w as f32 - cx) * aspect;
            let v = (y as f32 + 0.5) / h as f32 - cy;
            let d = ((u / rx).powi(2) + (v / ry).powi(2)).sqrt();
            lerp_colour(&stops, d)
        })
    })
}

/// Paint a texture into a rounded rect (the gradient fills of [`linear_gradient`] and [`radial_gradient`]).
pub fn paint_texture(painter: &Painter, rect: Rect, radius: f32, tex: TextureId) {
    painter.add(
        RectShape::filled(rect, CornerRadius::same(radius.round() as u8), Color32::WHITE)
            .with_texture(tex, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))),
    );
}

/// A horizontal gradient as a mesh of vertex colours: for thin things (a rule a pixel or two
/// tall), which a textured rect does not fill.
pub fn hgradient(painter: &Painter, rect: Rect, stops: &[Stop]) {
    let mut mesh = egui::Mesh::default();
    let mut ts: Vec<f32> = stops.iter().map(|s| s.0).collect();
    for i in 0..=16 {
        ts.push(i as f32 / 16.0);
    }
    ts.sort_by(f32::total_cmp);
    ts.dedup();
    for (k, &t) in ts.iter().enumerate() {
        let c = lerp_colour(stops, t);
        let colour = Color32::from_rgba_unmultiplied(c[0] as u8, c[1] as u8, c[2] as u8, c[3] as u8);
        let x = rect.left() + rect.width() * t;
        mesh.colored_vertex(Pos2::new(x, rect.top()), colour);
        mesh.colored_vertex(Pos2::new(x, rect.bottom()), colour);
        if k > 0 {
            let b = 2 * k as u32;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b, b + 1);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

// --------------------------------------------------------------------------------------- panels

/// One layer of a panel's background.
#[derive(Clone, Debug)]
pub enum Fill {
    Solid(Color32),
    /// CSS angle and stops
    Linear(u32, Vec<Stop>),
    /// centre (0..1 of the box) and stops
    Radial((f32, f32), Vec<Stop>),
}

/// A CSS box's look: background layers (first on top, as CSS lists them), its border, the inset
/// rings (`box-shadow: inset 0 0 0 Npx colour`) and the drop shadow.
#[derive(Clone, Debug)]
pub struct PanelLook {
    pub fills: Vec<Fill>,
    pub border: Stroke,
    pub radius: f32,
    /// inset rings, from the border inwards: width and colour, each drawn from the padding edge
    pub insets: Vec<(f32, Color32)>,
    /// `box-shadow: 0 <y> <blur> <colour>`
    pub shadow: Option<(f32, f32, Color32)>,
}

impl PanelLook {
    /// `.hud-dock .popup` (dock.css): crawl's popups in the dock's dark stone with a gold trim.
    pub fn dock_popup() -> Self {
        PanelLook {
            fills: vec![
                Fill::Radial((0.5, 0.0), vec![(0.0, rgba(120, 90, 40, 0.14)), (0.6, rgba(120, 90, 40, 0.0))]),
                Fill::Linear(180, vec![(0.0, rgba(27, 22, 18, 0.97)), (1.0, rgba(12, 10, 8, 0.97))]),
            ],
            border: Stroke::new(1.0, palette::TRIM),
            radius: 6.0,
            insets: vec![(1.0, Color32::BLACK), (3.0, rgba(184, 145, 63, 0.15))],
            shadow: Some((12.0, 48.0, rgba(0, 0, 0, 0.85))),
        }
    }

    /// `.popup` (styles.css): the plain look without the dock.
    pub fn popup() -> Self {
        PanelLook {
            fills: vec![Fill::Solid(rgba(10, 10, 14, 0.96))],
            border: Stroke::new(1.0, hex(0x555555)),
            radius: 6.0,
            insets: vec![],
            shadow: Some((8.0, 40.0, rgba(0, 0, 0, 0.7))),
        }
    }

    /// `.prompt-card`: the yes/no and choice card in the middle of the view.
    pub fn prompt_card() -> Self {
        PanelLook {
            fills: vec![Fill::Solid(rgba(10, 10, 14, 0.94))],
            border: Stroke::new(1.0, hex(0x555555)),
            radius: 8.0,
            insets: vec![],
            shadow: Some((8.0, 40.0, rgba(0, 0, 0, 0.7))),
        }
    }

    /// `.popup.newgame`: character creation's parchment-dark frame.
    pub fn newgame(u: f32) -> Self {
        PanelLook {
            fills: vec![
                Fill::Radial((0.5, 0.0), vec![(0.0, rgba(120, 90, 40, 0.18)), (0.6, rgba(120, 90, 40, 0.0))]),
                Fill::Linear(180, vec![(0.0, hex(0x1b1612)), (1.0, hex(0x0c0a08))]),
            ],
            border: Stroke::new(1.0, palette::TRIM),
            radius: 6.0 * u,
            insets: vec![(1.0, Color32::BLACK), (4.0 * u, rgba(184, 145, 63, 0.14))],
            shadow: Some((12.0, 60.0, rgba(0, 0, 0, 0.9))),
        }
    }
}

/// Paint a panel into `rect` (its border box).
pub fn paint_panel(painter: &Painter, rect: Rect, look: &PanelLook) {
    let cr = CornerRadius::same(look.radius.round() as u8);
    if let Some((dy, blur, colour)) = look.shadow {
        let shadow = Shadow { offset: [0, dy.round() as i8], blur: blur.min(255.0) as u8, spread: 0, color: colour };
        painter.add(shadow.as_shape(rect, cr));
    }
    let ctx = painter.ctx();
    for fill in look.fills.iter().rev() {
        match fill {
            Fill::Solid(c) => {
                painter.rect_filled(rect, cr, *c);
            }
            Fill::Linear(angle, stops) => paint_texture(painter, rect, look.radius, linear_gradient(ctx, *angle, stops)),
            Fill::Radial(centre, stops) => {
                paint_texture(painter, rect, look.radius, radial_gradient(ctx, *centre, rect.width() / rect.height().max(1.0), stops))
            }
        }
    }
    let inner = rect.shrink(look.border.width);
    for &(w, c) in &look.insets {
        let r = (look.radius - look.border.width).max(0.0);
        painter.add(RectShape::stroke(inner, CornerRadius::same(r.round() as u8), Stroke::new(w, c), StrokeKind::Inside));
    }
    if look.border.width > 0.0 {
        painter.rect_stroke(rect, cr, look.border, StrokeKind::Inside);
    }
}

/// A horizontal rule (a `border-bottom: 1px solid`), `y` its top.
pub fn rule(painter: &Painter, x: std::ops::RangeInclusive<f32>, y: f32, colour: Color32) {
    painter.rect_filled(Rect::from_x_y_ranges(x, y..=y + 1.0), 0.0, colour);
}

/// `outline: 1px solid` around a box, as the browser draws it (outside the border box).
pub fn outline(painter: &Painter, rect: Rect, radius: f32, colour: Color32) {
    painter.rect_stroke(rect, CornerRadius::same(radius.round() as u8), Stroke::new(1.0, colour), StrokeKind::Outside);
}

// ------------------------------------------------------------------------------------- key caps

/// How a key cap looks: the prompt card's (`.prompt-card .chip kbd`), the level-up banner's
/// (bigger, a 3px foot), or a menu footer's (`.popup .more kbd`, a plain outline).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeycapLook {
    /// the label's font size
    pub size: f32,
    /// min width and height, in em of `size`
    pub min_em: f32,
    pub pad_x: f32,
    pub bg: Color32,
    pub fg: Color32,
    pub border: Color32,
    pub foot: f32,
    pub radius: f32,
}

impl KeycapLook {
    /// `.prompt-card .chip kbd`: 0.85em bold white on #15171c, a 2px foot. `size` is the chip's font size.
    pub fn chip(size: f32) -> Self {
        KeycapLook {
            size: size * 0.85,
            min_em: 1.5,
            pad_x: 0.3 * size * 0.85,
            bg: hex(0x15171c),
            fg: Color32::WHITE,
            border: rgba(255, 255, 255, 0.35),
            foot: 2.0,
            radius: 3.0,
        }
    }
    /// `.level-up kbd`: 0.85rem, 1.7em square, a 3px foot.
    pub fn level_up(size: f32) -> Self {
        KeycapLook { size, min_em: 1.7, pad_x: 0.35 * size, bg: hex(0x15171c), fg: Color32::WHITE, border: rgba(255, 255, 255, 0.35), foot: 3.0, radius: 4.0 }
    }
    /// `.popup .more kbd`: the key named in a footer line, outlined in the line's own colour.
    pub fn footer(size: f32) -> Self {
        KeycapLook {
            size: size * 0.9,
            min_em: 0.0,
            pad_x: 0.3 * size * 0.9,
            bg: Color32::TRANSPARENT,
            fg: hex(0xdddddd),
            border: hex(0x555555),
            foot: 1.0,
            radius: 3.0,
        }
    }
}

/// The width a key cap takes for `text`.
pub fn keycap_size(ctx: &egui::Context, text: &str, look: &KeycapLook) -> Vec2 {
    let galley = ctx.fonts_mut(|f| f.layout_no_wrap(text.to_string(), mono_bold(look.size), look.fg));
    let h = (look.min_em * look.size).max(galley.size().y);
    Vec2::new((galley.size().x + 2.0 * look.pad_x).max(look.min_em * look.size), h)
}

/// Paint a key cap with its top-left at `at`; returns its rect.
pub fn keycap(painter: &Painter, at: Pos2, text: &str, look: &KeycapLook) -> Rect {
    let ctx = painter.ctx();
    let galley = ctx.fonts_mut(|f| f.layout_no_wrap(text.to_string(), mono_bold(look.size), look.fg));
    let size = keycap_size(ctx, text, look);
    let rect = Rect::from_min_size(at, size);
    let cr = CornerRadius::same(look.radius as u8);
    painter.rect_filled(rect, cr, look.bg);
    // the border, its bottom edge thicker: the cap's foot
    painter.rect_stroke(rect, cr, Stroke::new(1.0, look.border), StrokeKind::Inside);
    if look.foot > 1.0 {
        let foot = Rect::from_min_max(
            Pos2::new(rect.left() + look.radius * 0.5, rect.bottom() - look.foot),
            Pos2::new(rect.right() - look.radius * 0.5, rect.bottom()),
        );
        painter.rect_filled(foot, 0.0, look.border);
    }
    let text_pos = Pos2::new(rect.center().x - galley.size().x / 2.0, rect.top() + (rect.height() - look.foot + 1.0 - galley.size().y) / 2.0);
    painter.galley(text_pos, galley, look.fg);
    rect
}

/// Several caps and words in a row (`[Enter] SELECT [Esc] BACK`): each part a cap or a label.
pub enum KeyHint<'a> {
    Cap(&'a str),
    Text(&'a str, FontId, Color32),
    Gap(f32),
}

/// Paint a row of key hints starting at `at` (vertically centred on `at.y + height/2`); returns its rect.
pub fn key_hints(painter: &Painter, at: Pos2, parts: &[KeyHint], look: &KeycapLook) -> Rect {
    let ctx = painter.ctx().clone();
    let height = parts
        .iter()
        .map(|p| match p {
            KeyHint::Cap(t) => keycap_size(&ctx, t, look).y,
            KeyHint::Text(_, f, _) => f.size * 1.2,
            KeyHint::Gap(_) => 0.0,
        })
        .fold(0.0, f32::max);
    let mut x = at.x;
    for p in parts {
        match p {
            KeyHint::Cap(t) => {
                let s = keycap_size(&ctx, t, look);
                keycap(painter, Pos2::new(x, at.y + (height - s.y) / 2.0), t, look);
                x += s.x;
            }
            KeyHint::Text(t, f, c) => {
                let g = ctx.fonts_mut(|fo| fo.layout_no_wrap(t.to_string(), f.clone(), *c));
                painter.galley(Pos2::new(x, at.y + (height - g.size().y) / 2.0), g.clone(), *c);
                x += g.size().x;
            }
            KeyHint::Gap(g) => x += g,
        }
    }
    Rect::from_min_max(at, Pos2::new(x, at.y + height))
}

// ----------------------------------------------------------------------------------------- cards

/// A card: a title over a rule, then whatever the caller draws in `body` (the tooltip cards, the
/// level-up's stat cards). Returns the rect left for the body.
pub fn card(painter: &Painter, rect: Rect, title: &str, title_font: FontId, title_colour: Color32, look: &PanelLook, pad: f32) -> Rect {
    paint_panel(painter, rect, look);
    let ctx = painter.ctx();
    let galley = ctx.fonts_mut(|f| f.layout(title.to_string(), title_font, title_colour, rect.width() - 2.0 * pad));
    let top = rect.top() + pad;
    let h = galley.size().y;
    painter.galley(Pos2::new(rect.left() + pad, top), galley, title_colour);
    let y = top + h + pad * 0.5;
    // a rule that fades out at both ends (the tooltip cards' and the level-up banner's)
    hgradient(
        painter,
        Rect::from_min_max(Pos2::new(rect.left() + pad, y), Pos2::new(rect.right() - pad, y + 1.0)),
        &[(0.0, Color32::TRANSPARENT), (0.3, palette::GOLD), (0.7, palette::GOLD), (1.0, Color32::TRANSPARENT)],
    );
    Rect::from_min_max(Pos2::new(rect.left() + pad, y + pad * 0.5), Pos2::new(rect.right() - pad, rect.bottom() - pad))
}

// --------------------------------------------------------------------------------------- buttons

/// The looks of the web's buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonLook {
    /// `.dialog-html button`: a quiet glass button
    Plain,
    /// `.touch-bar button`: dark bronze, Cinzel caps
    Bronze,
    /// `.touch-bar button.primary`
    BronzePrimary,
    /// `.touch-close`: the round close button of a popup on a touch screen
    RoundClose,
}

/// A button: painted, and answered with egui's response (hover and click).
pub fn button(ui: &mut egui::Ui, rect: Rect, id: egui::Id, label: &str, look: ButtonLook) -> egui::Response {
    let resp = ui.interact(rect, id, egui::Sense::click());
    let painter = ui.painter();
    let ctx = ui.ctx().clone();
    let hovered = resp.hovered();
    match look {
        ButtonLook::Plain => {
            painter.rect_filled(rect, 4.0, rgba(255, 255, 255, if hovered { 0.14 } else { 0.08 }));
            painter.rect_stroke(rect, 4.0, Stroke::new(1.0, rgba(255, 255, 255, 0.25)), StrokeKind::Inside);
            let g = ctx.fonts_mut(|f| f.layout_no_wrap(label.to_string(), mono(&ctx, 15.0), hex(0xdddddd)));
            painter.galley(rect.center() - g.size() / 2.0, g, hex(0xdddddd));
        }
        ButtonLook::Bronze | ButtonLook::BronzePrimary => {
            let primary = look == ButtonLook::BronzePrimary;
            let shadow = Shadow { offset: [0, 3], blur: 10, spread: 0, color: rgba(0, 0, 0, 0.6) };
            painter.add(shadow.as_shape(rect, CornerRadius::same(6)));
            let stops = if primary {
                vec![(0.0, rgba(74, 58, 30, 0.96)), (1.0, rgba(24, 18, 10, 0.96))]
            } else {
                vec![(0.0, rgba(42, 36, 29, 0.95)), (1.0, rgba(14, 12, 9, 0.95))]
            };
            paint_texture(painter, rect, 6.0, linear_gradient(&ctx, 180, &stops));
            painter.rect_stroke(rect, 6.0, Stroke::new(1.0, if primary { palette::BRONZE } else { palette::BRONZE_DIM }), StrokeKind::Inside);
            let colour = if primary { palette::BRONZE_BRIGHT } else { palette::INK };
            let g = ctx.fonts_mut(|f| f.layout_no_wrap(label.to_string(), FontId::new(13.0, crate::fonts::caps()), colour));
            painter.galley(rect.center() - g.size() / 2.0, g, colour);
        }
        ButtonLook::RoundClose => {
            let shadow = Shadow { offset: [0, 2], blur: 8, spread: 0, color: rgba(0, 0, 0, 0.6) };
            let r = (rect.width() / 2.0).round() as u8;
            painter.add(shadow.as_shape(rect, CornerRadius::same(r)));
            let tex = radial_gradient(&ctx, (0.5, 0.3), 1.0, &[(0.0, rgba(40, 36, 31, 0.92)), (0.8, rgba(10, 9, 8, 0.92))]);
            paint_texture(painter, rect, rect.width() / 2.0, tex);
            painter.circle_stroke(rect.center(), rect.width() / 2.0 - 0.5, Stroke::new(1.0, palette::BRONZE_DIM));
            let g = ctx.fonts_mut(|f| f.layout_no_wrap(label.to_string(), mono_bold(16.0), palette::BRONZE_BRIGHT));
            painter.galley(rect.center() - g.size() / 2.0, g, palette::BRONZE_BRIGHT);
        }
    }
    resp
}

/// Keep `Arc` in the public surface for callers building galleys once.
pub type Galley = Arc<egui::Galley>;
