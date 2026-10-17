//! A draw list placed on the screen: its quads turned and moved into place, culled to what
//! shows, cut to a disc for the minimap, and batched into one egui mesh per run of one texture
//! (one draw call each).

use crate::atlas::MapTiles;
use crate::build::{DrawList, Item, Tex, Turn};
use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Pos2, Rect, Shape, Vec2};
use std::sync::Arc;

/// Where the map's own points go on the screen: turned by `rot` (radians, clockwise on the
/// screen) about `pivot` (the map's points), which lands on `at` (the screen's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub rot: f32,
    pub pivot: Pos2,
    pub at: Pos2,
    /// How far what leans (render-2d `lean`) is turned back from the ground, in radians:
    /// `-rot` stands it upright on the screen.
    pub lean_back: f32,
    /// How far past its square a base tile is drawn (`layout::seam`).
    pub seam: f32,
}

impl Place {
    /// An unturned map whose point `pivot` is at `at`.
    pub fn shift(pivot: Pos2, at: Pos2) -> Self {
        Place { rot: 0.0, pivot, at, lean_back: 0.0, seam: 0.0 }
    }

    fn turn(&self, p: Pos2, angle: f32) -> Pos2 {
        let (s, c) = angle.sin_cos();
        let d = p - self.pivot;
        self.at + Vec2::new(d.x * c - d.y * s, d.x * s + d.y * c)
    }
}

/// The disc the minimap is cut to, its rim antialiased over `feather` points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Disc {
    pub centre: Pos2,
    pub r: f32,
    pub feather: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct V {
    p: Pos2,
    uv: Pos2,
    c: [f32; 4],
}

fn lerp_v(a: &V, b: &V, t: f32) -> V {
    let l = |x: f32, y: f32| x + (y - x) * t;
    V { p: a.p.lerp(b.p, t), uv: a.uv.lerp(b.uv, t), c: [l(a.c[0], b.c[0]), l(a.c[1], b.c[1]), l(a.c[2], b.c[2]), l(a.c[3], b.c[3])] }
}

fn cross(o: Pos2, a: Pos2, b: Pos2) -> f32 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// The part of a convex polygon inside another convex polygon (Sutherland-Hodgman), either
/// winding.
fn clip_convex(poly: &[V], clip: &[Pos2]) -> Vec<V> {
    let n = clip.len();
    let mut area = 0.0;
    for i in 0..n {
        area += cross(Pos2::ZERO, clip[i], clip[(i + 1) % n]);
    }
    let sign = area.signum();
    let mut out: Vec<V> = poly.to_vec();
    for i in 0..n {
        if out.is_empty() {
            break;
        }
        let (a, b) = (clip[i], clip[(i + 1) % n]);
        let side = |p: Pos2| cross(a, b, p) * sign;
        let input = std::mem::take(&mut out);
        for j in 0..input.len() {
            let cur = &input[j];
            let prev = &input[(j + input.len() - 1) % input.len()];
            let (sc, sp) = (side(cur.p), side(prev.p));
            if sc >= 0.0 {
                if sp < 0.0 {
                    out.push(lerp_v(prev, cur, sp / (sp - sc)));
                }
                out.push(*cur);
            } else if sp >= 0.0 {
                out.push(lerp_v(prev, cur, sp / (sp - sc)));
            }
        }
    }
    out
}

/// The disc's rim as a polygon of this many sides: a tenth of a point off the circle at the
/// minimap's biggest size.
const RIM_SIDES: usize = 96;

struct Rim {
    disc: Disc,
    inner: Vec<Pos2>,
    outer: Vec<Pos2>,
}

impl Rim {
    fn new(disc: Disc) -> Self {
        let ring = |r: f32| -> Vec<Pos2> {
            (0..RIM_SIDES)
                .map(|k| {
                    let a = k as f32 / RIM_SIDES as f32 * std::f32::consts::TAU;
                    disc.centre + Vec2::new(a.cos(), a.sin()) * r
                })
                .collect()
        };
        let half = disc.feather / 2.0;
        Rim { disc, inner: ring(disc.r - half), outer: ring(disc.r + half) }
    }

    /// The polygon's parts inside the disc: wholly inside the rim's inner edge as it is, and
    /// across the rim faded out to nothing at its outer edge.
    fn cut(&self, poly: &[V], out: &mut Vec<Vec<V>>) {
        let c = self.disc.centre;
        let half = self.disc.feather / 2.0;
        let (r_in, r_out) = (self.disc.r - half, self.disc.r + half);
        let far = poly.iter().map(|v| v.p.distance(c)).fold(0.0, f32::max);
        if far <= r_in {
            out.push(poly.to_vec());
            return;
        }
        if distance_to_polygon(c, poly) >= r_out {
            return;
        }
        let inside = clip_convex(poly, &self.inner);
        if inside.len() >= 3 {
            out.push(inside);
        }
        let bounds = Rect::from_points(&poly.iter().map(|v| v.p).collect::<Vec<_>>());
        for k in 0..RIM_SIDES {
            let k1 = (k + 1) % RIM_SIDES;
            let trapezoid = [self.inner[k], self.inner[k1], self.outer[k1], self.outer[k]];
            if !Rect::from_points(&trapezoid).intersects(bounds) {
                continue;
            }
            let mut part = clip_convex(poly, &trapezoid);
            if part.len() < 3 {
                continue;
            }
            for v in &mut part {
                let f = ((r_out - v.p.distance(c)) / (r_out - r_in)).clamp(0.0, 1.0);
                for ch in &mut v.c {
                    *ch *= f;
                }
            }
            out.push(part);
        }
    }
}

/// How far `p` is from a convex polygon: 0 inside it.
fn distance_to_polygon(p: Pos2, poly: &[V]) -> f32 {
    let n = poly.len();
    let mut sign = 0.0f32;
    let mut inside = true;
    let mut best = f32::INFINITY;
    for i in 0..n {
        let (a, b) = (poly[i].p, poly[(i + 1) % n].p);
        let s = cross(a, b, p);
        if s != 0.0 {
            if sign == 0.0 {
                sign = s.signum();
            } else if s.signum() != sign {
                inside = false;
            }
        }
        let ab = b - a;
        let t = if ab.length_sq() > 0.0 { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
        best = best.min(p.distance(a + ab * t));
    }
    if inside {
        0.0
    } else {
        best
    }
}

fn colour_f(c: Color32) -> [f32; 4] {
    [c.r() as f32, c.g() as f32, c.b() as f32, c.a() as f32]
}

fn colour_32(c: [f32; 4]) -> Color32 {
    let b = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_premultiplied(b(c[0]), b(c[1]), b(c[2]), b(c[3]))
}

/// The four corners of a quad on the screen, clockwise from the top-left, with their texture
/// coordinates.
fn corners(item: &Item, place: &Place) -> [V; 4] {
    let (mut min, mut max) = (Pos2::from(item.min), Pos2::from(item.max));
    if item.base && place.seam > 0.0 {
        min -= Vec2::splat(place.seam);
        max += Vec2::splat(place.seam);
    }
    let (uv0, uv1) = if item.tex == Tex::White { (WHITE_UV, WHITE_UV) } else { (Pos2::from(item.uv_min), Pos2::from(item.uv_max)) };
    let pts = [min, Pos2::new(max.x, min.y), max, Pos2::new(min.x, max.y)];
    let uvs = [uv0, Pos2::new(uv1.x, uv0.y), uv1, Pos2::new(uv0.x, uv1.y)];
    let c = colour_f(item.colour);
    let lean = match item.turn {
        Turn::Ground => None,
        Turn::Upright => Some(-place.rot),
        Turn::Lean => Some(place.lean_back),
    };
    let mut out = [V { p: Pos2::ZERO, uv: Pos2::ZERO, c }; 4];
    match lean {
        // unturned (the level map): a move
        _ if place.rot == 0.0 && lean.is_none_or(|l| l == 0.0) => {
            let d = place.at - place.pivot;
            for i in 0..4 {
                out[i] = V { p: pts[i] + d, uv: uvs[i], c };
            }
        }
        None => {
            for i in 0..4 {
                out[i] = V { p: place.turn(pts[i], place.rot), uv: uvs[i], c };
            }
        }
        // the cell's centre goes with the ground; the quad turns about it by the ground's turn
        // less what it is turned back
        Some(back) => {
            let anchor = Pos2::from(item.anchor);
            let centre = place.turn(anchor, place.rot);
            let (s, co) = (place.rot + back).sin_cos();
            for i in 0..4 {
                let d = pts[i] - anchor;
                out[i] = V { p: centre + Vec2::new(d.x * co - d.y * s, d.x * s + d.y * co), uv: uvs[i], c };
            }
        }
    }
    out
}

/// What `emit` makes: meshes and glyphs in drawing order, in screen points.
#[derive(Clone, Default)]
pub struct Drawn {
    pub shapes: Vec<Shape>,
}

impl Drawn {
    pub fn paint(&self, painter: &egui::Painter) {
        painter.extend(self.shapes.iter().cloned());
    }

    /// Every vertex's colour as the shadow it casts: black at `alpha` of its own opacity,
    /// moved by `offset` (a css `drop-shadow` without the blur).
    pub fn shadow(&self, alpha: f32, offset: Vec2) -> Vec<Shape> {
        self.shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Mesh(m) => {
                    let mut m: Mesh = (**m).clone();
                    for v in &mut m.vertices {
                        v.pos += offset;
                        v.color = Color32::from_rgba_premultiplied(0, 0, 0, (v.color.a() as f32 * alpha).round() as u8);
                    }
                    Some(Shape::Mesh(Arc::new(m)))
                }
                _ => None,
            })
            .collect()
    }

    pub fn vertex_count(&self) -> usize {
        self.shapes.iter().map(|s| if let Shape::Mesh(m) = s { m.vertices.len() } else { 0 }).sum()
    }

    pub fn mesh_count(&self) -> usize {
        self.shapes.iter().filter(|s| matches!(s, Shape::Mesh(_))).count()
    }
}

/// Place a draw list on the screen. Quads wholly outside `cull` are left out; `disc` cuts what
/// is left to a disc.
pub fn emit(list: &DrawList, place: &Place, cull: Rect, disc: Option<Disc>, tiles: &mut MapTiles, painter: &egui::Painter) -> Drawn {
    let ctx = painter.ctx().clone();
    let rim = disc.map(Rim::new);
    let mut shapes: Vec<Shape> = Vec::new();
    let mut mesh: Option<(Tex, Mesh)> = None;
    let flush = |mesh: &mut Option<(Tex, Mesh)>, shapes: &mut Vec<Shape>| {
        if let Some((_, m)) = mesh.take() {
            if !m.is_empty() {
                shapes.push(Shape::Mesh(Arc::new(m)));
            }
        }
    };
    let mut parts: Vec<Vec<V>> = Vec::new();
    for item in &list.items {
        if item.tex == Tex::Glyph {
            let g = &list.glyphs[item.uv_min[0] as usize];
            let at = place.turn(Pos2::from(item.anchor), place.rot);
            if !cull.expand(g.size).contains(at) || rim.as_ref().is_some_and(|r| at.distance(r.disc.centre) > r.disc.r) {
                continue;
            }
            flush(&mut mesh, &mut shapes);
            let galley = painter.layout_no_wrap(g.text.clone(), egui::FontId::monospace(g.size), g.colour);
            let pos = at - galley.size() / 2.0;
            shapes.push(Shape::galley(pos, galley, g.colour));
            continue;
        }
        let quad = corners(item, place);
        let bounds = Rect::from_points(&quad.map(|v| v.p));
        if !bounds.intersects(cull) {
            continue;
        }
        let texture = match item.tex {
            Tex::Atlas(a) => match tiles.texture(&ctx, a) {
                Some(t) => t,
                None => continue,
            },
            _ => egui::TextureId::default(),
        };
        if mesh.as_ref().is_none_or(|(t, _)| *t != item.tex) {
            flush(&mut mesh, &mut shapes);
            mesh = Some((item.tex, Mesh::with_texture(texture)));
        }
        let m = &mut mesh.as_mut().unwrap().1;
        parts.clear();
        match &rim {
            Some(rim) => rim.cut(&quad, &mut parts),
            None => parts.push(quad.to_vec()),
        }
        for poly in &parts {
            let base = m.vertices.len() as u32;
            for v in poly {
                m.vertices.push(Vertex { pos: v.p, uv: v.uv, color: colour_32(v.c) });
            }
            for k in 1..poly.len() as u32 - 1 {
                m.indices.extend_from_slice(&[base, base + k, base + k + 1]);
            }
        }
    }
    flush(&mut mesh, &mut shapes);
    Drawn { shapes }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32) -> V {
        V { p: Pos2::new(x, y), uv: Pos2::new(x / 10.0, y / 10.0), c: [255.0; 4] }
    }

    fn area(poly: &[V]) -> f32 {
        let n = poly.len();
        (0..n).map(|i| cross(Pos2::ZERO, poly[i].p, poly[(i + 1) % n].p)).sum::<f32>().abs() / 2.0
    }

    #[test]
    fn clips_to_a_convex_polygon() {
        let square = [v(0.0, 0.0), v(10.0, 0.0), v(10.0, 10.0), v(0.0, 10.0)];
        let half = [Pos2::new(5.0, -1.0), Pos2::new(20.0, -1.0), Pos2::new(20.0, 20.0), Pos2::new(5.0, 20.0)];
        let out = clip_convex(&square, &half);
        assert!((area(&out) - 50.0).abs() < 1e-3);
        // texture coordinates follow the cut
        assert!(out.iter().all(|v| (v.uv.x - v.p.x / 10.0).abs() < 1e-5));
        // the other winding
        let mut rev = half;
        rev.reverse();
        assert!((area(&clip_convex(&square, &rev)) - 50.0).abs() < 1e-3);
        let away = [Pos2::new(50.0, 50.0), Pos2::new(60.0, 50.0), Pos2::new(60.0, 60.0)];
        assert!(clip_convex(&square, &away).is_empty());
    }

    #[test]
    fn the_disc_keeps_the_inside_and_fades_the_rim() {
        let rim = Rim::new(Disc { centre: Pos2::new(0.0, 0.0), r: 10.0, feather: 1.0 });
        let mut parts = Vec::new();
        // wholly inside: as it is
        rim.cut(&[v(-2.0, -2.0), v(2.0, -2.0), v(2.0, 2.0), v(-2.0, 2.0)], &mut parts);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].len(), 4);
        parts.clear();
        // wholly outside
        rim.cut(&[v(20.0, 20.0), v(30.0, 20.0), v(30.0, 30.0), v(20.0, 30.0)], &mut parts);
        assert!(parts.is_empty());
        // across the rim: what is kept is about the quarter of the disc in the square
        rim.cut(&[v(0.0, 0.0), v(20.0, 0.0), v(20.0, 20.0), v(0.0, 20.0)], &mut parts);
        let total: f32 = parts.iter().map(|p| area(p)).sum();
        // (the rim's band is kept whole, faded: between the quarter discs at its two edges)
        let q = |r: f32| std::f32::consts::PI * r * r / 4.0;
        assert!(total > q(9.5) - 0.5 && total < q(10.5) + 0.5, "{total}");
        // nothing past the outer edge, and the colour fades to nothing there
        for p in &parts {
            for v in p {
                assert!(v.p.distance(Pos2::ZERO) <= 10.5 + 1e-3);
                if v.p.distance(Pos2::ZERO) > 10.49 {
                    assert!(v.c[3] < 1.0);
                }
            }
        }
    }

    #[test]
    fn places_turn_the_ground_and_keep_the_upright_square() {
        let place = Place { rot: std::f32::consts::FRAC_PI_2, pivot: Pos2::new(5.0, 5.0), at: Pos2::new(100.0, 100.0), lean_back: -std::f32::consts::FRAC_PI_2, seam: 0.0 };
        let item = Item {
            tex: Tex::White,
            min: [10.0, 0.0],
            max: [20.0, 10.0],
            uv_min: [0.0; 2],
            uv_max: [0.0; 2],
            colour: Color32::WHITE,
            turn: Turn::Ground,
            anchor: [15.0, 5.0],
            base: false,
        };
        // a quarter turn clockwise: the cell east of the pivot goes south of it
        let q = corners(&item, &place);
        assert!((q[0].p - Pos2::new(105.0, 105.0)).length() < 1e-4);
        let up = corners(&Item { turn: Turn::Upright, ..item }, &place);
        assert!((up[0].p - Pos2::new(95.0, 105.0)).length() < 1e-4);
        assert!((up[2].p - Pos2::new(105.0, 115.0)).length() < 1e-4);
        // the seam widens a base tile
        let base = corners(&Item { base: true, ..item }, &Place { seam: 0.5, ..Place::shift(Pos2::ZERO, Pos2::ZERO) });
        assert_eq!(base[0].p, Pos2::new(9.5, -0.5));
    }
}
