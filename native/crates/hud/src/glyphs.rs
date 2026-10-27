//! Controller button glyphs, drawn as glyphs.ts draws its inline SVG, in the style each
//! platform's own games use: Xbox letters in coloured discs, PlayStation shapes, Nintendo's
//! swapped letters, bumpers as wide pills and triggers as tall ones, stick clicks as a stick seen
//! from above. Everything draws in the prompt's colour (`currentColor`) on top of a dark disc, so
//! a whole prompt dims or lights at once. Each glyph is its SVG's 24-unit box scaled to the em.

use egui::{Color32, Painter, Pos2, Shape, Stroke, Vec2};

/// What kind of pad is in the hand (gamepad.ts `PadKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PadKind {
    Xbox,
    PlayStation,
    Nintendo,
    #[default]
    Generic,
}

/// A button or a stick, as the prompts name them (glyphs.ts `GlyphName`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Glyph {
    A,
    B,
    X,
    Y,
    LB,
    RB,
    LT,
    RT,
    Select,
    Start,
    L3,
    R3,
    DU,
    DD,
    DL,
    DR,
    Home,
    DPad,
    LStick,
    LStickUp,
    RStick,
    Hold,
    Drag,
}

impl Glyph {
    /// The name the TypeScript uses (`'A'`, `'LSTICK'`), for ports of tables keyed by it.
    pub fn parse(s: &str) -> Option<Glyph> {
        Some(match s {
            "A" => Glyph::A,
            "B" => Glyph::B,
            "X" => Glyph::X,
            "Y" => Glyph::Y,
            "LB" => Glyph::LB,
            "RB" => Glyph::RB,
            "LT" => Glyph::LT,
            "RT" => Glyph::RT,
            "SELECT" => Glyph::Select,
            "START" => Glyph::Start,
            "L3" => Glyph::L3,
            "R3" => Glyph::R3,
            "DU" => Glyph::DU,
            "DD" => Glyph::DD,
            "DL" => Glyph::DL,
            "DR" => Glyph::DR,
            "HOME" => Glyph::Home,
            "DPAD" => Glyph::DPad,
            "LSTICK" => Glyph::LStick,
            "LSTICK_UP" => Glyph::LStickUp,
            "RSTICK" => Glyph::RStick,
            "HOLD" => Glyph::Hold,
            "DRAG" => Glyph::Drag,
            _ => return None,
        })
    }
}

const DISC: Color32 = Color32::from_rgb(0x15, 0x17, 0x1c);
const XBOX: [Color32; 4] =
    [Color32::from_rgb(0x6f, 0xcf, 0x6f), Color32::from_rgb(0xff, 0x6e, 0x6e), Color32::from_rgb(0x6d, 0x9c, 0xff), Color32::from_rgb(0xff, 0xd3, 0x5c)];
const NINTENDO: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xec);
const PS_CROSS: Color32 = Color32::from_rgb(0x7b, 0x9d, 0xff);
const PS_CIRCLE: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);
const PS_SQUARE: Color32 = Color32::from_rgb(0xff, 0x8a, 0xd8);
const PS_TRIANGLE: Color32 = Color32::from_rgb(0x4f, 0xe0, 0x8f);

/// The glyph's box in its SVG's units (24 to the em).
fn view_box(g: Glyph) -> Vec2 {
    match g {
        Glyph::LB | Glyph::RB => egui::vec2(34.0, 24.0),
        Glyph::LT | Glyph::RT => egui::vec2(30.0, 24.0),
        _ => egui::vec2(24.0, 24.0),
    }
}

/// How big the glyph is at a font size of `em`.
pub fn glyph_size(g: Glyph, em: f32) -> Vec2 {
    view_box(g) / 24.0 * em
}

/// The spoken name of a button on this controller, for tooltips.
pub fn glyph_name(g: Glyph, kind: PadKind) -> &'static str {
    let ps = kind == PadKind::PlayStation;
    let sw = kind == PadKind::Nintendo;
    match g {
        Glyph::A => pick(ps, sw, "Cross", "B", "A"),
        Glyph::B => pick(ps, sw, "Circle", "A", "B"),
        Glyph::X => pick(ps, sw, "Square", "Y", "X"),
        Glyph::Y => pick(ps, sw, "Triangle", "X", "Y"),
        Glyph::LB => pick(ps, sw, "L1", "L", "LB"),
        Glyph::RB => pick(ps, sw, "R1", "R", "RB"),
        Glyph::LT => pick(ps, sw, "L2", "ZL", "LT"),
        Glyph::RT => pick(ps, sw, "R2", "ZR", "RT"),
        Glyph::L3 => "Left stick click",
        Glyph::R3 => "Right stick click",
        Glyph::LStick => "Left stick",
        Glyph::LStickUp => "Left stick forward",
        Glyph::RStick => "Right stick",
        Glyph::Select => pick(ps, sw, "Share", "Minus", "View"),
        Glyph::Start => pick(ps, sw, "Options", "Plus", "Menu"),
        Glyph::Hold => "Hold",
        Glyph::Drag => "Drag",
        Glyph::DU => "D-pad up",
        Glyph::DR => "D-pad right",
        Glyph::DD => "D-pad down",
        Glyph::DL => "D-pad left",
        Glyph::DPad | Glyph::Home => "D-pad",
    }
}

fn pick(ps: bool, sw: bool, a: &'static str, b: &'static str, c: &'static str) -> &'static str {
    if ps {
        a
    } else if sw {
        b
    } else {
        c
    }
}

/// Draws in a glyph's own units.
struct Pen<'a> {
    p: &'a Painter,
    origin: Pos2,
    k: f32,
    current: Color32,
}

impl Pen<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.origin + egui::vec2(x, y) * self.k
    }
    fn circle_fill(&self, cx: f32, cy: f32, r: f32, c: Color32) {
        self.p.circle_filled(self.at(cx, cy), r * self.k, c);
    }
    fn circle_stroke(&self, cx: f32, cy: f32, r: f32, w: f32, c: Color32) {
        self.p.circle_stroke(self.at(cx, cy), r * self.k, Stroke::new(w * self.k, c));
    }
    fn cur(&self, opacity: f32) -> Color32 {
        self.current.gamma_multiply(opacity)
    }
    /// An SVG path's subpaths in screen points.
    fn path(&self, d: &str) -> Vec<(Vec<Pos2>, bool)> {
        parse_path(d).into_iter().map(|(pts, closed)| (pts.into_iter().map(|p| self.at(p.x, p.y)).collect(), closed)).collect()
    }
    fn stroke_path(&self, d: &str, w: f32, c: Color32) {
        for (pts, closed) in self.path(d) {
            let stroke = Stroke::new(w * self.k, c);
            if closed {
                self.p.add(Shape::closed_line(pts, stroke));
            } else {
                self.p.add(Shape::line(pts, stroke));
            }
        }
    }
    fn fill_path(&self, d: &str, fill: Color32, stroke: Option<(f32, Color32)>) {
        for (pts, _) in self.path(d) {
            let s = stroke.map_or(Stroke::NONE, |(w, c)| Stroke::new(w * self.k, c));
            self.p.add(Shape::convex_polygon(pts, fill, s));
        }
    }
    fn text(&self, x: f32, y: f32, s: &str, size: f32, c: Color32) {
        let font = egui::FontId::new(size * self.k, isorun_ui::fonts::bold());
        self.p.text(self.at(x, y), egui::Align2::CENTER_CENTER, s, font, c);
    }
}

/// Paint glyph `g` with its top-left corner at `origin`, at a font size of `em`, in `colour`
/// (the prompt's `currentColor`).
pub fn paint_glyph(painter: &Painter, g: Glyph, kind: PadKind, origin: Pos2, em: f32, colour: Color32) {
    let pen = Pen { p: painter, origin, k: em / 24.0, current: colour };
    let disc = |ring: Color32| {
        pen.circle_fill(12.0, 12.0, 11.0, DISC);
        pen.circle_stroke(12.0, 12.0, 10.0, 1.8, ring);
    };
    let letter = |ring: Color32, s: &str| {
        disc(ring);
        pen.text(12.0, 12.5, s, 12.5, ring);
    };
    let ps = |c: Color32, shape: u8| {
        disc(c);
        match shape {
            0 => pen.stroke_path("M8 8 L16 16 M16 8 L8 16", 2.0, c),
            1 => pen.circle_stroke(12.0, 12.0, 4.6, 2.0, c),
            2 => pen.stroke_path("M7.6 7.6 L16.4 7.6 L16.4 16.4 L7.6 16.4 Z", 2.0, c),
            _ => pen.stroke_path("M12 6.8 L17.2 16 L6.8 16 Z", 2.0, c),
        }
    };
    let ring_disc = |opacity: f32| {
        pen.circle_fill(12.0, 12.0, 11.0, DISC);
        pen.circle_stroke(12.0, 12.0, 10.0, 1.4, pen.cur(opacity));
    };
    let bumper = |label: &str| {
        pen.fill_path("M4 19 L4 11 Q4 5 12 5 L30 5 Q32 5 32 7 L32 19 Q32 21 30 21 L6 21 Q4 21 4 19 Z", DISC, Some((1.5, pen.cur(0.75))));
        pen.text(18.0, 13.5, label, 10.5, colour);
    };
    let trigger = |label: &str| {
        pen.fill_path("M5 22 L5 8 Q5 2 11 2 L23 2 Q25 2 25 4 L25 22 Q25 23 24 23 L6 23 Q5 23 5 22 Z", DISC, Some((1.5, pen.cur(0.75))));
        pen.stroke_path("M9 19 L21 19", 1.2, pen.cur(0.4));
        pen.text(15.0, 11.0, label, 10.0, colour);
    };
    let stick_click = |label: &str| {
        pen.circle_fill(12.0, 12.0, 11.0, DISC);
        pen.circle_stroke(12.0, 12.0, 9.5, 1.4, pen.cur(0.75));
        pen.circle_stroke(12.0, 12.0, 6.0, 1.2, pen.cur(0.45));
        pen.stroke_path("M7 3 L9 1 M17 3 L15 1", 1.4, pen.cur(0.75));
        pen.text(12.0, 12.5, label, 9.5, colour);
    };
    let stick = |label: &str| {
        pen.circle_fill(12.0, 12.0, 11.0, DISC);
        pen.circle_stroke(12.0, 12.0, 6.5, 1.5, pen.cur(0.8));
        pen.stroke_path(
            "M12 1.5 L10 3.5 M12 1.5 L14 3.5 M12 22.5 L10 20.5 M12 22.5 L14 20.5 M1.5 12 L3.5 10 M1.5 12 L3.5 14 M22.5 12 L20.5 10 M22.5 12 L20.5 14",
            1.3,
            pen.cur(0.55),
        );
        pen.text(12.0, 12.5, label, 8.5, colour);
    };
    let dpad = |arm: Option<u8>| {
        pen.circle_fill(12.0, 12.0, 11.0, DISC);
        pen.stroke_path("M9.5 3.5 h5 v6 h6 v5 h-6 v6 h-5 v-6 h-6 v-5 h6 z", 1.5, pen.cur(0.85));
        match arm {
            Some(0) => pen.fill_path("M9.5 3.5 h5 v6 h-5 z", pen.cur(0.9), None),
            Some(1) => pen.fill_path("M14.5 9.5 h6 v5 h-6 z", pen.cur(0.9), None),
            Some(2) => pen.fill_path("M9.5 14.5 h5 v6 h-5 z", pen.cur(0.9), None),
            Some(_) => pen.fill_path("M3.5 9.5 h6 v5 h-6 z", pen.cur(0.9), None),
            None => pen.circle_fill(12.0, 12.0, 1.3, pen.cur(0.6)),
        }
    };
    let menu = || {
        ring_disc(0.6);
        pen.stroke_path("M7.5 8.5 H16.5 M7.5 12 H16.5 M7.5 15.5 H16.5", 1.6, colour);
    };
    let ps_kind = kind == PadKind::PlayStation;
    let sw = kind == PadKind::Nintendo;
    match g {
        Glyph::A if ps_kind => ps(PS_CROSS, 0),
        Glyph::A if sw => letter(NINTENDO, "B"),
        Glyph::A => letter(XBOX[0], "A"),
        Glyph::B if ps_kind => ps(PS_CIRCLE, 1),
        Glyph::B if sw => letter(NINTENDO, "A"),
        Glyph::B => letter(XBOX[1], "B"),
        Glyph::X if ps_kind => ps(PS_SQUARE, 2),
        Glyph::X if sw => letter(NINTENDO, "Y"),
        Glyph::X => letter(XBOX[2], "X"),
        Glyph::Y if ps_kind => ps(PS_TRIANGLE, 3),
        Glyph::Y if sw => letter(NINTENDO, "X"),
        Glyph::Y => letter(XBOX[3], "Y"),
        Glyph::LB => bumper(pick(ps_kind, sw, "L1", "L", "LB")),
        Glyph::RB => bumper(pick(ps_kind, sw, "R1", "R", "RB")),
        Glyph::LT => trigger(pick(ps_kind, sw, "L2", "ZL", "LT")),
        Glyph::RT => trigger(pick(ps_kind, sw, "R2", "ZR", "RT")),
        Glyph::L3 => stick_click("L"),
        Glyph::R3 => stick_click("R"),
        Glyph::LStick => stick("L"),
        Glyph::RStick => stick("R"),
        Glyph::LStickUp => {
            pen.circle_fill(12.0, 12.0, 11.0, DISC);
            pen.circle_stroke(12.0, 16.0, 6.0, 1.5, pen.cur(0.8));
            pen.stroke_path("M12 9 V2 M8.5 5.5 L12 2 L15.5 5.5", 1.6, colour);
            pen.text(12.0, 16.5, "L", 8.5, colour);
        }
        Glyph::DPad => dpad(None),
        Glyph::DU => dpad(Some(0)),
        Glyph::DR => dpad(Some(1)),
        Glyph::DD => dpad(Some(2)),
        Glyph::DL => dpad(Some(3)),
        Glyph::Select if ps_kind => {
            ring_disc(0.6);
            pen.stroke_path("M12 15 V6.5 M9 9.5 L12 6.5 L15 9.5 M7.5 13 V17 H16.5 V13", 1.5, colour);
        }
        Glyph::Select if sw => {
            ring_disc(0.6);
            pen.stroke_path("M7 12 H17", 2.0, colour);
        }
        Glyph::Select => {
            ring_disc(0.6);
            pen.stroke_path("M6.5 8.5 H14.5 V15.5 H6.5 Z", 1.4, colour);
            pen.stroke_path("M10 8.5 V6.5 H17.5 V13.5 H14.5", 1.4, colour);
        }
        Glyph::Start if sw => {
            ring_disc(0.6);
            pen.stroke_path("M12 7 V17 M7 12 H17", 2.0, colour);
        }
        Glyph::Start | Glyph::Home => menu(),
        Glyph::Hold => {
            pen.circle_stroke(12.0, 12.0, 7.5, 2.0, pen.cur(0.25));
            pen.stroke_path("M12 4.5 A7.5 7.5 0 1 1 4.5 12", 2.0, colour);
        }
        Glyph::Drag => pen.stroke_path(
            "M10.2 11 V6 a1.3 1.3 0 0 0 -2.6 0 V14.6 M12.8 11 V4 a1.3 1.3 0 0 0 -2.6 0 V11 M15.4 11 V4.8 a1.3 1.3 0 0 0 -2.6 0 V11 M15.4 11 V7 a1.3 1.3 0 0 1 2.6 0 V15 a5.5 5.5 0 0 1 -5.5 5.5 H11.2 Q8.8 20.5 7.4 19 L4.4 15.5 a1.3 1.3 0 0 1 1.9 -1.8 L7.6 14.6",
            1.5,
            pen.cur(0.9),
        ),
    }
}

/// The hold ring part filled (styles.css `.chip.holding`): `fraction` of the three-quarter arc.
pub fn paint_hold_progress(painter: &Painter, origin: Pos2, em: f32, colour: Color32, fraction: f32) {
    let k = em / 24.0;
    let c = origin + egui::vec2(12.0, 12.0) * k;
    let n = 32;
    let sweep = 1.5 * std::f32::consts::PI * fraction.clamp(0.0, 1.0);
    let start = -std::f32::consts::FRAC_PI_2;
    let pts: Vec<Pos2> = (0..=n).map(|i| start + sweep * i as f32 / n as f32).map(|a| c + egui::vec2(a.cos(), a.sin()) * 7.5 * k).collect();
    painter.add(Shape::line(pts, Stroke::new(2.0 * k, colour)));
}

/// An SVG path's subpaths as points, curves flattened: M L H V Q A Z, absolute and relative.
fn parse_path(d: &str) -> Vec<(Vec<Pos2>, bool)> {
    let mut toks: Vec<String> = Vec::new();
    let mut num = String::new();
    let flush = |num: &mut String, toks: &mut Vec<String>| {
        if !num.is_empty() {
            toks.push(std::mem::take(num));
        }
    };
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() {
            flush(&mut num, &mut toks);
            toks.push(ch.to_string());
        } else if ch == '-' && !num.is_empty() && !num.ends_with('e') {
            flush(&mut num, &mut toks);
            num.push(ch);
        } else if ch == ' ' || ch == ',' {
            flush(&mut num, &mut toks);
        } else if ch == '.' && num.contains('.') {
            flush(&mut num, &mut toks);
            num.push(ch);
        } else {
            num.push(ch);
        }
    }
    flush(&mut num, &mut toks);
    let mut out: Vec<(Vec<Pos2>, bool)> = Vec::new();
    let mut cur = Pos2::ZERO;
    let mut start = Pos2::ZERO;
    let mut cmd = 'M';
    let mut i = 0;
    let nums = |i: &mut usize, n: usize| -> Vec<f32> {
        let v = toks[*i..*i + n].iter().map(|t| t.parse::<f32>().unwrap_or(0.0)).collect();
        *i += n;
        v
    };
    while i < toks.len() {
        if toks[i].chars().all(|c| c.is_ascii_alphabetic()) {
            cmd = toks[i].chars().next().unwrap();
            i += 1;
            if cmd == 'Z' || cmd == 'z' {
                if let Some(last) = out.last_mut() {
                    last.1 = true;
                }
                cur = start;
                continue;
            }
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { cur.to_vec2() } else { Vec2::ZERO };
        match cmd.to_ascii_uppercase() {
            'M' => {
                let v = nums(&mut i, 2);
                cur = egui::pos2(v[0], v[1]) + base;
                start = cur;
                out.push((vec![cur], false));
                cmd = if rel { 'l' } else { 'L' };
            }
            'L' => {
                let v = nums(&mut i, 2);
                cur = egui::pos2(v[0], v[1]) + base;
                out.last_mut().unwrap().0.push(cur);
            }
            'H' => {
                let v = nums(&mut i, 1);
                cur = egui::pos2(v[0] + if rel { cur.x } else { 0.0 }, cur.y);
                out.last_mut().unwrap().0.push(cur);
            }
            'V' => {
                let v = nums(&mut i, 1);
                cur = egui::pos2(cur.x, v[0] + if rel { cur.y } else { 0.0 });
                out.last_mut().unwrap().0.push(cur);
            }
            'Q' => {
                let v = nums(&mut i, 4);
                let c = egui::pos2(v[0], v[1]) + base;
                let e = egui::pos2(v[2], v[3]) + base;
                let p0 = cur;
                for k in 1..=8 {
                    let t = k as f32 / 8.0;
                    let a = p0.lerp(c, t);
                    let b = c.lerp(e, t);
                    out.last_mut().unwrap().0.push(a.lerp(b, t));
                }
                cur = e;
            }
            'A' => {
                let v = nums(&mut i, 7);
                let e = egui::pos2(v[5], v[6]) + base;
                let pts = arc_points(cur, e, v[0], v[1], v[3] != 0.0, v[4] != 0.0);
                out.last_mut().unwrap().0.extend(pts);
                cur = e;
            }
            _ => i += 1,
        }
    }
    out
}

/// An SVG elliptical arc (circular here: every arc the glyphs draw is) flattened, after the end
/// point parameterisation of the SVG spec, appendix B.2.4.
fn arc_points(from: Pos2, to: Pos2, rx: f32, _ry: f32, large: bool, sweep: bool) -> Vec<Pos2> {
    let mid = (from - to) / 2.0;
    let mut r = rx.abs();
    let d2 = mid.length_sq();
    if d2 > r * r {
        r = d2.sqrt();
    }
    let sign = if large == sweep { -1.0 } else { 1.0 };
    let k = sign * ((r * r - d2).max(0.0) / d2.max(1e-6)).sqrt();
    let c = egui::vec2(k * mid.y, -k * mid.x) + ((from.to_vec2() + to.to_vec2()) / 2.0);
    let a0 = (from.y - c.y).atan2(from.x - c.x);
    let mut a1 = (to.y - c.y).atan2(to.x - c.x);
    if sweep && a1 < a0 {
        a1 += std::f32::consts::TAU;
    }
    if !sweep && a1 > a0 {
        a1 -= std::f32::consts::TAU;
    }
    (1..=16).map(|i| a0 + (a1 - a0) * i as f32 / 16.0).map(|a| c.to_pos2() + egui::vec2(a.cos(), a.sin()) * r).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        let p = parse_path("M9.5 3.5 h5 v6 h-5 z");
        assert_eq!(p.len(), 1);
        assert!(p[0].1);
        assert_eq!(p[0].0[2], egui::pos2(14.5, 9.5));
        let p = parse_path("M8 8 L16 16 M16 8 L8 16");
        assert_eq!(p.len(), 2);
        let arc = arc_points(egui::pos2(12.0, 4.5), egui::pos2(4.5, 12.0), 7.5, 7.5, true, true);
        assert!((arc.last().unwrap().x - 4.5).abs() < 1e-3);
        // a three-quarter turn clockwise passes through the right and the bottom of the circle
        assert!(arc.iter().any(|p| p.x > 19.0));
        assert_eq!(glyph_name(Glyph::A, PadKind::PlayStation), "Cross");
    }
}
