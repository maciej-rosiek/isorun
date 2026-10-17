//! CSS boxes in egui: the dock's and the screens' backgrounds are stacks of CSS gradients
//! (linear, radial, conic, repeating) under a border and inset shadows, on a rounded box. egui
//! paints flat colours, so each box is rasterised once on the CPU, as the browser paints it,
//! into a texture kept by its style and size (`BoxCache`), and drawn as a textured rounded
//! rect; outer shadows are egui's own blurred rects. A box of the same style and size is the
//! same texture, so the 80 sockets of the inventory are one upload.
//!
//! Colours are sRGB with straight alpha, as CSS writes them; gradients interpolate in
//! premultiplied sRGB as browsers do.

use egui::{Color32, Pos2, Rect, Vec2};
use std::collections::HashMap;
use std::f32::consts::PI;

/// A colour as CSS writes it: sRGB 0..255 and an alpha 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct C {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

pub const fn hex(v: u32) -> C {
    C { r: ((v >> 16) & 255) as f32, g: ((v >> 8) & 255) as f32, b: (v & 255) as f32, a: 1.0 }
}

pub const fn rgba(r: u8, g: u8, b: u8, a: f32) -> C {
    C { r: r as f32, g: g as f32, b: b as f32, a }
}

pub const TRANSPARENT: C = C { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

impl C {
    pub fn alpha(self, a: f32) -> C {
        C { a: self.a * a, ..self }
    }

    pub fn to_color32(self) -> Color32 {
        Color32::from_rgba_unmultiplied(self.r.round().clamp(0.0, 255.0) as u8, self.g.round().clamp(0.0, 255.0) as u8, self.b.round().clamp(0.0, 255.0) as u8, (self.a * 255.0).round().clamp(0.0, 255.0) as u8)
    }

    pub fn from_color32(c: Color32) -> C {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        C { r: r as f32, g: g as f32, b: b as f32, a: a as f32 / 255.0 }
    }

    /// `color-mix(in srgb, self p, other)`
    pub fn mix(self, other: C, p: f32) -> C {
        let a = self.a * p + other.a * (1.0 - p);
        if a <= 0.0 {
            return TRANSPARENT;
        }
        let ch = |x: f32, y: f32| (x * self.a * p + y * other.a * (1.0 - p)) / a;
        C { r: ch(self.r, other.r), g: ch(self.g, other.g), b: ch(self.b, other.b), a }
    }

    /// `color-mix(in oklab, self p, other)`, the status chips' ink
    pub fn mix_oklab(self, other: C, p: f32) -> C {
        let a = oklab(self);
        let b = oklab(other);
        let m = [a[0] * p + b[0] * (1.0 - p), a[1] * p + b[1] * (1.0 - p), a[2] * p + b[2] * (1.0 - p)];
        let [r, g, bl] = from_oklab(m);
        C { r, g, b: bl, a: self.a * p + other.a * (1.0 - p) }
    }
}

fn to_linear(c: f32) -> f32 {
    let c = c / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    255.0 * if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

fn oklab(c: C) -> [f32; 3] {
    let (r, g, b) = (to_linear(c.r), to_linear(c.g), to_linear(c.b));
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s, 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s]
}

fn from_oklab(lab: [f32; 3]) -> [f32; 3] {
    let l = (lab[0] + 0.3963377774 * lab[1] + 0.2158037573 * lab[2]).powi(3);
    let m = (lab[0] - 0.1055613458 * lab[1] - 0.0638541728 * lab[2]).powi(3);
    let s = (lab[0] - 0.0894841775 * lab[1] - 1.2914855480 * lab[2]).powi(3);
    [to_srgb(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s), to_srgb(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s), to_srgb(-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s)]
}

/// Premultiplied sRGB, for interpolating and compositing as the browser does.
#[derive(Clone, Copy, Debug, Default)]
struct P {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

impl From<C> for P {
    fn from(c: C) -> P {
        P { r: c.r * c.a, g: c.g * c.a, b: c.b * c.a, a: c.a }
    }
}

impl P {
    fn lerp(self, o: P, t: f32) -> P {
        P { r: self.r + (o.r - self.r) * t, g: self.g + (o.g - self.g) * t, b: self.b + (o.b - self.b) * t, a: self.a + (o.a - self.a) * t }
    }
    /// `self` over `under`
    fn over(self, under: P) -> P {
        let k = 1.0 - self.a;
        P { r: self.r + under.r * k, g: self.g + under.g * k, b: self.b + under.b * k, a: self.a + under.a * k }
    }
    fn scale(self, k: f32) -> P {
        P { r: self.r * k, g: self.g * k, b: self.b * k, a: self.a * k }
    }
}

/// Where a colour stop stands on the gradient line: a fraction of it, a length in points, or
/// left out (spread evenly between its neighbours, as CSS does).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum At {
    F(f32),
    Pt(f32),
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop(pub C, pub At);

/// a stop at a fraction
pub fn s(c: C, f: f32) -> Stop {
    Stop(c, At::F(f))
}
/// a stop placed by CSS's rule for stops without a position
pub fn sa(c: C) -> Stop {
    Stop(c, At::Auto)
}
/// a stop at a length
pub fn sp(c: C, pt: f32) -> Stop {
    Stop(c, At::Pt(pt))
}

/// Where a radial or conic gradient is centred, as fractions of the box.
pub type Centre = (f32, f32);

#[derive(Clone, Debug, PartialEq)]
pub enum RadialSize {
    /// `circle` sized to the farthest corner
    CircleFarthest,
    /// `ellipse` sized to the farthest corner, the farthest sides' aspect
    EllipseFarthest,
    /// an ellipse with these radii as fractions of the box's width and height
    Ellipse(f32, f32),
}

/// One CSS background layer.
#[derive(Clone, Debug, PartialEq)]
pub enum Layer {
    Solid(C),
    /// `linear-gradient(<deg>, ...)`: 180 runs top to bottom, 90 left to right
    Linear(f32, Vec<Stop>),
    Radial(RadialSize, Centre, Vec<Stop>),
    /// `conic-gradient(from <deg> at <centre>, ...)`
    Conic(f32, Centre, Vec<Stop>),
    /// `repeating-linear-gradient(<deg>, ...)`, its stops in lengths
    RepeatingLinear(f32, Vec<Stop>),
}

/// `box-shadow: inset <x> <y> <blur> <spread> <colour>`
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inset {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub c: C,
}

pub fn inset(x: f32, y: f32, blur: f32, spread: f32, c: C) -> Inset {
    Inset { x, y, blur, spread, c }
}

/// `box-shadow: <x> <y> <blur> <spread> <colour>`, outside the box
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outer {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub c: C,
}

pub fn outer(x: f32, y: f32, blur: f32, spread: f32, c: C) -> Outer {
    Outer { x, y, blur, spread, c }
}

/// A box's border: its width and colour on each side (top, right, bottom, left).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    pub w: [f32; 4],
    pub c: [C; 4],
}

impl Border {
    pub fn all(w: f32, c: C) -> Border {
        Border { w: [w; 4], c: [c; 4] }
    }
    pub fn none() -> Border {
        Border { w: [0.0; 4], c: [TRANSPARENT; 4] }
    }
}

/// Everything CSS paints for a box but its content: the background layers (the first on top),
/// the border, and the shadows.
#[derive(Clone, Debug, PartialEq)]
pub struct BoxStyle {
    pub layers: Vec<Layer>,
    /// corner radii: top-left, top-right, bottom-right, bottom-left
    pub radius: [f32; 4],
    pub border: Border,
    pub insets: Vec<Inset>,
    pub outers: Vec<Outer>,
    /// the whole box's `opacity`
    pub opacity: f32,
}

impl Default for BoxStyle {
    fn default() -> Self {
        BoxStyle { layers: vec![], radius: [0.0; 4], border: Border::none(), insets: vec![], outers: vec![], opacity: 1.0 }
    }
}

impl BoxStyle {
    pub fn round(mut self, r: f32) -> Self {
        self.radius = [r; 4];
        self
    }
    pub fn border(mut self, w: f32, c: C) -> Self {
        self.border = Border::all(w, c);
        self
    }
}

/// Signed distance from `p` to a rounded box (`half` its half size, centred at the origin),
/// negative inside.
fn sd_round_box(p: Vec2, half: Vec2, radius: [f32; 4]) -> f32 {
    // the corner whose quadrant `p` is in: top-left, top-right, bottom-right, bottom-left
    let r = match (p.x >= 0.0, p.y >= 0.0) {
        (false, false) => radius[0],
        (true, false) => radius[1],
        (true, true) => radius[2],
        (false, true) => radius[3],
    }
    .min(half.x)
    .min(half.y)
    .max(0.0);
    let q = egui::vec2(p.x.abs() - half.x + r, p.y.abs() - half.y + r);
    let outside = egui::vec2(q.x.max(0.0), q.y.max(0.0)).length();
    outside + q.x.max(q.y).min(0.0) - r
}

/// The normal distribution's cumulative function, for a shadow's soft edge.
fn phi(x: f32) -> f32 {
    // Abramowitz and Stegun 7.1.26 on erf
    let z = x / std::f32::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.3275911 * z.abs());
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * (-z * z).exp();
    0.5 * (1.0 + if z >= 0.0 { y } else { -y })
}

/// The stops placed on the line, as fractions, CSS's way: missing positions spread evenly,
/// and none before the one ahead of it.
fn place_stops(stops: &[Stop], length: f32) -> Vec<(f32, P)> {
    let n = stops.len();
    let mut pos: Vec<Option<f32>> = stops
        .iter()
        .map(|s| match s.1 {
            At::F(f) => Some(f),
            At::Pt(p) => Some(if length > 0.0 { p / length } else { 0.0 }),
            At::Auto => None,
        })
        .collect();
    if n == 0 {
        return vec![];
    }
    if pos[0].is_none() {
        pos[0] = Some(0.0);
    }
    if pos[n - 1].is_none() {
        pos[n - 1] = Some(1.0);
    }
    let mut max = f32::MIN;
    for p in pos.iter_mut().flatten() {
        if *p < max {
            *p = max;
        }
        max = p.max(max);
    }
    let mut i = 0;
    while i < n {
        if pos[i].is_none() {
            let start = i - 1;
            let mut end = i;
            while pos[end].is_none() {
                end += 1;
            }
            let (a, b) = (pos[start].unwrap(), pos[end].unwrap());
            for k in i..end {
                pos[k] = Some(a + (b - a) * (k - start) as f32 / (end - start) as f32);
            }
            i = end;
        }
        i += 1;
    }
    stops.iter().zip(pos).map(|(s, p)| (p.unwrap(), P::from(s.0))).collect()
}

fn sample(stops: &[(f32, P)], t: f32) -> P {
    let Some(first) = stops.first() else { return P::default() };
    if t <= first.0 {
        return first.1;
    }
    for w in stops.windows(2) {
        let (a, b) = (w[0], w[1]);
        if t <= b.0 {
            if b.0 - a.0 <= 1e-6 {
                return b.1;
            }
            return a.1.lerp(b.1, (t - a.0) / (b.0 - a.0));
        }
    }
    stops.last().unwrap().1
}

/// A layer ready to sample at a point of a box `size` big.
enum Prepared {
    Solid(P),
    Linear { dir: Vec2, len: f32, stops: Vec<(f32, P)>, repeat: Option<(f32, f32)> },
    Radial { centre: Pos2, rx: f32, ry: f32, stops: Vec<(f32, P)> },
    Conic { centre: Pos2, from: f32, stops: Vec<(f32, P)> },
}

fn prepare(layer: &Layer, size: Vec2) -> Prepared {
    let linear = |deg: f32| {
        let a = deg * PI / 180.0;
        let dir = egui::vec2(a.sin(), -a.cos());
        let len = (size.x * a.sin()).abs() + (size.y * a.cos()).abs();
        (dir, len)
    };
    match layer {
        Layer::Solid(c) => Prepared::Solid(P::from(*c)),
        Layer::Linear(deg, stops) => {
            let (dir, len) = linear(*deg);
            Prepared::Linear { dir, len, stops: place_stops(stops, len), repeat: None }
        }
        Layer::RepeatingLinear(deg, stops) => {
            let (dir, len) = linear(*deg);
            let placed = place_stops(stops, len);
            let first = placed.first().map_or(0.0, |s| s.0);
            let last = placed.last().map_or(1.0, |s| s.0);
            Prepared::Linear { dir, len, stops: placed, repeat: Some((first, (last - first).max(1e-4))) }
        }
        Layer::Radial(shape, at, stops) => {
            let centre = egui::pos2(at.0 * size.x, at.1 * size.y);
            let fx = centre.x.max(size.x - centre.x);
            let fy = centre.y.max(size.y - centre.y);
            let (rx, ry) = match shape {
                RadialSize::CircleFarthest => {
                    let r = (fx * fx + fy * fy).sqrt();
                    (r, r)
                }
                RadialSize::EllipseFarthest => (fx * 2f32.sqrt(), fy * 2f32.sqrt()),
                RadialSize::Ellipse(x, y) => (x * size.x, y * size.y),
            };
            Prepared::Radial { centre, rx: rx.max(1e-3), ry: ry.max(1e-3), stops: place_stops(stops, rx.max(1e-3)) }
        }
        Layer::Conic(from, at, stops) => Prepared::Conic { centre: egui::pos2(at.0 * size.x, at.1 * size.y), from: *from, stops: place_stops(stops, 1.0) },
    }
}

impl Prepared {
    fn at(&self, p: Pos2, size: Vec2) -> P {
        match self {
            Prepared::Solid(c) => *c,
            Prepared::Linear { dir, len, stops, repeat } => {
                let c = p - size / 2.0;
                let mut t = (c.x * dir.x + c.y * dir.y) / len.max(1e-4) + 0.5;
                if let Some((first, period)) = repeat {
                    t = first + (t - first).rem_euclid(*period);
                }
                sample(stops, t)
            }
            Prepared::Radial { centre, rx, ry, stops } => {
                let d = p - *centre;
                sample(stops, ((d.x / rx).powi(2) + (d.y / ry).powi(2)).sqrt())
            }
            Prepared::Conic { centre, from, stops } => {
                let d = p - *centre;
                let deg = d.x.atan2(-d.y) * 180.0 / PI;
                sample(stops, (deg - from).rem_euclid(360.0) / 360.0)
            }
        }
    }
}

/// The box painted into an image `px` texels a point: everything but the outer shadows.
pub fn rasterise(style: &BoxStyle, size: Vec2, px: f32) -> egui::ColorImage {
    let w = (size.x * px).ceil().max(1.0) as usize;
    let h = (size.y * px).ceil().max(1.0) as usize;
    let layers: Vec<Prepared> = style.layers.iter().map(|l| prepare(l, size)).collect();
    let half = size / 2.0;
    let b = style.border;
    // the padding box: the border's inside edge, its radii less the border (CSS's inner radii)
    let inner_min = egui::vec2(b.w[3], b.w[0]);
    let inner_size = egui::vec2(size.x - b.w[1] - b.w[3], size.y - b.w[0] - b.w[2]).max(Vec2::ZERO);
    let inner_half = inner_size / 2.0;
    let inner_centre = inner_min + inner_half;
    let r = style.radius;
    let inner_r = [(r[0] - b.w[3].max(b.w[0])).max(0.0), (r[1] - b.w[1].max(b.w[0])).max(0.0), (r[2] - b.w[1].max(b.w[2])).max(0.0), (r[3] - b.w[3].max(b.w[2])).max(0.0)];
    let has_border = b.w.iter().any(|w| *w > 0.0);
    let aa = 0.5 / px;
    let mut pixels = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let p = egui::pos2((x as f32 + 0.5) / px, (y as f32 + 0.5) / px);
            let d = sd_round_box(p - half.to_pos2(), half, r);
            let cover = (0.5 - d * px).clamp(0.0, 1.0);
            if cover <= 0.0 {
                pixels.push(Color32::TRANSPARENT);
                continue;
            }
            let mut c = P::default();
            for l in layers.iter().rev() {
                c = l.at(p, size).over(c);
            }
            let di = sd_round_box(p.to_vec2() - inner_centre, inner_half, inner_r);
            for s in style.insets.iter().rev() {
                // the shadow lies outside a copy of the padding box moved by its offset and shrunk
                // by its spread, blurred by half its blur radius, and only inside the padding box
                let q = p.to_vec2() - inner_centre - egui::vec2(s.x, s.y);
                let ds = sd_round_box(q, (inner_half - Vec2::splat(s.spread)).max(Vec2::ZERO), inner_r.map(|r| (r - s.spread).max(0.0)));
                let k = if s.blur > 0.0 { phi(ds / (s.blur / 2.0)) } else { (0.5 + ds * px).clamp(0.0, 1.0) };
                let clip = (0.5 - di * px).clamp(0.0, 1.0);
                c = P::from(s.c).scale(k * clip).over(c);
            }
            if has_border {
                let edge = (0.5 + di * px).clamp(0.0, 1.0);
                if edge > 0.0 {
                    // which side's colour: the one whose edge is nearest, as CSS splits the corners
                    let side = [p.y, size.x - p.x, size.y - p.y, p.x];
                    let rel = [side[0] / b.w[0].max(aa), side[1] / b.w[1].max(aa), side[2] / b.w[2].max(aa), side[3] / b.w[3].max(aa)];
                    let mut k = 0;
                    for i in 1..4 {
                        if rel[i] < rel[k] {
                            k = i;
                        }
                    }
                    c = P::from(b.c[k]).scale(edge).over(c);
                }
            }
            let c = c.scale(cover * style.opacity);
            if c.a <= 0.0 {
                pixels.push(Color32::TRANSPARENT);
            } else {
                pixels.push(Color32::from_rgba_premultiplied(c.r.round().clamp(0.0, 255.0) as u8, c.g.round().clamp(0.0, 255.0) as u8, c.b.round().clamp(0.0, 255.0) as u8, (c.a * 255.0).round().clamp(0.0, 255.0) as u8));
            }
        }
    }
    egui::ColorImage::new([w, h], pixels)
}

/// The boxes as textures, by their key and size. Keep one for the HUD; it drops what was not
/// drawn for a while.
#[derive(Default)]
pub struct BoxCache {
    textures: HashMap<(String, u32, u32, u32), (egui::TextureHandle, u64)>,
    frame: u64,
}

impl BoxCache {
    /// Start a frame: textures not drawn for ten seconds' worth of frames go.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        if self.frame % 600 == 0 {
            let f = self.frame;
            self.textures.retain(|_, (_, used)| f - *used < 600);
        }
    }

    /// The texture of `style` at `size`, made the first time; `key` names the style (the same key
    /// must always mean the same style).
    pub fn texture(&mut self, ctx: &egui::Context, key: &str, size: Vec2, style: impl FnOnce() -> BoxStyle) -> egui::TextureHandle {
        let px = ctx.pixels_per_point();
        let k = (key.to_string(), (size.x * 8.0).round() as u32, (size.y * 8.0).round() as u32, (px * 100.0).round() as u32);
        let frame = self.frame;
        if let Some((t, used)) = self.textures.get_mut(&k) {
            *used = frame;
            return t.clone();
        }
        let img = rasterise(&style(), size, px);
        let t = ctx.load_texture(format!("hud-box:{key}"), img, egui::TextureOptions::LINEAR);
        self.textures.insert(k, (t.clone(), frame));
        t
    }

    /// Paint a box with its outer shadows, `style` built every call (it is cheap; the raster is
    /// what is kept).
    pub fn paint_box(&mut self, painter: &egui::Painter, key: &str, rect: Rect, style: &BoxStyle) {
        for o in &style.outers {
            outer_shadow(painter, rect, style.radius, *o);
        }
        let t = self.texture(painter.ctx(), key, rect.size(), || style.clone());
        painter.image(t.id(), snap(painter, rect), Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
    }
}

/// A rect's corner on a whole device pixel, so a box's texture lands texel for pixel.
fn snap(painter: &egui::Painter, rect: Rect) -> Rect {
    let px = painter.ctx().pixels_per_point();
    let min = egui::pos2((rect.min.x * px).round() / px, (rect.min.y * px).round() / px);
    Rect::from_min_size(min, egui::vec2((rect.width() * px).ceil() / px, (rect.height() * px).ceil() / px))
}

/// An outer `box-shadow`: egui's blurred rect, moved and spread.
pub fn outer_shadow(painter: &egui::Painter, rect: Rect, radius: [f32; 4], o: Outer) {
    let r = rect.translate(egui::vec2(o.x, o.y)).expand(o.spread);
    let cr = egui::CornerRadius { nw: (radius[0] + o.spread).max(0.0).min(255.0) as u8, ne: (radius[1] + o.spread).max(0.0).min(255.0) as u8, se: (radius[2] + o.spread).max(0.0).min(255.0) as u8, sw: (radius[3] + o.spread).max(0.0).min(255.0) as u8 };
    if o.blur <= 0.0 {
        painter.rect_filled(r, cr, o.c.to_color32());
        return;
    }
    // the browser's blur reaches `blur` past the edge on either side with half its strength at
    // the edge; egui's blur feathers over `blur_width` centred on the edge
    let shape = egui::epaint::RectShape::filled(r, cr, o.c.to_color32()).with_blur_width(o.blur * 2.0);
    painter.add(shape);
}

/// Text with the HUD's text shadows (`0 0 3px #000, 0 1px 2px #000` and the like): egui has no
/// blur on text, so a shadow is the glyphs drawn in its colour around the offset, faded.
pub fn shadowed_galley(painter: &egui::Painter, pos: Pos2, galley: std::sync::Arc<egui::Galley>, colour: Color32, shadows: &[(f32, f32, f32, Color32)]) {
    for &(x, y, blur, c) in shadows {
        if blur <= 0.5 {
            painter.galley_with_override_text_color(pos + egui::vec2(x, y), galley.clone(), c);
            continue;
        }
        // a ring of copies at the blur's reach, each a share of the colour
        let reach = (blur / 2.0).max(0.6);
        let ring = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (0.7, 0.7), (-0.7, 0.7), (0.7, -0.7), (-0.7, -0.7)];
        let share = c.gamma_multiply(0.38);
        for (dx, dy) in ring {
            painter.galley_with_override_text_color(pos + egui::vec2(x + dx * reach, y + dy * reach), galley.clone(), share);
        }
    }
    painter.galley_with_override_text_color(pos, galley, colour);
}

/// The common shadow of text over the view (`0 0 3px #000, 0 1px 2px #000`).
pub const VIEW_TEXT_SHADOW: [(f32, f32, f32, Color32); 2] = [(0.0, 0.0, 3.0, Color32::BLACK), (0.0, 1.0, 2.0, Color32::BLACK)];

/// Draw `galley` bolder than its face, as a browser makes a synthetic bold: the glyphs again a
/// hair to the right.
pub fn bold_galley(painter: &egui::Painter, pos: Pos2, galley: std::sync::Arc<egui::Galley>, colour: Color32, amount: f32) {
    painter.galley_with_override_text_color(pos + egui::vec2(amount, 0.0), galley.clone(), colour);
    painter.galley_with_override_text_color(pos, galley, colour);
}

/// A convex polygon filled from a texture laid over `tex_rect` (the orbs' liquid: the well's
/// circle cut at the level).
pub fn textured_polygon(painter: &egui::Painter, points: &[Pos2], texture: egui::TextureId, tex_rect: Rect, tint: Color32) {
    if points.len() < 3 {
        return;
    }
    let mut mesh = egui::Mesh::with_texture(texture);
    let uv = |p: Pos2| egui::pos2((p.x - tex_rect.min.x) / tex_rect.width(), (p.y - tex_rect.min.y) / tex_rect.height());
    for p in points {
        mesh.vertices.push(egui::epaint::Vertex { pos: *p, uv: uv(*p), color: tint });
    }
    for i in 1..points.len() as u32 - 1 {
        mesh.indices.extend_from_slice(&[0, i, i + 1]);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// A horizontal or vertical gradient fill of a plain rect, as a mesh of vertex colours, its
/// stops in fractions.
pub fn gradient_rect(painter: &egui::Painter, rect: Rect, vertical: bool, stops: &[(f32, C)]) {
    let mut mesh = egui::Mesh::default();
    for (i, (t, c)) in stops.iter().enumerate() {
        let col = c.to_color32();
        let (a, b) = if vertical {
            let y = rect.top() + rect.height() * t;
            (egui::pos2(rect.left(), y), egui::pos2(rect.right(), y))
        } else {
            let x = rect.left() + rect.width() * t;
            (egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom()))
        };
        mesh.colored_vertex(a, col);
        mesh.colored_vertex(b, col);
        if i > 0 {
            let k = (i as u32) * 2;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k + 1, k);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_spread_like_css() {
        let p = place_stops(&[sa(hex(0)), s(hex(0), 0.25), sa(hex(0)), sa(hex(0)), sa(hex(0))], 1.0);
        let at: Vec<f32> = p.iter().map(|x| x.0).collect();
        assert_eq!(at, [0.0, 0.25, 0.5, 0.75, 1.0]);
        let p = place_stops(&[s(hex(0), 0.5), s(hex(0), 0.2)], 1.0);
        assert_eq!(p[1].0, 0.5);
    }

    #[test]
    fn a_vertical_gradient() {
        let st = BoxStyle { layers: vec![Layer::Linear(180.0, vec![sa(hex(0x000000)), sa(hex(0xffffff))])], ..Default::default() };
        let img = rasterise(&st, egui::vec2(2.0, 10.0), 1.0);
        let top = img.pixels[0].r();
        let bottom = img.pixels[img.pixels.len() - 1].r();
        assert!(top < 20 && bottom > 235);
    }

    #[test]
    fn rounded_corners_are_clear() {
        let st = BoxStyle { layers: vec![Layer::Solid(hex(0xffffff))], ..Default::default() }.round(5.0);
        let img = rasterise(&st, egui::vec2(10.0, 10.0), 1.0);
        assert_eq!(img.pixels[0].a(), 0);
        assert_eq!(img.pixels[55].a(), 255);
    }

    #[test]
    fn oklab_mix_of_white_is_lighter() {
        let c = hex(0x8ae234).mix_oklab(hex(0xffffff), 0.6);
        assert!(c.r > 0x8a as f32 && c.g > 0xe2 as f32);
    }
}
