//! The game screen (the port of `game.ts` `GameScreen`): the isometric view, the HUD over it,
//! crawl's overlays, the client's own lists and menus, the level map and the minimap, the sound,
//! and the input wired through them. The other crates are the parts; this is where they meet
//! once a frame:
//!
//! 1. the session's messages: the fight read off them before the scene they come with
//!    (`track_combat`), the player's hops (`track_hop`), the travel's pace (`pace_travel`), the
//!    runner's waits moved on, the sound;
//! 2. the scene, once a frame (`on_scene`): the camera put on the player for a new level or a
//!    jump, the actors' moves;
//! 3. the context (context.rs `derive_context`, once per change of what it reads), filled with
//!    what the runner, the settings and the overlays know;
//! 4. the layout (the HUD's `layout`, game.ts `relayout`), then the input: the keys, the pad, the
//!    pointer and the fingers, through the router (input crate) or crawl's overlays;
//! 5. the view (or the level map while it is up), the HUD, crawl's overlays, the client's lists,
//!    the touch bar, the cell menu;
//! 6. what went out: the runner's outbox and the screens' messages to the session, the
//!    renderer's events to the sound and the damage numbers.

pub mod client;
pub mod convert;
mod host;
pub mod perf;

use client::{ClientEvent, ClientMenu, Row, Run};
use convert::HudRun;
use eframe::egui_wgpu::RenderState;
use host::{Deferred, Host};
use isorun_audio::{AudioFrame, AudioSettings, GameAudio, RendererView, SoundEngine};
use isorun_client::session::{GamedataStatus, Session, SessionEvent};
use isorun_front::{GameMenus, MenuAction};
use isorun_hud::{Hud, HudCommand, HudCx};
use isorun_input::action_tabs::{action_tab, action_tab_of, ActionTabId, OKAY_THEN, SHOUT_KEY};
use isorun_input::bindings::{bar_labels, contextual_label, level_map as level_map_action, SeqItem, UiOp};
use isorun_input::combat::{combat_events, lines_since, monster_cell, CombatEvent, Fighter, LastLine};
use isorun_input::context::{derive_context, derive_mode, Context, Layer, Mode};
use isorun_input::gamepad::{pad_key_input, GamepadInput};
use isorun_input::gamepad_hints::{GamepadHints, HintMode};
use isorun_input::keys::{key_inputs, KeyInput};
use isorun_input::map_hold::MapHold;
use isorun_input::router::{InputDevice, InputRouter, Io, RouterSettings};
use isorun_input::runner::{Env, Runner, RunnerHooks};
use isorun_input::touch::{touch_screen, Gestures, TOUCH_CUT_AWAY_CELLS};
use isorun_input::Action;
use isorun_overlays::{OverlayCx, Overlays};
use isorun_render::actors::STEP_SECONDS;
use isorun_render::{ActorAction, IsoCamera, PLAYER_ACTOR};
use isorun_render2d::{map_cell_size, minibars, LevelMap, MapTiles, Minimap};
use isorun_scene::{CursorMode, Scene, SceneCursor};
use isorun_ui::tiles::TileIcons;
use isorun_ui::view3d::View3d;
use isorun_webtiles::{cm, formatted_string_to_text, GameMessage, GameState, MouseMode, Msgch, Phase, UiState};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// A move of more cells than this in one scene is a jump the view cuts to (the actors' MAX_WALK).
const SNAP_CELLS: i32 = 8;
/// How long after the player's own key swung a blow the server's line about it is that same blow.
const OWN_SWING_MS: f64 = 1000.0;
/// How long after the player's step a blow on something unseen is that step's (crawl answers a
/// move at once; a slow server or a long turn takes a while).
const UNSEEN_BLOW_MS: f64 = 3000.0;
/// Below this share of their hit points the screen's edge turns red, deeper as they fall.
const DANGER_HP: f32 = 0.5;
/// The safety tick while the screen rests (game.ts `IDLE_TICK_MS`): a frame every quarter second,
/// so anything that changed without waking it (a runner's timeout, the torches' flicker, the
/// actors' breath) is seen. At rest the native screen draws nothing else: on a phone that is the
/// battery.
const IDLE_TICK_MS: f64 = 250.0;
/// How long after the last thing that wanted a frame the screen keeps the display's rate before
/// it rests (game.ts `ACTIVE_MS`): play is a stream of events, and frames that came only when a
/// packet did would judder.
const ACTIVE_MS: f64 = 500.0;
/// a screen of the dock (the inventory) that leaves less of the view than this in sight stops it
const SCREEN_LEAVES: f32 = 0.5;
/// While a pad is plugged in the resting screen still reads it this often: gilrs and the Gamepad
/// API are polled, and wake nothing on their own. The dungeon is not drawn again for it.
const PAD_POLL_MS: f64 = 33.0;

/// `ISORUN_ALWAYS_DRAW=1`: every frame drawn whole, as before the screen learnt to rest (to
/// compare the CPU at rest, or to rule the resting out).
fn always_draw() -> bool {
    static ALWAYS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ALWAYS.get_or_init(|| std::env::var("ISORUN_ALWAYS_DRAW").is_ok_and(|v| v != "0"))
}
/// How long a turn of X's tabs waits on crawl before it gives up (`track_actions`).
const ACTION_TURN_MS: f64 = 1500.0;

/// How the game screen ended.
pub enum Outcome {
    /// the server put its lobby up (a death, a save, a quit)
    Lobby,
    /// the player left from the game's menu
    Leave,
    /// the connection dropped
    Dropped,
}

/// What the app tells the game screen about where it runs.
#[derive(Clone, Debug, Default)]
pub struct GameOptions {
    /// the `--perf` readout
    pub perf: bool,
    /// the sounds played go to the log (`--audio-trace`, the web's `?audio-trace`)
    pub audio_trace: bool,
    pub can_fullscreen: bool,
    /// a touch screen: the touch bar, the corner buttons, the walls cut away round the player
    pub touch: bool,
}

/// A turn to one of X's tabs under way (game.ts `actionTurn`).
#[derive(Clone, Debug)]
struct ActionTurn {
    target: ActionTabId,
    sent: Option<ActionTabId>,
    stage: TurnStage,
    last: Option<GameMessage>,
    t: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TurnStage {
    Closing,
    Sent,
    Listing,
}

/// What a blow shown will put up when it lands: the damage number's kind and cell.
type Landing = (isorun_hud::damage_numbers::Landing, (i32, i32));

/// Everything the router's host reaches: the screens over the view and the view itself.
pub struct Parts {
    pub view: View3d,
    pub overlays: Overlays,
    pub hud: Hud,
    pub icons: TileIcons,
    pub menus: GameMenus,
    pub client: Option<ClientMenu>,
    pub cell_menu: Option<isorun_input::cell_menu::CellMenu>,
    pub map_hold: MapHold,
    pub map_shown: bool,
    pub deferred: Vec<Deferred>,
    /// where the pointer is aiming, in logical px: the hovering mouse, or the finger
    pub hover: Option<egui::Pos2>,
    pub hover_by_touch: bool,
    /// the mouse moved since the last keyboard or pad action: its own selection shows
    pub pointer_live: bool,
    /// the touch bar's height as last drawn (0 when it is not up): crawl's popups keep clear of it
    pub touch_bar_h: f32,
    /// the pointer moved since the last target sent
    pub hover_moved: bool,
    pub last_input: InputDevice,
    pub chat_capturing: bool,
    pub dock_on: bool,
    pub level_map: LevelMap,
    pub aim: Option<isorun_scene::Dir8>,
    /// what the Isorun menu opens with (the game and the window), and the tab and rows it was left on
    pub system_opts: client::SystemOpts,
    pub system_at: (usize, [usize; 2]),
    /// the player left from the Isorun menu (a spectator's Stop watching)
    pub leave: bool,
    /// the Find screen: crawl's stash search behind it (isorun_find), and the screen
    pub find: isorun_find::Driver,
    pub find_screen: isorun_find::FindScreen,
    /// the Find screen's search field has the keys
    pub find_typing: bool,
}

impl Parts {
    /// Open the Isorun menu where it was left (overlays.ts `showSystem`).
    pub fn show_system(&mut self) {
        self.menus.close();
        self.client = Some(ClientMenu::system(self.system_opts, self.system_at.0, self.system_at.1));
    }
}

pub struct GameScreen {
    pub session: Box<Session>,
    rs: RenderState,
    camera: IsoCamera,
    router: InputRouter,
    runner: Runner,
    pad: GamepadInput,
    /// the app's pad source, lent to the game while it is up
    pub pads: crate::app::Pads,
    gestures: Gestures,
    parts: Parts,
    map_tiles: MapTiles,
    minimap: Minimap,
    perf: perf::PerfOverlay,
    audio: GameAudio,
    engine: SoundEngine,
    trace: bool,
    opts: GameOptions,
    settings: isorun_front::settings::Settings,
    client_settings: isorun_client::servers::Settings,
    tiles_set: bool,
    started: bool,
    loading: Option<String>,
    /// the context as last derived, and the key of what it was derived from
    ctx: Context,
    ctx_key: (u64, u64, isorun_scene::Dir8),
    ctx_base: Option<Context>,
    last_state_rev: u64,
    last_scene_rev: u64,
    last_pos: Option<(i32, i32)>,
    hop_pos: Option<(i32, i32)>,
    hops: Vec<(i32, i32)>,
    jumped: bool,
    last_level: String,
    travel_delay_sent: Option<i64>,
    exploring_at_once: bool,
    swung: Option<(i32, i32, f64)>,
    /// the newest message line the fight was read up to, and where it stood in the log
    last_combat_line: Option<LastLine>,
    /// the blows given to the renderer, by token: what lands when each shows
    landings: HashMap<u64, Landing>,
    next_token: u64,
    last_mode: Mode,
    action_tab_up: ActionTabId,
    action_turn: Option<ActionTurn>,
    pack_page: Option<String>,
    pack_row: HashMap<String, i64>,
    pack_return: Option<(String, bool)>,
    last_cursor: Option<SceneCursor>,
    cursor_is_ours: bool,
    last_target_sent: Option<(i32, i32)>,
    saved_view: String,
    last_frame: f64,
    /// when something last wanted frames (input, a message, a pad), for `ACTIVE_MS`
    last_active: f64,
    /// when the dungeon was last drawn, for the idle tick
    last_3d: f64,
    /// the view moved this frame (the camera's easing, the renderer's animations)
    animating: bool,
    level_map_was: bool,
    touch: bool,
    /// the settings over the game were opened from the Isorun menu: their Back goes back to it
    settings_from_system: bool,
}

fn now_ms() -> f64 {
    isorun_client::platform::now_ms()
}

impl GameScreen {
    pub fn new(ctx: &egui::Context, session: Box<Session>, rs: RenderState, engine: SoundEngine, opts: GameOptions, pads: crate::app::Pads) -> Self {
        isorun_hud::install(ctx);
        let settings = isorun_front::settings::get();
        let client_settings = isorun_front::settings::client_settings(&settings);
        let mut view = View3d::new(&rs);
        isorun_front::settings::apply_graphics(&mut view.renderer, &settings, ctx.pixels_per_point());
        let touch = touch_screen() || opts.touch;
        view.renderer.set_cut_away_reach(if touch { TOUCH_CUT_AWAY_CELLS } else { 0 });
        let mut camera = IsoCamera::default();
        if let Some(v) = isorun_client::servers::get_saved_view() {
            camera.set_angles(v.angles);
            if let Some(z) = v.zoom {
                camera.zoom_by(z / camera.zoom_goal());
            }
        }
        if opts.audio_trace {
            engine.enable_trace();
        }
        engine.apply_settings(&audio_settings(&client_settings));
        let audio = GameAudio::new(engine.clone());
        let mut router = InputRouter::new(RouterSettings::from_settings(&client_settings));
        router.hints = Some(GamepadHints::new());
        let parts = Parts {
            view,
            overlays: Overlays::new(),
            hud: {
                let mut h = Hud::new();
                // the router hands the screens their keys and the pad's ops
                h.screens_read_keys = false;
                h
            },
            icons: TileIcons::default(),
            menus: GameMenus::default(),
            client: None,
            cell_menu: None,
            map_hold: MapHold::default(),
            map_shown: false,
            deferred: Vec::new(),
            hover: None,
            hover_by_touch: false,
            pointer_live: false,
            touch_bar_h: 0.0,
            hover_moved: false,
            last_input: InputDevice::Keyboard,
            chat_capturing: false,
            dock_on: false,
            level_map: LevelMap::new(),
            aim: None,
            system_opts: client::SystemOpts::default(),
            system_at: (0, [0; 2]),
            leave: false,
            find: isorun_find::Driver::default(),
            find_screen: isorun_find::FindScreen::new(),
            find_typing: false,
        };
        let perf = perf::PerfOverlay::new(opts.perf);
        let mut s = GameScreen {
            session,
            rs,
            camera,
            router,
            runner: Runner::new(),
            pad: GamepadInput::default(),
            pads,
            gestures: Gestures::default(),
            parts,
            map_tiles: MapTiles::default(),
            minimap: Minimap::new(),
            perf,
            audio,
            engine,
            trace: opts.audio_trace,
            opts,
            settings,
            client_settings,
            tiles_set: false,
            started: false,
            loading: Some("Loading game data…".into()),
            ctx: Context::default(),
            ctx_key: (u64::MAX, u64::MAX, 0),
            ctx_base: None,
            last_state_rev: u64::MAX,
            last_scene_rev: u64::MAX,
            last_pos: None,
            hop_pos: None,
            hops: Vec::new(),
            jumped: false,
            last_level: String::new(),
            travel_delay_sent: None,
            exploring_at_once: false,
            swung: None,
            last_combat_line: None,
            landings: HashMap::new(),
            next_token: 1,
            last_mode: Mode::Lobby,
            action_tab_up: ActionTabId::Spells,
            action_turn: None,
            pack_page: None,
            pack_row: HashMap::new(),
            pack_return: None,
            last_cursor: None,
            cursor_is_ours: false,
            last_target_sent: None,
            saved_view: String::new(),
            last_frame: now_ms(),
            last_active: now_ms(),
            last_3d: 0.0,
            animating: true,
            level_map_was: false,
            touch,
            settings_from_system: false,
        };
        // the game's messages so far (the front end's connection carried the lobby): the fight starts from here
        s.last_combat_line = LastLine::newest(&s.session.state.messages.lines);
        if s.session.gamedata.is_some() && !s.session.loading() {
            s.loading = None;
        }
        s
    }

    /// The settings changed (the Isorun menu's settings): read them again and apply them (game.ts `applySettings`).
    pub fn apply_settings(&mut self, ctx: &egui::Context) {
        self.settings = isorun_front::settings::get();
        self.client_settings = isorun_front::settings::client_settings(&self.settings);
        isorun_front::settings::apply_graphics(&mut self.parts.view.renderer, &self.settings, ctx.pixels_per_point());
        self.router.settings = RouterSettings::from_settings(&self.client_settings);
        self.engine.apply_settings(&audio_settings(&self.client_settings));
        self.pace_travel();
    }

    /// Remember the camera's angles and zoom so the next game starts with the same view.
    pub fn save_view(&mut self) {
        let v = isorun_client::servers::SavedView { angles: self.camera.angles(), zoom: Some(self.camera.zoom_goal()) };
        let key = format!("{:?}", (v.angles.rotation, v.angles.tilt, v.angles.perspective, v.zoom));
        if key == self.saved_view {
            return;
        }
        self.saved_view = key;
        isorun_client::servers::save_view(&v);
    }

    fn watching(&self) -> bool {
        self.session.watching()
    }

    fn chat_on(&self) -> bool {
        !self.session.server.offline
    }

    // ------------------------------------------------------------------ the frame

    /// One frame of the game screen. Some when the game is over and the front end comes back.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<Outcome> {
        let ctx = ui.ctx().clone();
        drop_button_focus(&ctx);
        let now = now_ms();
        let dt = ((now - self.last_frame) / 1000.0).clamp(0.0, 0.1) as f32;
        self.last_frame = now;
        if self.perf.enabled {
            self.perf.profile.begin();
        }

        // 1. the session's messages
        let mut states = 0usize;
        let mut outcome = None;
        for e in self.session.poll() {
            match e {
                SessionEvent::State(msg) => {
                    states += 1;
                    let kind = msg.get("msg").and_then(Value::as_str).unwrap_or("");
                    if kind == "game_client" || kind == "go_lobby" {
                        if self.session.gamedata.is_some() && !self.session.loading() {
                            self.loading = None;
                        } else {
                            self.loading = Some("Loading game data…".into());
                        }
                    }
                }
                SessionEvent::Sent(msg) => {
                    let kind = msg.get("msg").and_then(Value::as_str).unwrap_or("");
                    if self.perf.enabled {
                        self.perf.net.sent(kind);
                    }
                    let command = self.ctx.mode == Mode::Command;
                    match kind {
                        "input" => {
                            if let Some(t) = msg.get("text").and_then(Value::as_str) {
                                self.audio.sent_input(t, command);
                            }
                        }
                        "key" => {
                            if let Some(k) = msg.get("keycode").and_then(Value::as_i64) {
                                self.audio.sent_key(k);
                            }
                        }
                        _ => {}
                    }
                }
                SessionEvent::Gamedata(GamedataStatus::Ready) => {
                    self.loading = None;
                    self.tiles_set = false;
                }
                SessionEvent::Gamedata(GamedataStatus::Error(e)) => self.loading = Some(format!("Could not load game data: {e}")),
                SessionEvent::Gamedata(GamedataStatus::Loading { done, total }) => self.loading = Some(format!("Loading game data… {done}/{total}")),
                SessionEvent::Closed(_) => {
                    self.perf.net.open = false;
                    outcome = Some(Outcome::Dropped);
                }
                SessionEvent::Open => self.perf.net.open = true,
                SessionEvent::Scene => {}
            }
        }
        if self.perf.enabled {
            self.perf.net.arrived(states);
        }
        if outcome.is_some() {
            return outcome;
        }
        // a message, a key, the pointer or a finger: frames at the display's rate for a while
        if states > 0 || ctx.input(|i| !i.events.is_empty() || i.pointer.is_moving() || i.pointer.any_down() || i.any_touches()) {
            self.last_active = now;
        }
        let phase = self.session.state.phase;
        if matches!(phase, Phase::Playing | Phase::Watching) {
            self.started = true;
        }
        if self.started && matches!(phase, Phase::Lobby | Phase::Ended) {
            self.save_view();
            return Some(Outcome::Lobby);
        }
        if !self.tiles_set {
            if let Some(gd) = self.session.gamedata.clone() {
                self.tiles_set = true;
                self.parts.view.renderer.set_tiles(gd.clone());
                self.parts.view.renderer.new_level();
                self.loading = None;
            }
        }
        self.parts.icons.set_gamedata(self.session.gamedata.clone());
        self.map_tiles.set_source(self.session.gamedata.clone().map(|g| g as Arc<dyn isorun_render::TileSource>));
        if self.session.state.rev.any != self.last_state_rev {
            self.last_state_rev = self.session.state.rev.any;
            self.on_state(now);
        }
        self.update_find(now);
        if self.perf.enabled {
            self.perf.profile.mark("state");
        }

        // 2. the scene, once a frame
        if self.session.flush_scene() {
            if self.perf.enabled {
                self.perf.profile.tag("scene");
            }
        }
        let scene = self.session.scene.clone();
        if scene.revision != self.last_scene_rev {
            self.last_scene_rev = scene.revision;
            self.on_scene(&scene, now);
        }
        if self.perf.enabled {
            self.perf.profile.mark("scene");
        }

        // 3. the context
        self.derive_context(&scene, now);
        let st_mode = self.ctx.mode;
        self.parts.map_shown = self.parts.map_hold.shown(self.session.state.ui_state == UiState::VIEW_MAP, st_mode, now);
        if self.parts.cell_menu.is_some() && st_mode != Mode::Command {
            self.parts.cell_menu = None;
        }
        self.track_pack();
        self.track_actions(now, true);

        // 4. the layout, then the input
        let screen = isorun_ui::safe::rect(&ctx);
        let map_view = self.parts.map_shown;
        let layout = self.parts.hud.layout(&ctx, screen, &self.session.state, &self.client_settings, map_view);
        self.parts.dock_on = layout.dock;
        self.parts.view.renderer.set_centre_lift(layout.centre_lift);
        self.parts.chat_capturing = self.parts.hud.chat.capturing(&ctx);
        let fullscreen = isorun_ui::fullscreen::is_on(&ctx);
        self.parts.system_opts =
            client::SystemOpts { spectating: self.session.watching(), chat: self.chat_on(), can_fullscreen: self.opts.can_fullscreen, fullscreen };
        if let Some(ClientMenu { kind: client::Kind::System { tab, .. }, tab_focus, .. }) = &self.parts.client {
            self.parts.system_at = (*tab, *tab_focus);
        }
        if self.started && self.tiles_set {
            self.input(ui, layout.view, now, dt);
        }
        self.run_deferred(&ctx, now);
        self.relayout_map_switch();
        if self.perf.enabled {
            self.perf.profile.mark("input");
        }

        // 5. the view (or the level map), the HUD, the overlays
        let playing = self.started && self.tiles_set;
        ui.painter().rect_filled(screen, 0.0, egui::Color32::BLACK);
        let mut drew = false;
        if playing {
            let player = self.parts.view.renderer.player_shown().map(|(x, y)| (x.round() as i32, y.round() as i32)).unwrap_or(scene.player);
            let _ = player;
            let moving = self.camera.update(dt, scene.player.0, scene.player.1);
            self.animating = moving;
            // saved as soon as a turn or zoom ends, as an app that is killed never saves on the way out
            if !moving {
                self.save_view();
            }
            let cursor = self.cursor_for(&scene);
            if cursor != self.last_cursor {
                self.last_cursor = cursor;
                self.parts.view.renderer.set_cursor(cursor);
            }
            self.sync_target();
            if self.parts.map_shown {
                // the level map is crawl's 2D view: the iso view is not drawn under it (the page is black around the map)
                self.draw_level_map(ui, &scene, layout.view);
            } else {
                let p = &self.session.state.player;
                let danger = if p.hp_max > 0 && !self.watching() { (DANGER_HP - p.hp as f32 / p.hp_max as f32) / DANGER_HP } else { 0.0 };
                self.parts.view.renderer.set_danger(danger);
                self.parts.view.renderer.set_scene(scene.clone());
                self.parts.view.renderer.follow(&self.camera);
                // the dungeon is drawn while anything moves or plays, and at rest on the idle tick
                // only (the ambient: the torches' flicker, the fog's drift, the breath)
                self.animating = moving || self.parts.view.renderer.animating();
                if log::log_enabled!(target: "isorun::idle", log::Level::Debug) {
                    log::debug!(target: "isorun::idle", "camera {moving} renderer {:?} active {}", self.parts.view.renderer.animating_why(), now - self.last_active < ACTIVE_MS);
                }
                let mut redraw = always_draw() || self.animating || now - self.last_active < ACTIVE_MS || now - self.last_3d >= IDLE_TICK_MS;
                // under the inventory or the character sheet crawl waits for a command and nothing
                // moves: the dungeon is drawn only for what still plays out (the camera settling, a
                // hop), not for the pointer or the fingers on the screen nor for the ambient on the
                // idle tick, and not at all where the panel hides most of it (a phone's)
                if let Some(panel) = self.parts.hud.screen_panel() {
                    let hidden = if panel.intersects(layout.view) { panel.intersect(layout.view).area() } else { 0.0 };
                    let seen = layout.view.area() - hidden;
                    redraw = always_draw() || (seen > layout.view.area() * SCREEN_LEAVES && self.animating);
                }
                drew = self.parts.view.paint_if(ui, &self.rs, layout.view, redraw);
                if redraw {
                    self.last_3d = now;
                }
            }
        }
        if self.perf.enabled {
            self.perf.profile.mark("render");
            if drew {
                self.perf.net.drew();
            }
        }
        // the renderer's events: the blows shown land their numbers and sounds, the hops and doors sound
        let events = self.parts.view.renderer.take_events();
        let secs = isorun_client::platform::seconds();
        for t in &events.shown {
            if let Some((landing, cell)) = self.landings.remove(t) {
                log::debug!("blow shown: {landing:?} at {cell:?}");
                self.parts.hud.damage.landing(landing, cell, secs, self.client_settings.damage_numbers);
            }
        }
        {
            let width = layout.view.width();
            let view = RendererView { renderer: &self.parts.view.renderer, width, animation_speed: self.client_settings.animation_speed };
            self.audio.on_frame(AudioFrame { shown: &events.shown, hops: &events.hops, doors: &events.doors, landings: &events.landings }, &scene, &view);
        }
        if self.trace {
            for name in self.engine.take_trace() {
                log::info!("sound {name}");
            }
        }

        if playing {
            if !self.parts.map_shown {
                self.draw_minimap(ui, &scene, layout.minimap, layout.minimap_cell);
            }
            self.draw_hud(ui, &scene, layout.view, now);
            if self.perf.enabled {
                self.perf.profile.mark("hud");
            }
            self.draw_overlays(&ctx, now);
            self.draw_find(&ctx, now);
            self.draw_client(&ctx);
            self.draw_touch(ui, now);
        }
        if let Some(text) = &self.loading {
            // the veil over the screen while the tiles load; behind crawl's overlays when one is up
            let behind = Overlays::up(&self.session.state) && self.started;
            let painter = ctx
                .layer_painter(egui::LayerId::new(if behind { egui::Order::Background } else { egui::Order::Foreground }, egui::Id::unique("isorun-loading")));
            painter.rect_filled(screen, 0.0, egui::Color32::from_black_alpha(217));
            let g = painter.layout_no_wrap(text.clone(), egui::FontId::monospace(16.0), isorun_ui::theme::INK);
            let at = if behind { egui::pos2(screen.center().x, screen.top() + screen.height() * 0.08) } else { screen.center() };
            painter.galley(at - egui::vec2(g.size().x / 2.0, 0.0), g, isorun_ui::theme::INK);
        } else if !self.started {
            let painter = ui.painter();
            let g = painter.layout_no_wrap("Entering the dungeon…".into(), egui::FontId::monospace(16.0), egui::Color32::from_gray(150));
            painter.galley(screen.center() - g.size() / 2.0, g, egui::Color32::from_gray(150));
        }
        self.run_deferred(&ctx, now);

        // 6. what went out
        self.flush_outbox(now);
        if std::mem::take(&mut self.parts.leave) {
            self.save_view();
            return Some(Outcome::Leave);
        }
        let actions = self.parts.menus.ui(&ctx);
        if let Some(o) = self.menu_actions(&ctx, actions) {
            return Some(o);
        }
        if self.perf.enabled {
            self.perf.profile.mark("ui");
            self.perf.profile.end();
            let lc = self.log_context();
            self.perf.sample(&lc);
            self.perf.ui(&ctx);
        }
        // the next frame: at once while something moves or plays (game.ts `awake`), else the idle
        // tick, or the pad's poll while one is plugged in; the session's messages wake the screen
        // themselves, and the HUD's own easings ask for their frames
        if always_draw() || !self.started || self.loading.is_some() || self.animating || now - self.last_active < ACTIVE_MS || self.pad.busy() {
            ctx.request_repaint();
        } else {
            let wait = if self.pad.connected() { PAD_POLL_MS } else { IDLE_TICK_MS };
            ctx.request_repaint_after(std::time::Duration::from_millis(wait as u64));
        }
        None
    }

    fn log_context(&self) -> perf::LogContext {
        let st = &self.session.state;
        perf::LogContext {
            mode: format!("{:?}", self.ctx.mode).to_lowercase(),
            place: if st.player.place.is_empty() { String::new() } else { format!("{}:{}", st.player.place, st.player.depth) },
            cells: self.session.scene.cells.len(),
            crowd: self.session.scene.billboards.len(),
        }
    }

    /// A state came in: the hops, the fight, the sound, the travel's pace, the runner's waits,
    /// the turns of X's tabs.
    fn on_state(&mut self, now: f64) {
        self.track_hop();
        self.track_actions(now, false);
        self.track_combat(now);
        self.pace_travel();
        // a sequence waiting on a prompt goes on: on the context of the state just in
        self.ctx = derive_context(&self.session.state, &self.session.scene, self.runner.heading, Layer::Micro);
        self.route(now, |r, io| r.server_state(io));
        // the menus coming and going, as sounds
        let mode = derive_mode(&self.session.state);
        if mode == Mode::Menu && self.last_mode == Mode::Command {
            self.engine.play(isorun_audio::SoundName::UiOpen, Default::default());
        } else if mode == Mode::Command && self.last_mode == Mode::Menu {
            self.engine.play(isorun_audio::SoundName::UiClose, Default::default());
        }
        self.last_mode = mode;
    }

    /// The context of the frame, derived once per change of what it reads and filled with what
    /// the runner, the settings and the overlays know (game.ts `deriveContext` and `loop`).
    fn derive_context(&mut self, scene: &Arc<Scene>, now: f64) {
        let st = &self.session.state;
        let key = (st.rev.any, scene.revision, self.runner.heading);
        if self.ctx_base.is_none() || key != self.ctx_key {
            self.ctx_key = key;
            self.ctx_base = Some(derive_context(st, scene, self.runner.heading, Layer::Micro));
        }
        let mut ctx = self.ctx_base.clone().unwrap_or_default();
        self.router.fill_context(&mut ctx, &mut self.runner, st.input_mode, now);
        let mut out = Vec::new();
        {
            let mut cx = OverlayCx::new(st, &mut self.parts.icons, &mut out);
            cx.watching = self.session.watching();
            if ctx.mode == Mode::Popup {
                ctx.popup_actions =
                    Some(self.parts.overlays.popup_actions().into_iter().map(|(key, label)| isorun_input::context::PopupAction { key, label }).collect());
            }
            ctx.pageable = self.parts.overlays.pageable(&cx);
        }
        if let Some(f) = self.parts.overlays.focus_info(st) {
            ctx.focus = Some(convert::focus_info(f));
        }
        self.ctx = ctx;
    }

    /// A router call with the runner, the situation and the host around it (an `Io`), built
    /// for the call: the host borrows the parts, so they are free again between calls.
    fn route(&mut self, now: f64, f: impl FnOnce(&mut InputRouter, &mut Io)) {
        let scene = self.session.scene.clone();
        let st = &self.session.state;
        let watching = self.session.watching();
        let ctx = self.ctx.clone();
        let chat = !self.session.server.offline;
        let mut host = Host::new(&mut self.parts, st, &scene, &ctx, watching, now, chat, chat);
        let env = Env { state: st, scene: &scene, ctx: &ctx, camera: &mut self.camera, now, watching, confirm_dangerous: self.client_settings.confirm_stairs };
        let mut io = Io { runner: &mut self.runner, env, host: &mut host };
        f(&mut self.router, &mut io);
    }

    /// A call into the overlays with the frame's state; what they send goes to crawl.
    fn overlay_keys(&mut self, now: f64, f: impl FnOnce(&mut Overlays, &mut OverlayCx) -> bool) {
        let mut out = Vec::new();
        {
            let mut cx = OverlayCx::new(&self.session.state, &mut self.parts.icons, &mut out);
            cx.device = convert::device(self.parts.last_input).1;
            if f(&mut self.parts.overlays, &mut cx) {
                self.parts.pointer_live = false;
            }
        }
        for m in out {
            self.send(m, now);
        }
    }

    /// The input of the frame: the keys, the pad, the pointer and the fingers.
    fn input(&mut self, ui: &mut egui::Ui, view_rect: egui::Rect, now: f64, dt: f32) {
        let ctx = ui.ctx().clone();
        let (events, modifiers, time) = ui.input(|i| (i.events.clone(), i.modifiers, i.time));
        let watching = self.session.watching();
        // a key answers the level-up banner, whatever it goes on to do
        if events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. } | egui::Event::Text(_))) {
            self.parts.overlays.level_up.answer(time);
        }
        // the settings take the keyboard whole while they are up; Tune the look leaves the keys to
        // the game but for Escape, which the router gives it (`look_panel_open`)
        let menus_keys = self.parts.menus.is_open();
        if menus_keys {
            let (_, actions) = self.parts.menus.keys(&ctx);
            self.parts.deferred.push(Deferred::MenuActions(actions));
        }
        // the chat takes its own keys; the HUD's screens get theirs from the router (`client_overlay_input`)
        let hud_keys = self.parts.chat_capturing;
        self.route(now, |r, io| r.tick(io));
        // every key goes down the router (game.ts `onKeyDown`): it decides what is Isorun's, what
        // is a screen's pad button, and hands crawl's menus and popups their own keys
        // (`Overlays::key_like`); a key it leaves while a field of crawl's has the keys is the field's
        if !menus_keys && !hud_keys {
            for k in key_inputs(&events, modifiers) {
                if pad_key_input(&mut self.pad, &k, now) {
                    continue;
                }
                match k {
                    KeyInput::Down { key, repeat } => {
                        // Backspace in the palette's filter, which the router does not pass on
                        if key.key == "Backspace" {
                            if let Some(c) = self.parts.client.as_mut() {
                                c.backspace();
                                continue;
                            }
                        }
                        let mut took = false;
                        self.route(now, |r, io| took = r.key_down(&key, repeat, io));
                        if !took && !watching && !self.parts.find.hides() && self.parts.overlays.text_entry(&self.session.state) {
                            self.overlay_keys(now, |o, cx| o.text_key(&key, cx));
                        }
                    }
                    KeyInput::Up { key } => self.route(now, |r, io| r.key_up(&key, io)),
                }
            }
            if !watching {
                for e in &events {
                    if let egui::Event::Paste(t) = e {
                        self.overlay_keys(now, |o, cx| o.paste(t, cx));
                    }
                }
            }
        }
        if isorun_input::back_button::back_pressed(&events) {
            self.route(now, |r, io| r.back(io));
        }
        // the pad: gilrs natively, the Gamepad API on the web
        #[cfg(all(not(target_arch = "wasm32"), feature = "gilrs"))]
        if let Some(g) = self.pads.as_mut() {
            self.pad.poll_source(now, g);
        }
        #[cfg(target_arch = "wasm32")]
        self.pad.poll_source(now, &mut isorun_input::gamepad::WebSource);
        let pad_events = self.pad.take_events();
        if !pad_events.is_empty() || self.pad.busy() {
            self.last_active = now;
        }
        for ev in pad_events {
            if self.parts.menus.is_open() {
                // the Isorun menu reads the pad its own way
                if let Some(pe) = front_pad(&ev) {
                    let actions = self.parts.menus.pad(pe);
                    self.parts.deferred.push(Deferred::MenuActions(actions));
                }
                continue;
            }
            self.route(now, |r, io| r.pad(&ev, io));
        }
        // a press outside a popup of crawl's closes it (ui.js popup_clickoutside_handler: "for crawl we
        // really need to send a 'close popup' message to the server"), and a right click closes it
        // wherever it lands, the mirror of the right click that opened a describe; a tap outside a
        // menu or one of our windows on a touch screen is Back (game.ts `tapAway`)
        let mut swallowed = false;
        // the press read off the events: a quick tap's press, release and PointerGone can come in
        // one frame (a browser's frames often do), which leaves the pointer no press origin
        if let Some((pos, button)) = isorun_ui::pointer::any_pressed_at(&ctx).map(|(p, b)| (p, if b == egui::PointerButton::Secondary { 2 } else { 0 })) {
            let outside = !pointer_over_panel_at(&ctx, pos);
            let touch = events.iter().any(|e| matches!(e, egui::Event::Touch { .. }));
            let screen_panel = self.parts.hud.screen_panel();
            if touch && self.parts.client.is_none() && screen_panel.is_some_and(|r| !r.contains(pos)) {
                // a tap outside the inventory or the character sheet puts it away, as Back
                // (game.ts `tapAway`); the screen's own block keeps it from the world
                swallowed = true;
                self.parts.hud.screen_op(isorun_hud::screens::ScreenOp::Cancel);
            } else if self.parts.cell_menu.is_some() && outside {
                // a tap or click beside the cell menu puts it away, and does nothing else
                swallowed = true;
                self.parts.cell_menu = None;
            } else if touch && !watching && (self.parts.client.is_some() || self.parts.find.open() || self.ctx.mode == Mode::Menu) && outside {
                swallowed = true;
                self.route(now, |r, io| r.back(io));
            } else if !watching
                && self.parts.client.is_none()
                && !menus_keys
                && matches!(self.ctx.mode, Mode::Popup | Mode::Dialog)
                && mouse_control(&self.session.state)
                && (outside || button == 2)
            {
                swallowed = true;
                self.send(cm::key(27), now);
            }
        }
        // the pointer and the fingers over the view; none while a screen of ours or a panel covers it
        let covered = swallowed
            || self.parts.client.is_some()
            || self.parts.find.open()
            || self.parts.hud.screen_open().is_some()
            || menus_keys
            || pointer_over_panel(&ctx);
        self.gestures.view = view_rect;
        // what takes the pointer (the dock, the touch bar, the corner buttons, panels): a press
        // there is that widget's for its whole gesture, never the view's
        self.gestures.ui = ctx.interactive_rects_last_pass();
        self.gestures.level_map = self.parts.map_shown;
        self.gestures.cell_menu_open = self.parts.cell_menu.is_some();
        let mut gestures = self.gestures.feed(&events, now);
        // a press taken as a tap away (Back, a popup or the cell menu put away) is not the
        // view's either when it is released
        if swallowed {
            self.gestures.cancel_drag();
        }
        self.gestures.tick(now);
        gestures.extend(self.gestures.take());
        for g in gestures {
            if covered && !matches!(g, isorun_input::touch::Gesture::MouseGone | isorun_input::touch::Gesture::SnapTurn) {
                continue;
            }
            self.route(now, |r, io| r.gesture(&g, io));
        }
        if self.gestures.seen_touch {
            self.touch = true;
        }
        let held: Vec<isorun_input::Button> = ALL_BUTTONS.iter().copied().filter(|b| self.pad.is_held(*b)).collect();
        let mut turns = false;
        self.route(now, |r, io| turns = r.frame(io, &|b| held.contains(&b), dt));
        self.pad.right_stick_turns = turns;
    }

    /// Carry out what the host asked for while the runner was lent out.
    fn run_deferred(&mut self, ctx: &egui::Context, now: f64) {
        for _ in 0..4 {
            let list = std::mem::take(&mut self.parts.deferred);
            if list.is_empty() {
                return;
            }
            for d in list {
                match d {
                    Deferred::Execute(a) => self.execute(&a, now),
                    Deferred::Send(m) => self.send(m, now),
                    Deferred::SendThen(m, seq) => self.with_runner(now, |r, env, h| r.send_then(m, &seq, env, h)),
                    Deferred::Swing(x, y) => self.swing(x, y, now),
                    Deferred::ActionTab(id) => self.open_action_tab(id, now),
                    Deferred::ActionTabStep(step) => {
                        let from = self
                            .action_turn
                            .as_ref()
                            .map(|t| t.target)
                            .or(self.ctx.menu.as_ref().and_then(|m| m.actions.as_ref().map(|a| a.current)))
                            .unwrap_or(self.action_tab_up);
                        self.open_action_tab(isorun_input::action_tabs::action_neighbour(from, step), now);
                    }
                    Deferred::ActionKey(key) => {
                        // a menu still opening would take the key as a row's letter: the frame waits for it
                        if self.action_turn.is_none() {
                            self.parts.client = None;
                            let label = if key == SHOUT_KEY { "Shout" } else { "Swap weapons" };
                            self.execute(&Action::keys(vec![SeqItem::text(&key)], label), now);
                        }
                    }
                    Deferred::Equipment => self.equipment(now),
                    Deferred::PaceTravel => self.pace_travel(),
                    Deferred::Client(e) => self.client_event(e, now),
                    Deferred::MenuActions(actions) => {
                        let _ = self.menu_actions(ctx, actions);
                    }
                    Deferred::Hud(c) => self.hud_command(c, now),
                    Deferred::DockScreen(id) => self.hud_call(ctx, now, |hud, cx| hud.dock_button(id, cx)),
                    Deferred::DockNav(op) => self.hud_call(ctx, now, |hud, cx| hud.dock_nav(op, cx)),
                    Deferred::Fullscreen(on) => isorun_ui::fullscreen::set(ctx, on),
                    Deferred::ExploreAtOnce => {
                        self.exploring_at_once = true;
                        self.pace_travel();
                    }
                }
            }
        }
    }

    /// The runner with an `Env` and the host around it, for one call.
    fn with_runner(&mut self, now: f64, f: impl FnOnce(&mut Runner, &mut Env, &mut dyn RunnerHooks)) {
        self.route(now, |_, io| f(io.runner, &mut io.env, io.host));
    }

    fn execute(&mut self, a: &Action, now: f64) {
        self.route(now, |r, io| r.execute(a, io));
    }

    fn send(&mut self, m: Value, now: f64) {
        self.with_runner(now, |r, env, _| r.send(m, env));
    }

    /// The runner's outbox to the session.
    fn flush_outbox(&mut self, now: f64) {
        let _ = now;
        for m in self.runner.take_outbox() {
            self.session.send(m);
        }
    }

    /// Crawl's `travel_delay` and `explore_delay` for the Travel setting, sent once a game takes
    /// commands and again when the setting or the animation speed changes: paced, a travel waits
    /// as long as one hop takes, so every cell shows; fast, the server sends the arrival alone.
    fn pace_travel(&mut self) {
        let st = &self.session.state;
        if self.session.watching() || st.phase != Phase::Playing || st.input_mode != MouseMode::COMMAND {
            return;
        }
        let s = &self.client_settings;
        let delay = if s.travel == "paced" && !self.exploring_at_once { (STEP_SECONDS * 1000.0 / s.animation_speed.max(0.1)).round() as i64 } else { -1 };
        if self.travel_delay_sent == Some(delay) {
            return;
        }
        self.travel_delay_sent = Some(delay);
        self.session.send(cm::set_option("travel_delay", &Value::from(delay)));
        self.session.send(cm::set_option("explore_delay", &Value::from(delay)));
    }

    /// A melee attack went out: the player strikes at the monster on that cell at once, before the server's answer.
    fn swing(&mut self, x: i32, y: i32, now: f64) {
        let token = self.token();
        self.parts.view.renderer.act(PLAYER_ACTOR, ActorAction::Attack, Some((x, y)), Some(token));
        self.audio.swing();
        self.swung = Some((x, y, now));
    }

    /// A fight's action on the view: on the fighter's actor, or on the cell of one out of sight.
    fn act(&mut self, who: Fighter, action: ActorAction, target: Option<(i32, i32)>, token: u64) {
        let r = &mut self.parts.view.renderer;
        match who {
            Fighter::Player => r.act(PLAYER_ACTOR, action, target, Some(token)),
            Fighter::Monster(id) => r.act(id, action, target, Some(token)),
            Fighter::Unseen(cell) => r.act_at(action, cell, target, Some(token)),
        }
    }

    /// The cell next to the player that the player's last step went at, while crawl may still be
    /// answering it: where "You hit something." lands (an invisible monster stops the step).
    fn struck_at(&self, scene: &Scene, now: f64) -> Option<(i32, i32)> {
        let step = self.runner.last_step.filter(|s| now - s.t < UNSEEN_BLOW_MS)?;
        let (x, y) = scene.player;
        Some((x + isorun_scene::DIR8_DX[step.dir as usize], y + isorun_scene::DIR8_DY[step.dir as usize]))
    }

    fn token(&mut self) -> u64 {
        self.next_token += 1;
        self.next_token
    }

    /// A server message came in: note the step the player made, if any (game.ts `hops`). The
    /// scene is rebuilt once a frame, so a fast explore arrives as several cells at once: the
    /// hops keep the path the feet took.
    fn track_hop(&mut self) {
        let p = self.session.state.player.pos;
        let pos = (p.x, p.y);
        if self.hop_pos == Some(pos) {
            return;
        }
        if let Some((x, y)) = self.hop_pos {
            let (dx, dy) = (pos.0 - x, pos.1 - y);
            if dx.abs() <= 1 && dy.abs() <= 1 {
                self.hops.push((dx, dy));
            } else {
                self.jumped = true;
            }
        }
        self.hop_pos = Some(pos);
    }

    /// The fight in the messages since the last ones read (combat.rs), played on the view: blows,
    /// hits and deaths. Read as the messages land, before the scene they come with, so a monster
    /// that dies is still there to fall. A blow the player's own key already swung is not swung again.
    fn track_combat(&mut self, now: f64) {
        let st = &self.session.state;
        let lines = &st.messages.lines;
        let fresh: Vec<GameMessage> = lines_since(lines, self.last_combat_line.as_ref(), st.player.turn).into_iter().cloned().collect();
        self.last_combat_line = LastLine::newest(lines);
        let scene = self.session.scene.clone();
        self.audio.on_state(st, &scene, &fresh);
        if fresh.is_empty() {
            return;
        }
        self.parts.overlays.messages(&fresh);
        let events = combat_events(&fresh, &scene, &st.map.monsters, self.struck_at(&scene, now));
        log::debug!("{} fresh lines, {} fight events", fresh.len(), events.len());
        let width = self.parts.view.rect.width().max(1.0);
        for e in events {
            let audio_event = match e {
                CombatEvent::Attack { by, at } => isorun_audio::CombatEvent::Attack { by: fighter(by), at },
                CombatEvent::Hit { on, from, .. } => isorun_audio::CombatEvent::Hit { on: fighter(on), from },
                CombatEvent::Miss { on, from } => isorun_audio::CombatEvent::Miss { on: fighter(on), from },
                CombatEvent::Death { on, from } => isorun_audio::CombatEvent::Death { on: fighter(on), from },
            };
            let cell_of = |f: Fighter| match f {
                Fighter::Player => Some(scene.player),
                Fighter::Monster(id) => monster_cell(&scene, id),
                Fighter::Unseen(cell) => Some(cell),
            };
            match e {
                CombatEvent::Attack { by, at } => {
                    let own = by == Fighter::Player && self.swung.is_some_and(|(x, y, t)| (x, y) == at && now - t < OWN_SWING_MS);
                    if own {
                        continue;
                    }
                    let token = self.token();
                    {
                        let view = RendererView { renderer: &self.parts.view.renderer, width, animation_speed: self.client_settings.animation_speed };
                        self.audio.combat(&audio_event, &scene, &view, Some(token));
                    }
                    self.act(by, ActorAction::Attack, Some(at), token);
                }
                CombatEvent::Hit { on, from, strength } => {
                    let token = self.token();
                    if let Some(cell) = cell_of(on) {
                        self.landings.insert(
                            token,
                            (
                                isorun_hud::damage_numbers::Landing::Hit {
                                    on_player: on == Fighter::Player,
                                    tier: strength.tier,
                                    amount: strength.amount.map(|a| a as i64),
                                },
                                cell,
                            ),
                        );
                    }
                    {
                        let view = RendererView { renderer: &self.parts.view.renderer, width, animation_speed: self.client_settings.animation_speed };
                        self.audio.combat(&audio_event, &scene, &view, Some(token));
                    }
                    self.act(on, ActorAction::Hit, from, token);
                }
                CombatEvent::Miss { on, from } => {
                    let token = self.token();
                    if let Some(cell) = cell_of(on) {
                        self.landings.insert(token, (isorun_hud::damage_numbers::Landing::Miss, cell));
                    }
                    {
                        let view = RendererView { renderer: &self.parts.view.renderer, width, animation_speed: self.client_settings.animation_speed };
                        self.audio.combat(&audio_event, &scene, &view, Some(token));
                    }
                    self.act(on, ActorAction::Miss, from, token);
                }
                CombatEvent::Death { on, from } => {
                    let token = self.token();
                    if let Some(cell) = cell_of(on) {
                        self.landings.insert(token, (isorun_hud::damage_numbers::Landing::Death { on_player: on == Fighter::Player }, cell));
                    }
                    {
                        let view = RendererView { renderer: &self.parts.view.renderer, width, animation_speed: self.client_settings.animation_speed };
                        self.audio.combat(&audio_event, &scene, &view, Some(token));
                    }
                    self.act(on, ActorAction::Death, from, token);
                }
            }
        }
        // a blow the renderer never showed (its actor gone) lands nothing: the table does not grow
        if self.landings.len() > 256 {
            let keep = self.next_token.saturating_sub(256);
            self.landings.retain(|t, _| *t > keep);
        }
    }

    /// A new scene: the view on the player for a new level or a long jump, the pad's step
    /// confirmed, the aim arrow again.
    fn on_scene(&mut self, scene: &Arc<Scene>, now: f64) {
        let hops = std::mem::take(&mut self.hops);
        let jumped = std::mem::replace(&mut self.jumped, false);
        let st = &self.session.state;
        if scene.player_on_level {
            let p = scene.player;
            let level = format!("{}:{}", st.player.place, st.player.depth);
            let new_level = level != self.last_level;
            self.last_level = level;
            let moved = self.last_pos != Some(p);
            if moved || new_level {
                let (dx, dy) = self.last_pos.map_or((0, 0), |l| (p.0 - l.0, p.1 - l.1));
                let same_level = !new_level && self.last_pos.is_some() && (dx, dy) != (0, 0);
                // a level change or a long jump (a teleport): the view is put on the player at
                // once; a walk of a few cells is hopped by the player's actor, and the view follows
                if !same_level || dx.abs().max(dy.abs()) > SNAP_CELLS {
                    self.camera.snap_to(p.0, p.1);
                }
                let own = self.runner.step_for_move(dx, dy, now, if jumped { &[] } else { &hops });
                if same_level && own.is_some() && self.router.pad_step == own && !self.session.watching() {
                    if let Some(h) = self.router.hints.as_mut() {
                        h.moved(hops.len().max(1) as u32);
                    }
                    self.router.pad_step = None;
                }
                self.last_pos = Some(p);
                if new_level {
                    self.parts.view.renderer.new_level();
                }
            }
        }
        self.parts.view.renderer.set_scene(scene.clone());
        // the runner's door wait, and the aim arrow for the new ground
        self.runner.on_scene();
        if self.router.aim_dir.is_some() {
            self.route(now, |r, io| r.refresh_aim(io));
        }
    }

    // ------------------------------------------------------------------ X's actions

    /// One of X's actions as tabs (action-tabs.rs), by its key. The frame lights the tab at once;
    /// crawl is asked for one menu at a time. From one of its menus that one goes away first, and
    /// the key follows once it has (`track_actions`). A press while a turn is under way only moves
    /// its target, so a quick player never has a key land in the menu that was opening.
    fn open_action_tab(&mut self, id: ActionTabId, now: f64) {
        self.action_tab_up = id;
        match &mut self.parts.client {
            Some(c) if matches!(c.kind, client::Kind::Actions(..)) => c.kind = client::Kind::Actions(id, None),
            _ => self.parts.client = Some(ClientMenu::actions(id, None)),
        }
        if let Some(t) = self.action_turn.as_mut() {
            t.target = id;
            return;
        }
        if derive_mode(&self.session.state) == Mode::Menu {
            self.close_for_turn(id, now);
        } else {
            self.send_action_key(id, now);
        }
    }

    fn close_for_turn(&mut self, target: ActionTabId, now: f64) {
        self.action_turn = Some(ActionTurn { target, sent: None, stage: TurnStage::Closing, last: None, t: now });
        self.execute(&Action::keys(vec![SeqItem::key(27)], "Cancel"), now);
    }

    fn send_action_key(&mut self, target: ActionTabId, now: f64) {
        let tab = action_tab(target);
        self.action_turn =
            Some(ActionTurn { target, sent: Some(target), stage: TurnStage::Sent, last: self.session.state.messages.lines.last().cloned(), t: now });
        self.execute(&Action::keys(vec![SeqItem::text(tab.key)], tab.label), now);
    }

    fn actions_frame_up(&self) -> bool {
        self.parts.client.as_ref().is_some_and(|c| matches!(c.kind, client::Kind::Actions(..)))
    }

    /// A turn of X's tabs, seen through: the tab's menu up (the frame goes), or crawl's line
    /// refusing it, which the frame shows where the menu would be. `z` asks before it lists, and
    /// `*` answers. Anything else crawl puts up, or nothing for a while, ends the turn.
    fn track_actions(&mut self, now: f64, drawn: bool) {
        let st = &self.session.state;
        let mode = derive_mode(st);
        let up = if mode == Mode::Menu { st.menus.last().and_then(action_tab_of) } else { None };
        let Some(turn) = self.action_turn.clone() else {
            if let Some(u) = up {
                self.action_tab_up = u;
            }
            return;
        };
        let end = |s: &mut Self| {
            if !drawn {
                return;
            }
            s.action_turn = None;
            if s.actions_frame_up() {
                s.parts.client = None;
            }
        };
        // the frame put away (B, F3) while crawl was still answering: what lands goes too
        let abandoned = !self.actions_frame_up();
        // a menu going away passes through `prompt`; anything else crawl puts up ends the turn
        let elsewhere = !matches!(mode, Mode::Command | Mode::Menu | Mode::Prompt);
        if elsewhere || now - turn.t > ACTION_TURN_MS {
            return end(self);
        }
        if turn.stage == TurnStage::Closing {
            if mode != Mode::Command {
                return;
            }
            if abandoned {
                return end(self);
            }
            return self.send_action_key(turn.target, now);
        }
        if let Some(u) = up {
            if abandoned {
                self.action_turn = None;
                self.execute(&Action::keys(vec![SeqItem::key(27)], "Cancel"), now);
                return;
            }
            if u != turn.target {
                return self.close_for_turn(turn.target, now);
            }
            self.action_tab_up = u;
            return end(self);
        }
        let Some(last) = st.messages.lines.last().cloned() else { return };
        if Some(&last) == turn.last.as_ref() {
            return;
        }
        let text = formatted_string_to_text(&last.text).trim().to_string();
        if text == OKAY_THEN {
            return;
        }
        if mode != Mode::Command {
            if turn.sent == Some(ActionTabId::Spells) && turn.stage == TurnStage::Sent && last.channel == Some(Msgch::PROMPT) {
                if let Some(t) = self.action_turn.as_mut() {
                    t.stage = TurnStage::Listing;
                    t.last = Some(last);
                }
                self.execute(&Action::keys(vec![SeqItem::text("*")], "List spells"), now);
            }
            return;
        }
        if abandoned {
            return end(self);
        }
        if turn.sent != Some(turn.target) {
            return self.send_action_key(turn.target, now);
        }
        self.action_turn = None;
        self.parts.client = Some(ClientMenu::actions(turn.target, Some(text)));
    }

    /// The pack's page and row, kept as the player leaves them, so Y brings the pack back where it was.
    fn track_pack(&mut self) {
        let m = if self.ctx.mode == Mode::Menu { self.ctx.menu.as_ref() } else { None };
        let page = m.and_then(|m| m.pack.as_ref()).and_then(|p| p.current.clone());
        let (Some(m), Some(page)) = (m, page) else {
            if self.pack_return.as_ref().is_some_and(|b| b.1) && self.ctx.mode != Mode::Menu {
                self.pack_return = None;
            }
            return;
        };
        if let Some(back) = self.pack_return.as_mut() {
            back.1 = true;
            if page != back.0 {
                return;
            }
            self.pack_return = None;
            if let Some(&key) = self.pack_row.get(&page) {
                let mut out = Vec::new();
                let mut cx = OverlayCx::new(&self.session.state, &mut self.parts.icons, &mut out);
                self.parts.overlays.hover_hotkey(&mut cx, key);
                for msg in out {
                    self.session.send(msg);
                }
            }
        }
        if let Some(h) =
            m.menu.items.get(m.menu.last_hovered.max(0) as usize).and_then(|i| i.as_ref()).and_then(|i| i.hotkeys.as_ref().and_then(|h| h.first().copied()))
        {
            self.pack_row.insert(page.clone(), h);
        }
        self.pack_page = Some(page);
    }

    /// Y's gear: the inventory screen with the dock; crawl's own pack, on the page and row it was left on, without.
    fn equipment(&mut self, now: f64) {
        if self.parts.dock_on {
            self.parts.deferred.push(Deferred::DockScreen(isorun_hud::DockButtonId::Inventory));
            return;
        }
        let seq = isorun_input::pack_tabs::open_pack_keys(&self.session.state, self.pack_page.as_deref());
        let page = if seq.len() > 1 { self.pack_page.clone().unwrap_or_else(|| "gear".into()) } else { "gear".into() };
        self.execute(&Action::keys(seq, "Inventory"), now);
        self.pack_return = Some((page, false));
    }

    // ------------------------------------------------------------------ the pointer's cursor

    /// The cursor the view shows (game.ts `cursorFor`): crawl's (the mouse or targeting one, then
    /// the map's, then the tutorial's), or the pointer's own cell, which wears CURSOR_MOUSE.
    fn cursor_for(&mut self, scene: &Scene) -> Option<SceneCursor> {
        let st = &self.session.state;
        let c = |i: usize| st.cursors.get(i).copied().flatten();
        self.cursor_is_ours = false;
        let Some(cur) = c(0).or(c(2)).or(c(1)) else {
            let cell = self.pointer_cell(scene)?;
            self.cursor_is_ours = true;
            let tile = self.session.gamedata.as_ref().and_then(|g| g.gd.icons.id("CURSOR"));
            return Some(SceneCursor { x: cell.0, y: cell.1, mode: CursorMode::Examine, tile, grid: false });
        };
        let gd = self.session.gamedata.as_ref();
        let mode = if st.ui_state == UiState::VIEW_MAP {
            CursorMode::Map
        } else if st.input_mode == MouseMode::COMMAND {
            CursorMode::Examine
        } else {
            CursorMode::Target
        };
        let tutorial = c(0).is_none() && c(2).is_none() && c(1).is_some();
        let tile = gd.and_then(|g| g.gd.icons.id(if tutorial { "TUTORIAL_CURSOR" } else { "CURSOR" }));
        Some(SceneCursor { x: cur.x, y: cur.y, mode, tile, grid: false })
    }

    /// The cell the pointer selects in command mode: nothing for a spectator, with mouse
    /// control off, outside command mode, or until the mouse moves after a key.
    fn pointer_cell(&mut self, scene: &Scene) -> Option<(i32, i32)> {
        let at = self.parts.hover?;
        if !self.parts.pointer_live || self.session.watching() || self.ctx.mode != Mode::Command || !mouse_control(&self.session.state) {
            return None;
        }
        if self.parts.map_shown {
            return self.parts.level_map.pick(scene, at);
        }
        self.parts.view.pick(at)
    }

    /// While an aim is up the pointer steers its cursor (crawl's `target_cursor` on a mouse move);
    /// outside one the pointer's moves are forgotten.
    fn sync_target(&mut self) {
        if self.ctx.mode != Mode::Targeting {
            self.last_target_sent = None;
            self.parts.hover_moved = false;
            return;
        }
        if self.session.watching() || !self.parts.hover_moved || !mouse_control(&self.session.state) {
            return;
        }
        self.parts.hover_moved = false;
        let Some(at) = self.parts.hover else { return };
        let cell = if self.parts.map_shown { self.parts.level_map.pick(&self.session.scene, at) } else { self.parts.view.pick(at) };
        let Some(cell) = cell else { return };
        if Some(cell) == self.last_target_sent {
            return;
        }
        self.last_target_sent = Some(cell);
        self.runner.outbox.push(cm::target_cursor(cell.0, cell.1));
    }

    /// The level map opened or closed: the pointer's place is the other view's now.
    fn relayout_map_switch(&mut self) {
        if self.parts.map_shown != self.level_map_was {
            self.level_map_was = self.parts.map_shown;
            self.parts.hover = None;
            self.last_cursor = None;
        }
    }

    // ------------------------------------------------------------------ drawing

    fn draw_level_map(&mut self, ui: &mut egui::Ui, scene: &Arc<Scene>, rect: egui::Rect) {
        let st = &self.session.state;
        let lm = &mut self.parts.level_map;
        self.map_tiles.set_filter(st.options.get("tile_filter_scaling").and_then(Value::as_bool) == Some(true));
        lm.set_options(&st.options);
        let p = &st.player;
        lm.minibars = Some(minibars(&st.options, p.hp, p.hp_max, p.mp, p.mp_max));
        let cell = map_cell_size(&st.options, rect.width(), rect.height());
        // dungeon_renderer.js set_view_center: the level map sits on the server's view centre (vgrdc)
        let centre = st.map.view_center.map(|c| (c.x, c.y)).unwrap_or(scene.player);
        let r = lm.ui(ui, rect, scene, &mut self.map_tiles, cell, centre, self.last_cursor.as_ref());
        let _ = r;
    }

    /// The minimap in `rect` (none when empty: the setting hides it), at `cell` points a tile (the
    /// HUD's, the setting's cell fitted to the window).
    fn draw_minimap(&mut self, ui: &mut egui::Ui, scene: &Arc<Scene>, rect: egui::Rect, cell: f32) {
        if rect.width() <= 0.0 {
            return;
        }
        let st = &self.session.state;
        self.minimap.cell = cell;
        self.minimap.set_options(&st.options);
        let cam = self.camera.camera();
        // the minimap turns with the view, so up on the map is up on the screen
        self.minimap.upright_yaw = Some(cam.yaw as f32);
        // the pointer's own hover cell stays out of the minimap; only a cursor the server placed shows there
        let cursor = if self.cursor_is_ours { None } else { self.last_cursor };
        let r = self.minimap.ui(ui, rect, scene, &mut self.map_tiles, (cam.x, cam.y), cam.yaw as f32, cursor.as_ref());
        if r.clicked() {
            self.parts.deferred.push(Deferred::Execute(level_map_action()));
        }
    }

    /// The pad's prompts for the corner (game.ts `loop`): the client list's own, or the hints'.
    fn pad_labels(&self) -> Vec<isorun_input::bindings::BindingLabel> {
        if self.parts.chat_capturing {
            return vec![];
        }
        let mut labels = match &self.parts.client {
            Some(c) => c.pad_prompts(),
            None => self.router.hints.as_ref().map(|h| h.prompts(&self.ctx, HintMode::parse(&self.client_settings.hints))).unwrap_or_default(),
        };
        // a held tap-or-hold shows its prompt while it is down, so the hold's progress has a place to show
        if let Some((b, _)) = self.router.holding {
            if !labels.iter().any(|l| l.button == b) {
                if let Some(l) = bar_labels(&self.ctx).into_iter().find(|l| l.button == b) {
                    let at = labels.iter().position(|l| l.teaching).unwrap_or(labels.len());
                    labels.insert(at, l);
                }
            }
        }
        labels
    }

    /// The details card about the cell the aim or the server's cursor is on (game.ts `updateInspect`).
    fn inspect(&self, ctx: &egui::Context, scene: &Scene) -> Option<(Vec<isorun_hud::classic::InspectLine>, Vec<String>)> {
        if self.parts.map_shown || self.parts.cell_menu.is_some() {
            return None;
        }
        let (x, y) = match self.router.aim_dir {
            Some(d) => (scene.player.0 + isorun_scene::DIR8_DX[d as usize], scene.player.1 + isorun_scene::DIR8_DY[d as usize]),
            None => {
                let c = self.last_cursor.filter(|c| c.mode != CursorMode::Map)?;
                // on touch the card about the cell under the finger shows only while it rests
                // there (a long press), not for the moment a tap leaves its cursor on the cell
                if self.cursor_is_ours && isorun_ui::pointer::touch_mode(ctx) && !isorun_ui::pointer::card_shows(ctx, true, self.parts.view.rect) {
                    return None;
                }
                (c.x, c.y)
            }
        };
        let lines: Vec<isorun_hud::classic::InspectLine> = isorun_input::cell_info::cell_details(scene, x, y)
            .into_iter()
            .map(|d| isorun_hud::classic::InspectLine { text: d.text, tone: d.tone.map(tone_name), wound: d.wound })
            .collect();
        if lines.is_empty() {
            return None;
        }
        let hints = if self.router.aim_dir.is_none() && self.parts.pointer_live { self.click_hints(scene, x, y) } else { vec![] };
        Some((lines, hints))
    }

    /// What the clicks do on a cell with a monster (mouse_control.js handle_cell_tooltip).
    fn click_hints(&self, scene: &Scene, x: i32, y: i32) -> Vec<String> {
        let st = &self.session.state;
        if !mouse_control(st) || st.map.cell(x, y).is_none_or(|c| c.mon.is_none()) {
            return vec![];
        }
        let im = st.input_mode;
        let can_target = matches!(im, MouseMode::TARGET | MouseMode::TARGET_DIR | MouseMode::TARGET_PATH);
        let can_describe = can_target || im == MouseMode::COMMAND || st.ui_state == UiState::VIEW_MAP;
        let visible = scene.cell(x, y).is_some_and(|c| c.visibility == isorun_scene::Visibility::Visible);
        let mut out = vec![];
        if can_target && visible {
            out.push("Left click: select target".into());
        }
        if can_describe {
            out.push("Right click: describe".into());
        }
        out
    }

    fn draw_hud(&mut self, ui: &mut egui::Ui, scene: &Arc<Scene>, view_rect: egui::Rect, now: f64) {
        let ctx = ui.ctx().clone();
        let labels = self.pad_labels();
        let prompts = convert::prompts(&labels);
        let holding = self.router.holding.map(|(b, f)| isorun_hud::classic::HoldProgress { button: convert::glyph(b), fraction: f });
        let inspect = self.inspect(ui.ctx(), scene);
        let st = &self.session.state;
        let (device, _) = convert::device(self.parts.last_input);
        let renderer = &self.parts.view.renderer;
        let map_shown = self.parts.map_shown;
        let screen_of = |x: i32, y: i32| -> Option<egui::Pos2> {
            if map_shown {
                return None;
            }
            let (sx, sy) = renderer.screen_of(x as f32, y as f32);
            Some(view_rect.min + egui::vec2(sx, sy))
        };
        let mut out = Vec::new();
        let mut commands = Vec::new();
        let client_overlay = self.parts.client.is_some() || self.parts.menus.is_open();
        let chat = self.chat_on();
        let mut hud = std::mem::take(&mut self.parts.hud);
        let output = {
            let mut cx = HudCx {
                state: st,
                scene,
                tiles: &mut self.parts.icons,
                settings: &self.client_settings,
                now: isorun_client::platform::seconds(),
                mode: convert::hud_mode(self.ctx.mode),
                device,
                pad: convert::pad_kind(self.pad.kind),
                touch: self.touch,
                facing: self.camera.facing(),
                map_view: map_shown,
                dock_context: None,
                prompts: &prompts,
                menu_prompts: &[],
                holding,
                panel: None,
                inspect: inspect.as_ref().map(|(l, h)| (l.as_slice(), h.as_slice())),
                chat,
                client_overlay,
                screen_of: &screen_of,
                out: &mut out,
                commands: &mut commands,
            };
            hud.ui(ui, &mut cx)
        };
        self.parts.hud = hud;
        let _ = (output, &ctx);
        for m in out {
            self.runner.outbox.push(m);
        }
        for c in commands {
            self.hud_command(c, now);
        }
    }

    /// What the HUD asked of the game screen.
    fn hud_command(&mut self, c: HudCommand, now: f64) {
        match c {
            HudCommand::Run(a) => match convert::hud_action(a) {
                HudRun::Action(a) => self.execute(&a, now),
                HudRun::Send(m) => self.send(m, now),
                HudRun::SendThen(m, seq) => self.with_runner(now, |r, env, h| r.send_then(m, &seq, env, h)),
            },
            HudCommand::SystemMenu => self.open_system(),
            HudCommand::ActionTab(tab) => {
                let id = match tab {
                    "abilities" => ActionTabId::Abilities,
                    "quiver" => ActionTabId::Quiver,
                    _ => ActionTabId::Spells,
                };
                self.parts.client = None;
                self.open_action_tab(id, now);
            }
            HudCommand::SelectMonster { .. } => {}
            HudCommand::MinimapClick => self.execute(&level_map_action(), now),
            HudCommand::PanelSettings => self.parts.menus.open_settings(),
            HudCommand::CloseClientOverlay => {
                self.parts.client = None;
                self.parts.menus.close();
            }
            HudCommand::PanelShown(shown) => {
                // action_panel.js show_panel / hide_panel: the change goes back to the rc
                let st = &mut self.session.state;
                if st.options.get("action_panel_disabled").and_then(Value::as_bool) == Some(true) {
                    return;
                }
                st.options.insert("action_panel_show".into(), Value::Bool(shown));
                st.rev.player += 1;
                if !self.session.watching() {
                    self.session.send(cm::set_option("action_panel_show", &Value::Bool(shown)));
                }
            }
            HudCommand::Prompt(i) => {
                if let Some(l) = self.pad_labels().get(i) {
                    if !l.teaching {
                        let a = l.action.clone();
                        self.execute(&a, now);
                    }
                }
            }
            HudCommand::Choices { title, choices, remember } => {
                let rows = choices
                    .into_iter()
                    .enumerate()
                    .map(|(i, c)| Row { label: c.label, sub: c.sub, key: None, tile: c.tile, tiles: c.tiles, sep: c.sep, run: Some(Run::HudChoice(i)) })
                    .collect();
                self.parts.client = Some(ClientMenu::choices(&title, rows, remember, true));
            }
        }
    }

    /// A HUD call that needs its context (a dock button, a move of the pad's cursor on the dock).
    fn hud_call(&mut self, _ctx: &egui::Context, now: f64, f: impl FnOnce(&mut Hud, &mut HudCx)) {
        let scene = self.session.scene.clone();
        let st = &self.session.state;
        let mut out = Vec::new();
        let mut commands = Vec::new();
        let no_screen = |_: i32, _: i32| None;
        let chat = self.chat_on();
        let client_overlay = self.parts.client.is_some() || self.parts.menus.is_open();
        let mut hud = std::mem::take(&mut self.parts.hud);
        {
            let mut cx = HudCx {
                state: st,
                scene: &scene,
                tiles: &mut self.parts.icons,
                settings: &self.client_settings,
                now: isorun_client::platform::seconds(),
                mode: convert::hud_mode(self.ctx.mode),
                device: convert::device(self.parts.last_input).0,
                pad: convert::pad_kind(self.pad.kind),
                touch: self.touch,
                facing: self.camera.facing(),
                map_view: self.parts.map_shown,
                dock_context: None,
                prompts: &[],
                menu_prompts: &[],
                holding: None,
                panel: None,
                inspect: None,
                chat,
                client_overlay,
                screen_of: &no_screen,
                out: &mut out,
                commands: &mut commands,
            };
            f(&mut hud, &mut cx);
        }
        self.parts.hud = hud;
        for m in out {
            self.runner.outbox.push(m);
        }
        for c in commands {
            self.hud_command(c, now);
        }
    }

    /// Crawl's menus, popups, prompts and the level-up banner (the overlays crate).
    fn draw_overlays(&mut self, ctx: &egui::Context, now: f64) {
        let st = &self.session.state;
        let mut out = Vec::new();
        let hide = self.parts.hud.screen_open() == Some(isorun_hud::screens::ScreenId::Inventory);
        // crawl's stash search under the Find screen is the screen's: its menu is not drawn
        let finding = self.parts.find.hides();
        let probing = |m: &isorun_webtiles::MenuState| (hide && isorun_input::pack_tabs::is_pack(m)) || (finding && isorun_find::entry::is_stash_menu(m));
        let mut back = false;
        {
            let mut on_back = || back = true;
            let mut cx = OverlayCx::new(st, &mut self.parts.icons, &mut out);
            cx.watching = self.session.watching();
            cx.device = convert::device(self.parts.last_input).1;
            cx.touch = self.touch;
            cx.dock = self.client_settings.hud == "dock";
            cx.ui_scale = self.client_settings.ui_scale.max(0.5);
            cx.hide_menu = Some(&probing);
            cx.back = Some(&mut on_back);
            cx.bottom = self.parts.touch_bar_h;
            cx.hide_text = finding && isorun_find::driver::stash_prompt(st);
            self.parts.overlays.ui(ctx, &mut cx);
        }
        for m in out {
            self.runner.outbox.push(m);
        }
        if back {
            self.back(now);
        }
    }

    /// The Find screen's driver follows crawl: it answers crawl's stash search prompt and reads
    /// its results (isorun_find::Driver). The screen opens with it.
    fn update_find(&mut self, now: f64) {
        let was = self.parts.find.open();
        let gd = self.session.gamedata.clone();
        let name_of = |r: &isorun_webtiles::TileRef| gd.as_ref().and_then(|g| isorun_find::entry::tile_name(&g.gd, r)).map(str::to_string);
        let mut out = Vec::new();
        self.parts.find.update(&self.session.state, self.session.watching(), now, &name_of, &mut out);
        if !was && self.parts.find.open() {
            self.parts.find_screen.opened();
        }
        for m in out {
            self.send(m, now);
        }
    }

    /// The Find screen, while it is up.
    fn draw_find(&mut self, ctx: &egui::Context, now: f64) {
        if !self.parts.find.open() {
            self.parts.find_typing = false;
            return;
        }
        let p = &self.session.state.player;
        let here = isorun_find::entry::level_label(&p.place, p.depth);
        let events = {
            let view = isorun_find::View {
                driver: &self.parts.find,
                here: &here,
                scale: self.client_settings.ui_scale.max(0.5),
                pad: self.parts.last_input == InputDevice::Pad,
                touch: self.touch,
            };
            self.parts.find_screen.ui(ctx, &view, &mut self.parts.icons)
        };
        self.parts.find_typing = self.parts.find_screen.typing(ctx);
        let mut out = Vec::new();
        for e in events {
            match e {
                isorun_find::FindEvent::Travel(entry) => self.parts.find.travel(&entry, &mut out),
                isorun_find::FindEvent::Close => self.parts.find.close(&self.session.state, now, &mut out),
                isorun_find::FindEvent::Search(words) => self.parts.find.search(&words, now, &mut out),
                isorun_find::FindEvent::ToggleUseless => self.parts.find.toggle_useless(&mut out),
            }
        }
        if !self.parts.find.open() {
            self.parts.find_typing = false;
            ctx.memory_mut(|m| m.surrender_focus(isorun_find::screen::field_id()));
        }
        for m in out {
            self.send(m, now);
        }
    }

    /// The game's Back (the phone's, a popup's close button).
    fn back(&mut self, now: f64) {
        self.route(now, |r, io| r.back(io));
    }

    /// The client's own lists over the game.
    fn draw_client(&mut self, ctx: &egui::Context) {
        let keyboard = self.parts.last_input != InputDevice::Pad;
        let scale = self.client_settings.ui_scale.max(0.5);
        if let Some(c) = self.parts.client.as_mut() {
            let (e, stays) = c.ui(ctx, &mut self.parts.icons, scale, keyboard);
            if !stays {
                self.parts.client = None;
            }
            if let Some(e) = e {
                self.parts.deferred.push(Deferred::Client(e));
            }
        }
        // the cell menu a long press opened
        if let Some(m) = &self.parts.cell_menu {
            let hu = isorun_input::cell_menu::hud_unit(ctx.content_rect().size(), scale);
            let r = m.show(ctx, hu);
            if let Some(row) = r.picked {
                let (x, y) = (m.x, m.y);
                self.parts.cell_menu = None;
                match row.run {
                    isorun_input::cell_menu::CellMenuRun::Click(button) => {
                        let now = now_ms();
                        self.with_runner(now, |r, env, h| r.click_cell(x, y, button, env, h));
                    }
                    isorun_input::cell_menu::CellMenuRun::Action(a) => self.parts.deferred.push(Deferred::Execute(a)),
                }
            } else if isorun_ui::pointer::any_pressed_at(&ctx).is_some_and(|(p, _)| !r.rect.contains(p)) {
                // a press outside the cell menu only puts it away
                self.parts.cell_menu = None;
            }
        }
    }

    /// The touch bar: the pad's bindings as buttons, on a touch screen, in every mode but command mode.
    fn draw_touch(&mut self, ui: &mut egui::Ui, now: f64) {
        if !self.touch {
            return;
        }
        let shown = !self.session.watching()
            && self.parts.client.is_none()
            && !self.parts.find.open()
            && !self.parts.menus.is_open()
            && !self.parts.chat_capturing
            && !self.parts.overlays.level_up.choosing();
        let screen = isorun_ui::safe::rect(ui.ctx());
        let ctx = self.ctx.clone();
        let (a, bar) = isorun_input::touch_bar::touch_bar(ui, &ctx, shown, screen);
        self.parts.touch_bar_h = bar.map_or(0.0, |b| b.height());
        if let Some(a) = a {
            self.parts.pointer_live = false;
            self.execute(&a, now);
        }
        // the full screen and menu (the system menu) buttons in the top-left corner (touch.css `.touch-corner`)
        let ctx = ui.ctx().clone();
        let mut x = screen.left() + 8.0;
        let full = self.opts.can_fullscreen;
        let mut pressed = None;
        for (i, kind) in [(0, "full"), (1, "menu")] {
            if kind == "full" && !full {
                continue;
            }
            let r = egui::Rect::from_min_size(egui::pos2(x, screen.top() + 8.0), egui::vec2(40.0, 40.0));
            x += 48.0;
            let resp = egui::Area::new(egui::Id::unique(("isorun-touch-corner", i))).order(egui::Order::Middle).fixed_pos(r.min).show(&ctx, |ui| {
                let (r, resp) = ui.allocate_exact_size(r.size(), egui::Sense::click());
                let p = ui.painter();
                // radial-gradient(circle at 50% 30%, rgba(40, 36, 31, 0.92), rgba(10, 9, 8, 0.92) 80%), a bronze rim, a soft drop shadow
                p.circle_filled(r.center() + egui::vec2(0.0, 2.0), 22.0, egui::Color32::from_black_alpha(90));
                p.circle_filled(r.center(), 20.0, egui::Color32::from_rgba_unmultiplied(10, 9, 8, 235));
                p.circle_filled(r.center() - egui::vec2(0.0, 4.0), 13.0, egui::Color32::from_rgba_unmultiplied(40, 36, 31, 120));
                p.circle_stroke(r.center(), 20.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(0x6b, 0x55, 0x38)));
                let c = r.center();
                let bronze = egui::Stroke::new(1.8, egui::Color32::from_rgb(0xd8, 0xa8, 0x5a));
                if kind == "full" {
                    // FULLSCREEN_ICON: the four corners
                    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                        let k = c + egui::vec2(8.0 * sx, 8.0 * sy);
                        p.line_segment([k, k - egui::vec2(5.0 * sx, 0.0)], bronze);
                        p.line_segment([k, k - egui::vec2(0.0, 5.0 * sy)], bronze);
                    }
                } else {
                    // SETTINGS_ICON: a ring and eight spokes
                    p.circle_stroke(c, 3.0, bronze);
                    for i in 0..8 {
                        let a = i as f32 * std::f32::consts::FRAC_PI_4;
                        let d = egui::vec2(a.cos(), a.sin());
                        p.line_segment([c + d * 7.0, c + d * 10.0], bronze);
                    }
                }
                resp
            });
            if resp.inner.clicked() {
                pressed = Some(kind);
            }
        }
        match pressed {
            Some("full") => {
                let on = isorun_ui::fullscreen::is_on(&ctx);
                isorun_ui::fullscreen::set(&ctx, !on);
            }
            Some(_) => self.open_system(),
            None => {}
        }
    }

    fn open_system(&mut self) {
        self.parts.show_system();
    }

    /// What the Isorun menu asked of the game.
    fn menu_actions(&mut self, ctx: &egui::Context, actions: Vec<MenuAction>) -> Option<Outcome> {
        for a in actions {
            match a {
                MenuAction::SettingsChanged => self.apply_settings(ctx),
                MenuAction::Lights(l) => self.parts.view.renderer.set_lights(l),
                MenuAction::Resume => {
                    if std::mem::take(&mut self.settings_from_system) {
                        self.parts.show_system();
                    }
                }
            }
        }
        None
    }

    /// What the player did on a client list.
    fn client_event(&mut self, e: ClientEvent, now: f64) {
        match e {
            ClientEvent::Fire(Run::HudChoice(i)) => {
                let ctx = egui::Context::default();
                self.hud_call(&ctx, now, |hud, cx| hud.choose(i, cx));
            }
            ClientEvent::Fire(Run::Interact(alt)) => self.route(now, |r, io| r.contextual(alt, io)),
            ClientEvent::Fire(Run::Send(m)) => self.send(m, now),
            ClientEvent::Fire(Run::Execute(a)) => self.execute(&a, now),
            ClientEvent::Fire(Run::Resume) => {}
            ClientEvent::Fire(Run::Settings) => {
                // the settings' last Back lands on the Isorun menu they were reached from (overlays.ts `showSettings(again)`)
                self.parts.menus.open_settings();
                self.settings_from_system = true;
            }
            ClientEvent::Fire(Run::Fullscreen(on)) => self.parts.deferred.push(Deferred::Fullscreen(on)),
            ClientEvent::Fire(Run::Chat) => self.parts.hud.chat.focus(),
            ClientEvent::Fire(Run::Leave) => self.parts.leave = true,
            ClientEvent::Closed { from_hud } => {
                if from_hud {
                    let ctx = egui::Context::default();
                    self.hud_call(&ctx, now, |hud, cx| hud.choices_closed(cx));
                }
            }
            ClientEvent::Tab(id) => self.open_action_tab(id, now),
        }
    }

    /// The interaction chooser (game.ts `uiOp('interact')`): the feature or the pile.
    pub(crate) fn interact_rows(ctx: &Context) -> Vec<Row> {
        vec![
            Row { label: contextual_label(ctx, false), run: Some(Run::Interact(false)), ..Default::default() },
            Row { label: "Pick up items".into(), run: Some(Run::Interact(true)), ..Default::default() },
        ]
    }

    /// The game screen goes: the view's angles kept, the sounds fade.
    pub fn close(&mut self) {
        self.save_view();
        self.runner.abort_sequence();
        let _ = UiOp::System;
    }
}

/// Let go of egui's keyboard focus on anything but a text field. Tab is crawl's (autofight) and
/// egui's too: with nothing focused it gives the focus to the first button drawn, the arrows then
/// move it from button to button, and Space or Enter clicks it, so a player who aimed with the
/// arrows and stepped with Space opened a quick slot or a screen now and then. The game screen's
/// buttons are the mouse's and the pad's (the dock has a cursor of its own); the keys are crawl's.
fn drop_button_focus(ctx: &egui::Context) {
    let Some(id) = ctx.memory(|m| m.focused()) else { return };
    if egui::text_edit::TextEditState::load(ctx, id).is_none() {
        ctx.memory_mut(|m| m.surrender_focus(id));
    }
}

/// The pointer is over a panel of the screen's (a menu, the dock, a popup): not over the view.
fn pointer_over_panel(ctx: &egui::Context) -> bool {
    ctx.pointer_latest_pos().is_some_and(|p| pointer_over_panel_at(ctx, p))
}

fn pointer_over_panel_at(ctx: &egui::Context, p: egui::Pos2) -> bool {
    ctx.layer_id_at(p).is_some_and(|l| l.order != egui::Order::Background)
}

fn fighter(f: Fighter) -> isorun_audio::Fighter {
    match f {
        Fighter::Player => isorun_audio::Fighter::Player,
        Fighter::Monster(id) => isorun_audio::Fighter::Monster(id),
        Fighter::Unseen(cell) => isorun_audio::Fighter::Unseen(cell),
    }
}

fn tone_name(t: isorun_input::cell_info::Tone) -> &'static str {
    use isorun_input::cell_info::Tone;
    match t {
        Tone::Trivial => "trivial",
        Tone::Easy => "easy",
        Tone::Tough => "tough",
        Tone::Nasty => "nasty",
        Tone::Invisible => "note",
        Tone::Friendly => "friendly",
        Tone::Neutral => "neutral",
        Tone::Note => "note",
    }
}

/// `tile_web_mouse_control`: clicks on the view act.
pub fn mouse_control(st: &GameState) -> bool {
    st.options.get("tile_web_mouse_control").and_then(Value::as_bool) != Some(false)
}

fn audio_settings(s: &isorun_client::servers::Settings) -> AudioSettings {
    AudioSettings {
        master_volume: s.master_volume,
        effects_volume: s.effects_volume,
        ambient_volume: s.ambient_volume,
        music_volume: s.music_volume,
        muted: s.muted,
    }
}

/// Every pad button that can be held, for the router's holds.
const ALL_BUTTONS: [isorun_input::Button; 17] = {
    use isorun_input::Button as B;
    [B::A, B::B, B::X, B::Y, B::LB, B::RB, B::LT, B::RT, B::Select, B::Start, B::L3, B::R3, B::DU, B::DD, B::DL, B::DR, B::Home]
};

/// A pad event as the front end's menus read it.
fn front_pad(ev: &isorun_input::PadEvent) -> Option<isorun_front::PadEvent> {
    use isorun_front::{NavDir, PadButton, PadEvent as F};
    use isorun_input::{Button as B, PadEvent as P};
    match ev {
        P::Press { button, .. } => Some(F::Press(match button {
            B::A => PadButton::A,
            B::B => PadButton::B,
            B::X => PadButton::X,
            B::Y => PadButton::Y,
            B::LB => PadButton::LB,
            B::RB => PadButton::RB,
            B::Start => PadButton::Start,
            B::Select => PadButton::Select,
            _ => return None,
        })),
        P::Dir { dir: Some(d), .. } | P::DirRepeat { dir: d, .. } => Some(F::Dir(match d {
            0 => NavDir::Up,
            4 => NavDir::Down,
            6 => NavDir::Left,
            2 => NavDir::Right,
            _ => return None,
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: egui::Key) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }
    }

    /// Frames of two buttons with these keys one frame each; whether a button was clicked.
    fn clicked_by(keys: &[egui::Key], drop_focus: bool) -> bool {
        let ctx = egui::Context::default();
        let mut clicked = false;
        for key in keys.iter().copied().chain([egui::Key::F20]) {
            let input = egui::RawInput { events: vec![press(key)], ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                if drop_focus {
                    drop_button_focus(ui.ctx());
                }
                clicked |= ui.button("quick slot").clicked() | ui.button("inventory").clicked();
            });
            out.textures_delta.clear();
        }
        clicked
    }

    #[test]
    fn tab_then_space_clicks_no_button() {
        use egui::Key::{ArrowDown, Space, Tab};
        assert!(clicked_by(&[Tab, ArrowDown, Space], false), "egui's own focus clicks a button");
        assert!(!clicked_by(&[Tab, ArrowDown, Space], true));
    }
}
