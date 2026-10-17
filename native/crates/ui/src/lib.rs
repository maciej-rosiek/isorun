//! Isorun's UI toolkit on egui: what every screen of the native app shares.

pub mod fonts;
pub mod harness;
pub mod keys;
pub mod text;
pub mod theme;
pub mod view3d;

use std::sync::Arc;

/// The fonts and the web build's look on an egui context.
pub fn install(ctx: &egui::Context) {
    fonts::install(ctx);
    theme::install(ctx);
}

/// The wgpu device Isorun asks for: the adapter's own limits, as the renderer's atlas pages and
/// shadow maps want more than the defaults.
pub fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.device_descriptor = Arc::new(|adapter| wgpu::DeviceDescriptor { label: Some("isorun"), required_limits: adapter.limits(), ..Default::default() });
    eframe::egui_wgpu::WgpuConfiguration { wgpu_setup: eframe::egui_wgpu::WgpuSetup::CreateNew(setup), ..Default::default() }
}
