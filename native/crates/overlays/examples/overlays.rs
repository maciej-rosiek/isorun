//! The overlays over an offline game: crawl's menus, popups, prompts and the level-up banner, as
//! the game screen will drive them. Play with the keyboard and the mouse.
//!
//! `OVERLAYS_FAKE=levelup` announces a level a few seconds in (with a recorded `player` change),
//! `OVERLAYS_FAKE=statgain` puts crawl's stat-gain prompt up, `OVERLAYS_FAKE=yesno` a yes/no and
//! `OVERLAYS_FAKE=letters` the letter picker, `OVERLAYS_FAKE=skills` the skills screen recorded from a server, to see them without playing to them.
//! `OVERLAYS_SCALE` sets the UI scale.

use isorun_overlays::{OverlayCx, Overlays};
use isorun_ui::tiles::TileIcons;
use isorun_webtiles::{GameMessage, MouseMode};

fn main() -> eframe::Result {
    let mut overlays = Overlays::new();
    let mut icons = TileIcons::default();
    let mut seen = 0usize;
    let fake = std::env::var("OVERLAYS_FAKE").unwrap_or_default();
    let scale: f32 = std::env::var("OVERLAYS_SCALE").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let mut started: Option<f64> = None;
    let mut faked = false;
    isorun_ui::harness::run("overlays", move |ui, game| {
        icons.set_gamedata(game.session.gamedata.clone());
        if !game.playing() {
            return false;
        }
        let now = ui.input(|i| i.time);
        let started_at = *started.get_or_insert(now);
        // a state of our own to show what play would take long to reach
        let mut state = game.session.state.clone();
        let since = now - started_at;
        match fake.as_str() {
            "levelup" if since > 4.0 && !faked => {
                faked = true;
                let mut p = state.player.clone();
                overlays.level_up.sync(&p, false, now);
                p.xl += 1;
                p.hp_max += 7;
                p.mp_max += 1;
                overlays.messages(&[GameMessage { text: format!("You have reached level {}!", p.xl), channel: Some(11), ..Default::default() }]);
                overlays.level_up.sync(&p, false, now);
            }
            "statgain" if since > 3.0 => {
                state.input_mode = MouseMode::PROMPT;
                state.messages.lines.push(GameMessage { text: "You have reached level 3!".into(), channel: Some(0), ..Default::default() });
                state.messages.lines.push(GameMessage { text: "<cyan>Increase (S)trength, (I)ntelligence, or (D)exterity? </cyan>".into(), channel: Some(2), ..Default::default() });
            }
            "yesno" if since > 3.0 => {
                state.input_mode = MouseMode::YESNO;
                state.messages.lines.push(GameMessage { text: "Really attack the plant? (y/N)".into(), channel: Some(2), ..Default::default() });
            }
            "letters" if since > 3.0 => {
                state.input_mode = MouseMode::PROMPT;
                state.messages.lines.push(GameMessage { text: "a - a +0 war axe (weapon)".into(), channel: Some(0), ..Default::default() });
                state.messages.lines.push(GameMessage { text: "Adjust to which letter? (? for menu, Esc to quit)".into(), channel: Some(2), ..Default::default() });
            }
            "skills" if since > 3.0 => {
                // the skills screen as a server sent it (the native engine paints no CRT popup while headless)
                let msgs: Vec<serde_json::Value> = serde_json::from_str(include_str!("../../../../apps/isorun/test/fixtures/skills-crt.json")).unwrap();
                for m in msgs.iter().take(2) {
                    isorun_webtiles::reduce(&mut state, m);
                }
            }
            _ => {}
        }
        // the message lines that arrived, for the level-up banner
        let lines = &game.session.state.messages.lines;
        if lines.len() < seen {
            seen = 0;
        }
        if lines.len() > seen {
            overlays.messages(&lines[seen..]);
            seen = lines.len();
        }
        if std::env::var("OVERLAYS_DEBUG").is_ok() && state.menus.last().is_some_and(|m| m.kind.as_deref() == Some("crt")) {
            log::info!("crt lines {} areas {:?} ui_state {} diags {:?}", state.crt.lines.len(), state.crt.areas.iter().map(|(k, v)| (k.clone(), v.len())).collect::<Vec<_>>(), state.ui_state, state.diagnostics.iter().rev().take(3).map(|d| d.text.clone()).collect::<Vec<_>>());
        }
        let mut out = Vec::new();
        let took = {
            let mut cx = OverlayCx::new(&state, &mut icons, &mut out);
            cx.ui_scale = scale;
            cx.touch = std::env::var("OVERLAYS_TOUCH").is_ok();
            overlays.ui(ui.ctx(), &mut cx);
            overlays.input(ui.ctx(), &mut cx)
        };
        if !out.is_empty() {
            log::info!("overlays sent {out:?}");
        }
        for m in out {
            game.send(m);
        }
        took
    })
}
