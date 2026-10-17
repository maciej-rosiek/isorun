//! The app: the home screen (servers, the test bed), a server's lobby and the game, drawn with
//! egui over the isometric view.

use crate::input::{self, Delta};
use crate::servers::{self, ServerInfo, Settings};
use crate::session::{GamedataStatus, Session, SessionEvent};
use crate::testbed;
use crate::view3d::View3d;
use eframe::egui_wgpu::RenderState;
use isorun_render::{IsoCamera, SurfaceDetail};
use isorun_scene::Scene;
use isorun_webtiles::{cm, formatted_string_to_spans, game_link_rows, key_message, Phase};
use std::sync::Arc;

type Wake = Arc<dyn Fn() + Send + Sync>;

enum Screen {
    Home,
    Testbed(Box<Scene>),
    Server(Box<ServerScreen>),
}

struct ServerScreen {
    session: Session,
    username: String,
    password: String,
    status: String,
    tiles_set: bool,
}

pub struct IsorunApp {
    rs: RenderState,
    view: View3d,
    camera: IsoCamera,
    screen: Screen,
    settings: Settings,
    wake: Wake,
    last: f64,
    error: Option<String>,
    torch_loaded: bool,
}

/// The terminal's sixteen colours, as the web client's CSS has them.
pub const TERM_COLOURS: [egui::Color32; 16] = [
    egui::Color32::from_rgb(0x00, 0x00, 0x00),
    egui::Color32::from_rgb(0x40, 0x40, 0xff),
    egui::Color32::from_rgb(0x00, 0x80, 0x00),
    egui::Color32::from_rgb(0x00, 0x80, 0x80),
    egui::Color32::from_rgb(0x80, 0x00, 0x00),
    egui::Color32::from_rgb(0x80, 0x00, 0x80),
    egui::Color32::from_rgb(0x80, 0x80, 0x00),
    egui::Color32::from_rgb(0xc0, 0xc0, 0xc0),
    egui::Color32::from_rgb(0x80, 0x80, 0x80),
    egui::Color32::from_rgb(0x80, 0x80, 0xff),
    egui::Color32::from_rgb(0x00, 0xff, 0x00),
    egui::Color32::from_rgb(0x00, 0xff, 0xff),
    egui::Color32::from_rgb(0xff, 0x00, 0x00),
    egui::Color32::from_rgb(0xff, 0x00, 0xff),
    egui::Color32::from_rgb(0xff, 0xff, 0x00),
    egui::Color32::from_rgb(0xff, 0xff, 0xff),
];

/// A crawl formatted string as an egui layout.
pub fn formatted(text: &str, size: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for span in formatted_string_to_spans(text) {
        let colour = span.fg.map(|c| TERM_COLOURS[c as usize & 15]).unwrap_or(TERM_COLOURS[7]);
        let mut format = egui::TextFormat { font_id: egui::FontId::monospace(size), color: colour, ..Default::default() };
        if let Some(bg) = span.bg {
            format.background = TERM_COLOURS[bg as usize & 15];
        }
        job.append(&span.text, 0.0, format);
    }
    job
}

impl IsorunApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc.wgpu_render_state.clone().expect("Isorun draws with wgpu");
        let ctx = cc.egui_ctx.clone();
        let wake: Wake = Arc::new(move || ctx.request_repaint());
        crate::fonts::install(&cc.egui_ctx);
        let settings = servers::get_settings();
        let mut view = View3d::new(&rs);
        apply_settings(&mut view, &settings);
        let mut app = IsorunApp { rs, view, camera: IsoCamera::default(), screen: Screen::Home, settings, wake, last: crate::platform::seconds(), error: None, torch_loaded: false };
        if let Some(view) = servers::get_saved_view() {
            app.camera.set_angles(view.angles);
            if let Some(z) = view.zoom {
                app.camera.zoom_by(z / app.camera.zoom_goal());
            }
        }
        let args = crate::launch_args();
        if args.iter().any(|a| a == "--testbed") {
            app.open_testbed();
        }
        app
    }

    fn ensure_torch(&mut self) {
        if !self.torch_loaded {
            self.torch_loaded = true;
            if let Err(e) = self.view.renderer.set_torch_model(testbed::TORCH_GLB) {
                log::warn!("torch model: {e}");
            }
        }
    }

    fn open_testbed(&mut self) {
        match testbed::load() {
            Ok((tiles, scene)) => {
                self.ensure_torch();
                self.view.renderer.set_tiles(tiles);
                self.view.renderer.new_level();
                self.view.renderer.set_scene(Arc::new(scene.clone()));
                self.camera.snap_to(scene.player.0, scene.player.1);
                self.screen = Screen::Testbed(Box::new(scene));
            }
            Err(e) => self.error = Some(format!("The test bed did not load: {e}")),
        }
    }

    fn open_server(&mut self, server: ServerInfo) {
        let account = servers::list_accounts().into_iter().find(|a| a.server_id == server.id);
        let username = account.map(|a| a.username);
        let session = Session::new(server, username.clone(), self.wake.clone());
        self.screen = Screen::Server(Box::new(ServerScreen { session, username: username.unwrap_or_default(), password: String::new(), status: "Connecting…".into(), tiles_set: false }));
    }

    // ------------------------------------------------------------------ home

    fn home(&mut self, ui: &mut egui::Ui) {
        let mut open: Option<ServerInfo> = None;
        let mut testbed = false;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(egui::RichText::new("ISORUN").font(egui::FontId::new(64.0, crate::fonts::title())).color(egui::Color32::from_rgb(0xe8, 0xc8, 0x7a)));
                ui.label(egui::RichText::new("Dungeon Crawl Stone Soup, isometric").italics());
                ui.add_space(24.0);
                if let Some(e) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, e);
                }
                ui.heading("Play on a server");
                for s in servers::list_servers() {
                    let label = match &s.region {
                        Some(r) => format!("{} ({r})", s.name),
                        None => s.name.clone(),
                    };
                    if ui.add_sized([320.0, 32.0], egui::Button::new(label)).clicked() {
                        open = Some(s);
                    }
                }
                if servers::offline_offered() && ui.add_sized([320.0, 32.0], egui::Button::new("Play on this device")).clicked() {
                    open = Some(servers::offline_server());
                }
                ui.add_space(16.0);
                if ui.add_sized([320.0, 28.0], egui::Button::new("Test bed")).clicked() {
                    testbed = true;
                }
                ui.add_space(16.0);
                self.settings_ui(ui);
            });
        });
        if let Some(s) = open {
            self.open_server(s);
        }
        if testbed {
            self.open_testbed();
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        let before = self.settings.clone();
        egui::CollapsingHeader::new("Graphics").show(ui, |ui| {
            egui::Grid::new("graphics").num_columns(2).show(ui, |ui| {
                ui.label("Quality");
                egui::ComboBox::from_id_salt("quality").selected_text(&self.settings.graphics).show_ui(ui, |ui| {
                    for q in ["low", "medium", "high"] {
                        ui.selectable_value(&mut self.settings.graphics, q.to_string(), q);
                    }
                });
                ui.end_row();
                ui.label("Look");
                egui::ComboBox::from_id_salt("look").selected_text(&self.settings.look).show_ui(ui, |ui| {
                    for name in isorun_render::LIGHTS_NAMES {
                        ui.selectable_value(&mut self.settings.look, name.to_string(), name.to_string());
                    }
                });
                ui.end_row();
                ui.label("Surface detail");
                egui::ComboBox::from_id_salt("detail").selected_text(&self.settings.surface_detail).show_ui(ui, |ui| {
                    for d in ["off", "low", "medium", "high"] {
                        ui.selectable_value(&mut self.settings.surface_detail, d.to_string(), d);
                    }
                });
                ui.end_row();
            });
        });
        if self.settings != before {
            servers::save_settings(&self.settings);
            apply_settings(&mut self.view, &self.settings);
        }
    }

    // ------------------------------------------------------------------ the view

    /// The camera's input over the view: the wheel zooms (Shift turns), a drag pans, Alt and the
    /// arrows turn and tilt.
    fn camera_input(&mut self, ui: &egui::Ui, response: &egui::Response) {
        let (scroll, shift, alt, keys) = ui.input(|i| (i.smooth_scroll_delta.y + i.smooth_scroll_delta.x, i.modifiers.shift, i.modifiers.alt, (i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowRight), i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown))));
        if response.hovered() && scroll != 0.0 {
            if shift {
                self.camera.turn(if scroll > 0.0 { 45.0 } else { -45.0 });
            } else {
                self.camera.zoom_by((scroll * 0.002).exp());
            }
        }
        if response.dragged_by(egui::PointerButton::Primary) || response.dragged_by(egui::PointerButton::Middle) {
            let d = response.drag_delta();
            self.camera.drag(d.x, d.y);
        }
        if alt {
            if keys.0 {
                self.camera.turn(-45.0);
            }
            if keys.1 {
                self.camera.turn(45.0);
            }
            if keys.2 {
                self.camera.tilt_by(5.0);
            }
            if keys.3 {
                self.camera.tilt_by(-5.0);
            }
        }
    }

    /// Draw the iso view filling `rect`, following the player.
    fn draw_view(&mut self, ui: &mut egui::Ui, player: (i32, i32)) -> egui::Response {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        self.camera_input(ui, &response);
        let now = crate::platform::seconds();
        let dt = (now - self.last).clamp(0.0, 0.1) as f32;
        self.last = now;
        self.camera.update(dt, player.0, player.1);
        self.view.renderer.follow(&self.camera);
        if response.hovered() {
            if let Some(p) = response.hover_pos() {
                let _ = self.view.pick(p);
            }
        }
        self.view.paint(ui, &self.rs, rect);
        ui.ctx().request_repaint();
        response
    }

    fn testbed(&mut self, ui: &mut egui::Ui) {
        let Screen::Testbed(scene) = &mut self.screen else { return };
        let mut moved = false;
        let mut back = false;
        let mut turn = 0.0;
        let mut tilt = 0.0;
        ui.input(|i| {
            if i.modifiers.alt {
                return;
            }
            for (key, d) in [(egui::Key::ArrowUp, input::Dir::N), (egui::Key::ArrowDown, input::Dir::S), (egui::Key::ArrowLeft, input::Dir::W), (egui::Key::ArrowRight, input::Dir::E)] {
                if i.key_pressed(key) {
                    let (dx, dy) = d.rotated(self.camera.facing()).delta();
                    moved |= testbed::step(scene, dx, dy);
                }
            }
            if i.key_pressed(egui::Key::Escape) {
                back = true;
            }
            if i.key_pressed(egui::Key::Q) {
                turn -= 45.0;
            }
            if i.key_pressed(egui::Key::E) {
                turn += 45.0;
            }
            if i.key_pressed(egui::Key::R) {
                tilt += 5.0;
            }
            if i.key_pressed(egui::Key::F) {
                tilt -= 5.0;
            }
        });
        let player = scene.player;
        if moved {
            self.view.renderer.set_scene(Arc::new((**scene).clone()));
        }
        if turn != 0.0 {
            self.camera.turn(turn);
        }
        if tilt != 0.0 {
            self.camera.tilt_by(tilt);
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            self.draw_view(ui, player);
        });
        if back {
            self.screen = Screen::Home;
        }
    }

    // ------------------------------------------------------------------ a server

    fn server(&mut self, ui: &mut egui::Ui) {
        let Screen::Server(s) = &mut self.screen else { return };
        for e in s.session.poll() {
            match e {
                SessionEvent::Open => s.status = "Connected".into(),
                SessionEvent::Closed(why) => s.status = format!("Closed: {why}"),
                SessionEvent::Gamedata(GamedataStatus::Loading { done, total }) => s.status = format!("Loading tiles {done}/{total}"),
                SessionEvent::Gamedata(GamedataStatus::Error(e)) => s.status = format!("Tiles did not load: {e}"),
                SessionEvent::Gamedata(GamedataStatus::Ready) => s.tiles_set = false,
                SessionEvent::Scene => {}
                _ => {}
            }
        }
        if !s.tiles_set {
            if let Some(gd) = s.session.gamedata.clone() {
                s.tiles_set = true;
                if !self.torch_loaded {
                    self.torch_loaded = true;
                    let _ = self.view.renderer.set_torch_model(testbed::TORCH_GLB);
                }
                self.view.renderer.set_tiles(gd);
                self.view.renderer.new_level();
            }
        }
        let Screen::Server(s) = &mut self.screen else { return };
        let in_game = matches!(s.session.state.phase, Phase::Playing | Phase::Watching) && s.session.gamedata.is_some();
        if in_game {
            self.game(ui);
        } else {
            self.lobby(ui);
        }
    }

    fn lobby(&mut self, ui: &mut egui::Ui) {
        let Screen::Server(s) = &mut self.screen else { return };
        let mut back = false;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("◀ Back").clicked() {
                    back = true;
                }
                ui.heading(&s.session.server.name);
                ui.label(&s.status);
            });
            ui.separator();
            let lobby = s.session.state.lobby.clone();
            match &lobby.username {
                Some(name) => {
                    ui.label(format!("Logged in as {name}"));
                    let rows = game_link_rows(&lobby.games);
                    let mut play = None;
                    ui.horizontal_wrapped(|ui| {
                        for g in rows.latest.iter().chain(rows.trunk.iter()).chain(rows.other.iter()) {
                            let label = match &g.save {
                                Some(save) => format!("{} — {save}", g.label),
                                None => g.label.clone(),
                            };
                            if ui.add_enabled(!g.disabled, egui::Button::new(label)).clicked() {
                                play = Some(g.id.clone());
                            }
                        }
                    });
                    if let Some(id) = play {
                        s.session.send(cm::play(&id));
                    }
                }
                None if !s.session.server.offline => {
                    if let Some(f) = &lobby.login_failed {
                        ui.colored_label(egui::Color32::LIGHT_RED, f);
                    }
                    egui::Grid::new("login").show(ui, |ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut s.username);
                        ui.end_row();
                        ui.label("Password");
                        ui.add(egui::TextEdit::singleline(&mut s.password).password(true));
                        ui.end_row();
                    });
                    if ui.button("Log in").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) && !s.password.is_empty() {
                        s.session.username = Some(s.username.clone());
                        s.session.send(cm::login(&s.username, &s.password));
                        s.session.send(cm::set_login_cookie());
                        s.password.clear();
                    }
                }
                None => {}
            }
            ui.separator();
            ui.heading("Watch");
            let mut watch = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("roster").striped(true).show(ui, |ui| {
                    for e in &lobby.entries {
                        if ui.link(&e.username).clicked() {
                            watch = Some(e.username.clone());
                        }
                        ui.label(e.game_id.as_str());
                        ui.label(e.xl.as_deref().unwrap_or(""));
                        ui.label(e.char.as_deref().unwrap_or(""));
                        ui.label(e.place.as_deref().unwrap_or(""));
                        ui.label(e.spectator_count.map(|c| c.to_string()).unwrap_or_default());
                        ui.end_row();
                    }
                });
            });
            if let Some(name) = watch {
                s.session.send(cm::watch(&name));
            }
        });
        if back {
            if let Screen::Server(s) = &mut self.screen {
                s.session.close();
            }
            self.screen = Screen::Home;
        }
    }

    fn game(&mut self, ui: &mut egui::Ui) {
        let Screen::Server(s) = &mut self.screen else { return };
        let scene = s.session.scene.clone();
        self.view.renderer.set_scene(scene.clone());
        let playing = s.session.playing();
        let typing = ui.ctx().egui_wants_keyboard_input();
        if playing && !typing {
            let alt = ui.input(|i| i.modifiers.alt);
            if !alt {
                for code in input::crawl_keys(ui, self.camera.facing()) {
                    s.session.send(key_message(code));
                }
            }
        }
        let lines: Vec<String> = s.session.state.messages.lines.iter().rev().take(8).rev().map(|m| m.text.clone()).collect();
        let more = s.session.state.messages.more;
        egui::Panel::bottom("messages").frame(egui::Frame::NONE.fill(egui::Color32::from_black_alpha(200)).inner_margin(8.0)).show(ui, |ui| {
            for l in &lines {
                ui.label(formatted(l, 14.0));
            }
            if more {
                ui.label(formatted("<cyan>--more--</cyan>", 14.0));
            }
        });
        let menus: Vec<_> = s.session.state.menus.iter().filter(|m| !m.hidden).cloned().collect();
        let popups: Vec<_> = s.session.state.ui.iter().filter(|p| !p.hidden).cloned().collect();
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            self.draw_view(ui, scene.player);
        });
        for m in menus.iter().rev().take(1) {
            egui::Window::new(formatted_plain(&m.title.text)).id(egui::Id::new(("menu", m.stack_order))).collapsible(false).resizable(true).default_size([560.0, 480.0]).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ui.ctx(), |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for item in m.items.iter().flatten() {
                        ui.label(formatted(&item.text, 14.0));
                    }
                    if !m.more.is_empty() {
                        ui.label(formatted(&m.more, 13.0));
                    }
                });
            });
        }
        for p in popups.iter().rev().take(1) {
            let text = popup_text(p);
            egui::Window::new("").id(egui::Id::new(("popup", p.stack_order))).title_bar(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).max_width(640.0).show(ui.ctx(), |ui| {
                egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                    for l in text.lines() {
                        ui.label(formatted(l, 14.0));
                    }
                });
            });
        }
    }
}

fn formatted_plain(text: &str) -> String {
    isorun_webtiles::formatted_string_to_text(text)
}

/// A popup's text, as far as it has any: its title, body, prompt and the like.
fn popup_text(p: &isorun_webtiles::Popup) -> String {
    let mut out = Vec::new();
    for key in ["title", "prompt", "body", "text", "highlight", "footer", "more"] {
        if let Some(v) = p.data.get(key).and_then(|v| v.as_str()) {
            if !v.is_empty() {
                out.push(v.to_string());
            }
        }
    }
    out.join("\n")
}

/// The settings' look, quality and surface detail on the renderer.
pub fn apply_settings(view: &mut View3d, s: &Settings) {
    if let Some(l) = isorun_render::lights(&s.look) {
        view.renderer.set_lights(l);
    }
    if let Some(q) = isorun_render::quality(&s.graphics) {
        view.renderer.set_quality(q);
    }
    view.renderer.set_surface_detail(match s.surface_detail.as_str() {
        "off" => SurfaceDetail::Off,
        "low" => SurfaceDetail::Low,
        "high" => SurfaceDetail::High,
        _ => SurfaceDetail::Medium,
    });
    view.renderer.set_animation_speed(s.animation_speed);
}

impl eframe::App for IsorunApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match self.screen {
            Screen::Home => self.home(ui),
            Screen::Testbed(_) => self.testbed(ui),
            Screen::Server(_) => self.server(ui),
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        let a = self.camera.angles();
        servers::save_view(&servers::SavedView { angles: a, zoom: Some(self.camera.zoom_goal()) });
    }
}
