//! The isometric view inside the egui frame: `IsoRenderer` draws into a texture of its own size,
//! which egui shows as an image under the HUD.

use eframe::egui_wgpu::RenderState;
use isorun_render::IsoRenderer;

/// The texture the view is drawn into: written through its sRGB view, read by egui through the
/// plain one (egui blends in gamma space).
struct Target {
    _texture: wgpu::Texture,
    srgb: wgpu::TextureView,
    id: egui::TextureId,
    size: (u32, u32),
}

pub struct View3d {
    pub renderer: IsoRenderer,
    target: Option<Target>,
    pub rect: egui::Rect,
}

impl View3d {
    pub fn new(rs: &RenderState) -> Self {
        View3d { renderer: IsoRenderer::new(rs.device.clone(), rs.queue.clone()), target: None, rect: egui::Rect::NOTHING }
    }

    /// Draw a frame into `rect`; false while there is nothing to draw (no scene or tiles yet).
    pub fn paint(&mut self, ui: &egui::Ui, rs: &RenderState, rect: egui::Rect) -> bool {
        self.rect = rect;
        let ppp = ui.ctx().pixels_per_point();
        self.renderer.resize(rect.width(), rect.height(), ppp);
        let size = self.renderer.render_size();
        if self.target.as_ref().map(|t| t.size) != Some(size) {
            let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("iso view"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
            });
            let plain = texture.create_view(&Default::default());
            let srgb = texture.create_view(&wgpu::TextureViewDescriptor { format: Some(wgpu::TextureFormat::Rgba8UnormSrgb), ..Default::default() });
            let mut egui_renderer = rs.renderer.write();
            let id = match self.target.take() {
                Some(old) => {
                    egui_renderer.update_egui_texture_from_wgpu_texture(&rs.device, &plain, wgpu::FilterMode::Linear, old.id);
                    old.id
                }
                None => egui_renderer.register_native_texture(&rs.device, &plain, wgpu::FilterMode::Linear),
            };
            self.target = Some(Target { _texture: texture, srgb, id, size });
        }
        let target = self.target.as_ref().unwrap();
        let mut encoder = rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("iso frame") });
        let drawn = self.renderer.render(&mut encoder, &target.srgb, wgpu::TextureFormat::Rgba8UnormSrgb);
        rs.queue.submit([encoder.finish()]);
        if drawn {
            ui.painter().image(target.id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
        drawn
    }

    /// The cell under a point of the screen, in egui points.
    pub fn pick(&mut self, pos: egui::Pos2) -> Option<(i32, i32)> {
        let p = pos - self.rect.min;
        self.renderer.pick(p.x, p.y).map(isorun_scene::key_to_xy)
    }
}
