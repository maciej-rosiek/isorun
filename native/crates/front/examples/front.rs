//! The front end in a plain eframe window: the title screen over the turning room, the accounts,
//! the servers, the settings. Play on this device reaches a game, drawn by a stand-in game view
//! (the iso view, the message lines, the keys to crawl); Escape over it opens the Isorun menu, and
//! a game that ends (or Leave game) comes back to the front end.
//!
//!     ISORUN_PROFILE=front ISORUN_DATA_DIR=/tmp/front cargo run -p isorun-front --example front
//!
//! `--size WxH` opens the window at that size (a phone's: 390x844, 844x390); `--touch` lays it
//! out for touch (no mouse legend).

use eframe::egui_wgpu::RenderState;
use isorun_client::session::{Session, SessionEvent};
use isorun_front::{FrontAction, FrontEnd, FrontOptions, GameEnd, GameMenus, MenuAction, SystemOptions};
use isorun_render::IsoCamera;
use isorun_ui::view3d::View3d;
use isorun_webtiles::{key_message, Phase};

/// A stand-in for the game screen: the session's game in the iso view, with its message lines.
struct Game {
    session: Box<Session>,
    view: View3d,
    camera: IsoCamera,
    tiles_set: bool,
    started: bool,
    last: f64,
    menus: GameMenus,
}

enum Over {
    None,
    Lobby,
    Dropped,
    Leave,
}

impl Game {
    fn new(session: Box<Session>, rs: &RenderState) -> Self {
        let mut view = View3d::new(rs);
        let s = isorun_front::settings::get();
        isorun_front::settings::apply_graphics(&mut view.renderer, &s, 1.0);
        Game { session, view, camera: IsoCamera::default(), tiles_set: false, started: false, last: isorun_client::platform::seconds(), menus: GameMenus::default() }
    }

    fn ui(&mut self, ui: &mut egui::Ui, rs: &RenderState) -> Over {
        for e in self.session.poll() {
            if let SessionEvent::Closed(_) = e {
                return Over::Dropped;
            }
        }
        self.session.flush_scene();
        let st = &self.session.state;
        if matches!(st.phase, Phase::Playing | Phase::Watching) {
            self.started = true;
        }
        if self.started && st.phase == Phase::Lobby {
            return Over::Lobby;
        }
        if !self.tiles_set {
            if let Some(gd) = self.session.gamedata.clone() {
                self.tiles_set = true;
                let _ = self.view.renderer.set_torch_model(isorun_client::testbed::TORCH_GLB);
                self.view.renderer.set_tiles(gd);
                self.view.renderer.new_level();
            }
        }
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);
        if self.tiles_set && self.started {
            self.view.renderer.set_scene(self.session.scene.clone());
            let now = isorun_client::platform::seconds();
            let dt = (now - self.last).clamp(0.0, 0.1) as f32;
            self.last = now;
            let p = self.session.scene.player;
            self.camera.update(dt, p.0, p.1);
            self.view.renderer.follow(&self.camera);
            self.view.paint(ui, rs, rect);
        } else {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Entering the dungeon…", egui::FontId::monospace(16.0), egui::Color32::from_gray(150));
        }
        let lines: Vec<String> = self.session.state.messages.lines.iter().rev().take(6).rev().map(|m| m.text.clone()).collect();
        let mut y = rect.bottom() - 8.0 - lines.len() as f32 * 18.0;
        for l in &lines {
            let g = ui.fonts_mut(|f| f.layout_job(isorun_ui::text::formatted(l, 15.0)));
            ui.painter().galley(egui::pos2(rect.left() + 8.0, y), g, egui::Color32::WHITE);
            y += 18.0;
        }
        // the Isorun menu over the game: Escape opens it while crawl has nothing of its own up
        let ctx = ui.ctx().clone();
        let mut over = Over::None;
        let mut actions = Vec::new();
        if self.menus.is_open() {
            let (_, a) = self.menus.keys(&ctx);
            actions.extend(a);
        } else {
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let crawl_menu = !self.session.state.menus.is_empty() || !self.session.state.ui.is_empty() || self.session.state.messages.more;
            if escape && !crawl_menu {
                let watching = self.session.state.watching.is_some();
                self.menus.open_system(SystemOptions { spectating: watching, in_game: true, chat: !self.session.server.offline, can_fullscreen: true, fullscreen: false });
            } else if self.session.playing() && !self.menus.tuning() {
                for code in isorun_ui::keys::crawl_keys(ui, self.camera.facing()) {
                    self.session.send(key_message(code));
                }
            }
        }
        actions.extend(self.menus.ui(&ctx));
        for a in actions {
            match a {
                MenuAction::Keys(k) => {
                    for c in k.chars() {
                        self.session.send(key_message(c as i32));
                    }
                }
                MenuAction::Leave => over = Over::Leave,
                MenuAction::SettingsChanged => {
                    let s = isorun_front::settings::get();
                    isorun_front::settings::apply_graphics(&mut self.view.renderer, &s, ctx.pixels_per_point());
                }
                MenuAction::Lights(l) => self.view.renderer.set_lights(l),
                MenuAction::Fullscreen(f) => ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(f)),
                MenuAction::Resume | MenuAction::Chat => {}
            }
        }
        ctx.request_repaint();
        over
    }
}

struct App {
    rs: RenderState,
    front: FrontEnd,
    game: Option<Game>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(game) = &mut self.game {
            let over = game.ui(ui, &self.rs);
            let end: fn(Box<Session>) -> GameEnd = match over {
                Over::None => return,
                Over::Lobby => GameEnd::Lobby,
                Over::Dropped => GameEnd::Dropped,
                Over::Leave => GameEnd::Leave,
            };
            let g = self.game.take().unwrap();
            self.front.game_over(end(g.session));
            return;
        }
        match self.front.ui(ui) {
            Some(FrontAction::Play { session, .. }) | Some(FrontAction::Watch { session, .. }) => {
                self.game = Some(Game::new(session, &self.rs));
            }
            Some(FrontAction::Quit) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
            Some(FrontAction::Fullscreen(f)) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(f)),
            Some(FrontAction::Testbed) | None => {}
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }
}

fn main() -> eframe::Result {
    let _ = env_logger::try_init();
    let args: Vec<String> = std::env::args().collect();
    let size = args.iter().position(|a| a == "--size").and_then(|i| args.get(i + 1)).and_then(|s| s.split_once('x')).and_then(|(w, h)| Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?))).unwrap_or((1600.0, 1000.0));
    let touch = args.iter().any(|a| a == "--touch");
    let options = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_title("Isorun front end").with_inner_size([size.0, size.1]), wgpu_options: isorun_ui::wgpu_options(), ..Default::default() };
    eframe::run_native(
        "Isorun front end",
        options,
        Box::new(move |cc| {
            isorun_ui::install(&cc.egui_ctx);
            let rs = cc.wgpu_render_state.clone().expect("wgpu");
            isorun_front::settings::set_phone(touch && size.0.min(size.1) < 560.0);
            let opts = FrontOptions { mouse: !touch, testbed: false, ..Default::default() };
            let front = FrontEnd::new(&cc.egui_ctx, rs.clone(), opts);
            Ok(Box::new(App { rs, front, game: None }))
        }),
    )
}
