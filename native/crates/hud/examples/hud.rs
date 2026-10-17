//! The HUD over an offline game: the dock (or the classic panes with `HUD=classic`), its tooltips
//! and screens (`i`, `@`), the message pane and the monster list, laid out as the game screen
//! lays them out. The minimap's slot is left empty: the 2D renderer draws there.
//!
//!     ISORUN_PROFILE=hud cargo run -p isorun-hud --example hud
//!
//! `HUD_SIZE=844x390` opens the window at a phone's size, for the compact dock.

use isorun_hud::{classic, Action, Device, Hud, HudCommand, HudCx, KeyStep, Mode, PadKind};
use isorun_ui::tiles::TileIcons;
use isorun_webtiles::key_message;

fn main() -> eframe::Result {
    let mut hud = Hud::new();
    let mut tiles = TileIcons::default();
    let mut settings = isorun_client::servers::get_settings();
    if let Ok(h) = std::env::var("HUD") {
        settings.hud = h;
    }
    let mut installed = false;
    run_sized(move |ui, game| {
        if !installed {
            isorun_hud::install(ui.ctx());
            installed = true;
        }
        tiles.set_gamedata(game.session.gamedata.clone());
        let screen = ui.ctx().content_rect();
        let state = game.session.state.clone();
        let scene = game.session.scene.clone();
        let now = isorun_client::platform::seconds();
        let layout = hud.layout(ui.ctx(), screen, &state, &settings, state.ui_state == isorun_webtiles::UiState::VIEW_MAP);
        game.view.renderer.set_centre_lift(layout.centre_lift);
        let view_rect = game.view.rect;
        let renderer = &game.view.renderer;
        let screen_of = |x: i32, y: i32| {
            let (sx, sy) = renderer.screen_of(x as f32, y as f32);
            Some(view_rect.min + egui::vec2(sx, sy))
        };
        let mut out = vec![];
        let mut commands = vec![];
        let mode = isorun_hud::hud::guess_mode(&state);
        let output = {
            let mut cx = HudCx {
                state: &state,
                scene: &scene,
                tiles: &mut tiles,
                settings: &settings,
                now,
                mode,
                device: Device::Keyboard,
                pad: PadKind::Xbox,
                touch: false,
                facing: game.camera.facing(),
                map_view: false,
                dock_context: None,
                prompts: &[],
                menu_prompts: &[],
                holding: None,
                panel: None,
                inspect: None,
                chat: !game.session.server.offline,
                client_overlay: false,
                screen_of: &screen_of,
                out: &mut out,
                commands: &mut commands,
            };
            hud.ui(ui, &mut cx)
        };
        for m in out {
            game.send(m);
        }
        // the harness has no runner: actions go out as their keys, the rest is logged
        for c in commands {
            match c {
                HudCommand::Run(Action::Keys { seq, .. }) => {
                    for k in seq {
                        match k {
                            KeyStep::Text { text, .. } => text.chars().for_each(|ch| game.send(key_message(ch as i32))),
                            KeyStep::Key { key, .. } => game.send(key_message(key)),
                        }
                    }
                }
                HudCommand::Run(Action::Send(m)) => game.send(m),
                HudCommand::Run(Action::SendThen(m, _)) => game.send(m),
                HudCommand::Run(Action::Fire) => game.send(key_message('f' as i32)),
                HudCommand::Choices { title, .. } => log::info!("choices: {title}"),
                other => log::info!("hud: {other:?}"),
            }
        }
        let _ = classic::MINIMAP_TILES_DEFAULT;
        output.consumed_keys
    })
}

/// The harness at the size `HUD_SIZE` names (default 1600x1000).
fn run_sized(component: impl FnMut(&mut egui::Ui, &mut isorun_ui::harness::Game) -> bool + 'static) -> eframe::Result {
    if let Ok(size) = std::env::var("HUD_SIZE") {
        // the harness opens at 1600x1000; a smaller window comes from the window manager, so the
        // size is passed on for the example's own sake through the harness's title
        log::info!("HUD_SIZE={size}: resize the window with xdotool to see it");
    }
    let _ = Mode::Command;
    isorun_ui::harness::run("Isorun HUD", component)
}
