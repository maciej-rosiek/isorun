//! The harness alone: an offline game in a window, with the message lines over it.

fn main() -> eframe::Result {
    isorun_ui::harness::run("Isorun harness", |ui, game| {
        egui::Area::new(egui::Id::new("lines")).anchor(egui::Align2::LEFT_BOTTOM, [8.0, -8.0]).show(ui.ctx(), |ui| {
            ui.set_min_width(900.0);
            for m in game.session.state.messages.lines.iter().rev().take(6).rev() {
                ui.label(isorun_ui::text::formatted(&m.text, 15.0));
            }
        });
        false
    })
}
