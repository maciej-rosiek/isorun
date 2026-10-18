//! The HUD's look: the colours of `dock.css :root` and `styles.css`, its fonts (Cinzel's weights
//! as the dock sets them), the `--hu` unit (hud-unit.ts) and the compact window (touch.ts), and
//! text laid out the way CSS sets it (letter spacing, small capitals' uppercase, line height).

use crate::paint::{hex, C};
use egui::{Color32, FontFamily, FontId};
use std::sync::Arc;

pub const TRIM: C = hex(0xb8913f);
pub const TRIM_DARK: C = hex(0x4a3815);
pub const IRON_DEEP: C = hex(0x080707);
pub const IRON: C = hex(0x121110);
pub const IRON_LINE: C = hex(0x2c2824);
pub const BRONZE: C = hex(0x9c7a45);
pub const BRONZE_DIM: C = hex(0x4d3d24);
pub const BRONZE_BRIGHT: C = hex(0xe8c98a);
pub const BETTER: C = hex(0x7fd35a);
pub const WORSE: C = hex(0xff6b5b);
pub const GOLD: C = hex(0xd8b45f);
pub const GOLD_BRIGHT: C = hex(0xf3d98a);
pub const INK: C = hex(0xe9e2d0);
pub const INK_DIM: C = hex(0x9d9483);
pub const DANGER: C = hex(0xff5555);

/// The sixteen terminal colours (`--color-N`).
pub fn term(i: i64) -> C {
    C::from_color32(isorun_ui::text::TERM_COLOURS[(i & 15) as usize])
}

/// The colour classes the stats pane names (styles.css), the CSS named colours of the rc's
/// `hp_colour` and friends among them.
pub fn class_colour(cls: &str) -> Option<C> {
    Some(match cls {
        "degenerated_stat" => term(14),
        "zero_stat" => term(12),
        "boosted_stat" | "boosted_defense" => term(9),
        "degenerated_defense" => term(4),
        "colour_black" => hex(0x000000),
        "colour_blue" => hex(0x0000ff),
        "colour_green" => hex(0x008000),
        "colour_cyan" => hex(0x00ffff),
        "colour_red" => hex(0xff0000),
        "colour_magenta" => hex(0xff00ff),
        "colour_brown" => hex(0xa52a2a),
        "colour_lightgray" | "colour_lightgrey" => hex(0xd3d3d3),
        "colour_darkgray" | "colour_darkgrey" => hex(0xa9a9a9),
        "colour_lightblue" => hex(0xadd8e6),
        "colour_lightgreen" => hex(0x90ee90),
        "colour_lightcyan" => hex(0xe0ffff),
        "colour_lightred" => hex(0xee9090),
        "colour_lightmagenta" => hex(0xee90ee),
        "colour_yellow" => hex(0xffff00),
        "colour_white" => hex(0xffffff),
        _ => return None,
    })
}

/// A font as the CSS names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    /// `var(--mono)`, regular
    Mono,
    /// `var(--mono)` at 700
    MonoBold,
    /// `var(--caps)` (Cinzel) at 400, 600, 700, 900
    Caps400,
    Caps600,
    Caps700,
    Caps900,
    /// Grenze Gotisch
    Title,
}

pub fn family(face: Face) -> FontFamily {
    match face {
        Face::Mono => FontFamily::Monospace,
        Face::MonoBold => isorun_ui::fonts::bold(),
        Face::Caps400 => isorun_ui::fonts::caps(),
        Face::Caps600 => FontFamily::Name("hud-caps-600".into()),
        Face::Caps700 => FontFamily::Name("hud-caps-700".into()),
        Face::Caps900 => FontFamily::Name("hud-caps-900".into()),
        Face::Title => isorun_ui::fonts::title(),
    }
}

pub fn font(face: Face, size: f32) -> FontId {
    FontId::new(size, family(face))
}

/// Cinzel's weights, cut from the variable font as static faces (the web build asks for 600, 700
/// and 900 and the browser takes them from the one file; egui reads only the default instance).
/// Added to the context's fonts beside isorun-ui's; `ensure_fonts` puts them back if a later
/// `set_fonts` dropped them.
pub fn install_fonts(ctx: &egui::Context) {
    use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
    for (name, bytes, fam) in [
        ("hud-cinzel-600", include_bytes!("../assets/fonts/cinzel-600.ttf").as_slice(), Face::Caps600),
        ("hud-cinzel-700", include_bytes!("../assets/fonts/cinzel-700.ttf").as_slice(), Face::Caps700),
        ("hud-cinzel-900", include_bytes!("../assets/fonts/cinzel-900.ttf").as_slice(), Face::Caps900),
    ] {
        ctx.add_font(FontInsert::new(name, egui::FontData::from_static(bytes), vec![InsertFontFamily { family: family(fam), priority: FontPriority::Highest }]));
    }
    // what Cinzel lacks (the stars of the piety, the arrows) falls back on the mono, as the
    // browser falls back on the next family
    let fallback = [family(Face::Caps600), family(Face::Caps700), family(Face::Caps900)].into_iter().map(|f| InsertFontFamily { family: f, priority: FontPriority::Lowest }).collect();
    ctx.add_font(FontInsert::new("hud-mono-fallback", egui::FontData::from_static(include_bytes!("../../ui/assets/fonts/dejavu-sans-mono-400.ttf")), fallback));
}

/// The HUD's fonts are there (another `set_fonts` would drop them): put them back if not.
pub fn ensure_fonts(ctx: &egui::Context) {
    let there = ctx.fonts(|f| f.definitions().families.contains_key(&family(Face::Caps700)));
    if !there {
        install_fonts(ctx);
    }
}

/// hud-unit.ts: the web build's reference screen, and the unit's bounds
const REFERENCE_HEIGHT: f32 = 1080.0;
const REFERENCE_WIDTH: f32 = 1700.0;
const MIN_UNIT: f32 = 0.75;
const MAX_UNIT: f32 = 1.6;

/// `--hu`: 1 point at 1080 lines, more on a taller window, times the UI scale (hud-unit.ts
/// `hudUnit`); the dock and its screens are sized in it.
pub fn hud_unit(screen: egui::Vec2, ui_scale: f32) -> f32 {
    let fit = (screen.y / REFERENCE_HEIGHT).min(screen.x / REFERENCE_WIDTH);
    ui_scale * fit.clamp(MIN_UNIT, MAX_UNIT)
}

/// touch.ts: a window under 900 × 560 gets the compact dock
pub fn compact_window(screen: egui::Vec2) -> bool {
    screen.x < 900.0 || screen.y < 560.0
}

/// The "Minimap" setting on a window this size: Auto hides it on the compact window (a phone's,
/// where a map in the corner covers a good part of a small view, and the level map is a tap
/// away), and shows it on a bigger one; On and Off are what they say.
pub fn minimap_shown(setting: &str, screen: egui::Vec2) -> bool {
    match setting {
        "on" => true,
        "off" => false,
        _ => !compact_window(screen),
    }
}

/// The minimap's cell on a window this size, from the "Minimap tile size" setting `cell`, which
/// is meant at the reference 1600 x 1000 window: on a smaller one the cell shrinks with it (to no
/// less than 0.6 of it), so a 1024 x 768 window shows the same tiles in a map of about 250 points
/// rather than the 380 that took a third of its width.
pub fn minimap_cell(cell: u32, screen: egui::Vec2) -> u32 {
    let fit = (screen.x / 1600.0).min(screen.y / 1000.0).clamp(0.6, 1.0);
    ((cell.max(1) as f32 * fit).round() as u32).max(4)
}

pub fn landscape_window(screen: egui::Vec2) -> bool {
    screen.x > screen.y
}

/// Text set as CSS sets it: one face and size, letter spacing in em, an optional wrap width.
#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    pub face: Face,
    pub size: f32,
    /// `letter-spacing` in em
    pub spacing: f32,
    pub colour: Color32,
    /// `line-height` as a multiple of the size; None for the font's own
    pub line: Option<f32>,
    pub upper: bool,
}

impl TextStyle {
    pub fn new(face: Face, size: f32, colour: C) -> TextStyle {
        TextStyle { face, size, spacing: 0.0, colour: colour.to_color32(), line: None, upper: false }
    }
    pub fn spacing(mut self, em: f32) -> Self {
        self.spacing = em;
        self
    }
    pub fn line(mut self, k: f32) -> Self {
        self.line = Some(k);
        self
    }
    pub fn upper(mut self) -> Self {
        self.upper = true;
        self
    }
    pub fn colour(mut self, c: Color32) -> Self {
        self.colour = c;
        self
    }

    pub fn format(&self) -> egui::TextFormat {
        egui::TextFormat { font_id: font(self.face, self.size), color: self.colour, extra_letter_spacing: self.spacing * self.size, line_height: self.line.map(|k| k * self.size), ..Default::default() }
    }

    /// The text laid out, wrapped at `wrap` when given.
    pub fn galley(&self, ctx: &egui::Context, text: &str, wrap: Option<f32>) -> Arc<egui::Galley> {
        let mut job = egui::text::LayoutJob::default();
        let t = if self.upper { text.to_uppercase() } else { text.to_string() };
        job.append(&t, 0.0, self.format());
        if let Some(w) = wrap {
            job.wrap.max_width = w;
        }
        ctx.fonts_mut(|f| f.layout_job(job))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_follows_the_window() {
        assert_eq!(hud_unit(egui::vec2(1700.0, 1080.0), 1.0), 1.0);
        assert!((hud_unit(egui::vec2(1600.0, 1000.0), 1.0) - 1000.0 / 1080.0).abs() < 1e-6);
        assert_eq!(hud_unit(egui::vec2(800.0, 400.0), 1.0), 0.75);
        assert_eq!(hud_unit(egui::vec2(5120.0, 2880.0), 1.0), 1.6);
        assert!(compact_window(egui::vec2(844.0, 390.0)));
        assert!(!compact_window(egui::vec2(1600.0, 1000.0)));
    }

    #[test]
    fn minimap_setting() {
        let (phone, tilted, desk) = (egui::vec2(412.0, 915.0), egui::vec2(915.0, 412.0), egui::vec2(1600.0, 1000.0));
        assert!(!minimap_shown("auto", phone) && !minimap_shown("auto", tilted) && minimap_shown("auto", desk));
        assert!(minimap_shown("on", phone) && !minimap_shown("off", desk));
        // the setting's cell at the reference window, smaller on a mid-size one, never under 0.6 of it
        assert_eq!(minimap_cell(20, desk), 20);
        assert_eq!(minimap_cell(20, egui::vec2(2560.0, 1440.0)), 20);
        assert_eq!(minimap_cell(20, egui::vec2(1024.0, 768.0)), 13);
        assert_eq!(minimap_cell(20, phone), 12);
    }
}
