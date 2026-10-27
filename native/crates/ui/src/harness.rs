//! A window that plays an offline game around a component under test: the game is crawl on this
//! device (the native engine), drawn by the isometric view, and the component draws over it
//! each frame. The keys the component leaves go to crawl, turned to the screen as the game
//! screen turns them. For trying a port against the web build's look on a real game state:
//!
//! ```ignore
//! isorun_ui::harness::run("overlays", |ui, game| { my_component.ui(ui, &game.session.state); false })
//! ```
//!
//! `ISORUN_PROFILE` names the profile (default `harness`), so its save and rc file stay apart;
//! `ISORUN_DATA_DIR` moves the whole store.

use crate::keys;
use crate::view3d::View3d;
use eframe::egui_wgpu::RenderState;
use isorun_client::servers;
use isorun_client::session::{Session, SessionEvent};
use isorun_render::IsoCamera;
use isorun_webtiles::{cm, key_message, Phase};
use std::sync::Arc;

/// The game around the component: its session (state, send) and the view.
pub struct Game {
    pub session: Session,
    pub view: View3d,
    pub camera: IsoCamera,
    pub rs: RenderState,
    tiles_set: bool,
    played: bool,
    last: f64,
}

impl Game {
    pub fn send(&mut self, msg: serde_json::Value) {
        self.session.send(msg);
    }

    /// Poll the connection, log in and start the game once the lobby is up.
    fn tick(&mut self) {
        for e in self.session.poll() {
            if let SessionEvent::Closed(why) = e {
                log::warn!("closed: {why}");
            }
        }
        self.session.flush_scene();
        let lobby = &self.session.state.lobby;
        if self.session.state.phase == Phase::Lobby && lobby.username.is_some() && !self.played {
            if let Some(g) = lobby.games.first().cloned() {
                self.played = true;
                self.session.send(cm::play(&g.id));
            }
        }
        if !self.tiles_set {
            if let Some(gd) = self.session.gamedata.clone() {
                self.tiles_set = true;
                self.view.renderer.set_tiles(gd);
                self.view.renderer.new_level();
            }
        }
    }

    /// Whether the game is up, with its tiles.
    pub fn playing(&self) -> bool {
        self.session.state.phase == Phase::Playing && self.tiles_set
    }

    /// Draw the view in `rect`.
    pub fn draw_view(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        if !self.playing() {
            return;
        }
        self.view.renderer.set_scene(self.session.scene.clone());
        let now = isorun_client::platform::seconds();
        let dt = (now - self.last).clamp(0.0, 0.1) as f32;
        self.last = now;
        let p = self.session.scene.player;
        self.camera.update(dt, p.0, p.1);
        self.view.renderer.follow(&self.camera);
        self.view.paint(ui, &self.rs, rect);
    }
}

type Component = Box<dyn FnMut(&mut egui::Ui, &mut Game) -> bool>;

struct HarnessApp {
    game: Game,
    component: Component,
}

impl eframe::App for HarnessApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.game.tick();
        let rect = ui.max_rect();
        self.game.draw_view(ui, rect);
        if !self.game.playing() {
            ui.centered_and_justified(|ui| ui.label(format!("Starting… {:?}", self.game.session.state.phase)));
        }
        let consumed = (self.component)(ui, &mut self.game);
        let mut sent = Vec::new();
        if !consumed && self.game.session.playing() {
            for code in keys::crawl_keys(ui, self.game.camera.facing()) {
                sent.push(code);
                self.game.session.send(key_message(code));
            }
        }
        if !sent.is_empty() {
            log::info!("keys {sent:?}");
        }
        // what the game is at, for whoever drives the harness from outside
        let st = &self.game.session.state;
        let status =
            format!("{:?} · ui {} · menus {} · lines {} · input_mode {}", st.phase, st.ui.len(), st.menus.len(), st.messages.lines.len(), st.input_mode);
        ui.painter().text(egui::pos2(6.0, 4.0), egui::Align2::LEFT_TOP, status, egui::FontId::monospace(11.0), egui::Color32::from_gray(140));
        ui.ctx().request_repaint();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }
}

/// Open the window and play. `component` draws over the view each frame and says whether it
/// took this frame's keys (true: none go to crawl).
#[cfg(not(target_arch = "wasm32"))]
pub fn run(title: &str, component: impl FnMut(&mut egui::Ui, &mut Game) -> bool + 'static) -> eframe::Result {
    let _ = env_logger::try_init();
    let profile = std::env::var("ISORUN_PROFILE").unwrap_or_else(|_| "harness".into());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title(title).with_inner_size([1600.0, 1000.0]),
        wgpu_options: crate::wgpu_options(),
        ..Default::default()
    };
    let mut component: Option<Component> = Some(Box::new(component));
    eframe::run_native(
        title,
        options,
        Box::new(move |cc| {
            crate::install(&cc.egui_ctx);
            let rs = cc.wgpu_render_state.clone().expect("wgpu");
            let ctx = cc.egui_ctx.clone();
            let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
            assert!(servers::offline_offered(), "no offline engine: build crawl natively (engine/native.sh)");
            let session = Session::new(servers::offline_server(), Some(profile.clone()), wake);
            let mut camera = IsoCamera::default();
            if let Some(v) = servers::get_saved_view() {
                camera.set_angles(v.angles);
            }
            let game = Game { view: View3d::new(&rs), session, camera, rs, tiles_set: false, played: false, last: isorun_client::platform::seconds() };
            Ok(Box::new(HarnessApp { game, component: component.take().unwrap() }))
        }),
    )
}
