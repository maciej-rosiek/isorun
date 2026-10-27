//! The sound on a real game: an offline game in the harness, with `GameAudio` fed by the state,
//! the message lines and the renderer's events, and the sounds played listed over the view (the
//! trace). With no sound card it runs silent and the list still shows what would sound.

use isorun_audio::{AudioFrame, AudioSettings, GameAudio, RendererView, SoundEngine};
use isorun_webtiles::GameMessage;

fn main() -> eframe::Result {
    let engine = SoundEngine::new();
    engine.apply_settings(&AudioSettings::default());
    engine.unlock();
    engine.enable_trace();
    let mut audio = GameAudio::new(engine.clone());
    let mut last: Option<GameMessage> = None;
    let mut seen_state = 0u64;
    let mut played: Vec<String> = Vec::new();
    isorun_ui::harness::run("Isorun audio", move |ui, game| {
        let width = ui.max_rect().width();
        let speed = 1.5;
        let scene = game.session.scene.clone();
        if game.playing() {
            let st = &game.session.state;
            if st.rev.any != seen_state {
                seen_state = st.rev.any;
                let lines = &st.messages.lines;
                // the lines since the last one read (combat.ts `linesSince`)
                let from = last.as_ref().and_then(|l| lines.iter().rposition(|m| m == l)).map_or(0, |i| i + 1);
                let fresh = if last.is_none() { &lines[..0] } else { &lines[from.min(lines.len())..] };
                audio.on_state(st, &scene, fresh);
                last = lines.last().cloned();
            }
            let ev = game.view.renderer.take_events();
            let view = RendererView { renderer: &game.view.renderer, width, animation_speed: speed };
            audio.on_frame(AudioFrame { shown: &ev.shown, hops: &ev.hops, doors: &ev.doors, landings: &ev.landings }, &scene, &view);
        }
        played.extend(engine.take_trace());
        let keep = played.len().saturating_sub(14);
        played.drain(..keep);
        egui::Area::new(egui::Id::unique("sounds")).anchor(egui::Align2::RIGHT_TOP, [-12.0, 24.0]).show(ui.ctx(), |ui| {
            ui.label(egui::RichText::new("sounds played").monospace().color(egui::Color32::from_gray(150)));
            for name in played.iter().rev() {
                ui.label(egui::RichText::new(name).monospace().size(15.0).color(egui::Color32::from_rgb(230, 200, 120)));
            }
        });
        false
    })
}
