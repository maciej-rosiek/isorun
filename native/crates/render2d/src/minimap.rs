//! The minimap (hud.ts `renderMinimap`, `drawMinimap`): the WebTiles 2D view in miniature at
//! the top of the right column, following the player and turning with the view so up on the
//! map is up on the screen. It is cut to the disc inscribed in its square (a disc shows the
//! same reach on every heading), stands bare over the view with a soft drop shadow, and
//! carries a north mark on its rim.

use crate::atlas::MapTiles;
use crate::build::{build, build_marks, Style};
use crate::emit::{emit, Disc, Drawn, Place};
use crate::layout::{self, NORTH_ALPHA};
use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use isorun_scene::{Scene, SceneCursor};

#[derive(Clone, PartialEq)]
struct Key {
    revision: u64,
    cells: usize,
    cs: f32,
    centre: (i32, i32),
    up: f32,
    upright: Option<f32>,
    rect: Rect,
    ppp: f32,
    style: Style,
    tiles: u64,
    cursor: Option<SceneCursor>,
}

/// The drop shadow (styles.css `canvas.minimap`: `drop-shadow(0 1px 2px rgba(0, 0, 0, 0.7))`)
/// as copies of the map in black: one a point down, and four round it for the blur. Lighter
/// than the text's two-step drop: the cells are solid blocks, not thin strokes.
const SHADOW_ALPHA: f32 = 0.7;
const SHADOW_TAPS: [(f32, f32); 5] = [(0.0, 1.0), (-1.0, 1.0), (1.0, 1.0), (0.0, 0.0), (0.0, 2.0)];

/// The minimap as a widget. Keep one for the game screen.
pub struct Minimap {
    /// The "Minimap tile size" setting, in points: the cell the map is drawn at.
    pub cell: f32,
    /// The display mode and the colour blocks' palette (drawn until the tiles are in).
    pub style: Style,
    /// The map turns with the view (the north mark shows only then).
    pub turns: bool,
    /// The heading the things painted on the ground are turned back to (render-2d
    /// `uprightYaw`); `None` stands them upright on screen.
    pub upright_yaw: Option<f32>,
    /// The panes' text colour (styles.css `--color-7`), for the north mark.
    pub ink: Color32,
    cache: Option<(Key, Drawn, Vec<Shape>)>,
}

impl Default for Minimap {
    fn default() -> Self {
        Minimap { cell: layout::MINIMAP_CELL_DEFAULT as f32, style: Style::default(), turns: true, upright_yaw: None, ink: Color32::from_rgb(0xba, 0xbd, 0xb6), cache: None }
    }
}

impl Minimap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take the display mode and the palette from the rc options the server sent.
    pub fn set_options(&mut self, options: &serde_json::Map<String, serde_json::Value>) {
        self.style = Style::from_options(options);
    }

    /// Draw the map in `rect` (the square `layout::minimap_box` gives, hung off the right
    /// column's top-right corner), centred on the cell `centre` (the camera's) with `up_yaw`
    /// (radians, the camera's yaw) at the top, and the server's own cursor (targeting, `x`
    /// examine) when it has one. A click on it is the game's cue to open the level map.
    pub fn ui(&mut self, ui: &mut egui::Ui, rect: Rect, scene: &Scene, tiles: &mut MapTiles, centre: (i32, i32), up_yaw: f32, cursor: Option<&SceneCursor>) -> egui::Response {
        let response = ui.allocate_rect(rect, Sense::click());
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return response;
        }
        let painter = ui.painter_at(rect.expand(4.0));
        let ppp = ui.ctx().pixels_per_point();
        let up = if self.turns { up_yaw } else { 0.0 };
        let key = Key {
            revision: scene.revision,
            cells: scene.cells.len(),
            cs: self.cell,
            centre,
            up,
            upright: self.upright_yaw,
            rect,
            ppp,
            style: self.style.clone(),
            tiles: tiles.version,
            cursor: cursor.copied(),
        };
        if self.cache.as_ref().is_none_or(|(k, _, _)| *k != key) {
            let drawn = self.draw(rect, scene, tiles, centre, up, cursor, ppp, &painter);
            let mut shadow = Vec::new();
            let tap = 1.0 - (1.0 - SHADOW_ALPHA).powf(1.0 / SHADOW_TAPS.len() as f32);
            for (dx, dy) in SHADOW_TAPS {
                shadow.extend(drawn.shadow(tap, Vec2::new(dx, dy)));
            }
            self.cache = Some((key, drawn, shadow));
        }
        let (_, drawn, shadow) = self.cache.as_ref().unwrap();
        painter.extend(shadow.iter().cloned());
        drawn.paint(&painter);
        // no level yet (the character is still being picked): a north mark round an empty disc points at nothing
        if !scene.cells.is_empty() && self.turns {
            self.north(&painter, rect, up);
        }
        response
    }

    #[allow(clippy::too_many_arguments)]
    fn draw(&self, rect: Rect, scene: &Scene, tiles: &mut MapTiles, centre: (i32, i32), up: f32, cursor: Option<&SceneCursor>, ppp: f32, painter: &egui::Painter) -> Drawn {
        let cs = self.cell.max(1.0);
        let (w, h) = (rect.width(), rect.height());
        let origin = layout::view_origin(w, h, cs, Some(centre), scene.bounds);
        // the map turned so the heading `up` is at the top, about the cell the view follows
        let rot = -up;
        let pivot_local = ((centre.0 - origin.0) as f32 + 0.5) * cs;
        let pivot_local = (pivot_local, ((centre.1 - origin.1) as f32 + 0.5) * cs);
        let range = layout::visible_range(w, h, cs, rot, pivot_local, origin);
        let list = build(scene, tiles, cs, &self.style, Some(range));
        let marks = build_marks(scene, tiles, cs, &self.style, None, cursor);
        let place = Place {
            rot,
            pivot: Pos2::new((centre.0 as f32 + 0.5) * cs, (centre.1 as f32 + 0.5) * cs),
            at: rect.min + Vec2::new(pivot_local.0, pivot_local.1),
            // what is painted on the ground leans back to `upright_yaw`, all the way without one
            lean_back: self.upright_yaw.unwrap_or(-rot),
            seam: layout::seam(rot, ppp),
        };
        let disc = Disc { centre: rect.center(), r: w.min(h) / 2.0, feather: 1.0 / ppp };
        let mut drawn = emit(&list, &place, rect, Some(disc), tiles, painter);
        drawn.shapes.extend(emit(&marks, &place, rect, Some(disc), tiles, painter).shapes);
        drawn
    }

    /// The north mark (hud.ts `drawMinimapNorth`): an N on the rim with a short tick out to
    /// it, riding the rim as the ground turns, the letter upright. The panes' own text colour
    /// a little faint, with a dark halo that keeps it legible where the view shows through.
    fn north(&self, painter: &egui::Painter, rect: Rect, up: f32) {
        let size = rect.width();
        let m = layout::north_mark(size, up);
        let at = |x: f32, y: f32| rect.min + Vec2::new(x, y);
        let ink = self.ink.gamma_multiply(NORTH_ALPHA);
        let halo = Color32::from_rgba_unmultiplied(0, 0, 0, (0.8 * NORTH_ALPHA * 0.35 * 255.0) as u8);
        let width = (m.font / 8.0).round().max(1.0);
        let font = egui::FontId::monospace(m.font.round());
        let (a, b) = (at(m.tick[0].0, m.tick[0].1), at(m.tick[1].0, m.tick[1].1));
        let centre = at(m.x, m.y);
        for d in [Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, -1.0), Vec2::new(0.0, 1.0)] {
            painter.line_segment([a + d, b + d], Stroke::new(width, halo));
            painter.text(centre + d, egui::Align2::CENTER_CENTER, "N", font.clone(), halo);
        }
        painter.line_segment([a, b], Stroke::new(width, ink));
        // the round caps of the canvas's stroke
        if width > 1.0 {
            painter.circle_filled(a, width / 2.0, ink);
            painter.circle_filled(b, width / 2.0, ink);
        }
        painter.text(centre, egui::Align2::CENTER_CENTER, "N", font, ink);
    }
}
