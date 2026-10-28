//! Isorun on the desktop. On the web there is no binary to run: the page starts the library
//! (`lib.rs`, its `wasm_bindgen(start)`), so this one is empty there and a workspace build for
//! wasm32 still builds.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    // RUST_LOG says what reaches the terminal; the app's log keeps its lines either way (diag)
    let terminal = env_logger::Builder::from_default_env().build();
    isorun_client::platform::diag::init(Box::new(terminal), log::LevelFilter::Info);
    if let Some(dir) = isorun_client::platform::storage::data_dir().filter(|d| std::fs::create_dir_all(d).is_ok()) {
        isorun_client::platform::diag::open_file(&dir);
    }
    isorun_app::log_panics();
    isorun_app::run_native()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
