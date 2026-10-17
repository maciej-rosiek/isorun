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
fn android_main(android: eframe::winit::platform::android::activity::AndroidApp) {
    android_logger_init();
    if let Some(dir) = android.internal_data_path() { let _ = platform::storage::ANDROID_DIR.set(dir); }
    let mut options = native_options();
    options.android_app = Some(android);
    let _ = eframe::run_native(servers::APP_NAME, options, Box::new(|cc| Ok(Box::new(app::IsorunApp::new(cc)))));
}

#[cfg(target_os = "android")]
fn android_logger_init() {}

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
