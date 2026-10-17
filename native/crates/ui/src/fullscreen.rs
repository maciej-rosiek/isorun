//! Full screen: the window's natively (eframe's viewport), the page's on the web, where eframe's
//! web runner does nothing with the viewport's full screen command and never says whether the
//! page is full screen (`Element.requestFullscreen`, `document.fullscreenElement`).

/// Whether the app fills the screen.
pub fn is_on(ctx: &egui::Context) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ctx;
        web_sys::window().and_then(|w| w.document()).is_some_and(|d| d.fullscreen_element().is_some())
    }
    #[cfg(not(target_arch = "wasm32"))]
    ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
}

/// Fill the screen, or go back to a window. On the web it must come from a key or a click (the
/// browsers allow it only then), as the menus' rows and F11 do.
pub fn set(ctx: &egui::Context, on: bool) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ctx;
        let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
        if on {
            if let Some(root) = document.document_element() {
                if let Ok(p) = root.request_fullscreen() {
                    let _ = p;
                }
            }
        } else if document.fullscreen_element().is_some() {
            document.exit_fullscreen();
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
}
