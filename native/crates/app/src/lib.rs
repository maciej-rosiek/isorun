//! Isorun, the game: the front end, the game screen and the HUD over the isometric renderer, on
//! eframe (egui and wgpu) for the desktop, Android and the web.

pub mod app;
#[cfg(target_os = "android")]
mod android_insets;
#[cfg(target_os = "android")]
mod android_keyboard;

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
    // `--size=412x915`: a window of that size from the start (a phone's, for a test), as the app
    // decides some things (a phone's layout, the Full screen row) when it starts
    let size = launch_args().iter().find_map(|a| {
        let (w, h) = a.strip_prefix("--size=")?.split_once('x')?;
        Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
    });
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title(servers::APP_NAME).with_inner_size(size.unwrap_or([1600.0, 1000.0])).with_min_inner_size([320.0, 240.0]),
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
    log_panics();
    android_insets::set_app(&android);
    if let Some(dir) = android.internal_data_path() {
        if let Err(e) = unpack_engine(&android, &dir.join("engine")) {
            log::error!("the offline engine was not unpacked: {e}");
        }
        let _ = platform::storage::ANDROID_DIR.set(dir);
    }
    let mut options = native_options();
    options.android_app = Some(android);
    if let Err(e) = eframe::run_native(servers::APP_NAME, options, Box::new(|cc| Ok(Box::new(app::IsorunApp::new(cc))))) {
        log::error!("the app could not start: {e}");
    }
}

/// A panic's message, where and the stack in the log (logcat): Android shows a panicked app only
/// closing, and the standard hook writes to a stderr nobody reads.
#[cfg(target_os = "android")]
fn log_panics() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("isorun panicked: {info}\n{}", std::backtrace::Backtrace::force_capture());
        previous(info);
    }));
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

/// The web entry point: the app on the page's canvas `#isorun`. In a Web Worker (the tile
/// workers, `tile-worker.js`, which load this same module for `enhance_job`) it does nothing.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    use wasm_bindgen::JsCast;
    let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
    console_error_panic_hook_set();
    let _ = eframe::WebLogger::init(log::LevelFilter::Info);
    use_tile_workers();
    wasm_bindgen_futures::spawn_local(async move {
        let canvas = document.get_element_by_id("isorun").unwrap().dyn_into::<web_sys::HtmlCanvasElement>().unwrap();
        // the page's offline engine is looked up first, so the front end offers it from the start
        isorun_client::engine::init().await;
        let options = eframe::WebOptions { wgpu_options: isorun_ui::wgpu_options(), ..Default::default() };
        let _ = eframe::WebRunner::new().start(canvas, options, Box::new(|cc| Ok(Box::new(app::IsorunApp::new(cc))))).await;
    });
}

/// A panic's message on the console, not wasm's bare "unreachable".
#[cfg(target_arch = "wasm32")]
fn console_error_panic_hook_set() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        web_sys::console::error_1(&format!("isorun panicked: {info}").into());
        previous(info);
    }));
}

/// A tile worker's job (`isorun_render::remote`): the job's bytes in, the enhanced tile's out.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn enhance_job(job: &[u8]) -> Vec<u8> {
    isorun_render::remote::enhance_job(job)
}

/// The renderer's tiles are enhanced on the page's tile workers (`globalThis.isorunEnhance`,
/// index.html) where it set them up, as the web build's tile-enhancer.ts does: the page's thread
/// keeps its frames.
#[cfg(target_arch = "wasm32")]
fn use_tile_workers() {
    use wasm_bindgen::{closure::Closure, JsCast, JsValue};
    let Ok(f) = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("isorunEnhance")) else { return };
    let Some(f) = f.dyn_ref::<js_sys::Function>().cloned() else { return };
    isorun_render::remote::set_job_runner(Box::new(move |job: Vec<u8>, done: Box<dyn FnOnce(Vec<u8>)>| {
        let back = Closure::once_into_js(move |bytes: JsValue| {
            let bytes = bytes.dyn_into::<js_sys::Uint8Array>().map(|a| a.to_vec()).unwrap_or_default();
            done(bytes);
        });
        if f.call2(&JsValue::NULL, &js_sys::Uint8Array::from(&job[..]), &back).is_err() {
            log::warn!("a tile worker did not take its job");
        }
    }));
}
