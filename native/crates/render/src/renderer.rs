//! `IsoRenderer` (the port of `render-iso-3d.ts`): the level's geometry in chunks of 8 by 8 cells,
//! built again only where the cells, the cut-away walls or what stands on them changed; the actors,
//! the doors and stairs and the layers over them; the torch lights; and the screen
//! effects.

use crate::actors::{actor_id, ActorEnv, ActorLayer, UprightPose, STRIKE_DELAY};
use crate::camera::IsoCamera;
use crate::cell_light::{CellLight, CellLights};
use crate::combat::{CombatLayer, ImpactKind};
use crate::features::{is_tile_feature, TileFeatures};
use crate::fog::FogLayer;
use crate::gpu::{Gpu, Material, DEPTH_FORMAT};
use crate::lights::{diorama, linear, IsoLights, IsoQuality, SurfaceDetail, QUALITY_MEDIUM};
use crate::liquids::{Liquids, FILMS};
use crate::loot::{Landing, LootLayer, LIGHT_REACH as LOOT_LIGHT_REACH};
use crate::materials::{door_metal, MaterialLayer};
use crate::memory::MemoryMask;
use crate::mesh::{GpuMesh, Point3, QuadBatch, ATLAS_INSET, SPRITE_LEAN};
use crate::mipmap::Mipmapper;
use crate::pages::{page_bind_group, Pages};
use crate::plan::{
    aim_colour, cut_away_cells, cut_away_focus, ground_basis, ground_point, is_pointable, is_shown, plan_scene, side, tile_frame, wall_height_at, GroundLayer,
    IsoAim, IsoRoute, IsoView, Side, Sprite, SpriteLayer, WallBlock, DEFAULT_VIEW, TORCH_HEIGHT,
};
use crate::post::{Post, PostFrame};
use crate::steps::StepLayer;
use crate::torch_light::TorchLightMap;
use crate::uniforms::{DrawBuffer, DrawUniform, FrameUniform, LightUniform, MAX_LIGHTS};
use crate::voxels::{layer_depth, VoxelChunk, VoxelKind, VoxelLayer, VoxelPart, VoxelStats};
use crate::{now_seconds, TileSource};
use glam::{Mat4, Vec3, Vec4};
use isorun_scene::{cell_key, has_items_under, key_to_xy, Camera, CellKey, Feature, Scene, SceneCell, SceneCursor, TileId, Visibility, DIR8_DX, DIR8_DY};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub use crate::actors::{ActorAction, ActorStep, PLAYER_ACTOR};

const LIFT_STEP: f32 = 0.002;
pub const CHUNK_CELLS: i32 = 8;
const HEAD_HEIGHT: f32 = 1.0;
const SHORE_SEARCH: i32 = 4;
const MAX_UPRIGHT_STRETCH: f32 = 2.5;
pub const TORCH_LIGHTS: usize = 8;
const CAMERA_DISTANCE: f32 = 60.0;
const PERSPECTIVE_DEPTH: f32 = 60.0;
const CAP_SHADE: f32 = 0.6;
const BEHIND_ACTORS: f32 = 0.1;
const PILE_COPIES: [(f32, f32, bool); 2] = [(-0.2, 0.08, true), (0.18, 0.14, false)];
const PILE_SCALE: f32 = 0.75;
const PILE_SHADE: f32 = 0.55;
const ROUTE_STEPS: usize = 64;
const AIM_LIFT: f32 = 0.03;
const SNAP_CELLS: i32 = 3;
const SNAP_SCREEN_CELLS: f32 = 1.5;
const LEVEL_ANGLE_STEP: f32 = 1.0 * std::f32::consts::PI / 180.0;
const DANGER_PULSE: f32 = 0.3;
const DANGER_PULSE_SPEED: f32 = 3.0;

/// Which textures a draw reads: a page of tiles, or one of the renderer's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PageRef {
    Page(u32),
    Cap,
    Door,
}

/// One mesh drawn with one material this frame.
#[derive(Clone)]
pub struct DrawItem {
    pub material: Material,
    pub page: PageRef,
    pub mesh: Arc<GpuMesh>,
    pub uniform: DrawUniform,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Torch {
    pub position: Vec3,
    /// The open cell the torch hangs over.
    pub cell: (i32, i32),
}

struct Chunk {
    key: u64,
    meshes: Vec<(Material, PageRef, Arc<GpuMesh>)>,
    torches: Vec<Torch>,
    glows: bool,
    voxels: Option<VoxelChunk>,
}

#[derive(Clone, Copy, Debug)]
struct CellArea {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

struct Built {
    cells: HashMap<CellKey, u64>,
    cut: HashSet<CellKey>,
    standing: HashMap<CellKey, u64>,
}

struct PendingImpact {
    time: f64,
    id: i64,
    action: ActorAction,
    from: Option<(i32, i32)>,
    token: Option<u64>,
}

struct Pipelines {
    samples: u32,
    cell_lights: u32,
    world: HashMap<Material, wgpu::RenderPipeline>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameCounts {
    pub draws: u32,
    pub triangles: u64,
}

/// What happened in a frame that the client acts on: blows shown, hops started, doors moved.
#[derive(Clone, Debug, Default)]
pub struct FrameEvents {
    /// The `token` of each `act` whose action showed this frame (a swing forward, a blow landing).
    pub shown: Vec<u64>,
    pub hops: Vec<ActorStep>,
    /// Doors that stand open or shut where they did not before: x, y, open.
    pub doors: Vec<(i32, i32, bool)>,
    /// Items a death dropped that touched the floor.
    pub landings: Vec<Landing>,
}

pub struct IsoRenderer {
    pub gpu: Gpu,
    pages: Option<Pages>,
    mipmapper: Mipmapper,
    post: Post,
    memory: MemoryMask,
    torch_light: TorchLightMap,
    cell_light: CellLight,
    cell_lights: CellLights,
    pipelines: Option<Pipelines>,
    frame_buffer: wgpu::Buffer,
    frame_group: wgpu::BindGroup,
    draws: DrawBuffer,
    noise: wgpu::Texture,
    cap_group: wgpu::BindGroup,
    door_group: wgpu::BindGroup,
    tiles: Option<Arc<dyn TileSource>>,
    scene: Option<Arc<Scene>>,
    scene_revision: u64,
    cam: Camera,
    view: IsoView,
    lights: IsoLights,
    quality: IsoQuality,
    detail: SurfaceDetail,
    aim: Option<IsoAim>,
    route: Option<IsoRoute>,
    cursor: Option<SceneCursor>,
    cut_away_reach: i32,
    centre_lift: f32,
    animation_speed: f64,
    danger: f32,
    chunks: HashMap<(i32, i32), Chunk>,
    sprite_chunks: HashMap<(i32, i32), Chunk>,
    sprites_view: u64,
    chunks_key: u64,
    chunk_pages: usize,
    built: Option<Built>,
    static_key: u64,
    measured: (u64, u64, u64),
    measured_scene: u64,
    focus_memo: Option<(u64, i32, Option<(i32, i32)>, Vec<(i32, i32)>)>,
    actors: ActorLayer,
    features: TileFeatures,
    fog: FogLayer,
    liquids: Liquids,
    materials: Option<MaterialLayer>,
    combat: CombatLayer,
    steps: StepLayer,
    loot: LootLayer,
    overlay: crate::overlay::Overlay,
    voxels: VoxelLayer,
    counts: FrameCounts,
    tile_torches: Vec<Torch>,
    tile_torches_version: u64,
    impacts: Vec<PendingImpact>,
    clock: f64,
    stop_until: f64,
    last_frame: f64,
    start: f64,
    width: f32,
    height: f32,
    dpr: f32,
    pub resolution: DynamicResolution,
    camera_view: Mat4,
    camera_proj: Mat4,
    camera_pos: Vec3,
    perspective: bool,
    events: FrameEvents,
    revision_counter: u64,
    /// whether a liquid lies on a cell in sight, by the scene's revision (for `animating`)
    liquid_sight: std::cell::Cell<(u64, bool)>,
    pub frame_number: u64,
}

fn hash_of<T: Hash>(v: &T) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

/// A cell as the geometry sees it: not its glyph, words, minimap colour, flash or cursor, and only
/// whether it was ever seen.
fn drawn_hash(c: &SceneCell) -> u64 {
    let mut h = DefaultHasher::new();
    (c.x, c.y, c.kind, c.visibility == Visibility::Unseen, c.occluder, c.floor_tile, c.wall_tile, c.wall_style).hash(&mut h);
    (c.face_tiles, c.ceiling_tile, c.feature_tile, &c.feature, c.stance, c.freestanding, c.beacon).hash(&mut h);
    (&c.underlays, &c.overlays, &c.wall_shadows, &c.shorelines, c.liquid, &c.wall_overlays, &c.translucent, &c.icons, c.cloud, c.trail).hash(&mut h);
    (c.flags.water, c.flags.lava, c.flags.excluded, c.flags.travel_trail, c.flags.new_stair, c.flags.magic_mapped).hash(&mut h);
    h.finish()
}

fn chunk_id(x: i32, y: i32) -> (i32, i32) {
    (x.div_euclid(CHUNK_CELLS), y.div_euclid(CHUNK_CELLS))
}

fn chunk_area(id: (i32, i32)) -> CellArea {
    CellArea { x0: (id.0 * CHUNK_CELLS) as f32, y0: (id.1 * CHUNK_CELLS) as f32, x1: ((id.0 + 1) * CHUNK_CELLS) as f32, y1: ((id.1 + 1) * CHUNK_CELLS) as f32 }
}

fn layer_signature(layers: &[isorun_scene::Layer]) -> Vec<(u32, u32, u32, u32)> {
    layers.iter().map(|l| (l.tile, l.ox.to_bits(), l.oy.to_bits(), l.ymax.map_or(u32::MAX, f32::to_bits))).collect()
}

/// What stands on each cell in the level's geometry: the items and features shown that are not actors.
fn standing_by_cell(scene: &Scene) -> HashMap<CellKey, u64> {
    let mut standing: HashMap<CellKey, u64> = HashMap::new();
    for b in &scene.billboards {
        if actor_id(b).is_some() || !is_shown(b, scene) {
            continue;
        }
        let key = cell_key(b.x, b.y);
        let sig = hash_of(&(standing.get(&key).copied(), b.kind, b.tile, b.lying, has_items_under(b), layer_signature(&b.layers)));
        standing.insert(key, sig);
    }
    standing
}

/// What stands in the level's geometry, and the actors in doorways, which lift the door's lintel.
fn statics_standing(scene: &Scene) -> u64 {
    let mut h = DefaultHasher::new();
    for b in &scene.billboards {
        if actor_id(b).is_none() {
            if is_shown(b, scene) {
                (b.kind, b.x, b.y, b.tile, b.lying, has_items_under(b), layer_signature(&b.layers)).hash(&mut h);
            }
        } else if matches!(scene.cell(b.x, b.y).and_then(|c| c.feature.as_ref()), Some(Feature::Door { .. })) {
            ("door", b.x, b.y).hash(&mut h);
        }
    }
    h.finish()
}

fn on_land(cell: &SceneCell) -> bool {
    cell.liquid.is_none_or(|l| FILMS.contains(&l.kind))
}

/// The floor tile of the nearest land to a water cell, laid under the drawn water.
fn shore_floor(scene: &Scene, x: i32, y: i32) -> Option<TileId> {
    for r in 1..=SHORE_SEARCH {
        for dy in -r..=r {
            for dx in -r..=r {
                if let Some(c) = scene.cell(x + dx, y + dy) {
                    if !c.occluder && on_land(c) {
                        return Some(c.floor_tile);
                    }
                }
            }
        }
    }
    None
}

/// Whether blood, dust and footprints show on a cell: open dry ground in sight.
pub fn ground_floor(scene: &Scene, x: i32, y: i32) -> bool {
    scene.cell(x, y).is_some_and(|c| !c.occluder && on_land(c) && c.visibility == Visibility::Visible)
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl IsoRenderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let gpu = Gpu::new(device, queue);
        let mipmapper = Mipmapper::new(&gpu.device);
        let post = Post::new(&gpu);
        let memory = MemoryMask::new(&gpu);
        let frame_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame"),
            size: std::mem::size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let noise = crate::fog::noise_texture(&gpu);
        let torch_light = TorchLightMap::new(&gpu);
        let cell_light = CellLight::new(&gpu);
        let frame_group = Self::frame_group(&gpu, &frame_buffer, &memory, &noise, &torch_light, &cell_light);
        let draws = DrawBuffer::new(&gpu.device, &gpu.draw_layout, 256);
        let (door_width, door_height, door_pixels) = door_pixels();
        let (cap, door) = (cap_texture(&gpu), gpu.texture_with("door", door_width, door_height, wgpu::TextureFormat::Rgba8UnormSrgb, &door_pixels));
        let blank = gpu.texture_with("blank", 1, 1, wgpu::TextureFormat::Rgba8Unorm, &[128, 128, 255, 0]);
        let door_normal = gpu.texture_with("door normal", door_width, door_height, wgpu::TextureFormat::Rgba8Unorm, &door_metal(&door_pixels));
        let cap_group = page_bind_group(&gpu.device, &gpu.page_layout, &gpu.nearest_sampler, &cap, &blank, &blank);
        let door_group = page_bind_group(&gpu.device, &gpu.page_layout, &gpu.page_sampler, &door, &door_normal, &blank);
        let fog = FogLayer::new(&gpu);
        let liquids = Liquids::new(&gpu);
        let combat = CombatLayer::new(&gpu);
        let steps = StepLayer::new(&gpu);
        let loot = LootLayer::new(&gpu);
        let overlay = crate::overlay::Overlay::new(&gpu);
        let features = TileFeatures::new();
        let lights = diorama();
        let mut r = IsoRenderer {
            gpu,
            pages: None,
            mipmapper,
            post,
            memory,
            torch_light,
            cell_light,
            cell_lights: CellLights::default(),
            pipelines: None,
            frame_buffer,
            frame_group,
            draws,
            noise,
            cap_group,
            door_group,
            tiles: None,
            scene: None,
            scene_revision: u64::MAX,
            cam: Camera::new(0, 0, 0),
            view: DEFAULT_VIEW,
            lights,
            quality: QUALITY_MEDIUM,
            detail: SurfaceDetail::High,
            aim: None,
            route: None,
            cursor: None,
            cut_away_reach: 0,
            centre_lift: 0.0,
            animation_speed: 1.5,
            danger: 0.0,
            chunks: HashMap::new(),
            sprite_chunks: HashMap::new(),
            sprites_view: 0,
            chunks_key: 0,
            chunk_pages: 0,
            built: None,
            static_key: 0,
            measured: (0, 0, 0),
            measured_scene: u64::MAX,
            focus_memo: None,
            actors: ActorLayer::default(),
            features,
            fog,
            liquids,
            materials: None,
            combat,
            steps,
            loot,
            overlay,
            voxels: VoxelLayer::default(),
            counts: FrameCounts::default(),
            tile_torches: Vec::new(),
            tile_torches_version: 0,
            impacts: Vec::new(),
            clock: 0.0,
            stop_until: 0.0,
            last_frame: 0.0,
            start: now_seconds(),
            width: 1.0,
            height: 1.0,
            dpr: 1.0,
            resolution: DynamicResolution::default(),
            camera_view: Mat4::IDENTITY,
            camera_proj: Mat4::IDENTITY,
            camera_pos: Vec3::ZERO,
            perspective: false,
            events: FrameEvents::default(),
            revision_counter: 0,
            liquid_sight: std::cell::Cell::new((u64::MAX, false)),
            frame_number: 0,
        };
        r.apply_lights();
        r
    }

    fn frame_group(
        gpu: &Gpu,
        buffer: &wgpu::Buffer,
        memory: &MemoryMask,
        noise: &wgpu::Texture,
        torch_light: &TorchLightMap,
        cell_light: &CellLight,
    ) -> wgpu::BindGroup {
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame"),
            layout: &gpu.frame_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&view(&memory.shade)) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&view(&memory.wetness)) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::Sampler(&gpu.nearest_sampler) },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::Sampler(&gpu.linear_sampler) },
                wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(&view(noise)) },
                wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::Sampler(&gpu.repeat_sampler) },
                wgpu::BindGroupEntry { binding: 10, resource: wgpu::BindingResource::TextureView(&view(&torch_light.texture)) },
                wgpu::BindGroupEntry { binding: 11, resource: cell_light.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 12, resource: wgpu::BindingResource::TextureView(&view(cell_light.gi_texture())) },
                wgpu::BindGroupEntry { binding: 13, resource: wgpu::BindingResource::TextureView(&view(cell_light.sight_texture())) },
            ],
        })
    }

    // ------------------------------------------------------------------ the renderer's inputs

    pub fn set_tiles(&mut self, tiles: Arc<dyn TileSource>) {
        self.tiles = Some(tiles);
        self.rebuild_pages();
    }

    /// A scene with the same revision as the last one is the same scene.
    pub fn set_scene(&mut self, scene: Arc<Scene>) {
        if self.scene.as_ref().is_some_and(|s| Arc::ptr_eq(s, &scene)) && scene.revision == self.scene_revision {
            return;
        }
        self.scene_revision = scene.revision;
        if let (true, Some(tiles)) = (self.voxels.on(), &self.tiles) {
            self.voxels.walls_broken(self.scene.as_deref(), &scene, tiles.as_ref());
        }
        self.memory.update(&self.gpu, &scene);
        self.scene = Some(scene);
        self.revision_counter += 1;
        self.sync_actors();
    }

    /// The player is on another level: the blood, the dust and the footprints of the last one go.
    pub fn new_level(&mut self) {
        self.combat.clear();
        self.steps.clear();
        self.loot.clear();
        self.voxels.new_level();
        self.cell_light.snap();
    }

    pub fn set_camera(&mut self, cam: Camera) {
        self.cam = cam;
    }

    pub fn set_cursor(&mut self, cursor: Option<SceneCursor>) {
        self.cursor = cursor;
    }

    pub fn set_view(&mut self, view: IsoView) {
        self.view = view;
    }

    /// The camera and the view from an `IsoCamera` in one call.
    pub fn follow(&mut self, camera: &IsoCamera) {
        self.cam = camera.camera();
        self.view = camera.view();
    }

    pub fn lights(&self) -> &IsoLights {
        &self.lights
    }

    pub fn set_lights(&mut self, lights: IsoLights) {
        self.lights = lights;
        self.apply_lights();
    }

    pub fn set_quality(&mut self, quality: IsoQuality) {
        self.quality = quality;
        self.post.set_ao_resolution(quality.ao_resolution);
        self.post.ao_samples = quality.ao_samples;
        self.resolution.reset();
        self.voxels.configure(self.voxels.on(), quality.voxel_cells);
    }

    pub fn set_voxels(&mut self, on: bool) {
        self.voxels.configure(on, self.quality.voxel_cells);
    }

    pub fn voxel_stats(&self) -> VoxelStats {
        self.voxels.stats
    }

    pub fn frame_counts(&self) -> FrameCounts {
        self.counts
    }

    pub fn quality(&self) -> IsoQuality {
        self.quality
    }

    pub fn set_surface_detail(&mut self, detail: SurfaceDetail) {
        self.detail = detail;
    }

    pub fn set_cell_lights(&mut self, on: CellLights) {
        self.cell_lights = on;
    }

    pub fn set_materials(&mut self, on: bool) {
        if on != self.materials.is_some() {
            self.materials = on.then(|| MaterialLayer::new(&self.gpu));
        }
    }

    pub fn set_aim(&mut self, aim: Option<IsoAim>) {
        self.aim = aim;
    }

    pub fn set_route(&mut self, route: Option<IsoRoute>) {
        self.route = route;
    }

    pub fn set_centre_lift(&mut self, px: f32) {
        self.centre_lift = px.round();
    }

    pub fn set_cut_away_reach(&mut self, cells: i32) {
        self.cut_away_reach = cells.max(0);
    }

    pub fn set_animation_speed(&mut self, speed: f64) {
        self.animation_speed = speed.max(0.1);
    }

    pub fn set_danger(&mut self, amount: f32) {
        self.danger = amount.clamp(0.0, 1.0);
    }

    /// Where the player is drawn now, in cells: between two cells during a hop.
    pub fn player_shown(&self) -> Option<(f32, f32)> {
        self.actors.shown_cell_of(PLAYER_ACTOR)
    }

    /// Play an action on the actor with this id (the player is `PLAYER_ACTOR`). `token` comes back
    /// in `FrameEvents::shown` when the action shows: an attack's swing, the moment a blow lands.
    pub fn act(&mut self, id: i64, action: ActorAction, target: Option<(i32, i32)>, token: Option<u64>) {
        if action == ActorAction::Attack {
            self.actors.act(id, action, self.clock, target);
            if token.is_some() {
                self.impacts.push(PendingImpact { time: self.clock + STRIKE_DELAY, id, action, from: target, token });
            }
            return;
        }
        let time = self.actors.impact_from(target, self.clock);
        if action == ActorAction::Death && id != PLAYER_ACTOR {
            if let Some(cell) = self.actors.cell_of(id) {
                self.loot.death(cell, time);
            }
        }
        self.actors.act(id, action, time, target);
        self.impacts.push(PendingImpact { time, id, action, from: target, token });
    }

    /// The floor cell under a point of the view, in logical pixels; over a wall, the fog or the
    /// void, the floor cell nearest to the point on the screen.
    pub fn pick(&mut self, px: f32, py: f32) -> Option<CellKey> {
        let scene = self.scene.clone()?;
        self.place_camera();
        let inv = (self.camera_proj * self.camera_view).inverse();
        let (nx, ny) = (px / self.width * 2.0 - 1.0, -(py / self.height) * 2.0 + 1.0);
        let near = inv.project_point3(Vec3::new(nx, ny, 0.0));
        let far = inv.project_point3(Vec3::new(nx, ny, 1.0));
        let dir = (far - near).normalize();
        let (gx, gy) = ground_point(near.to_array(), dir.to_array())?;
        let (gx, gy) = (gx.floor() as i32, gy.floor() as i32);
        if is_pointable(scene.cell(gx, gy)) {
            return Some(cell_key(gx, gy));
        }
        let mut best: Option<(CellKey, f32)> = None;
        for y in gy - SNAP_CELLS..=gy + SNAP_CELLS {
            for x in gx - SNAP_CELLS..=gx + SNAP_CELLS {
                if !is_pointable(scene.cell(x, y)) {
                    continue;
                }
                let (sx, sy) = self.project(Vec3::new(x as f32 + 0.5, 0.0, y as f32 + 0.5));
                let d = ((sx - px).powi(2) + (sy - py).powi(2)).sqrt();
                if best.is_none_or(|b| d < b.1) {
                    best = Some((cell_key(x, y), d));
                }
            }
        }
        best.filter(|b| b.1 <= SNAP_SCREEN_CELLS * self.view.zoom).map(|b| b.0)
    }

    fn project(&self, p: Vec3) -> (f32, f32) {
        let c = (self.camera_proj * self.camera_view).project_point3(p);
        ((c.x + 1.0) / 2.0 * self.width, (1.0 - c.y) / 2.0 * self.height)
    }

    /// Where the middle of a cell, at head height, is on the view, in logical pixels.
    pub fn screen_of(&self, x: f32, y: f32) -> (f32, f32) {
        self.project(Vec3::new(x + 0.5, HEAD_HEIGHT, y + 0.5))
    }

    /// The view's size in logical pixels and the device pixels per logical pixel.
    pub fn resize(&mut self, width: f32, height: f32, dpr: f32) {
        let (width, height) = (width.max(1.0), height.max(1.0));
        // the view is sized every frame: only a real change starts the dynamic resolution over
        if (width, height, dpr) == (self.width, self.height, self.dpr) {
            return;
        }
        self.width = width;
        self.height = height;
        self.dpr = dpr;
        self.resolution.reset();
    }

    /// Show one of the screen effects' images alone (post.wgsl `debug`: 1 the glow, 2 the
    /// occlusion, 3 the normals, 4 the blur), 0 for the frame; natively also `ISORUN_DEBUG_VIEW`.
    pub fn set_debug_view(&mut self, view: u32) {
        self.post.debug = view;
    }

    /// The size the frame is drawn at, in device pixels.
    pub fn render_size(&self) -> (u32, u32) {
        let ratio = self.pixel_ratio();
        (((self.width * ratio).round() as u32).max(1), ((self.height * ratio).round() as u32).max(1))
    }

    fn pixel_ratio(&self) -> f32 {
        self.dpr.min(self.quality.pixel_ratio) * if self.quality.adaptive { self.resolution.scale } else { 1.0 }
    }

    /// Whether the next frames show motion that is more than the ambient (the torches' flicker,
    /// the actors' breath, the fog's drift): a hop or an action under way, a blow still to land,
    /// the hit stop, blood, sparks, the shake or the flash, dust of a step or footprints fading,
    /// a door swinging, the fog blending over a new explored area, the torches' dust, or a liquid
    /// in sight. A screen that draws only on demand keeps drawing every frame while it is true,
    /// and otherwise may wait (game.ts `awake`, which keeps the iso view always going; natively
    /// an idle tick shows the ambient instead, for the batteries). The camera's own easing is
    /// `IsoCamera::update`'s to say.
    pub fn animating(&self) -> bool {
        self.animating_why().is_some()
    }

    /// What keeps `animating` true, for the perf log: the first reason found.
    pub fn animating_why(&self) -> Option<&'static str> {
        let now = now_seconds();
        let reasons: [(&'static str, &dyn Fn() -> bool); 12] = [
            ("impact", &|| !self.impacts.is_empty()),
            ("hit stop", &|| now < self.stop_until),
            // the low hit points' red edge pulses
            ("danger", &|| self.danger > 0.0),
            ("actors", &|| self.actors.animating(self.clock)),
            ("combat", &|| self.combat.animating()),
            ("steps", &|| self.steps.animating()),
            ("loot", &|| self.loot.animating()),
            ("doors", &|| self.features.animating()),
            ("fog fade", &|| self.fog.fading()),
            ("liquid", &|| self.liquids.drawn() && self.liquid_in_sight()),
            ("voxels", &|| self.voxels.animating()),
            ("light fade", &|| self.cell_lights.any() && self.cell_light.easing()),
        ];
        reasons.iter().find(|(_, f)| f()).map(|(n, _)| *n)
    }

    fn liquid_in_sight(&self) -> bool {
        let (rev, seen) = self.liquid_sight.get();
        if rev == self.revision_counter {
            return seen;
        }
        let seen = self.scene.as_ref().is_some_and(|s| s.cells.values().any(|c| c.liquid.is_some() && c.visibility == Visibility::Visible));
        self.liquid_sight.set((self.revision_counter, seen));
        seen
    }

    /// What happened in the last frame: blows shown, hops started, doors moved.
    pub fn take_events(&mut self) -> FrameEvents {
        std::mem::take(&mut self.events)
    }

    // ------------------------------------------------------------------ settings applied

    fn rebuild_pages(&mut self) {
        if self.tiles.is_none() {
            return;
        }
        self.pages = Some(Pages::new(self.lights.paint, self.gpu.max_texture));
        self.voxels.pages_changed();
        self.invalidate();
    }

    fn invalidate(&mut self) {
        self.static_key = 0;
        self.chunks_key = 0;
        self.features.invalidate();
        self.sync_actors();
    }

    fn sync_actors(&mut self) {
        let (Some(scene), Some(tiles)) = (self.scene.clone(), self.tiles.clone()) else { return };
        let Some(pages) = self.pages.as_mut() else { return };
        let mut env = ActorEnv { gpu: &self.gpu, pages, tiles: tiles.as_ref() };
        self.actors.sync(&scene, self.clock, &mut env);
        self.loot.sync(&scene, &mut env);
    }

    fn apply_lights(&mut self) {
        if self.pages.as_ref().is_some_and(|p| !p.same_paint(&self.lights.paint)) {
            self.rebuild_pages();
        }
        self.fog.configure(&self.lights.fog);
        self.liquids.configure(&self.gpu, &self.lights);
        self.combat.configure(self.lights.combat);
        self.steps.configure(self.lights.steps);
    }

    fn level_yaw(&self) -> f32 {
        ((self.cam.yaw as f32) / LEVEL_ANGLE_STEP).round() * LEVEL_ANGLE_STEP
    }

    fn level_pitch(&self) -> f32 {
        ((self.cam.pitch as f32) / LEVEL_ANGLE_STEP).round() * LEVEL_ANGLE_STEP
    }

    /// How upright sprites stand: stretched up so the view's angle does not flatten them, and,
    /// when the camera is too high for that, leaned back toward it.
    fn upright(&self, pitch: f32) -> UprightPose {
        let elevation = -pitch;
        UprightPose { stretch: MAX_UPRIGHT_STRETCH.min(1.0 / elevation.cos().max(0.01)), lean: (elevation - (1.0 / MAX_UPRIGHT_STRETCH).acos()).max(0.0) }
    }

    /// A count that goes up when the scene changes what the level's geometry is built from.
    fn statics_revision(&mut self, scene: &Scene) -> u64 {
        if self.measured_scene == self.revision_counter {
            return self.measured.2;
        }
        let standing = statics_standing(scene);
        let mut cells: Vec<(CellKey, u64)> = scene.cells.iter().map(|(k, c)| (*k, drawn_hash(c))).collect();
        cells.sort_unstable();
        let cells_hash = hash_of(&cells);
        let changed = standing != self.measured.0 || cells_hash != self.measured.1;
        self.measured = (standing, cells_hash, self.measured.2 + changed as u64);
        self.measured_scene = self.revision_counter;
        self.measured.2
    }

    fn focus(&mut self, scene: &Scene) -> Vec<(i32, i32)> {
        let target = self.route.as_ref().map(|r| r.target).or(self.cursor.filter(|c| c.mode != isorun_scene::CursorMode::Map).map(|c| (c.x, c.y)));
        let extra = target.filter(|t| is_pointable(scene.cell(t.0, t.1)));
        if let Some((rev, reach, e, focus)) = &self.focus_memo {
            if *rev == self.revision_counter && *reach == self.cut_away_reach && *e == extra {
                return focus.clone();
            }
        }
        let focus = cut_away_focus(scene, extra, self.cut_away_reach);
        self.focus_memo = Some((self.revision_counter, self.cut_away_reach, extra, focus.clone()));
        focus
    }

    fn ground_tile(&self, scene: &Scene, g: &GroundLayer) -> TileId {
        let drawn = scene.cell(g.x, g.y).and_then(|c| c.liquid).is_some_and(|l| {
            !FILMS.contains(&l.kind) && if l.kind == isorun_scene::LiquidKind::Lava { self.lights.lava.enabled } else { self.lights.water.enabled }
        });
        if g.lift == 0 && drawn {
            shore_floor(scene, g.x, g.y).unwrap_or(g.tile)
        } else {
            g.tile
        }
    }
}

include!("renderer_build.rs");
include!("renderer_frame.rs");

/// Lowers the share of the pixels the view draws while frames come late, and raises it again once
/// they have been on time for a while (the port of `dynamic-resolution.ts`).
#[derive(Clone, Debug)]
pub struct DynamicResolution {
    pub scale: f32,
    intervals: Vec<f32>,
    screen: f32,
    second: (u32, u32, f32),
    calm_seconds: u32,
    late_seconds: u32,
    trial: u32,
    wait: u32,
}

impl Default for DynamicResolution {
    fn default() -> Self {
        DynamicResolution { scale: 1.0, intervals: Vec::new(), screen: f32::INFINITY, second: (0, 0, 0.0), calm_seconds: 0, late_seconds: 0, trial: 0, wait: 0 }
    }
}

impl DynamicResolution {
    /// A frame came `ms` after the one before; returns whether the scale changed.
    pub fn frame(&mut self, ms: f32) -> bool {
        if ms <= 0.0 || ms > 100.0 {
            return false;
        }
        self.intervals.push(ms);
        if self.intervals.len() > 240 {
            self.intervals.remove(0);
        }
        self.second.0 += 1;
        self.second.2 += ms;
        if ms > self.screen * 1.4 {
            self.second.1 += 1;
        }
        if self.second.2 < 1000.0 {
            return false;
        }
        let mut sorted = self.intervals.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        self.screen = sorted[(sorted.len() / 10).min(sorted.len() - 1)];
        let late = self.second.1 as f32 / self.second.0 as f32;
        self.second = (0, 0, 0.0);
        self.wait = self.wait.saturating_sub(1);
        self.trial = self.trial.saturating_sub(1);
        if late > 0.25 {
            self.calm_seconds = 0;
            self.late_seconds += 1;
            if self.late_seconds < 2 && self.trial == 0 {
                return false;
            }
            self.late_seconds = 0;
            if self.trial > 0 {
                self.wait = 60;
            }
            self.trial = 0;
            return self.set(self.scale - 0.1);
        }
        self.late_seconds = 0;
        self.calm_seconds = if late < 0.02 { self.calm_seconds + 1 } else { 0 };
        if self.calm_seconds < 10 || self.wait > 0 || self.scale >= 1.0 {
            return false;
        }
        self.calm_seconds = 0;
        self.trial = 2;
        self.set(self.scale + 0.1)
    }

    pub fn reset(&mut self) {
        *self = DynamicResolution::default();
    }

    fn set(&mut self, scale: f32) -> bool {
        let next = (scale.clamp(0.7, 1.0) * 10.0).round() / 10.0;
        if next == self.scale {
            return false;
        }
        self.scale = next;
        self.second = (0, 0, 0.0);
        true
    }
}

/// A faint stone grain for wall tops, mid grey so vertex colours set the hue.
fn cap_texture(gpu: &Gpu) -> wgpu::Texture {
    let size = 32u32;
    let mut random = seeded(11);
    let mut px = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let edge = x == 0 || y == 0 || x == size - 1 || y == size - 1;
            let v = if edge { 150 } else { (205.0 + (random() - 0.5) * 40.0).round() as u8 };
            let i = ((y * size + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    gpu.texture_with("cap", size, size, wgpu::TextureFormat::Rgba8UnormSrgb, &px)
}

/// A door leaf in the DCSS manner: dark planks, two iron bands with rivets and a ring, 32 by 48.
fn door_pixels() -> (u32, u32, Vec<u8>) {
    let (w, h) = (32usize, 48usize);
    let mut px = vec![0u8; w * h * 4];
    let mut random = seeded(7);
    let mut put = |x: usize, y: usize, c: [f32; 3], px: &mut Vec<u8>| {
        let i = (y * w + x) * 4;
        px[i..i + 4].copy_from_slice(&[c[0].clamp(0.0, 255.0) as u8, c[1].clamp(0.0, 255.0) as u8, c[2].clamp(0.0, 255.0) as u8, 255]);
    };
    let planks = [[92.0, 60.0, 34.0], [80.0, 52.0, 30.0], [88.0, 57.0, 32.0], [76.0, 49.0, 28.0]];
    for y in 0..h {
        for x in 0..w {
            let [r, g, b] = planks[x / 8];
            let a = random();
            let grain = (a - 0.5) * 14.0 + if x % 8 == 3 && random() < 0.6 { -10.0 } else { 0.0 };
            let c = if x % 8 == 0 { [38.0, 24.0, 14.0] } else { [r + grain, g + grain, b + grain * 0.6] };
            put(x, y, c, &mut px);
        }
    }
    for band in [7usize, 38] {
        for x in 0..w {
            put(x, band, [112.0, 112.0, 120.0], &mut px);
            put(x, band + 1, [72.0, 72.0, 80.0], &mut px);
            put(x, band + 2, [46.0, 46.0, 52.0], &mut px);
        }
        for x in (4..w).step_by(8) {
            put(x, band + 1, [150.0, 150.0, 158.0], &mut px);
        }
    }
    for a in 0..16 {
        let angle = a as f32 / 16.0 * std::f32::consts::TAU;
        put((24.0 + angle.cos() * 2.5).round() as usize, (25.0 + angle.sin() * 2.5).round() as usize, [130.0, 130.0, 138.0], &mut px);
    }
    for x in 0..w {
        put(x, 0, [30.0, 18.0, 10.0], &mut px);
        put(x, h - 1, [30.0, 18.0, 10.0], &mut px);
    }
    for y in 0..h {
        put(0, y, [30.0, 18.0, 10.0], &mut px);
        put(w - 1, y, [30.0, 18.0, 10.0], &mut px);
    }
    (w as u32, h as u32, px)
}

pub fn seeded(seed: u32) -> impl FnMut() -> f32 {
    let mut state = seed;
    move || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        state as f32 / 4294967296.0
    }
}

#[allow(dead_code)]
fn unused(
    _: Point3,
    _: &QuadBatch,
    _: f32,
    _: f32,
    _: &[Sprite],
    _: &[SpriteLayer],
    _: &[WallBlock],
    _: Side,
    _: Vec4,
    _: &[LightUniform; MAX_LIGHTS],
    _: [i32; 8],
    _: [i32; 8],
    _: ImpactKind,
) {
    let _ = (ATLAS_INSET, SPRITE_LEAN, ROUTE_STEPS, AIM_LIFT, DEPTH_FORMAT, DIR8_DX, DIR8_DY);
    let _ = (aim_colour, cut_away_cells, ground_basis, plan_scene, side, tile_frame, wall_height_at, key_to_xy, is_tile_feature, linear, smoothstep);
    let _ = (
        CAMERA_DISTANCE,
        PERSPECTIVE_DEPTH,
        CAP_SHADE,
        BEHIND_ACTORS,
        PILE_COPIES,
        PILE_SCALE,
        PILE_SHADE,
        DANGER_PULSE,
        DANGER_PULSE_SPEED,
        TORCH_HEIGHT,
        LIFT_STEP,
    );
}
