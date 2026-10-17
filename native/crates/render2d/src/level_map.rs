//! The level map (`X`): crawl's own 2D view of the whole level, over the dungeon view while
//! it is open (game.ts `syncMapRenderer`). It sits on the server's view centre (`vgrdc`, the
//! map cursor while the map is open), so the map scrolls under the cursor as WebTiles' does.

use crate::atlas::MapTiles;
use crate::build::{build, build_marks, DrawList, Style};
use crate::emit::{emit, Drawn, Place};
use crate::layout;
use egui::{Color32, Pos2, Rect, Sense};
use isorun_scene::{Minibars, Scene, SceneCursor};

/// What the level map answered this frame.
pub struct MapResponse {
    pub response: egui::Response,
    /// The level's cell under the pointer, if it is one the level has.
    pub hovered: Option<(i32, i32)>,
    /// A cell clicked this frame, and with which button (game.ts `onPointer`: the game hands
    /// it to crawl as a click on that cell).
    pub clicked: Option<((i32, i32), egui::PointerButton)>,
}

#[derive(Clone, PartialEq)]
struct ListKey {
    revision: u64,
    cells: usize,
    cs: f32,
    style: Style,
    tiles: u64,
}

#[derive(Clone, PartialEq)]
struct MarksKey {
    revision: u64,
    cs: f32,
    tiles: u64,
    minibars: Option<Minibars>,
    cursor: Option<SceneCursor>,
}

#[derive(Clone, Copy, PartialEq)]
struct PlaceKey {
    list: u64,
    origin: (i32, i32),
    rect: Rect,
}

/// The level map as a widget. Keep one for the game screen; it keeps the level's meshes from
/// frame to frame and builds them again only for a new scene, a new cell size or style, and
/// places them again only when the map scrolls or the view changes size.
pub struct LevelMap {
    /// The display mode (`tile_display_mode`) and the colour blocks' palette.
    pub style: Style,
    /// The player's mini bars (`tile_show_minihealthbar`, `tile_show_minimagicbar`).
    pub minibars: Option<Minibars>,
    /// What the view is cleared to before the cells: black, as crawl's.
    pub background: Color32,
    list: Option<(ListKey, u64, DrawList)>,
    lists_built: u64,
    drawn: Option<(PlaceKey, Drawn)>,
    marks: Option<(MarksKey, PlaceKey, Drawn)>,
    origin: (i32, i32),
    cs: f32,
    rect: Rect,
}

impl Default for LevelMap {
    fn default() -> Self {
        LevelMap { style: Style::default(), minibars: None, background: Color32::BLACK, list: None, lists_built: 0, drawn: None, marks: None, origin: (0, 0), cs: 32.0, rect: Rect::NOTHING }
    }
}

impl LevelMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take the display mode and the palette from the rc options the server sent.
    pub fn set_options(&mut self, options: &serde_json::Map<String, serde_json::Value>) {
        let style = Style::from_options(options);
        if style != self.style {
            self.style = style;
        }
    }

    /// The cell at the view's top-left corner in the last frame, and the cell size.
    pub fn origin(&self) -> ((i32, i32), f32) {
        (self.origin, self.cs)
    }

    /// The level's cell under a point of the screen (render-2d `pick`), if the level has it.
    pub fn pick(&self, scene: &Scene, pos: Pos2) -> Option<(i32, i32)> {
        if !self.rect.contains(pos) {
            return None;
        }
        let p = pos - self.rect.min;
        let (x, y) = layout::pick(p.x, p.y, self.cs, self.origin);
        scene.cell(x, y).map(|_| (x, y))
    }

    /// Draw the map in `rect` at `cell` points a cell (`layout::map_cell_size`), centred on
    /// `centre` (the server's view centre, or the player's cell without one), with the cursor.
    /// The pointer over it picks cells.
    #[allow(clippy::too_many_arguments)]
    pub fn ui(&mut self, ui: &mut egui::Ui, rect: Rect, scene: &Scene, tiles: &mut MapTiles, cell: f32, centre: (i32, i32), cursor: Option<&SceneCursor>) -> MapResponse {
        let response = ui.allocate_rect(rect, Sense::click());
        let cs = cell.max(1.0);
        self.cs = cs;
        self.rect = rect;
        self.origin = layout::view_origin(rect.width(), rect.height(), cs, Some(centre), scene.bounds);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, self.background);

        let key = ListKey { revision: scene.revision, cells: scene.cells.len(), cs, style: self.style.clone(), tiles: tiles.version };
        if self.list.as_ref().is_none_or(|(k, _, _)| *k != key) {
            self.lists_built += 1;
            self.list = Some((key, self.lists_built, build(scene, tiles, cs, &self.style, None)));
        }
        let (_, list_id, list) = self.list.as_ref().unwrap();
        // the map's point at the origin cell's corner goes to the view's corner: a move, no turn
        let place = Place::shift(Pos2::new(self.origin.0 as f32 * cs, self.origin.1 as f32 * cs), rect.min);
        let pkey = PlaceKey { list: *list_id, origin: self.origin, rect };
        if self.drawn.as_ref().is_none_or(|(k, _)| *k != pkey) {
            self.drawn = Some((pkey, emit(list, &place, rect, None, tiles, &painter)));
        }
        self.drawn.as_ref().unwrap().1.paint(&painter);

        let mkey = MarksKey { revision: scene.revision, cs, tiles: tiles.version, minibars: self.minibars, cursor: cursor.copied() };
        if self.marks.as_ref().is_none_or(|(k, p, _)| *k != mkey || *p != pkey) {
            let marks = build_marks(scene, tiles, cs, &self.style, self.minibars.as_ref(), cursor);
            self.marks = Some((mkey, pkey, emit(&marks, &place, rect, None, tiles, &painter)));
        }
        self.marks.as_ref().unwrap().2.paint(&painter);

        let hovered = response.hover_pos().and_then(|p| self.pick(scene, p));
        let clicked = if response.clicked() || response.secondary_clicked() || response.middle_clicked() {
            let button = if response.secondary_clicked() {
                egui::PointerButton::Secondary
            } else if response.middle_clicked() {
                egui::PointerButton::Middle
            } else {
                egui::PointerButton::Primary
            };
            response.interact_pointer_pos().and_then(|p| self.pick(scene, p)).map(|c| (c, button))
        } else {
            None
        };
        MapResponse { response, hovered, clicked }
    }

    /// How much the map holds now: meshes and vertices of the level's batch.
    pub fn stats(&self) -> (usize, usize) {
        self.drawn.as_ref().map(|(_, d)| (d.mesh_count(), d.vertex_count())).unwrap_or((0, 0))
    }
}
