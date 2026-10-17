//! Doors and stairs built in 3D from the level's own tiles (the port of `tile-features.ts`): a
//! frame of the wall's stone with a swinging wooden leaf, and steps down into a pit or up onto a
//! landing.

use crate::gpu::{Gpu, Material};
use crate::mesh::{add_box, BoxSide, GpuMesh, Point3, QuadBatch, ALL_SIDES, ATLAS_INSET};
use crate::pages::Pages;
use crate::renderer::{Cast, DrawItem, PageRef};
use crate::uniforms::DrawUniform;
use crate::TileSource;
use glam::{Mat4, Vec3};
use isorun_scene::{cell_key, BillboardKind, CellKey, DoorState, Feature, Scene, SceneCell, TileId, UpDown, Visibility, DIR8_DX, DIR8_DY};
use std::collections::HashMap;
use std::sync::Arc;

const POST_WIDTH: f32 = 0.18;
const FRAME_DEPTH: f32 = 0.32;
const LEAF_THICKNESS: f32 = 0.06;
const LEAF_HEIGHT: f32 = 0.8;
const DOOR_OPEN_ANGLE: f32 = -1.6;
const DOOR_SWING_PER_SECOND: f32 = 7.0;
const DOOR_STILL: f32 = 1e-4;
const STEPS: usize = 4;
const STEP_DROP: f32 = 0.2;
const STEP_RISE: f32 = 0.14;
const PIT_BOTTOM: f32 = -1.2;
const PIT_WALL_SHADE: f32 = 0.35;
const RIM_WIDTH: f32 = 0.09;
const RIM_HEIGHT: f32 = 0.07;
const RIM_SHADE: f32 = 1.15;

/// The side of a cell farthest from the camera, of the four: stairs run toward it.
fn away_from(forward: [f32; 2]) -> usize {
    let score = |d: usize| DIR8_DX[d] as f32 * forward[0] + DIR8_DY[d] as f32 * forward[1];
    [0, 2, 4, 6].into_iter().fold(0, |best, d| if score(d) > score(best) { d } else { best })
}

pub fn is_tile_feature(cell: Option<&SceneCell>) -> bool {
    matches!(cell.and_then(|c| c.feature.as_ref()), Some(Feature::Door { .. }) | Some(Feature::Stairs { .. }))
}

struct Door {
    hinge: Mat4,
    angle: f32,
    target: f32,
    cell: (i32, i32),
}

pub struct TileFeatures {
    doors: HashMap<CellKey, Door>,
    ground: HashMap<CellKey, f32>,
    meshes: Vec<(u32, Arc<GpuMesh>)>,
    leaf: Option<Arc<GpuMesh>>,
    built_key: u64,
    built_pages: usize,
    pub glowing: bool,
    doors_changed: Vec<(i32, i32, bool)>,
}

impl Default for TileFeatures {
    fn default() -> Self {
        Self::new()
    }
}

impl TileFeatures {
    /// A door is still swinging.
    pub fn animating(&self) -> bool {
        self.doors.values().any(|d| (d.target - d.angle).abs() > DOOR_STILL * 10.0)
    }

    pub fn new() -> Self {
        TileFeatures { doors: HashMap::new(), ground: HashMap::new(), meshes: Vec::new(), leaf: None, built_key: 0, built_pages: 0, glowing: false, doors_changed: Vec::new() }
    }

    pub fn invalidate(&mut self) {
        self.built_key = 0;
    }

    /// Returns whether the doors and stairs were built again.
    pub fn build(&mut self, gpu: &Gpu, scene: &Scene, tiles: &dyn TileSource, pages: &mut Pages, wall_height: f32, forward: [f32; 2]) -> bool {
        use std::hash::{Hash, Hasher};
        let mut features: Vec<&SceneCell> = scene.cells.values().filter(|c| c.visibility != Visibility::Unseen && is_tile_feature(Some(c))).collect();
        features.sort_by_key(|c| (c.y, c.x));
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (wall_height.to_bits(), away_from(forward), scene.level.ceiling_tile).hash(&mut h);
        for c in &features {
            (c.x, c.y, &c.feature, c.floor_tile, c.feature_tile).hash(&mut h);
            for d in [0usize, 2, 4, 6] {
                let n = scene.cell(c.x + DIR8_DX[d], c.y + DIR8_DY[d]);
                (n.map(|n| n.occluder), n.and_then(|n| n.wall_tile)).hash(&mut h);
            }
            let someone = scene.billboards.iter().any(|b| b.x == c.x && b.y == c.y && matches!(b.kind, BillboardKind::Player | BillboardKind::Monster));
            someone.hash(&mut h);
        }
        let key = h.finish();
        let pages_id = pages as *const Pages as usize;
        if key == self.built_key && pages_id == self.built_pages {
            return false;
        }
        self.built_key = key;
        self.built_pages = pages_id;
        let before: HashMap<CellKey, (f32, f32)> = self.doors.iter().map(|(k, d)| (*k, (d.angle, d.target))).collect();
        self.doors.clear();
        self.ground.clear();
        self.glowing = false;
        let texels = pages.page_texels();
        let mut batches: HashMap<u32, QuadBatch> = HashMap::new();
        let mut glowing = false;
        let mut boxer = |pages: &mut Pages, tile: TileId, min: Point3, max: Point3, shade: f32, sides: &[BoxSide]| {
            let Some(rect) = tiles.tile(tile) else { return };
            let Some(placed) = pages.for_tile(&gpu.device, &gpu.page_layout, &gpu.page_sampler, tiles, &rect) else { return };
            if placed.label.is_some_and(|l| l.glow > 0) {
                glowing = true;
            }
            let batch = batches.entry(placed.page).or_insert_with(|| QuadBatch::new(texels, texels, 0.0, ATLAS_INSET));
            add_box(batch, (placed.sx, placed.sy, rect.w as f32, rect.h as f32), min, max, shade, sides);
        };
        for c in &features {
            match &c.feature {
                Some(Feature::Door { state }) => {
                    let previous = before.get(&cell_key(c.x, c.y)).copied();
                    self.add_door(scene, c, pages, &mut boxer, wall_height, *state, previous);
                }
                Some(Feature::Stairs { dir, .. }) => self.add_stairs(scene, c, pages, &mut boxer, *dir, forward),
                _ => {}
            }
        }
        self.glowing = glowing;
        let mut meshes: Vec<(u32, Arc<GpuMesh>)> = batches.into_iter().filter(|(_, b)| !b.is_empty()).map(|(p, b)| (p, Arc::new(GpuMesh::from_batch(&gpu.device, &b)))).collect();
        meshes.sort_by_key(|m| m.0);
        self.meshes = meshes;
        // the leaf: a box from the hinge across the doorway, a little shorter than the wall
        let width = 1.0 - 2.0 * POST_WIDTH;
        let mut leaf = QuadBatch::new(1.0, 1.0, 0.0, 0.0);
        let (x1, y1, z0, z1) = (width, LEAF_HEIGHT * wall_height, -LEAF_THICKNESS / 2.0, LEAF_THICKNESS / 2.0);
        // every face takes the whole door texture, as three's BoxGeometry maps it
        for corners in [
            [[0.0, y1, z1], [x1, y1, z1], [x1, 0.0, z1], [0.0, 0.0, z1]],
            [[x1, y1, z0], [0.0, y1, z0], [0.0, 0.0, z0], [x1, 0.0, z0]],
            [[0.0, y1, z0], [x1, y1, z0], [x1, y1, z1], [0.0, y1, z1]],
            [[x1, y1, z1], [x1, y1, z0], [x1, 0.0, z0], [x1, 0.0, z1]],
            [[0.0, y1, z0], [0.0, y1, z1], [0.0, 0.0, z1], [0.0, 0.0, z0]],
        ] {
            leaf.add_grey(corners, 0.0, 0.0, 1.0, 1.0, 1.0);
        }
        self.leaf = Some(Arc::new(GpuMesh::from_batch(&gpu.device, &leaf)));
        true
    }

    /// The height of the ground an actor stands on in cell `x`, `y`: 0 off the stairs.
    pub fn ground_at(&self, x: i32, y: i32) -> f32 {
        self.ground.get(&cell_key(x, y)).copied().unwrap_or(0.0)
    }

    /// Swing the doors; returns the cells of the doors that moved.
    pub fn update(&mut self, dt: f32) -> Vec<(i32, i32)> {
        let mut moved = Vec::new();
        for door in self.doors.values_mut() {
            let turn = (door.target - door.angle) * (dt * DOOR_SWING_PER_SECOND).min(1.0);
            door.angle += turn;
            if turn.abs() > DOOR_STILL {
                moved.push(door.cell);
            }
        }
        moved
    }

    pub fn take_doors(&mut self) -> Vec<(i32, i32, bool)> {
        std::mem::take(&mut self.doors_changed)
    }

    pub fn draws(&self, out: &mut Vec<DrawItem>) {
        for (page, mesh) in &self.meshes {
            out.push(DrawItem { material: Material::Ground, page: PageRef::Page(*page), mesh: mesh.clone(), uniform: DrawUniform::plain(true), cast: Cast::Static });
        }
        let Some(leaf) = &self.leaf else { return };
        let mut doors: Vec<&Door> = self.doors.values().collect();
        doors.sort_by_key(|d| d.cell);
        for d in doors {
            let model = d.hinge * Mat4::from_rotation_y(d.angle);
            let mut uniform = DrawUniform::at(model, false);
            uniform.look[3] = 0.0;
            out.push(DrawItem { material: Material::Clay, page: PageRef::Door, mesh: leaf.clone(), uniform, cast: Cast::Static });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_door(&mut self, scene: &Scene, cell: &SceneCell, pages: &mut Pages, boxer: &mut dyn FnMut(&mut Pages, TileId, Point3, Point3, f32, &[BoxSide]), wall_height: f32, state: DoorState, previous: Option<(f32, f32)>) {
        let (x, y) = (cell.x, cell.y);
        let (xf, yf) = (x as f32, y as f32);
        let west = scene.cell(x - 1, y);
        let east = scene.cell(x + 1, y);
        let north = scene.cell(x, y - 1);
        let south = scene.cell(x, y + 1);
        let walled = |c: Option<&SceneCell>| c.is_some_and(|c| c.occluder) as i32;
        let across = walled(west) + walled(east);
        let along = walled(north) + walled(south);
        let runs_east_west = across > along || (across == along && across > 0);
        let neighbours = if runs_east_west { [west, east] } else { [north, south] };
        let stone = neighbours.iter().find_map(|n| n.and_then(|n| n.wall_tile)).or(scene.level.ceiling_tile).unwrap_or(cell.floor_tile);
        boxer(pages, cell.floor_tile, [xf, -0.02, yf], [xf + 1.0, 0.0, yf + 1.0], 1.0, &[BoxSide::Top]);
        let h = wall_height;
        let half = FRAME_DEPTH / 2.0;
        let lintel = LEAF_HEIGHT * h;
        let someone_inside = scene.billboards.iter().any(|b| b.x == x && b.y == y && matches!(b.kind, BillboardKind::Player | BillboardKind::Monster));
        if runs_east_west {
            let z0 = if north.is_some_and(|c| c.occluder) { yf } else { yf + 0.5 - half };
            let z1 = if south.is_some_and(|c| c.occluder) { yf + 1.0 } else { yf + 0.5 + half };
            boxer(pages, stone, [xf, 0.0, z0], [xf + POST_WIDTH, h, z1], 1.0, &ALL_SIDES);
            boxer(pages, stone, [xf + 1.0 - POST_WIDTH, 0.0, z0], [xf + 1.0, h, z1], 1.0, &ALL_SIDES);
            if !someone_inside {
                boxer(pages, stone, [xf + POST_WIDTH, lintel, yf + 0.5 - half], [xf + 1.0 - POST_WIDTH, h, yf + 0.5 + half], 1.0, &ALL_SIDES);
            }
        } else {
            let x0 = if west.is_some_and(|c| c.occluder) { xf } else { xf + 0.5 - half };
            let x1 = if east.is_some_and(|c| c.occluder) { xf + 1.0 } else { xf + 0.5 + half };
            boxer(pages, stone, [x0, 0.0, yf], [x1, h, yf + POST_WIDTH], 1.0, &ALL_SIDES);
            boxer(pages, stone, [x0, 0.0, yf + 1.0 - POST_WIDTH], [x1, h, yf + 1.0], 1.0, &ALL_SIDES);
            if !someone_inside {
                boxer(pages, stone, [xf + 0.5 - half, lintel, yf + POST_WIDTH], [xf + 0.5 + half, h, yf + 1.0 - POST_WIDTH], 1.0, &ALL_SIDES);
            }
        }
        let hinge = if runs_east_west {
            Mat4::from_translation(Vec3::new(xf + POST_WIDTH, 0.0, yf + 0.5))
        } else {
            Mat4::from_translation(Vec3::new(xf + 0.5, 0.0, yf + POST_WIDTH)) * Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2)
        };
        let open = state == DoorState::Open;
        let target = if open { DOOR_OPEN_ANGLE } else { 0.0 };
        let angle = previous.map_or(target, |p| p.0);
        if let Some(p) = previous {
            if p.1 != target {
                self.doors_changed.push((x, y, open));
            }
        }
        self.doors.insert(cell_key(x, y), Door { hinge, angle, target, cell: (x, y) });
    }

    /// Stairs run away from the camera: down into the screen, or up into it.
    fn add_stairs(&mut self, scene: &Scene, cell: &SceneCell, pages: &mut Pages, boxer: &mut dyn FnMut(&mut Pages, TileId, Point3, Point3, f32, &[BoxSide]), dir: UpDown, forward: [f32; 2]) {
        let (x, y) = (cell.x, cell.y);
        let away = away_from(forward);
        let (fx, fy) = (DIR8_DX[away] as f32, DIR8_DY[away] as f32);
        let stone = [0usize, 2, 4, 6].iter().find_map(|&d| scene.cell(x + DIR8_DX[d], y + DIR8_DY[d]).and_then(|c| c.wall_tile)).or(scene.level.ceiling_tile).unwrap_or(cell.floor_tile);
        // a box in the stairs' own frame: u runs toward the wall the stairs face, v across, both 0..1
        let local = |u0: f32, u1: f32, v0: f32, v1: f32, y0: f32, y1: f32| -> (Point3, Point3) {
            let at = |u: f32, v: f32| (x as f32 + 0.5 + fx * (u - 0.5) - fy * (v - 0.5), y as f32 + 0.5 + fy * (u - 0.5) + fx * (v - 0.5));
            let a = at(u0, v0);
            let b = at(u1, v1);
            ([a.0.min(b.0), y0, a.1.min(b.1)], [a.0.max(b.0), y1, a.1.max(b.1)])
        };
        let walls = [BoxSide::N, BoxSide::S, BoxSide::W, BoxSide::E];
        if dir == UpDown::Down {
            let (min, max) = local(0.0, 1.0, 0.0, 1.0, PIT_BOTTOM, 0.0);
            boxer(pages, stone, min, max, PIT_WALL_SHADE, &walls);
            for (u0, u1, v0, v1) in [(0.0, 1.0, 0.0, RIM_WIDTH), (0.0, 1.0, 1.0 - RIM_WIDTH, 1.0), (1.0 - RIM_WIDTH, 1.0, 0.0, 1.0)] {
                let (min, max) = local(u0, u1, v0, v1, 0.0, RIM_HEIGHT);
                boxer(pages, stone, min, max, RIM_SHADE, &ALL_SIDES);
            }
        }
        let middle = (STEPS / 2) as f32;
        self.ground.insert(cell_key(x, y), if dir == UpDown::Down { -0.1 - middle * STEP_DROP } else { (middle + 1.0) * STEP_RISE });
        for i in 0..STEPS {
            let fi = i as f32;
            let top = if dir == UpDown::Down { -0.1 - fi * STEP_DROP } else { (fi + 1.0) * STEP_RISE };
            let bottom = if dir == UpDown::Down { PIT_BOTTOM } else { 0.0 };
            let shade = if dir == UpDown::Down { 1.0 - fi * 0.12 } else { 1.0 };
            let (min, max) = local(fi / STEPS as f32, (fi + 1.0) / STEPS as f32, 0.0, 1.0, bottom, top);
            boxer(pages, if dir == UpDown::Down { stone } else { cell.floor_tile }, min, max, shade, &[BoxSide::Top]);
            boxer(pages, stone, min, max, shade * 0.8, &walls);
        }
    }
}
