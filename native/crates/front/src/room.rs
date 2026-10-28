//! The room behind the front end (the port of `room/*`): a place of a real game, one a visit,
//! drawn by the game's own renderer around the place's eye. The camera turns slowly round it; the
//! right stick and a drag on the scenery turn it faster.
//!
//! A place is a moment of a real game frozen to a file (`public/room/places/<id>.json`) with the
//! few tiles it is drawn from packed beside it (`<id>.png`), read back here into the `Scene` the
//! renderer draws. The files come with the app.

use eframe::egui_wgpu::RenderState;
use isorun_render::{AtlasImage, IsoCamera, TileSource};
use isorun_scene::*;
use isorun_ui::view3d::View3d;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// How fast the camera turns round the place by itself, and at full right stick, in degrees a second.
const ORBIT_DEGREES_PER_SECOND: f32 = 3.0;
/// The room turning on its own is drawn this often: at 3 degrees a second a frame moves it a
/// tenth of a degree, which no one sees at the display's rate, and a phone's battery feels.
/// A drag, the stick, the fade in and the renderer's own motion draw every frame.
const ORBIT_FRAME_SECONDS: f64 = 1.0 / 30.0;
/// With the turning stopped (reduced motion) the room is drawn again only this often, for the
/// torches' flicker and the fog's drift.
const STILL_FRAME_SECONDS: f64 = 0.25;
/// How long a drag keeps every frame coming after its last move (the camera eases after it).
const DRAG_SETTLE_SECONDS: f64 = 0.6;
const STICK_DEGREES_PER_SECOND: f32 = 90.0;
/// How far a drag across the scenery turns the camera, in degrees per point.
pub const DRAG_DEGREES_PER_PIXEL: f32 = 0.3;
/// A press on the scenery is a look once it has moved this far, in points.
pub const DRAG_SLOP: f32 = 6.0;
/// How long the room takes to fade in once its first frame is drawn.
const FADE_SECONDS: f64 = 0.6;

/// The veil laid over a place so the menus read on it (styles.css `.room-veil`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tint {
    Depths,
    Abyss,
    Lair,
    Dungeon,
    Shoals,
    Zot,
    Spider,
}

impl Tint {
    fn of(s: &str) -> Option<Tint> {
        Some(match s {
            "depths" => Tint::Depths,
            "abyss" => Tint::Abyss,
            "lair" => Tint::Lair,
            "dungeon" => Tint::Dungeon,
            "shoals" => Tint::Shoals,
            "zot" => Tint::Zot,
            "spider" => Tint::Spider,
            _ => return None,
        })
    }

    /// The veil's gradient, top to bottom: (stop, colour) as `.room-veil[data-tint]` sets it.
    pub fn veil(tint: Option<Tint>) -> [(f32, egui::Color32); 4] {
        let dark = |a: f32| egui::Color32::from_rgba_unmultiplied(7, 7, 11, (a * 255.0) as u8);
        let mid = |r: u8, g: u8, b: u8, a: f32| egui::Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0) as u8);
        let (top, m, bottom) = match tint {
            None => (dark(0.6), dark(0.3), dark(0.8)),
            Some(Tint::Depths) => (dark(0.7), mid(30, 18, 12, 0.45), dark(0.85)),
            Some(Tint::Lair) => (dark(0.7), mid(26, 30, 8, 0.45), dark(0.85)),
            Some(Tint::Dungeon) => (dark(0.7), mid(12, 22, 14, 0.5), dark(0.85)),
            Some(Tint::Abyss) => (dark(0.7), mid(18, 12, 34, 0.4), dark(0.85)),
            Some(Tint::Shoals) => (dark(0.7), mid(10, 22, 30, 0.45), dark(0.85)),
            Some(Tint::Zot) => (dark(0.7), mid(30, 10, 26, 0.45), dark(0.85)),
            Some(Tint::Spider) => (dark(0.7), mid(22, 16, 26, 0.5), dark(0.85)),
        };
        [(0.0, top), (0.3, m), (0.7, m), (1.0, bottom)]
    }
}

/// A place the front end stands in (`places.json`).
#[derive(Clone, Debug, PartialEq)]
pub struct PlaceEntry {
    pub id: String,
    /// Where the game stood, for the room's label.
    pub place: String,
    pub tint: Option<Tint>,
    /// The front screen stands in it, in its turn.
    pub cycle: bool,
}

const PLACES_JSON: &str = include_str!("../../../assets/room/places.json");

macro_rules! place_files {
    ($($id:literal),*) => {
        /// A place's file and its atlas, by id: they come with the app.
        pub fn place_files(id: &str) -> Option<(&'static str, &'static [u8])> {
            match id {
                $($id => Some((
                    include_str!(concat!("../../../assets/room/places/", $id, ".json")),
                    include_bytes!(concat!("../../../assets/room/places/", $id, ".png")),
                )),)*
                _ => None,
            }
        }
    };
}
place_files!("lair-grove", "abyss", "zot-4", "necropolis", "shoals", "d1-fountains", "elven-halls", "zot-crystal", "orc-mines", "vaults");

pub fn places() -> Vec<PlaceEntry> {
    let list: Vec<Value> = serde_json::from_str(PLACES_JSON).unwrap_or_default();
    list.iter()
        .filter_map(|p| {
            Some(PlaceEntry {
                id: p.get("id")?.as_str()?.to_string(),
                place: p.get("place").and_then(Value::as_str).unwrap_or("").to_string(),
                tint: p.get("tint").and_then(Value::as_str).and_then(Tint::of),
                cycle: p.get("cycle").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

pub fn place_by_id(id: &str) -> Option<PlaceEntry> {
    places().into_iter().find(|p| p.id == id)
}

const TURN_KEY: &str = "isorun.roomPlace";

/// The place after `last` round the cycle; a place not in it starts the cycle at `random`.
pub fn next_in_cycle(cycle: &[PlaceEntry], last: Option<&str>, random: f64) -> PlaceEntry {
    let n = cycle.len() as i64;
    let at = last.and_then(|l| cycle.iter().position(|p| p.id == l)).map(|i| i as i64).unwrap_or((random * n as f64).floor() as i64 - 1);
    cycle[((at + 1).rem_euclid(n)) as usize].clone()
}

/// The place this visit of the front screen stands in: the one after last visit's, round the
/// cycle. Remembered on the device only as a convenience.
pub fn next_home_place() -> PlaceEntry {
    let cycle: Vec<PlaceEntry> = places().into_iter().filter(|p| p.cycle).collect();
    let last = isorun_client::platform::storage::get_raw(TURN_KEY).map(|v| serde_json::from_str::<String>(&v).unwrap_or(v));
    let next = next_in_cycle(&cycle, last.as_deref(), crate::splash::random());
    isorun_client::platform::storage::set_raw(TURN_KEY, Some(&next.id));
    next
}

// ------------------------------------------------------------------ the place file

/// A place's own atlas: the tiles its level names, packed in one image, by the ids its scene uses.
pub struct PlaceTiles {
    rects: HashMap<TileId, TileRect>,
    image: Arc<AtlasImage>,
}

pub const PLACE_ATLAS: &str = "room";

impl TileSource for PlaceTiles {
    fn tile(&self, id: TileId) -> Option<TileRect> {
        self.rects.get(&id).cloned()
    }
    fn atlas(&self, name: &str) -> Option<Arc<AtlasImage>> {
        (name == PLACE_ATLAS).then(|| self.image.clone())
    }
}

/// A place loaded: the renderer's tiles, the level, and the cell the eye stands on.
pub struct LoadedPlace {
    pub id: String,
    pub tiles: Arc<PlaceTiles>,
    pub scene: Scene,
    pub eye: (i32, i32),
}

fn num(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

fn int(v: &Value, k: &str) -> i32 {
    num(v, k) as i32
}

fn tile_of(v: &Value, k: &str) -> Option<TileId> {
    v.get(k).and_then(Value::as_u64).map(|t| t as TileId)
}

fn tiles_of(v: &Value, k: &str) -> Vec<TileId> {
    v.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|t| t as TileId).collect()).unwrap_or_default()
}

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn updown(s: Option<&str>) -> UpDown {
    if s == Some("up") {
        UpDown::Up
    } else {
        UpDown::Down
    }
}

fn feature_of(f: &Value) -> Option<Feature> {
    let opt = |k: &str| str_of(f, k).map(str::to_string);
    let flag = |k: &str| f.get(k).and_then(Value::as_bool).unwrap_or(false);
    Some(match str_of(f, "type")? {
        "stairs" => Feature::Stairs { dir: updown(str_of(f, "dir")), branch: flag("branch"), exit: flag("exit") },
        "hatch" => Feature::Hatch { dir: updown(str_of(f, "dir")) },
        "door" => Feature::Door {
            state: match str_of(f, "state") {
                Some("open") => DoorState::Open,
                Some("runed") => DoorState::Runed,
                Some("sealed") => DoorState::Sealed,
                _ => DoorState::Closed,
            },
        },
        "altar" => Feature::Altar { god: opt("god") },
        "shop" => Feature::Shop,
        "fountain" => Feature::Fountain { kind: opt("kind").unwrap_or_default() },
        "portal" => Feature::Portal,
        "transporter" => Feature::Transporter,
        "trap" => Feature::Trap { kind: opt("kind") },
        _ => Feature::Other { name: opt("name").unwrap_or_default() },
    })
}

fn cell_of(c: &Value) -> SceneCell {
    let flags = c.get("flags").cloned().unwrap_or(Value::Null);
    let flag = |k: &str| flags.get(k).and_then(Value::as_bool).unwrap_or(false);
    let feature = c.get("feature").and_then(feature_of);
    // how a feature stands is the builder's call of today, not of the day the place was frozen; an
    // `other` feature is known by its name, which the file does not keep, so it keeps its stance
    let written = match str_of(c, "stance") {
        Some("upright") => Some(Stance::Upright),
        Some("lying") => Some(Stance::Lying),
        Some("decal") => Some(Stance::Decal),
        _ => None,
    };
    let stance = match (&feature, str_of(c, "kind")) {
        (Some(f), Some("feature")) if !matches!(f, Feature::Other { .. }) => Some(isorun_scene_webtiles::stance_for(None, Some(f))),
        _ => written,
    };
    SceneCell {
        x: int(c, "x"),
        y: int(c, "y"),
        kind: match str_of(c, "kind") {
            Some("floor") => CellKind::Floor,
            Some("wall") => CellKind::Wall,
            Some("door") => CellKind::Door,
            Some("feature") => CellKind::Feature,
            _ => CellKind::Unknown,
        },
        visibility: match str_of(c, "visibility") {
            Some("visible") => Visibility::Visible,
            Some("remembered") => Visibility::Remembered,
            _ => Visibility::Unseen,
        },
        occluder: c.get("occluder").and_then(Value::as_bool).unwrap_or(false),
        floor_tile: tile_of(c, "floorTile").unwrap_or(0),
        wall_tile: tile_of(c, "wallTile"),
        wall_style: match str_of(c, "wallStyle") {
            Some("solid") => Some(WallStyle::Solid),
            Some("transparent") => Some(WallStyle::Transparent),
            _ => None,
        },
        face_tiles: c.get("faceTiles").map(|f| FaceTiles { n: tile_of(f, "n"), s: tile_of(f, "s"), w: tile_of(f, "w"), e: tile_of(f, "e") }),
        ceiling_tile: tile_of(c, "ceilingTile"),
        feature_tile: tile_of(c, "featureTile"),
        feature,
        stance,
        freestanding: c.get("freestanding").and_then(Value::as_bool).unwrap_or(false),
        beacon: match str_of(c, "beacon") {
            Some("down") => Some(Beacon::Down),
            Some("up") => Some(Beacon::Up),
            Some("portal") => Some(Beacon::Portal),
            _ => None,
        },
        underlays: tiles_of(c, "underlays"),
        overlays: tiles_of(c, "overlays"),
        wall_shadows: tiles_of(c, "wallShadows"),
        shorelines: tiles_of(c, "shorelines"),
        liquid: c.get("liquid").and_then(|l| {
            let kind = match str_of(l, "kind")? {
                "water" => LiquidKind::Water,
                "murky" => LiquidKind::Murky,
                "starwater" => LiquidKind::Starwater,
                "bog" => LiquidKind::Bog,
                "mud" => LiquidKind::Mud,
                "slime" => LiquidKind::Slime,
                "acid" => LiquidKind::Acid,
                "lava" => LiquidKind::Lava,
                _ => return None,
            };
            Some(Liquid { kind, deep: l.get("deep").and_then(Value::as_bool).unwrap_or(false) })
        }),
        wall_overlays: tiles_of(c, "wallOverlays"),
        translucent: tiles_of(c, "translucent"),
        icons: tiles_of(c, "icons"),
        cloud: tile_of(c, "cloud"),
        trail: c
            .get("trail")
            .map(|t| Trail { from: t.get("from").and_then(Value::as_u64).map(|d| d as Dir8), to: t.get("to").and_then(Value::as_u64).map(|d| d as Dir8) }),
        flags: CellFlags {
            water: flag("water"),
            lava: flag("lava"),
            excluded: flag("excluded"),
            travel_trail: flag("travelTrail"),
            new_stair: flag("newStair"),
            cursor: flag("cursor"),
            cursor_kind: match flags.get("cursorKind").and_then(Value::as_str) {
                Some("tutorial") => Some(CursorKind::Tutorial),
                Some("cursor1") => Some(CursorKind::Cursor1),
                Some("cursor2") => Some(CursorKind::Cursor2),
                Some("cursor3") => Some(CursorKind::Cursor3),
                _ => None,
            },
            out_of_range: flag("outOfRange"),
            magic_mapped: flag("magicMapped"),
            torch_wall: flag("torchWall"),
            halo: flag("halo"),
            umbra: flag("umbra"),
        },
        flash: c.get("flash").map(|f| Rgba8 { r: num(f, "r") as u8, g: num(f, "g") as u8, b: num(f, "b") as u8, a: num(f, "a") as u8 }),
        glyph: None,
        colour: None,
        label: None,
        mf: None,
    }
}

fn billboard_of(b: &Value) -> Option<Billboard> {
    let kind = match str_of(b, "kind")? {
        "monster" => BillboardKind::Monster,
        "item" => BillboardKind::Item,
        "cloud" => BillboardKind::Cloud,
        "doll" => BillboardKind::Doll,
        "projectile" => BillboardKind::Projectile,
        "player" => BillboardKind::Player,
        _ => return None,
    };
    let mut out = Billboard::new(int(b, "x"), int(b, "y"), tile_of(b, "tile")?, kind, num(b, "height") as f32);
    out.layers = b
        .get("layers")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|l| {
                    Some(Layer {
                        tile: tile_of(l, "tile")?,
                        ox: num(l, "ox") as f32,
                        oy: num(l, "oy") as f32,
                        ymax: l.get("ymax").and_then(Value::as_f64).map(|v| v as f32),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    out.status_icons = b
        .get("statusIcons")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| {
                    let f = |k: &str| s.get(k).and_then(Value::as_bool).unwrap_or(false);
                    Some(StatusIcon {
                        tile: tile_of(s, "tile")?,
                        ox: num(s, "ox") as f32,
                        oy: num(s, "oy") as f32,
                        at_top: f("atTop"),
                        square: f("square"),
                        full: f("full"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    out.alpha = b.get("alpha").and_then(Value::as_f64).map(|a| a as f32);
    out.lying = b.get("lying").and_then(Value::as_bool).unwrap_or(false);
    out.attitude = match str_of(b, "attitude") {
        Some("hostile") => Some(Attitude::Hostile),
        Some("neutral") => Some(Attitude::Neutral),
        Some("good_neutral") => Some(Attitude::GoodNeutral),
        Some("friendly") => Some(Attitude::Friendly),
        _ => None,
    };
    out.threat = match str_of(b, "threat") {
        Some("trivial") => Some(Threat::Trivial),
        Some("easy") => Some(Threat::Easy),
        Some("tough") => Some(Threat::Tough),
        Some("nasty") => Some(Threat::Nasty),
        Some("invisible") => Some(Threat::Invisible),
        _ => None,
    };
    out.name = str_of(b, "name").map(str::to_string);
    out.scenery = b.get("scenery").and_then(Value::as_bool).unwrap_or(false);
    out.damage = match str_of(b, "damage") {
        Some("uninjured") => Some(WoundLevel::Uninjured),
        Some("lightly_damaged") => Some(WoundLevel::LightlyDamaged),
        Some("moderately_damaged") => Some(WoundLevel::ModeratelyDamaged),
        Some("heavily_damaged") => Some(WoundLevel::HeavilyDamaged),
        Some("severely_damaged") => Some(WoundLevel::SeverelyDamaged),
        Some("almost_dead") => Some(WoundLevel::AlmostDead),
        _ => None,
    };
    Some(out)
}

/// A place file's scene as the renderer takes it.
pub fn read_scene(s: &Value) -> Scene {
    let mut scene = Scene::default();
    if let Some(b) = s.get("bounds") {
        scene.bounds = Rect { left: int(b, "left"), top: int(b, "top"), right: int(b, "right"), bottom: int(b, "bottom") };
    }
    if let Some(p) = s.get("player") {
        scene.player = (int(p, "x"), int(p, "y"));
    }
    scene.player_on_level = true;
    if let Some(l) = s.get("level") {
        let tint = l.get("tint").cloned().unwrap_or(Value::Null);
        let t = |k: &str| tint.get(k).and_then(Value::as_f64).unwrap_or(1.0) as f32;
        scene.level = SceneLevel {
            ceiling_tile: tile_of(l, "ceilingTile"),
            sky: match str_of(l, "sky") {
                Some("open") => Sky::Open,
                Some("dark") => Sky::Dark,
                _ => Sky::None,
            },
            tint: [t("r"), t("g"), t("b")],
        };
    }
    for c in s.get("cells").and_then(Value::as_array).into_iter().flatten() {
        let cell = cell_of(c);
        scene.cells.insert(cell_key(cell.x, cell.y), cell);
    }
    scene.billboards = s.get("billboards").and_then(Value::as_array).into_iter().flatten().filter_map(billboard_of).collect();
    scene.revision = 1;
    scene.layout_revision = 1;
    scene
}

/// Read a place: its file, and its atlas decoded. The format is one this build reads, or an error.
pub fn load_place(id: &str) -> Result<LoadedPlace, String> {
    let (json, png) = place_files(id).ok_or_else(|| format!("no place {id}"))?;
    let file: Value = serde_json::from_str(json).map_err(|e| format!("place {id}: {e}"))?;
    if file.get("format").and_then(Value::as_u64) != Some(1) {
        return Err(format!("place {id}: a format this build does not read"));
    }
    let image = Arc::new(isorun_client::tiles::decode_png(png)?);
    let atlas = file.get("atlas").cloned().unwrap_or(Value::Null);
    let cell = atlas.get("cell").and_then(Value::as_u64).unwrap_or(32) as u32;
    let mut rects = HashMap::new();
    for (k, t) in atlas.get("tiles").and_then(Value::as_object).into_iter().flatten() {
        let Ok(tid) = k.parse::<TileId>() else { continue };
        rects.insert(
            tid,
            TileRect {
                atlas: PLACE_ATLAS.into(),
                sx: num(t, "sx") as u32,
                sy: num(t, "sy") as u32,
                w: num(t, "w") as u32,
                h: num(t, "h") as u32,
                ox: num(t, "ox") as i32,
                oy: num(t, "oy") as i32,
                cell,
            },
        );
    }
    let eye = file.get("eye").map(|e| (int(e, "x"), int(e, "y"))).unwrap_or((0, 0));
    let scene = read_scene(file.get("scene").unwrap_or(&Value::Null));
    Ok(LoadedPlace { id: id.to_string(), tiles: Arc::new(PlaceTiles { rects, image }), scene, eye })
}

// ------------------------------------------------------------------ the view

type Slot = Arc<Mutex<Option<Result<LoadedPlace, String>>>>;

/// The room: the place's renderer and the camera turning round its eye.
pub struct RoomView {
    pub place: PlaceEntry,
    view: Option<View3d>,
    cam: IsoCamera,
    loading: Option<Slot>,
    loaded: bool,
    /// The right stick's left and right, -1 to 1.
    stick: f32,
    last: f64,
    /// When the first frame was drawn: the room fades in from the dark from then.
    shown_at: Option<f64>,
    /// Turning stops (the reduced-motion setting of a system).
    pub reduced_motion: bool,
    failed: bool,
    /// when a frame was last drawn, and when the last drag moved the camera
    drawn_at: f64,
    dragged_at: f64,
    /// something changed what is drawn (the settings): the next frame is drawn whatever the cadence
    redraw: bool,
}

impl RoomView {
    /// The room standing in `place`; the place loads off the main thread, and the menus stand on
    /// the dark until it is drawn.
    pub fn new(place: PlaceEntry) -> Self {
        let slot: Slot = Arc::new(Mutex::new(None));
        let id = place.id.clone();
        let fill = slot.clone();
        let work = move || {
            let r = load_place(&id);
            *fill.lock().unwrap() = Some(r);
        };
        #[cfg(not(target_arch = "wasm32"))]
        let _ = std::thread::Builder::new().name("room".into()).spawn(work);
        #[cfg(target_arch = "wasm32")]
        work();
        let cam = IsoCamera::new(
            isorun_render::CameraAngles { rotation: (crate::splash::random() * 360.0) as f32, tilt: 0.0, perspective: 0.0 },
            isorun_render::DEFAULT_VIEW.zoom,
        );
        RoomView {
            place,
            view: None,
            cam,
            loading: Some(slot),
            loaded: false,
            stick: 0.0,
            last: isorun_client::platform::seconds(),
            shown_at: None,
            reduced_motion: false,
            failed: false,
            drawn_at: 0.0,
            dragged_at: f64::NEG_INFINITY,
            redraw: true,
        }
    }

    /// Right stick: dx in [-1, 1]; zero releases it.
    pub fn look(&mut self, dx: f32) {
        self.stick = dx;
    }

    /// A drag across the scenery: the camera turns with it.
    pub fn drag(&mut self, dx_points: f32) {
        self.dragged_at = isorun_client::platform::seconds();
        self.cam.turn(-dx_points * DRAG_DEGREES_PER_PIXEL);
    }

    /// The Graphics settings changed (the settings screen stands over the room): they show at once.
    pub fn apply_settings(&mut self, settings: &crate::settings::Settings, dpr: f32) {
        self.redraw = true;
        if let Some(v) = &mut self.view {
            crate::settings::apply_graphics(&mut v.renderer, settings, dpr);
        }
    }

    /// How far the room has faded in, 0 to 1.
    pub fn opacity(&self) -> f32 {
        match self.shown_at {
            Some(t) if !self.reduced_motion => (((isorun_client::platform::seconds() - t) / FADE_SECONDS) as f32).clamp(0.0, 1.0),
            Some(_) => 1.0,
            None => 0.0,
        }
    }

    pub fn failed(&self) -> bool {
        self.failed
    }

    /// Draw a frame of the room into `rect` (behind the menus); asks for the next frame while it shows.
    pub fn paint(&mut self, ui: &egui::Ui, rs: &RenderState, rect: egui::Rect, settings: &crate::settings::Settings) {
        let now = isorun_client::platform::seconds();
        let dt = (now - self.last).clamp(0.0, 0.1) as f32;
        self.last = now;
        if let Some(slot) = &self.loading {
            let done = slot.lock().unwrap().take();
            if let Some(r) = done {
                self.loading = None;
                match r {
                    Ok(p) => {
                        let mut view = View3d::new(rs);
                        crate::settings::apply_graphics(&mut view.renderer, settings, ui.ctx().pixels_per_point());
                        view.renderer.set_tiles(p.tiles.clone());
                        view.renderer.new_level();
                        view.renderer.set_scene(Arc::new(p.scene));
                        self.cam.snap_to(p.eye.0, p.eye.1);
                        self.view = Some(view);
                        self.loaded = true;
                    }
                    Err(e) => {
                        log::warn!("the place {} could not be drawn; the menus stand on the dark: {e}", self.place.id);
                        self.failed = true;
                    }
                }
            } else {
                ui.ctx().request_repaint();
            }
        }
        let Some(view) = &mut self.view else { return };
        let orbit = if self.reduced_motion { 0.0 } else { ORBIT_DEGREES_PER_SECOND };
        self.cam.turn((orbit + self.stick * STICK_DEGREES_PER_SECOND) * dt);
        let eye = self.cam.camera();
        self.cam.update(dt, eye.x, eye.y);
        view.renderer.follow(&self.cam);
        // every frame while the room is handled, fades in or moves of its own; the orbit alone
        // and the stillness have their own cadences (`ORBIT_FRAME_SECONDS`, `STILL_FRAME_SECONDS`)
        let fading = self.shown_at.is_none() || (!self.reduced_motion && self.shown_at.is_some_and(|t| now - t < FADE_SECONDS as f64));
        let busy = std::env::var_os("ISORUN_ALWAYS_DRAW").is_some_and(|v| v != "0")
            || fading
            || self.stick != 0.0
            || now - self.dragged_at < DRAG_SETTLE_SECONDS
            || view.renderer.animating();
        let cadence = if orbit > 0.0 { ORBIT_FRAME_SECONDS } else { STILL_FRAME_SECONDS };
        let redraw = busy || std::mem::take(&mut self.redraw) || now - self.drawn_at >= cadence;
        let drawn = view.paint_if(ui, rs, rect, redraw);
        if redraw {
            self.drawn_at = now;
        }
        if drawn && self.shown_at.is_none() {
            self.shown_at = Some(now);
        }
        // the fade from the dark: the frame is drawn, then the dark laid back over it
        let o = self.opacity();
        if drawn && o < 1.0 {
            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_black_alpha(((1.0 - o) * 255.0) as u8));
        }
        if busy {
            ui.ctx().request_repaint();
        } else {
            let wait = (self.drawn_at + cadence - now).max(0.0);
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_places() {
        let all = places();
        assert_eq!(all.len(), 10);
        for p in &all {
            assert!(place_files(&p.id).is_some(), "{}", p.id);
            assert!(p.tint.is_some());
        }
        let cycle: Vec<PlaceEntry> = all.iter().filter(|p| p.cycle).cloned().collect();
        assert_eq!(next_in_cycle(&cycle, Some("lair-grove"), 0.5).id, "abyss");
        assert_eq!(next_in_cycle(&cycle, Some("vaults"), 0.5).id, "lair-grove");
        assert_eq!(next_in_cycle(&cycle, None, 0.0).id, "lair-grove");
    }

    #[test]
    fn a_place_reads() {
        let p = load_place("lair-grove").unwrap();
        assert!(p.scene.cells.len() > 100);
        assert!(!p.scene.billboards.is_empty());
        assert_eq!(p.eye, (-33, -10));
        // every tile the scene names is in the place's atlas
        let missing = p.scene.cells.values().filter(|c| c.floor_tile != 0 && p.tiles.tile(c.floor_tile).is_none()).count();
        assert_eq!(missing, 0);
        // a way down lies in the floor as it does in a game
        let down = p.scene.cells.values().find(|c| matches!(c.feature, Some(Feature::Stairs { dir: UpDown::Down, .. }))).map(|c| c.stance);
        if let Some(stance) = down {
            assert_eq!(stance, Some(Stance::Lying));
        }
    }
}
