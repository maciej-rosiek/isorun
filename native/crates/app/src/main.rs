//! Isorun on the desktop. On the web there is no binary to run: the page starts the library
//! (`lib.rs`, its `wasm_bindgen(start)`), so this one is empty there and a workspace build for
//! wasm32 still builds.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::init();
    isorun_app::run_native()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
