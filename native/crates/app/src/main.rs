//! Isorun on the desktop.

fn main() -> eframe::Result {
    env_logger::init();
    isorun_app::run_native()
}
