//! Isorun, the game: the front end, the game screen and the HUD over the isometric renderer, on
//! eframe (egui and wgpu) for the desktop, Android and the web.

pub mod app;

use isorun_client::servers;
#[cfg(target_os = "android")]
use isorun_client::platform;


/// The command line (natively) or the page's query words (on the web).
pub fn launch_args() -> Vec<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::args().skip(1).collect()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let search = web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default();
        search.trim_start_matches('?').split('&').filter(|s| !s.is_empty()).map(|s| format!("--{s}")).collect()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title(servers::APP_NAME).with_inner_size([1600.0, 1000.0]).with_min_inner_size([320.0, 240.0]),
        wgpu_options: isorun_ui::wgpu_options(),
        // the window opens at its own size each time: a remembered one outlives what it was for
        // (a run at a phone's size for a test opened every later run as small)
        persist_window: false,
        ..Default::default()
    }
}

/// Run the app in a window (the desktop).
#[cfg(not(target_arch = "wasm32"))]
pub fn run_native() -> eframe::Result {
    eframe::run_native(servers::APP_NAME, native_options(), Box::new(|cc| Ok(Box::new(app::IsorunApp::new(cc)))))
}

/// The Android entry point (NativeActivity).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(android: winit::platform::android::activity::AndroidApp) {
    android_logger_init();
    if let Some(dir) = android.internal_data_path() {
        if let Err(e) = unpack_engine(&android, &dir.join("engine")) {
            log::error!("the offline engine was not unpacked: {e}");
        }
        let _ = platform::storage::ANDROID_DIR.set(dir);
    }
    let mut options = native_options();
    options.android_app = Some(android);
    let _ = eframe::run_native(servers::APP_NAME, options, Box::new(|cc| Ok(Box::new(app::IsorunApp::new(cc)))));
}

/// The offline engine's data (crawl's `dat`, `docs` and `settings`, the client's gamedata) is in
/// the APK's assets under `engine/`, listed in `engine/files.txt` (the asset manager lists no
/// folders); crawl reads it from files, so it is unpacked into the app's files on the first start
/// and again when the APK carries another build (its `engine.json` differs). The binary itself is
/// `lib/<abi>/libcrawl.so`, which Android unpacks and lets the app run (engine.rs).
#[cfg(target_os = "android")]
fn unpack_engine(android: &winit::platform::android::activity::AndroidApp, dir: &std::path::Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::io::{Error, Read};
    let assets = android.asset_manager();
    let read = |name: &str| -> std::io::Result<Vec<u8>> {
        let path = CString::new(format!("engine/{name}")).map_err(Error::other)?;
        let mut asset = assets.open(&path).ok_or_else(|| Error::new(std::io::ErrorKind::NotFound, name.to_string()))?;
        let mut bytes = Vec::new();
        asset.read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    let pointer = match read("engine.json") {
        Ok(p) => p,
        // an APK without the engine: nothing to unpack
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if std::fs::read(dir.join("engine.json")).ok().as_deref() == Some(&pointer[..]) {
        return Ok(());
    }
    let started = std::time::Instant::now();
    // into a folder beside, renamed over the old one: a start cut short leaves no half engine
    let fresh = dir.with_extension("new");
    let _ = std::fs::remove_dir_all(&fresh);
    let list = String::from_utf8(read("files.txt")?).map_err(Error::other)?;
    for name in list.lines().map(str::trim).filter(|l| !l.is_empty() && !l.contains("..")) {
        let to = fresh.join(name);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&to, read(name)?)?;
    }
    std::fs::write(fresh.join("engine.json"), &pointer)?;
    let _ = std::fs::remove_dir_all(dir);
    std::fs::rename(&fresh, dir)?;
    log::info!("unpacked the offline engine in {:?}", started.elapsed());
    Ok(())
}

#[cfg(target_os = "android")]
fn android_logger_init() {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("isorun"));
}

/// The web entry point: the app on the page's canvas `#isorun`.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    use wasm_bindgen::JsCast;
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas = document.get_element_by_id("isorun").unwrap().dyn_into::<web_sys::HtmlCanvasElement>().unwrap();
        // the page's offline engine is looked up first, so the front end offers it from the start
        isorun_client::engine::init().await;
        let options = eframe::WebOptions { wgpu_options: isorun_ui::wgpu_options(), ..Default::default() };
        let _ = eframe::WebRunner::new().start(canvas, options, Box::new(|cc| Ok(Box::new(app::IsorunApp::new(cc))))).await;
    });
}
