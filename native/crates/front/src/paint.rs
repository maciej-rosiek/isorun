//! Painting the front end's CSS in egui: gradients as meshes of vertex colours, text laid out
//! with the web build's letter spacing and line heights, the colours of `styles.css :root`.

use egui::{Color32, FontFamily, FontId, Pos2, Rect};
use std::sync::Arc;

pub const GOLD: Color32 = Color32::from_rgb(0xd8, 0xb4, 0x5f);
pub const GOLD_BRIGHT: Color32 = Color32::from_rgb(0xf3, 0xd9, 0x8a);
pub const INK: Color32 = Color32::from_rgb(0xe9, 0xe2, 0xd0);
pub const INK_DIM: Color32 = Color32::from_rgb(0x9d, 0x94, 0x83);
pub const ACCENT: Color32 = Color32::from_rgb(0xff, 0xd4, 0x5c);
/// The terminal's colours the menus use: `--color-7`, `--color-8`, `--color-12`, `--color-14`, `--color-15`.
pub const C7: Color32 = Color32::from_rgb(0xba, 0xbd, 0xb6);
pub const C8: Color32 = Color32::from_rgb(0x55, 0x57, 0x53);
pub const C12: Color32 = Color32::from_rgb(0xef, 0x29, 0x29);
pub const C14: Color32 = Color32::from_rgb(0xfc, 0xe9, 0x4f);
pub const C15: Color32 = Color32::from_rgb(0xee, 0xee, 0xec);
/// The connection dot's colours.
pub const CONN_UP: Color32 = Color32::from_rgb(0x6f, 0xbf, 0x73);
pub const ERROR_MARK: Color32 = Color32::from_rgb(0xe0, 0x70, 0x5f);
/// The dark panel the roster, the settings and the documents stand on.
pub const PANEL: Color32 = Color32::from_rgba_premultiplied(5, 4, 9, 224);

/// A colour with its alpha scaled.
pub fn fade(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a.clamp(0.0, 1.0))
}

/// rgba() as CSS writes it.
pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// A gradient across `rect`, left to right (`horizontal`) or top to bottom, through `stops`
/// (position 0..1, colour).
pub fn gradient(painter: &egui::Painter, rect: Rect, horizontal: bool, stops: &[(f32, Color32)]) {
    if stops.len() < 2 || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let mut mesh = egui::Mesh::default();
    for (i, (t, c)) in stops.iter().enumerate() {
        let t = t.clamp(0.0, 1.0);
        let (a, b) = if horizontal {
            (Pos2::new(rect.left() + rect.width() * t, rect.top()), Pos2::new(rect.left() + rect.width() * t, rect.bottom()))
        } else {
            (Pos2::new(rect.left(), rect.top() + rect.height() * t), Pos2::new(rect.right(), rect.top() + rect.height() * t))
        };
        mesh.colored_vertex(a, *c);
        mesh.colored_vertex(b, *c);
        if i > 0 {
            let k = (i as u32) * 2;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k, k + 1);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// A radial gradient (an ellipse centred at `centre`, its radii the farthest corner's, as CSS's
/// `radial-gradient(ellipse at …)` sizes it), drawn as rings.
pub fn radial(painter: &egui::Painter, rect: Rect, centre: Pos2, stops: &[(f32, Color32)]) {
    let rx = (centre.x - rect.left()).abs().max((rect.right() - centre.x).abs()) * std::f32::consts::SQRT_2;
    let ry = (centre.y - rect.top()).abs().max((rect.bottom() - centre.y).abs()) * std::f32::consts::SQRT_2;
    let segments = 48;
    let mut mesh = egui::Mesh::default();
    let colour_at = |t: f32| -> Color32 {
        let mut prev = stops[0];
        for s in stops {
            if t <= s.0 {
                let span = (s.0 - prev.0).max(1e-6);
                return lerp(prev.1, s.1, ((t - prev.0) / span).clamp(0.0, 1.0));
            }
            prev = *s;
        }
        stops[stops.len() - 1].1
    };
    let rings = 16;
    for ring in 0..=rings {
        let t = ring as f32 / rings as f32;
        let c = colour_at(t);
        for s in 0..segments {
            let a = s as f32 / segments as f32 * std::f32::consts::TAU;
            let p = Pos2::new(centre.x + a.cos() * rx * t, centre.y + a.sin() * ry * t);
            mesh.colored_vertex(p, c);
        }
    }
    for ring in 0..rings {
        for s in 0..segments {
            let a = ring * segments + s;
            let b = ring * segments + (s + 1) % segments;
            let c = a + segments;
            let d = b + segments;
            mesh.add_triangle(a as u32, b as u32, c as u32);
            mesh.add_triangle(b as u32, d as u32, c as u32);
        }
    }
    painter.with_clip_rect(rect).add(egui::Shape::mesh(mesh));
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// How a run of text is set.
#[derive(Clone, Debug)]
pub struct Style {
    pub size: f32,
    pub family: FontFamily,
    pub colour: Color32,
    /// Extra space between letters, in points (CSS `letter-spacing`).
    pub spacing: f32,
    /// The line's height in points; none for the font's own.
    pub line: Option<f32>,
    pub italics: bool,
    pub underline: bool,
}

impl Style {
    pub fn new(size: f32, family: FontFamily, colour: Color32) -> Self {
        Style { size, family, colour, spacing: 0.0, line: None, italics: false, underline: false }
    }

    pub fn spacing(mut self, s: f32) -> Self {
        self.spacing = s;
        self
    }

    pub fn line(mut self, h: f32) -> Self {
        self.line = Some(h);
        self
    }

    pub fn italic(mut self) -> Self {
        self.italics = true;
        self
    }

    pub fn colour(mut self, c: Color32) -> Self {
        self.colour = c;
        self
    }

    pub fn format(&self) -> egui::TextFormat {
        egui::TextFormat {
            font_id: FontId::new(self.size, self.family.clone()),
            color: self.colour,
            extra_letter_spacing: self.spacing,
            line_height: self.line,
            italics: self.italics,
            underline: if self.underline { egui::Stroke::new(1.0, self.colour) } else { egui::Stroke::NONE },
            ..Default::default()
        }
    }
}

/// Lay out `text` in `style`, wrapped at `wrap` points (infinite for one line).
pub fn galley(ctx: &egui::Context, text: &str, style: &Style, wrap: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.to_string(), style.format());
    job.wrap.max_width = wrap;
    ctx.fonts_mut(|f| f.layout_job(job))
}

/// Lay out runs of differently set text as one paragraph.
pub fn galley_runs(ctx: &egui::Context, runs: &[(&str, &Style)], wrap: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap;
    for (t, s) in runs {
        job.append(t, 0.0, s.format());
    }
    ctx.fonts_mut(|f| f.layout_job(job))
}

/// A text shadow (`text-shadow: 0 2px 8px #000`) as egui can draw one: the words again, dark and
/// a little lower, under them.
pub fn shadowed(painter: &egui::Painter, pos: Pos2, g: &Arc<egui::Galley>, colour: Color32, shadow: f32) {
    if shadow > 0.0 {
        painter.galley_with_override_text_color(pos + egui::vec2(0.0, 2.0), g.clone(), Color32::from_black_alpha((shadow * 255.0) as u8));
    }
    painter.galley_with_override_text_color(pos, g.clone(), colour);
}

/// CSS `text-transform: uppercase`.
pub fn upper(s: &str) -> String {
    s.to_uppercase()
}

/// The front end's fonts: the console face (bold where the screen is a low-density one, as
/// `--text-weight` is), Grenze Gotisch for the titles.
#[derive(Clone, Copy, Debug)]
pub struct Faces {
    /// `--text-weight: 700` on a screen at 1.5 device pixels per point or less.
    pub bold_text: bool,
}

impl Faces {
    pub fn text(&self) -> FontFamily {
        if self.bold_text {
            isorun_ui::fonts::bold()
        } else {
            FontFamily::Monospace
        }
    }

    pub fn regular(&self) -> FontFamily {
        FontFamily::Monospace
    }

    pub fn bold(&self) -> FontFamily {
        isorun_ui::fonts::bold()
    }

    pub fn title(&self) -> FontFamily {
        isorun_ui::fonts::title()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours() {
        assert_eq!(lerp(Color32::BLACK, Color32::WHITE, 0.5), Color32::from_rgb(128, 128, 128));
        assert_eq!(rgba(255, 0, 0, 1.0), Color32::RED);
        assert_eq!(upper("They are kept"), "THEY ARE KEPT");
    }
}

/// A point of a 24x24 glyph in `rect`.
fn gp(rect: Rect, x: f32, y: f32) -> Pos2 {
    Pos2::new(rect.left() + x / 24.0 * rect.width(), rect.top() + y / 24.0 * rect.height())
}

/// The points of an arc of a 24x24 glyph, centred at (cx, cy), from angle a0 to a1 (radians, y down).
fn arc(rect: Rect, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    (0..=8).map(|i| a0 + (a1 - a0) * i as f32 / 8.0).map(|a| gp(rect, cx + r * a.cos(), cy + r * a.sin())).collect()
}

/// Drag to look (glyphs.ts `hand`): an open hand, the grab cursor, in outline.
pub fn paint_hand(p: &egui::Painter, rect: Rect, colour: Color32) {
    use std::f32::consts::PI;
    let stroke = egui::Stroke::new(1.5 * rect.width() / 24.0, fade(colour, 0.9));
    let line = |pts: Vec<Pos2>| p.add(egui::Shape::line(pts, stroke));
    // three fingers: up, over the top, down
    for (x1, x0, top, bottom) in [(10.2, 7.6, 6.0, 14.6), (12.8, 10.2, 4.0, 11.0), (15.4, 12.8, 4.8, 11.0)] {
        let mut pts = vec![gp(rect, x1, 11.0), gp(rect, x1, top)];
        pts.extend(arc(rect, (x1 + x0) / 2.0, top, 1.3, 0.0, -PI));
        pts.push(gp(rect, x0, bottom));
        line(pts);
    }
    // the thumb side and the palm
    let mut pts = vec![gp(rect, 15.4, 11.0), gp(rect, 15.4, 7.0)];
    pts.extend(arc(rect, 16.7, 7.0, 1.3, PI, 2.0 * PI));
    pts.push(gp(rect, 18.0, 15.0));
    pts.extend(arc(rect, 12.5, 15.0, 5.5, 0.0, 0.5 * PI));
    pts.push(gp(rect, 11.2, 20.5));
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        let (x, y) = ((1.0 - t) * (1.0 - t) * 11.2 + 2.0 * (1.0 - t) * t * 8.8 + t * t * 7.4, (1.0 - t) * (1.0 - t) * 20.5 + 2.0 * (1.0 - t) * t * 20.5 + t * t * 19.0);
        pts.push(gp(rect, x, y));
    }
    pts.push(gp(rect, 4.4, 15.5));
    pts.extend(arc(rect, 5.35, 14.6, 1.3, 0.75 * PI, 1.75 * PI));
    pts.push(gp(rect, 7.6, 14.6));
    line(pts);
}

/// The right stick (glyphs.ts `stick`): a dark disc, the stick's ring, the arrows round it, its letter.
pub fn paint_stick(p: &egui::Painter, rect: Rect, colour: Color32, label: &str) {
    let s = rect.width() / 24.0;
    let c = rect.center();
    p.circle_filled(c, 11.0 * s, Color32::from_rgb(0x15, 0x17, 0x1c));
    p.circle_stroke(c, 6.5 * s, egui::Stroke::new(1.5 * s, fade(colour, 0.8)));
    let st = egui::Stroke::new(1.3 * s, fade(colour, 0.55));
    for (a, b, d) in [((12.0, 1.5), (10.0, 3.5), (14.0, 3.5)), ((12.0, 22.5), (10.0, 20.5), (14.0, 20.5)), ((1.5, 12.0), (3.5, 10.0), (3.5, 14.0)), ((22.5, 12.0), (20.5, 10.0), (20.5, 14.0))] {
        p.line_segment([gp(rect, a.0, a.1), gp(rect, b.0, b.1)], st);
        p.line_segment([gp(rect, a.0, a.1), gp(rect, d.0, d.1)], st);
    }
    p.text(gp(rect, 12.0, 12.5), egui::Align2::CENTER_CENTER, label, FontId::new(8.5 * s, isorun_ui::fonts::bold()), colour);
}
