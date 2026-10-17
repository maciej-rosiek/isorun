//! The app (the routing of `main.ts` around the game screen): the front end (title, servers,
//! accounts, settings) until it hands over a session that started a game or a spectate, then
//! the game screen on it until the game is over, when the session goes back to the front end.
//! The test bed is a screen of its own, from the front end's footer or `--testbed`.

#[path = "game/mod.rs"]
pub mod game;

use eframe::egui_wgpu::RenderState;
use game::{GameOptions, GameScreen, Outcome};
use isorun_audio::SoundEngine;
use isorun_client::testbed;
use isorun_front::{FrontAction, FrontEnd, FrontOptions, GameEnd, NavDir, PadButton, PadEvent as FrontPad};
use isorun_input::gamepad::GamepadInput;
use isorun_input::{Button, PadEvent};
use isorun_render::IsoCamera;
use isorun_scene::Scene;
use isorun_ui::keys as basic_keys;
use isorun_ui::view3d::View3d;
use std::sync::Arc;

#[cfg(all(not(target_arch = "wasm32"), feature = "gilrs"))]
pub type Pads = Option<isorun_input::gamepad::GilrsSource>;
#[cfg(not(all(not(target_arch = "wasm32"), feature = "gilrs")))]
pub type Pads = ();

enum Screen {
    Front,
    Game(Box<GameScreen>),
    Testbed(Box<Testbed>),
}

/// The test bed: the room of `iso.des` with the player walking it, under the game's camera.
struct Testbed {
    view: View3d,
    camera: IsoCamera,
    scene: Scene,
    last: f64,
}

pub struct IsorunApp {
    rs: RenderState,
    front: FrontEnd,
    screen: Screen,
    engine: SoundEngine,
    opts: GameOptions,
    pad: GamepadInput,
    #[allow(dead_code)]
    pads: Pads,
    touch_sim: TouchSim,
    /// The visit counts of the hosted page's /stats (client::visits).
    visits: isorun_client::visits::Visits,
}

impl IsorunApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc.wgpu_render_state.clone().expect("Isorun draws with wgpu");
        isorun_ui::install(&cc.egui_ctx);
        isorun_hud::install(&cc.egui_ctx);
        // egui quits on Ctrl-Q and zooms on Ctrl-plus / minus: those keys are crawl's (Ctrl-Q is its quit prompt)
        cc.egui_ctx.options_mut(|o| {
            o.quit_shortcuts.clear();
            o.zoom_with_keyboard = false;
        });
        let args = crate::launch_args();
        let has = |a: &str| args.iter().any(|x| x == a || x.starts_with(&format!("{a}=")) && !x.ends_with("=0"));
        // `--touch` lays the game out for a touch screen (the touch bar, the corner buttons), as a phone's
        let touch = isorun_input::touch::touch_screen() || args.iter().any(|a| a == "--touch");
        let size = cc.egui_ctx.content_rect().size();
        let phone = touch && size.x.min(size.y) < 560.0;
        isorun_front::settings::set_phone(phone);
        let can_fullscreen = can_fullscreen(phone);
        let front_opts = FrontOptions { mouse: !touch, testbed: has("--testbed-link"), can_fullscreen, ..Default::default() };
        let front = FrontEnd::new(&cc.egui_ctx, rs.clone(), front_opts);
        let engine = SoundEngine::new();
        // the sounds are the hosted page's (public/audio), at its root, while this page is at /native/
        #[cfg(target_arch = "wasm32")]
        engine.set_base("/audio/");
        // natively the sound may start at once; the web waits for the first key or click
        #[cfg(not(target_arch = "wasm32"))]
        engine.unlock();
        let opts = GameOptions { perf: has("--perf"), audio_trace: has("--audio-trace"), can_fullscreen, touch: touch || has("--touch") };
        #[cfg(all(not(target_arch = "wasm32"), feature = "gilrs"))]
        let pads: Pads = isorun_input::gamepad::GilrsSource::new();
        #[cfg(not(all(not(target_arch = "wasm32"), feature = "gilrs")))]
        let pads: Pads = ();
        let mut app = IsorunApp { rs, front, screen: Screen::Front, engine, opts, pad: GamepadInput::default(), pads, touch_sim: TouchSim::default(), visits: Default::default() };
        if let Some(path) = args.iter().position(|a| a == "--open").and_then(|i| args.get(i + 1)).cloned() {
            app.front.open(&path);
        }
        if has("--testbed") {
            app.open_testbed();
        }
        app
    }

    fn open_testbed(&mut self) {
        match testbed::load() {
            Ok((tiles, scene)) => {
                let mut view = View3d::new(&self.rs);
                let s = isorun_front::settings::get();
                isorun_front::settings::apply_graphics(&mut view.renderer, &s, 1.0);
                if let Err(e) = view.renderer.set_torch_model(testbed::TORCH_GLB) {
                    log::warn!("torch model: {e}");
                }
                view.renderer.set_tiles(tiles);
                view.renderer.new_level();
                if let Some(v) = crate::launch_args().iter().find_map(|a| a.strip_prefix("--debug-view=")?.parse().ok()) {
                    view.renderer.set_debug_view(v);
                }
                view.renderer.set_scene(Arc::new(scene.clone()));
                let mut camera = IsoCamera::default();
                camera.snap_to(scene.player.0, scene.player.1);
                self.screen = Screen::Testbed(Box::new(Testbed { view, camera, scene, last: isorun_client::platform::seconds() }));
            }
            Err(e) => log::error!("the test bed did not load: {e}"),
        }
    }

    /// The pad while the front end is up: its events, as the front end reads them.
    fn front_pad(&mut self) {
        let now = isorun_client::platform::now_ms();
        #[cfg(all(not(target_arch = "wasm32"), feature = "gilrs"))]
        if let Some(g) = self.pads.as_mut() {
            self.pad.poll_source(now, g);
        }
        #[cfg(target_arch = "wasm32")]
        self.pad.poll_source(now, &mut isorun_input::gamepad::WebSource);
        let _ = now;
        for ev in self.pad.take_events() {
            if let Some(e) = to_front(&ev) {
                self.front.pad(e);
            }
        }
    }

    fn testbed(&mut self, ui: &mut egui::Ui) {
        let Screen::Testbed(t) = &mut self.screen else { return };
        let mut moved = false;
        let mut back = false;
        ui.input(|i| {
            if i.modifiers.alt {
                return;
            }
            for (key, d) in [(egui::Key::ArrowUp, basic_keys::Dir::N), (egui::Key::ArrowDown, basic_keys::Dir::S), (egui::Key::ArrowLeft, basic_keys::Dir::W), (egui::Key::ArrowRight, basic_keys::Dir::E)] {
                if i.key_pressed(key) {
                    let r = d.rotated(t.camera.facing()) as usize;
                    let (dx, dy) = (isorun_scene::DIR8_DX[r], isorun_scene::DIR8_DY[r]);
                    moved |= testbed::step(&mut t.scene, dx, dy);
                }
            }
            back = i.key_pressed(egui::Key::Escape);
            // Q / E turn it by 45 degrees, R / F tilt it
            if i.key_pressed(egui::Key::Q) {
                t.camera.turn(-45.0);
            }
            if i.key_pressed(egui::Key::E) {
                t.camera.turn(45.0);
            }
            if i.key_pressed(egui::Key::R) {
                t.camera.tilt_by(5.0);
            }
            if i.key_pressed(egui::Key::F) {
                t.camera.tilt_by(-5.0);
            }
            let scroll = i.smooth_scroll_delta.y;
            if scroll != 0.0 {
                t.camera.zoom_by((scroll * 0.0015).exp());
            }
        });
        if moved {
            t.view.renderer.set_scene(Arc::new(t.scene.clone()));
        }
        let now = isorun_client::platform::seconds();
        let dt = (now - t.last).clamp(0.0, 0.1) as f32;
        t.last = now;
        t.camera.update(dt, t.scene.player.0, t.scene.player.1);
        t.view.renderer.follow(&t.camera);
        let rect = ui.max_rect();
        let response = ui.allocate_rect(rect, egui::Sense::drag());
        if response.dragged() {
            let d = response.drag_delta();
            t.camera.drag(d.x, d.y);
        }
        t.view.paint(ui, &self.rs, rect);
        ui.ctx().request_repaint();
        if back {
            self.screen = Screen::Front;
        }
    }
}

/// A pad event as the front end reads it.
fn to_front(ev: &PadEvent) -> Option<FrontPad> {
    match ev {
        PadEvent::Press { button, .. } => Some(FrontPad::Press(match button {
            Button::A => PadButton::A,
            Button::B => PadButton::B,
            Button::X => PadButton::X,
            Button::Y => PadButton::Y,
            Button::LB => PadButton::LB,
            Button::RB => PadButton::RB,
            Button::Start => PadButton::Start,
            Button::Select => PadButton::Select,
            Button::DU => return Some(FrontPad::Dir(NavDir::Up)),
            Button::DD => return Some(FrontPad::Dir(NavDir::Down)),
            Button::DL => return Some(FrontPad::Dir(NavDir::Left)),
            Button::DR => return Some(FrontPad::Dir(NavDir::Right)),
            _ => return None,
        })),
        PadEvent::Dir { dir: Some(d), .. } | PadEvent::DirRepeat { dir: d, .. } => Some(FrontPad::Dir(match d {
            0 => NavDir::Up,
            4 => NavDir::Down,
            6 => NavDir::Left,
            2 => NavDir::Right,
            _ => return None,
        })),
        PadEvent::Look { dx, dy, .. } => Some(FrontPad::Look { dx: *dx, dy: *dy }),
        _ => None,
    }
}

/// Android's app is always full screen: no Full screen row there. A phone's browser does have
/// the page's full screen (it hides the address bar), so the web build keeps the row on a phone.
fn can_fullscreen(_phone: bool) -> bool {
    !cfg!(target_os = "android")
}

/// `ISORUN_TOUCH_SIM=1`: the mouse acts as a finger, as egui-winit hands a touch screen's
/// input on: a press is a Touch Start with its emulated button press, a held move a Touch
/// Move, a release a Touch End with the button's release and then PointerGone, and the mouse
/// moving with no button held is not seen at all (a finger does not hover). To test the touch
/// paths with xdotool on a desktop.
#[derive(Default)]
struct TouchSim {
    down: bool,
}

impl TouchSim {
    fn on() -> bool {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ON.get_or_init(|| std::env::var_os("ISORUN_TOUCH_SIM").is_some_and(|v| v != "0"))
    }

    fn rewrite(&mut self, raw: &mut egui::RawInput) {
        let touch = |phase, pos| egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(1), phase, pos, force: None };
        let mut out = Vec::with_capacity(raw.events.len());
        for e in raw.events.drain(..) {
            match e {
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers } => {
                    if pressed {
                        self.down = true;
                        out.push(touch(egui::TouchPhase::Start, pos));
                        out.push(egui::Event::PointerMoved(pos));
                        out.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers });
                    } else if self.down {
                        self.down = false;
                        out.push(touch(egui::TouchPhase::End, pos));
                        out.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers });
                        out.push(egui::Event::PointerGone);
                    }
                }
                egui::Event::PointerMoved(pos) => {
                    if self.down {
                        out.push(touch(egui::TouchPhase::Move, pos));
                        out.push(egui::Event::PointerMoved(pos));
                    }
                }
                egui::Event::PointerButton { .. } | egui::Event::PointerGone => {}
                other => out.push(other),
            }
        }
        raw.events = out;
    }
}

impl eframe::App for IsorunApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if TouchSim::on() {
            self.touch_sim.rewrite(raw_input);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // a phone is told by its window (the first frame's size is the real one, the app's start
        // may not know it yet)
        #[cfg(target_os = "android")]
        crate::android_insets::poll(ui.ctx());
        let size = ui.ctx().content_rect().size();
        let phone = self.opts.touch && size.x.min(size.y) < 560.0;
        isorun_front::settings::set_phone(phone);
        self.front.options.can_fullscreen = can_fullscreen(phone);
        self.opts.can_fullscreen = can_fullscreen(phone);
        // the screen's own state: F11 or Escape in the browser leaves full screen behind the menus' back
        self.front.options.fullscreen = isorun_ui::fullscreen::is_on(ui.ctx());
        // the web's sound starts with the player's first key, click or touch (the browsers let a
        // page make a sound only after a gesture); unlock does nothing the second time
        #[cfg(target_arch = "wasm32")]
        if ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. } | egui::Event::PointerButton { pressed: true, .. } | egui::Event::Touch { .. }))) {
            self.engine.unlock();
        }
        let game_on_screen = match &self.screen {
            Screen::Game(g) => Some(isorun_client::visits::GameOnScreen::of(&g.session)),
            _ => None,
        };
        let shown = ui.ctx().input(|i| i.viewport().minimized != Some(true));
        self.visits.tick(shown, game_on_screen);
        match &mut self.screen {
            Screen::Game(game) => {
                let Some(outcome) = game.ui(ui) else { return };
                let Screen::Game(mut game) = std::mem::replace(&mut self.screen, Screen::Front) else { unreachable!() };
                game.close();
                let GameScreen { session, pads, .. } = *game;
                self.pads = pads;
                self.front.game_over(match outcome {
                    Outcome::Lobby => GameEnd::Lobby(session),
                    Outcome::Leave => GameEnd::Leave(session),
                    Outcome::Dropped => GameEnd::Dropped(session),
                });
                ui.ctx().request_repaint();
            }
            Screen::Testbed(_) => self.testbed(ui),
            Screen::Front => {
                self.front_pad();
                // the front end inside the safe area (the system's bars, a camera cutout)
                let safe = isorun_ui::safe::rect(ui.ctx());
                let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(safe));
                match self.front.ui(&mut inside) {
                    Some(FrontAction::Play { session, .. }) | Some(FrontAction::Watch { session, .. }) => {
                        let pads = std::mem::take(&mut self.pads);
                        let game = GameScreen::new(ui.ctx(), session, self.rs.clone(), self.engine.clone(), self.opts.clone(), pads);
                        self.visits.game_started(isorun_client::visits::GameOnScreen::of(&game.session));
                        self.screen = Screen::Game(Box::new(game));
                        ui.ctx().request_repaint();
                    }
                    Some(FrontAction::Quit) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                    Some(FrontAction::Fullscreen(f)) => isorun_ui::fullscreen::set(ui.ctx(), f),
                    Some(FrontAction::Testbed) => self.open_testbed(),
                    None => {}
                }
            }
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        if let Screen::Game(g) = &mut self.screen {
            g.save_view();
        }
    }
}
