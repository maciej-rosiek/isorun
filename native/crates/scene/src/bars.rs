//! Mini health and magic bars, as DCSS draws them (the port of `packages/scene/src/bars.ts`,
//! which says where every number comes from).

/// The wound classes monster_list.js names, from the server's MDAM flags, least hurt first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WoundLevel {
    Uninjured = 0,
    LightlyDamaged = 1,
    ModeratelyDamaged = 2,
    HeavilyDamaged = 3,
    SeverelyDamaged = 4,
    AlmostDead = 5,
}

impl WoundLevel {
    pub fn class_name(self) -> &'static str {
        match self {
            WoundLevel::Uninjured => "uninjured",
            WoundLevel::LightlyDamaged => "lightly_damaged",
            WoundLevel::ModeratelyDamaged => "moderately_damaged",
            WoundLevel::HeavilyDamaged => "heavily_damaged",
            WoundLevel::SeverelyDamaged => "severely_damaged",
            WoundLevel::AlmostDead => "almost_dead",
        }
    }
}

/// mon-util.cc mons_get_damage_level, integer division and all.
pub fn wound_level(hp: i64, hp_max: i64) -> WoundLevel {
    if hp_max <= 0 || hp >= hp_max {
        WoundLevel::Uninjured
    } else if hp <= hp_max / 5 {
        WoundLevel::AlmostDead
    } else if hp <= hp_max * 2 / 5 {
        WoundLevel::SeverelyDamaged
    } else if hp <= hp_max * 3 / 5 {
        WoundLevel::HeavilyDamaged
    } else if hp <= hp_max * 4 / 5 {
        WoundLevel::ModeratelyDamaged
    } else {
        WoundLevel::LightlyDamaged
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarColours {
    pub top: u32,
    pub bottom: u32,
}

/// The fill colours of rltiles/misc/mdam_*.png, upper row then lower row.
pub fn wound_colours(w: WoundLevel) -> BarColours {
    let (top, bottom) = match w {
        WoundLevel::Uninjured | WoundLevel::LightlyDamaged => (0x0ebd3b, 0x086f42),
        WoundLevel::ModeratelyDamaged => (0x8bcd0d, 0x499f10),
        WoundLevel::HeavilyDamaged => (0xdcad08, 0xb1790c),
        WoundLevel::SeverelyDamaged => (0xe05001, 0x933d01),
        WoundLevel::AlmostDead => (0xdd0e00, 0x9f0000),
    };
    BarColours { top, bottom }
}

pub const MAGIC_COLOURS: BarColours = BarColours { top: 0x5e78ff, bottom: 0x3a4a9e };
pub const TRACK_COLOUR: u32 = 0x000000;
const TRACK_ALPHA_TOP: f32 = 128.0 / 255.0;
const TRACK_ALPHA_BOTTOM: f32 = 169.0 / 255.0;
pub const FILL_ALPHA: f32 = 225.0 / 255.0;
/// The icons' alpha steps along the fill: up to (exclusive) this texel of the 32-texel frame, this alpha.
pub const FILL_STEPS: [(f32, f32); 3] = [(6.0, 176.0 / 255.0), (15.0, 199.0 / 255.0), (f32::INFINITY, FILL_ALPHA)];
/// The lit tip: (from, top lift, bottom lift) as fractions of the fill's own length.
pub const FILL_TIP: [(f32, f32, f32); 2] = [(0.8, 0.3, 0.1), (0.95, 0.55, 0.25)];

pub const MINIBAR_CELL: f32 = 32.0;
pub const MINIBAR_ROWS: f32 = 2.0;

/// `0xrrggbb` moved toward white by `k` (0 keeps it, 1 is white).
pub fn lighten(rgb: u32, k: f32) -> u32 {
    let c = |v: u32| ((v as f32 + (255.0 - v as f32) * k).round() as u32).min(255);
    (c((rgb >> 16) & 255) << 16) | (c((rgb >> 8) & 255) << 8) | c(rgb & 255)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunSegment {
    pub x0: f32,
    pub x1: f32,
    pub alpha: f32,
    pub lift_top: f32,
    pub lift_bottom: f32,
}

/// The icons' structure along a fill `w` texels long in a `frame` texels wide.
pub fn fill_run(w: f32, frame: f32) -> Vec<RunSegment> {
    if w <= 0.0 {
        return Vec::new();
    }
    if w >= frame {
        return vec![RunSegment { x0: 0.0, x1: w, alpha: FILL_ALPHA, lift_top: 0.0, lift_bottom: 0.0 }];
    }
    let scale = frame / MINIBAR_CELL;
    let mut cuts = vec![0.0, w];
    for (to, _) in FILL_STEPS {
        if to * scale < w {
            cuts.push(to * scale)
        }
    }
    for (from, _, _) in FILL_TIP {
        cuts.push(w * from)
    }
    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    cuts.dedup();
    let mut out = Vec::new();
    for pair in cuts.windows(2) {
        let (x0, x1) = (pair[0], pair[1]);
        if x1 - x0 <= 1e-6 {
            continue;
        }
        let alpha = FILL_STEPS.iter().find(|(to, _)| x0 < to * scale).unwrap().1;
        let (mut lt, mut lb) = (0.0, 0.0);
        for (from, t, b) in FILL_TIP {
            if x0 >= w * from - 1e-6 {
                lt = t;
                lb = b;
            }
        }
        out.push(RunSegment { x0, x1, alpha, lift_top: lt, lift_bottom: lb });
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Minibars {
    pub hp: i64,
    pub hp_max: i64,
    pub mp: i64,
    pub mp_max: i64,
    pub show_hp: bool,
    pub show_mp: bool,
}

/// One filled rectangle, in texels of a 32-texel frame from its top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub colour: u32,
    pub alpha: f32,
}

/// The player's bars as rectangles on the top edge of the frame, health over magic.
pub fn minibar_rects(m: Option<&Minibars>) -> Vec<BarRect> {
    let Some(m) = m else { return Vec::new() };
    let hp_full = m.hp >= m.hp_max;
    let mp_full = m.mp >= m.mp_max;
    if (hp_full || !m.show_hp) && (mp_full || !m.show_mp) {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut y = 0.0;
    let mut bar = |frac: f32, fill: BarColours, out: &mut Vec<BarRect>| {
        let f = frac.clamp(0.0, 1.0);
        let w = if f > 0.0 { (MINIBAR_CELL * f).max(1.0) } else { 0.0 };
        out.push(BarRect { x: 0.0, y, w: MINIBAR_CELL, h: 1.0, colour: TRACK_COLOUR, alpha: TRACK_ALPHA_TOP });
        out.push(BarRect { x: 0.0, y: y + 1.0, w: MINIBAR_CELL, h: 1.0, colour: TRACK_COLOUR, alpha: TRACK_ALPHA_BOTTOM });
        for seg in fill_run(w, MINIBAR_CELL) {
            out.push(BarRect { x: seg.x0, y, w: seg.x1 - seg.x0, h: 1.0, colour: lighten(fill.top, seg.lift_top), alpha: seg.alpha });
            out.push(BarRect { x: seg.x0, y: y + 1.0, w: seg.x1 - seg.x0, h: 1.0, colour: lighten(fill.bottom, seg.lift_bottom), alpha: seg.alpha });
        }
        y += MINIBAR_ROWS;
    };
    if m.show_hp {
        bar(m.hp as f32 / m.hp_max.max(1) as f32, wound_colours(wound_level(m.hp, m.hp_max)), &mut out);
    }
    if m.show_mp && m.mp_max > 0 {
        bar(m.mp as f32 / m.mp_max as f32, MAGIC_COLOURS, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wound_levels() {
        assert_eq!(wound_level(10, 10), WoundLevel::Uninjured);
        assert_eq!(wound_level(2, 10), WoundLevel::AlmostDead);
        assert_eq!(wound_level(4, 10), WoundLevel::SeverelyDamaged);
        assert_eq!(wound_level(9, 10), WoundLevel::LightlyDamaged);
    }

    #[test]
    fn full_bar_is_one_segment() {
        assert_eq!(fill_run(32.0, 32.0).len(), 1);
        let run = fill_run(20.0, 32.0);
        assert_eq!(run.first().unwrap().x0, 0.0);
        assert_eq!(run.last().unwrap().x1, 20.0);
    }

    #[test]
    fn lighten_white() {
        assert_eq!(lighten(0x000000, 1.0), 0xffffff);
        assert_eq!(lighten(0x123456, 0.0), 0x123456);
    }
}
