//! The menu a long press opens on a cell (the port of `cell-menu.ts`): the cell's details over
//! what can be done there, beside the finger. [`cell_menu_rows`] is the logic; [`CellMenu`] the
//! egui widget, styled as touch.css `.cell-menu` draws it.

use crate::bindings::{Action, SeqItem};
use crate::cell_info::{cell_details, CellDetail};
use crate::context::{Context, Target};
use isorun_scene::{is_threat, BillboardKind, CellKind, DoorState, Feature, Scene, UpDown, Visibility, WoundLevel, DIR8_DX, DIR8_DY};
use isorun_webtiles::formatted_string_to_text;

/// What a row of the cell menu does: a click on the cell (1 left, 3 right, as `Runner::click_cell`
/// numbers them), or one of the dock's actions.
#[derive(Clone, Debug, PartialEq)]
pub enum CellMenuRun {
    Click(i32),
    Action(Action),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CellMenuRow {
    pub label: String,
    pub run: CellMenuRun,
}

fn keys(label: &str, text: &str) -> Action {
    Action::keys(vec![SeqItem::text(text)], label)
}

/// The actions the square the player stands on offers (dock-actions.ts, the `here` ones): the
/// stairs, a shop, an altar, a portal, a pile to pick up, an open door beside to close.
pub fn here_actions(ctx: &Context, scene: &Scene) -> Vec<(String, Action)> {
    let mut out = Vec::new();
    let cell = if scene.player_on_level { scene.cell_under() } else { None };
    let u = &ctx.under;
    if let (Target::Feature { feature, label, .. }, Some(_)) = (u, cell.and_then(|c| c.feature_tile)) {
        match feature {
            Feature::Stairs { dir, exit, .. } => {
                let down = *dir == UpDown::Down;
                let l = if *exit { "Leave the dungeon" } else if down { "Go down" } else { "Go up" };
                out.push((l.to_string(), keys(l, if down { ">" } else { "<" })));
            }
            Feature::Hatch { dir } => {
                let down = *dir == UpDown::Down;
                let l = if down { "Go down" } else { "Go up" };
                out.push((l.to_string(), keys(l, if down { ">" } else { "<" })));
            }
            Feature::Shop => out.push((format!("Enter {label}"), keys("Enter shop", ">"))),
            Feature::Altar { .. } => out.push((format!("Pray at {label}"), keys("Pray", ">"))),
            Feature::Portal | Feature::Transporter => out.push((format!("Enter {label}"), keys("Enter", ">"))),
            _ => {}
        }
    }
    let pile = match u {
        Target::Item { label } => Some(label.clone()),
        Target::Feature { items, .. } => items.clone(),
        _ => None,
    };
    if let Some(pile) = pile {
        out.push((format!("Pick up {}", formatted_string_to_text(&pile)), keys("Pick up", ",")));
    }
    if scene.player_on_level && open_door_next_to(scene) {
        out.push(("Close door".into(), keys("Close door", "C")));
    }
    out
}

fn open_door_next_to(scene: &Scene) -> bool {
    let (x, y) = scene.player;
    (0..8).any(|d| matches!(scene.cell(x + DIR8_DX[d], y + DIR8_DY[d]).and_then(|c| c.feature.as_ref()), Some(Feature::Door { state: DoorState::Open })))
}

/// The rows of the menu a long press opens on a cell: on the player's own
/// square what it offers (the stairs, a pile to pick up); elsewhere Attack for a
/// hostile next to the player, Go here for any other seen cell that is not a wall;
/// and Describe, crawl's own description (a right click), wherever there is something to describe.
pub fn cell_menu_rows(ctx: &Context, scene: &Scene, x: i32, y: i32) -> Vec<CellMenuRow> {
    let Some(cell) = scene.cell(x, y).filter(|c| c.visibility != Visibility::Unseen) else { return Vec::new() };
    let mut rows = Vec::new();
    let (px, py) = scene.player;
    if (x, y) == (px, py) {
        for (label, action) in here_actions(ctx, scene) {
            rows.push(CellMenuRow { label, run: CellMenuRun::Action(action) });
        }
    } else {
        let monster = if cell.visibility == Visibility::Visible { scene.billboards.iter().find(|b| b.x == x && b.y == y && b.kind == BillboardKind::Monster && !b.scenery) } else { None };
        let adjacent = (x - px).abs().max((y - py).abs()) == 1;
        if monster.is_some_and(is_threat) {
            if adjacent {
                rows.push(CellMenuRow { label: "Attack".into(), run: CellMenuRun::Click(1) });
            }
        } else if cell.kind != CellKind::Wall {
            rows.push(CellMenuRow { label: "Go here".into(), run: CellMenuRun::Click(1) });
        }
    }
    if !cell_details(scene, x, y).is_empty() {
        rows.push(CellMenuRow { label: "Describe".into(), run: CellMenuRun::Click(3) });
    }
    rows
}

// ---------------------------------------------------------------------------
// the widget

/// The web build's HUD unit (`--hu`, hud-unit.ts): 1px at 1080 lines (or 1700 across), between
/// 0.75 and 1.6, times the UI scale. `screen` in logical pixels.
pub fn hud_unit(screen: egui::Vec2, ui_scale: f32) -> f32 {
    ui_scale * (screen.y / 1080.0).min(screen.x / 1700.0).clamp(0.75, 1.6)
}

const TRIM: egui::Color32 = egui::Color32::from_rgb(0xb8, 0x91, 0x3f);
const TRIM_DARK: egui::Color32 = egui::Color32::from_rgb(0x4a, 0x38, 0x15);
const ROW_INK: egui::Color32 = egui::Color32::from_rgb(0xd9, 0xcc, 0xb0);
const GOLD_BRIGHT: egui::Color32 = egui::Color32::from_rgb(0xf3, 0xd9, 0x8a);

/// scene `WOUND_COLOURS` and hud.ts `WOUND_FILL`: the bar's fill and its gradient.
fn wound_bar(w: WoundLevel) -> (f32, egui::Color32, egui::Color32) {
    use egui::Color32 as C;
    match w {
        WoundLevel::Uninjured => (1.0, C::from_rgb(0x0e, 0xbd, 0x3b), C::from_rgb(0x08, 0x6f, 0x42)),
        WoundLevel::LightlyDamaged => (0.9, C::from_rgb(0x0e, 0xbd, 0x3b), C::from_rgb(0x08, 0x6f, 0x42)),
        WoundLevel::ModeratelyDamaged => (0.7, C::from_rgb(0x8b, 0xcd, 0x0d), C::from_rgb(0x49, 0x9f, 0x10)),
        WoundLevel::HeavilyDamaged => (0.5, C::from_rgb(0xdc, 0xad, 0x08), C::from_rgb(0xb1, 0x79, 0x0c)),
        WoundLevel::SeverelyDamaged => (0.3, C::from_rgb(0xe0, 0x50, 0x01), C::from_rgb(0x93, 0x3d, 0x01)),
        WoundLevel::AlmostDead => (0.1, C::from_rgb(0xdd, 0x0e, 0x00), C::from_rgb(0x9f, 0x00, 0x00)),
    }
}

/// A rectangle filled with a top-to-bottom gradient (egui paints gradients as a mesh of vertex colours).
pub fn vertical_gradient(painter: &egui::Painter, rect: egui::Rect, top: egui::Color32, bottom: egui::Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// The cell menu on the screen: the cell's details (as the inspect card shows them) over its
/// rows, beside the point that was pressed and kept inside the screen. The owner keeps it
/// while it is up and closes it when a row is picked, a tap lands outside, or a key is pressed.
#[derive(Clone, Debug)]
pub struct CellMenu {
    pub x: i32,
    pub y: i32,
    pub details: Vec<CellDetail>,
    pub rows: Vec<CellMenuRow>,
    /// where the finger pressed, in logical pixels
    pub at: egui::Pos2,
}

/// What a frame of the cell menu did.
#[derive(Clone, Debug)]
pub struct CellMenuResponse {
    /// the row picked this frame
    pub picked: Option<CellMenuRow>,
    /// where it is drawn: a press outside it puts it away
    pub rect: egui::Rect,
}

impl CellMenu {
    /// The menu for the cell (x, y) of `scene`, or None where there is nothing to show (an unseen cell).
    pub fn open(ctx: &Context, scene: &Scene, x: i32, y: i32, at: egui::Pos2) -> Option<Self> {
        let rows = cell_menu_rows(ctx, scene, x, y);
        if rows.is_empty() {
            return None;
        }
        Some(CellMenu { x, y, details: cell_details(scene, x, y), rows, at })
    }

    pub fn show(&self, ctx: &egui::Context, hu: f32) -> CellMenuResponse {
        let id = egui::Id::unique(("isorun-cell-menu", self.x, self.y));
        let caps = caps_family(ctx);
        let screen = isorun_ui::safe::rect(ctx);
        // beside the finger, not under it: above and to the right, turned back inside the screen where it would leave it
        let size = ctx.memory(|m| m.area_rect(id)).map_or(egui::vec2(220.0 * hu, 160.0 * hu), |r| r.size());
        let gap = 24.0;
        let mut left = self.at.x + gap;
        if left + size.x > screen.right() {
            left = self.at.x - gap - size.x;
        }
        let mut top = self.at.y - size.y - gap;
        if top < screen.top() {
            top = self.at.y + gap;
        }
        let pos = egui::pos2(left.clamp(screen.left(), (screen.right() - size.x).max(screen.left())), top.clamp(screen.top(), (screen.bottom() - size.y).max(screen.top())));
        let mut picked = None;
        let inner = egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            let pad = 6.0 * hu;
            let min_w = 200.0 * hu;
            let max_w = (360.0 * hu).min(screen.width() - 16.0);
            ui.set_min_width(min_w);
            ui.set_max_width(max_w);
            // the panel behind: the gradient, the trim and the shadow, drawn under what follows
            let bg = ui.painter().add(egui::Shape::Noop);
            let frame = egui::Frame::new().inner_margin(pad).show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                if !self.details.is_empty() {
                    egui::Frame::new().inner_margin(egui::Margin { left: (10.0 * hu) as i8, right: (10.0 * hu) as i8, top: (4.0 * hu) as i8, bottom: (8.0 * hu) as i8 }).show(ui, |ui| {
                        for (i, d) in self.details.iter().enumerate() {
                            let colour = d.tone.map_or(ROW_INK, |t| t.colour());
                            let (size, family) = if i == 0 { (22.0 * hu, caps.clone()) } else { (17.0 * hu, egui::FontFamily::Proportional) };
                            ui.label(egui::RichText::new(&d.text).color(colour).font(egui::FontId::new(size, family)));
                            if let Some(w) = d.wound {
                                ui.add_space(4.0 * hu);
                                let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(min_w - 2.0 * pad - 20.0 * hu), 8.0 * hu), egui::Sense::hover());
                                let p = ui.painter();
                                p.rect_filled(rect, 2.0 * hu, egui::Color32::BLACK);
                                let (fill, top, bottom) = wound_bar(w);
                                let bar = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * fill, rect.height())).shrink(1.0);
                                vertical_gradient(p, bar, top, bottom);
                                p.rect_stroke(rect, 2.0 * hu, egui::Stroke::new(1.0, TRIM_DARK), egui::StrokeKind::Inside);
                                ui.add_space(5.0 * hu);
                            }
                        }
                    });
                    let r = ui.min_rect();
                    ui.painter().hline(r.x_range(), r.bottom(), egui::Stroke::new(1.0, egui::Color32::from_rgb(0x3a, 0x2d, 0x16)));
                    ui.add_space(4.0 * hu);
                }
                for (i, row) in self.rows.iter().enumerate() {
                    let h = 44.0f32.max(30.0 * hu);
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(min_w - 2.0 * pad), h), egui::Sense::click());
                    if resp.is_pointer_button_down_on() || resp.hovered() {
                        let mut mesh = egui::Mesh::default();
                        let a = egui::Color32::from_rgba_unmultiplied(216, 180, 95, 77);
                        let b = egui::Color32::from_rgba_unmultiplied(216, 180, 95, 15);
                        mesh.colored_vertex(rect.left_top(), a);
                        mesh.colored_vertex(rect.right_top(), b);
                        mesh.colored_vertex(rect.right_bottom(), b);
                        mesh.colored_vertex(rect.left_bottom(), a);
                        mesh.add_triangle(0, 1, 2);
                        mesh.add_triangle(0, 2, 3);
                        ui.painter().add(egui::Shape::mesh(mesh));
                    }
                    let colour = if i == 0 { GOLD_BRIGHT } else { ROW_INK };
                    let font = egui::FontId::new(15.0f32.max(15.0 * hu), caps.clone());
                    ui.painter().text(egui::pos2(rect.left() + 10.0 * hu, rect.center().y), egui::Align2::LEFT_CENTER, &row.label, font, colour);
                    if resp.clicked() {
                        picked = Some(row.clone());
                    }
                }
            });
            let rect = frame.response.rect;
            let mut shapes = vec![egui::Shape::Noop];
            // box-shadow: 0 10hu 30hu rgba(0,0,0,0.9)
            shapes.push(egui::Shape::from(egui::epaint::Shadow { offset: [0, (10.0 * hu) as i8], blur: (30.0 * hu).min(255.0) as u8, spread: 0, color: egui::Color32::from_black_alpha(230) }.as_shape(rect, 4.0 * hu)));
            let mut mesh = egui::Mesh::default();
            let top = egui::Color32::from_rgba_unmultiplied(29, 25, 21, 247);
            let bottom = egui::Color32::from_rgba_unmultiplied(11, 9, 8, 247);
            mesh.colored_vertex(rect.left_top(), top);
            mesh.colored_vertex(rect.right_top(), top);
            mesh.colored_vertex(rect.right_bottom(), bottom);
            mesh.colored_vertex(rect.left_bottom(), bottom);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            shapes.push(egui::Shape::mesh(mesh));
            shapes.push(egui::Shape::rect_stroke(rect.shrink(1.0), 4.0 * hu, egui::Stroke::new(1.0, egui::Color32::BLACK), egui::StrokeKind::Inside));
            shapes.push(egui::Shape::rect_stroke(rect, 4.0 * hu, egui::Stroke::new(1.0, TRIM), egui::StrokeKind::Inside));
            ui.painter().set(bg, egui::Shape::Vec(shapes));
            rect
        });
        CellMenuResponse { picked, rect: inner.inner }
    }
}

/// Cinzel, the small capitals the web build's `--caps` names, when the ui crate's fonts are
/// installed (`isorun_ui::install`); the proportional font otherwise, as a named family egui
/// does not know would panic.
pub fn caps_family(ctx: &egui::Context) -> egui::FontFamily {
    let caps = isorun_ui::fonts::caps();
    if ctx.fonts(|f| f.families().contains(&caps)) {
        caps
    } else {
        egui::FontFamily::Proportional
    }
}
