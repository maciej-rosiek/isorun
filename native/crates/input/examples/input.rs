//! The input crate driving a real offline game in the harness: every key, the pad and the
//! pointer go through `isorun_input` (the router, the bindings, the runner), never through
//! `isorun_ui::keys`. Each message sent to crawl is logged as `SENT <json>` (stderr, and the
//! file `ISORUN_INPUT_LOG` names), to compare with the web build's.
//!
//!     cargo run -p isorun-input --example input
//!     ISORUN_PAD_SCRIPT="1000:RT;1100:;2000:LT;2100:" cargo run -p isorun-input --example input
//!
//! With no overlays of its own (they are another crate's), the example's host answers the
//! focus layer's and the menus' ops with crawl's raw keys, as the official client would.

use isorun_input::bindings::{prompt_labels, CommandCategory, MenuOp, OskOp, UiOp};
use isorun_input::context::{derive_context, Layer, Mode};
use isorun_input::focus::FocusOp;
use isorun_input::gamepad::{pad_key_input, GamepadInput, PadSource, ScriptedPad};
use isorun_input::keys::{key_inputs, KeyInput};
use isorun_input::router::{aim_intent, GameHost, InputRouter, Io, RouterSettings};
use isorun_input::runner::{Env, Runner, RunnerHooks};
use isorun_input::touch::Gestures;
use isorun_render::IsoAim;
use isorun_scene::{Dir8, Scene};
use isorun_webtiles::{cm, Keys};
use serde_json::Value;
use std::io::Write;
use std::sync::Arc;

/// The screen around the router, for the example: the view to pick cells on and draw the aim
/// arrow in, and crawl's raw keys for what overlays would do.
struct Host<'a> {
    view: &'a mut isorun_ui::view3d::View3d,
    scene: Arc<Scene>,
    raw: Vec<Value>,
    status: Option<String>,
}

impl RunnerHooks for Host<'_> {
    fn ui(&mut self, op: UiOp, _arg: Option<i32>, _category: Option<CommandCategory>, _section: Option<&str>) {
        log::info!("ui {op:?}");
        match op {
            // without the client's own screens: crawl's
            UiOp::Equipment => self.raw.push(cm::input("i")),
            UiOp::Commands => self.raw.push(cm::input("a")),
            UiOp::Interact => self.raw.push(cm::input(">")),
            _ => {}
        }
    }
    fn osk(&mut self, op: OskOp, _dir: Option<Dir8>) {
        match op {
            OskOp::Submit => self.raw.push(cm::key(Keys::ENTER)),
            OskOp::Cancel => self.raw.push(cm::key(Keys::ESC)),
            _ => {}
        }
    }
    fn menu(&mut self, op: MenuOp) {
        let k = match op {
            MenuOp::Select => Keys::ENTER,
            MenuOp::Cancel => Keys::ESC,
            MenuOp::Next => Keys::CK_DOWN,
            MenuOp::Prev => Keys::CK_UP,
            MenuOp::Left => Keys::CK_LEFT,
            MenuOp::Right => Keys::CK_RIGHT,
            MenuOp::PageNext | MenuOp::SectionNext => Keys::CK_PGDN,
            MenuOp::PagePrev | MenuOp::SectionPrev => Keys::CK_PGUP,
            _ => return,
        };
        self.raw.push(cm::key(k));
    }
    fn focus(&mut self, op: FocusOp) {
        let k = match op {
            FocusOp::Select => Keys::ENTER,
            FocusOp::Cancel => Keys::ESC,
            FocusOp::Next => Keys::CK_DOWN,
            FocusOp::Prev => Keys::CK_UP,
            FocusOp::Left => Keys::CK_LEFT,
            FocusOp::Right => Keys::CK_RIGHT,
            _ => return,
        };
        self.raw.push(cm::key(k));
    }
    fn status(&mut self, text: &str) {
        self.status = Some(text.into());
    }
}

impl GameHost for Host<'_> {
    fn pick(&mut self, at: egui::Pos2) -> Option<(i32, i32)> {
        self.view.pick(at)
    }
    fn set_aim(&mut self, dir: Option<Dir8>) {
        let aim = dir.map(|dir| IsoAim { dir, intent: aim_intent(&self.scene, dir) });
        self.view.renderer.set_aim(aim);
    }
}

struct Driver {
    router: InputRouter,
    runner: Runner,
    pad: GamepadInput,
    script: Option<ScriptedPad>,
    #[cfg(feature = "gilrs")]
    gilrs: Option<isorun_input::gamepad::GilrsSource>,
    gestures: Gestures,
    state_rev: u64,
    scene_rev: u64,
    log: Vec<String>,
    file: Option<std::fs::File>,
    last: f64,
}

fn main() -> eframe::Result {
    let settings = isorun_client::platform::storage::load::<isorun_client::servers::Settings>("isorun.settings", Default::default());
    let mut d = Driver {
        router: InputRouter::new(RouterSettings::from_settings(&settings)),
        runner: Runner::new(),
        pad: GamepadInput::default(),
        script: std::env::var("ISORUN_PAD_SCRIPT").ok().map(|s| ScriptedPad::parse(&s)),
        #[cfg(feature = "gilrs")]
        gilrs: isorun_input::gamepad::GilrsSource::new(),
        gestures: Gestures::default(),
        state_rev: 0,
        scene_rev: 0,
        log: Vec::new(),
        file: std::env::var("ISORUN_INPUT_LOG").ok().and_then(|p| std::fs::File::create(p).ok()),
        last: isorun_client::platform::now_ms(),
    };
    isorun_ui::harness::run("isorun input", move |ui, game| {
        let now = isorun_client::platform::now_ms();
        let dt = ((now - d.last) / 1000.0).clamp(0.0, 0.1) as f32;
        d.last = now;
        let (events, modifiers) = ui.input(|i| (i.events.clone(), i.modifiers));
        if !game.session.playing() {
            // the lobby and the loading screen: the keys wait for the game
            return true;
        }
        let scene = game.session.scene.clone();
        let st = &game.session.state;
        let mut ctx = derive_context(st, &scene, d.runner.heading, Layer::Micro);
        d.router.fill_context(&mut ctx, &mut d.runner, st.input_mode, now);
        d.gestures.view = game.view.rect;
        let mut host = Host { view: &mut game.view, scene: scene.clone(), raw: Vec::new(), status: None };
        {
            let env = Env {
                state: st,
                scene: &scene,
                ctx: &ctx,
                camera: &mut game.camera,
                now,
                watching: game.session.watching(),
                confirm_dangerous: settings.confirm_stairs,
            };
            let mut io = Io { runner: &mut d.runner, env, host: &mut host };

            // the server moved on: a sequence waiting on it may go on; the map moved: a door closed
            if st.rev.any != d.state_rev {
                d.state_rev = st.rev.any;
                d.router.server_state(&mut io);
            }
            if scene.revision != d.scene_rev {
                d.scene_rev = scene.revision;
                io.runner.on_scene();
                d.router.refresh_aim(&mut io);
            }
            d.router.tick(&mut io);

            // the keys
            for k in key_inputs(&events, modifiers) {
                if pad_key_input(&mut d.pad, &k, now) {
                    continue;
                }
                match k {
                    KeyInput::Down { key, repeat } => {
                        d.router.key_down(&key, repeat, &mut io);
                    }
                    KeyInput::Up { key } => d.router.key_up(&key, &mut io),
                }
            }
            if isorun_input::back_button::back_pressed(&events) {
                d.router.back(&mut io);
            }

            // the pad: a script, gilrs, and the keyboard's stand-ins
            if let Some(script) = d.script.as_mut() {
                script.now = now;
                let pads = script.pads();
                d.pad.poll(script.clock().unwrap_or(now).min(now), &pads);
            }
            #[cfg(feature = "gilrs")]
            if d.script.is_none() {
                if let Some(g) = d.gilrs.as_mut() {
                    d.pad.poll_source(now, g);
                }
            }
            for ev in d.pad.take_events() {
                log::info!("pad {ev:?}");
                d.router.pad(&ev, &mut io);
            }

            // the pointer and the fingers
            for g in d.gestures.feed(&events, now) {
                d.router.gesture(&g, &mut io);
            }

            let pad = &d.pad;
            d.pad.right_stick_turns = d.router.frame(&mut io, &|b| pad.is_held(b), dt);
        }
        // what the screen asked for in crawl's raw keys
        let raw = std::mem::take(&mut host.raw);
        let status = host.status.take();
        let mut env = Env {
            state: st,
            scene: &scene,
            ctx: &ctx,
            camera: &mut game.camera,
            now,
            watching: game.session.watching(),
            confirm_dangerous: settings.confirm_stairs,
        };
        for m in raw {
            d.runner.send(m, &mut env);
        }
        for m in d.runner.take_outbox() {
            let line = format!("SENT {m}");
            eprintln!("{line}");
            if let Some(f) = d.file.as_mut() {
                let _ = writeln!(f, "{line}");
            }
            d.log.push(m.to_string());
            game.session.send(m);
        }
        if d.log.len() > 14 {
            d.log.drain(..d.log.len() - 14);
        }

        // what went out, and the prompts the pad would show
        let painter = ui.painter();
        let font = egui::FontId::monospace(12.0);
        let mut y = 24.0;
        let mode = format!(
            "mode {:?} · facing {} · heading {} · aim {:?}{}",
            ctx.mode,
            game.camera.facing(),
            d.runner.heading,
            d.router.aim_dir,
            status.map(|s| format!(" · {s}")).unwrap_or_default()
        );
        painter.text(egui::pos2(8.0, y), egui::Align2::LEFT_TOP, mode, font.clone(), egui::Color32::from_rgb(0xff, 0xd4, 0x5c));
        y += 16.0;
        for l in &d.log {
            painter.text(egui::pos2(8.0, y), egui::Align2::LEFT_TOP, l, font.clone(), egui::Color32::from_gray(220));
            y += 14.0;
        }
        if ctx.mode != Mode::Lobby {
            let prompts: Vec<String> =
                prompt_labels(&ctx).iter().map(|l| format!("{} {}", l.button.name(), isorun_webtiles::formatted_string_to_text(&l.label))).collect();
            painter.text(
                egui::pos2(8.0, ui.max_rect().bottom() - 20.0),
                egui::Align2::LEFT_TOP,
                prompts.join("   "),
                font,
                egui::Color32::from_rgb(0xe8, 0xc9, 0x8a),
            );
        }
        true
    })
}
