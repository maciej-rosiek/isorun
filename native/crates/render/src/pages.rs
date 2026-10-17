//! The tiles in use, upscaled onto pages with their normal maps and surface data (the port of
//! `materials.ts` `AtlasMaterials`). Only the tiles drawn are enhanced, each once, on worker
//! threads where there are threads; a tile shows once its pixels are back. A page is 4096 texels
//! square (1024 source texels at four times), so every GPU's texture limit holds.

use crate::enhance::{enhance_tile, label_for, EnhancedPage, EnhancedTile, PageSlot, TileJob, PAGE_TEXELS};
use crate::labels::TileLabel;
use crate::lights::{linear, PaintSettings, Upscale};
use crate::mipmap::Mipmapper;
use crate::{AtlasImage, TileSource};
use isorun_scene::TileRect;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, Mutex};

pub const COLOUR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
pub const DATA_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A tile on its page: the page, the tile's place there in source texels, and what the tile is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub page: u32,
    pub sx: f32,
    pub sy: f32,
    pub label: Option<TileLabel>,
}

pub struct Page {
    enhanced: EnhancedPage,
    pub colour: wgpu::Texture,
    pub normal: wgpu::Texture,
    pub surface: wgpu::Texture,
    pub bind_group: wgpu::BindGroup,
    mips_stale: bool,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct RectKey(String, u32, u32, u32, u32, i32, i32, u32);

fn rect_key(r: &TileRect) -> RectKey {
    RectKey(r.atlas.clone(), r.sx, r.sy, r.w, r.h, r.ox, r.oy, r.cell)
}

struct Done {
    page: u32,
    slot: PageSlot,
    tile: EnhancedTile,
}

/// Enhances tiles away from the frame: on worker threads natively, a few a frame on the web.
struct Enhancer {
    jobs: Option<Sender<(u32, PageSlot, TileJob)>>,
    done: Receiver<Done>,
    done_tx: Sender<Done>,
    queued: Vec<(u32, PageSlot, TileJob)>,
}

impl Enhancer {
    fn new() -> Self {
        let (done_tx, done) = channel();
        let mut jobs = None;
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = channel::<(u32, PageSlot, TileJob)>();
            let rx = Arc::new(Mutex::new(rx));
            let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).saturating_sub(1).clamp(1, 4);
            for i in 0..workers {
                let rx = rx.clone();
                let out = done_tx.clone();
                let _ = std::thread::Builder::new().name(format!("tile-enhancer-{i}")).spawn(move || loop {
                    let job = { rx.lock().ok().and_then(|r| r.recv().ok()) };
                    let Some((page, slot, job)) = job else { break };
                    if out.send(Done { page, slot, tile: enhance_tile(&job) }).is_err() {
                        break;
                    }
                });
            }
            jobs = Some(tx);
        }
        Enhancer { jobs, done, done_tx, queued: Vec::new() }
    }

    fn enhance(&mut self, page: u32, slot: PageSlot, job: TileJob) {
        match &self.jobs {
            Some(tx) => {
                let _ = tx.send((page, slot, job));
            }
            None => {
                // the web's workers (remote.rs), when the page has set them up
                if let Some(run) = crate::remote::job_runner() {
                    let out = self.done_tx.clone();
                    let bytes = crate::remote::encode_job(&job);
                    // a worker that failed answers with nothing: the tile is enhanced here then
                    let back = Box::new(move |bytes: Vec<u8>| {
                        let tile = crate::remote::decode_tile(&bytes).unwrap_or_else(|| enhance_tile(&job));
                        let _ = out.send(Done { page, slot, tile });
                    });
                    return run(bytes, back);
                }
                self.queued.push((page, slot, job))
            }
        }
    }

    /// The tiles back since the last call; without workers, as many as a few milliseconds allow.
    fn take(&mut self) -> Vec<Done> {
        if !self.queued.is_empty() {
            let start = crate::now_seconds();
            while let Some((page, slot, job)) = self.queued.pop() {
                let _ = self.done_tx.send(Done { page, slot, tile: enhance_tile(&job) });
                if crate::now_seconds() - start > 0.006 {
                    break;
                }
            }
        }
        self.done.try_iter().collect()
    }
}

pub struct Pages {
    paint: PaintSettings,
    page_texels: u32,
    pub pages: Vec<Page>,
    placed: HashMap<RectKey, Option<Placed>>,
    averages: HashMap<RectKey, [f32; 3]>,
    enhancer: Enhancer,
    /// Whether a tile that gives light was placed: the screen then blooms the light it gives.
    pub glowing: bool,
    /// Bumps whenever tiles land on a page.
    pub revision: u64,
}

impl Pages {
    pub fn new(paint: PaintSettings, max_texture: u32) -> Self {
        let scale = crate::enhance::upscale_factor(paint.upscale);
        let page_texels = PAGE_TEXELS.min(max_texture / scale);
        Pages { paint, page_texels, pages: Vec::new(), placed: HashMap::new(), averages: HashMap::new(), enhancer: Enhancer::new(), glowing: false, revision: 0 }
    }

    pub fn paint(&self) -> &PaintSettings {
        &self.paint
    }

    /// The size, in source texels, that texture coordinates on a page are measured against.
    pub fn page_texels(&self) -> f32 {
        self.page_texels as f32
    }

    /// The mean colour of a tile's opaque texels, linear.
    pub fn average_colour(&mut self, tiles: &dyn TileSource, rect: &TileRect) -> [f32; 3] {
        let key = rect_key(rect);
        if let Some(c) = self.averages.get(&key) {
            return *c;
        }
        let colour = match tiles.atlas(&rect.atlas) {
            Some(img) => measure(&img, rect),
            None => [0.3, 0.3, 0.3],
        };
        self.averages.insert(key, colour);
        colour
    }

    /// The tile on its page; a new tile is placed and sent to be enhanced.
    pub fn for_tile(&mut self, device: &wgpu::Device, layout: &wgpu::BindGroupLayout, sampler: &wgpu::Sampler, tiles: &dyn TileSource, rect: &TileRect) -> Option<Placed> {
        let key = rect_key(rect);
        if let Some(p) = self.placed.get(&key) {
            return *p;
        }
        let placed = self.place(device, layout, sampler, tiles, rect);
        self.placed.insert(key, placed);
        placed
    }

    fn place(&mut self, device: &wgpu::Device, layout: &wgpu::BindGroupLayout, sampler: &wgpu::Sampler, tiles: &dyn TileSource, rect: &TileRect) -> Option<Placed> {
        let atlas = tiles.atlas(&rect.atlas)?;
        if rect.w == 0 || rect.h == 0 || rect.sx + rect.w > atlas.width || rect.sy + rect.h > atlas.height {
            return None;
        }
        let mut slot = self.pages.last_mut().and_then(|p| p.enhanced.allocate(rect.w, rect.h));
        if slot.is_none() {
            self.add_page(device, layout, sampler);
            slot = self.pages.last_mut().and_then(|p| p.enhanced.allocate(rect.w, rect.h));
        }
        let slot = slot?;
        let page = self.pages.len() as u32 - 1;
        let pixels = crop(&atlas, rect);
        let label = label_for(&pixels, rect.w, rect.h);
        let framed = rect.ox != 0 || rect.oy != 0 || rect.w != rect.cell || rect.h != rect.cell;
        if label.is_some_and(|l| l.glow > 0) {
            self.glowing = true;
        }
        self.enhancer.enhance(page, slot, TileJob { pixels, w: rect.w, h: rect.h, framed, paint: self.paint, label });
        Some(Placed { page, sx: slot.sx as f32, sy: slot.sy as f32, label })
    }

    fn add_page(&mut self, device: &wgpu::Device, layout: &wgpu::BindGroupLayout, sampler: &wgpu::Sampler) {
        let mut enhanced = EnhancedPage::new(&self.paint);
        enhanced.size = self.page_texels * crate::enhance::upscale_factor(self.paint.upscale);
        enhanced.limit(self.page_texels);
        let size = enhanced.size;
        let mips = mip_count(size);
        let texture = |label, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
                mip_level_count: mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
        };
        let colour = texture("page colour", COLOUR_FORMAT);
        let normal = texture("page normal", DATA_FORMAT);
        let surface = texture("page surface", DATA_FORMAT);
        let bind_group = page_bind_group(device, layout, sampler, &colour, &normal, &surface);
        self.pages.push(Page { enhanced, colour, normal, surface, bind_group, mips_stale: false });
    }

    /// Copy the tiles enhanced since the last call into their pages, and make the pages' mipmaps again.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, mipmapper: &Mipmapper) {
        let done = self.enhancer.take();
        for d in &done {
            let Some(page) = self.pages.get_mut(d.page as usize) else { continue };
            for (texture, pixels) in [(&page.colour, &d.tile.colour), (&page.normal, &d.tile.normal), (&page.surface, &d.tile.surface)] {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d { x: d.slot.x, y: d.slot.y, z: 0 }, aspect: wgpu::TextureAspect::All },
                    pixels,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(d.tile.w * 4), rows_per_image: Some(d.tile.h) },
                    wgpu::Extent3d { width: d.tile.w, height: d.tile.h, depth_or_array_layers: 1 },
                );
            }
            page.mips_stale = true;
        }
        if !done.is_empty() {
            self.revision += 1;
        }
        for page in &mut self.pages {
            if page.mips_stale {
                page.mips_stale = false;
                for t in [&page.colour, &page.normal, &page.surface] {
                    mipmapper.generate(device, encoder, t);
                }
            }
        }
    }

    pub fn bind_group(&self, page: u32) -> Option<&wgpu::BindGroup> {
        self.pages.get(page as usize).map(|p| &p.bind_group)
    }

    /// Whether the paint settings differ from these: the pages are then made again.
    pub fn same_paint(&self, paint: &PaintSettings) -> bool {
        self.paint == *paint
    }

    pub fn smooth(&self) -> bool {
        self.paint.upscale != Upscale::Pixel
    }
}

pub fn mip_count(size: u32) -> u32 {
    32 - size.max(1).leading_zeros()
}

pub fn page_bind_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, sampler: &wgpu::Sampler, colour: &wgpu::Texture, normal: &wgpu::Texture, surface: &wgpu::Texture) -> wgpu::BindGroup {
    let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("page"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view(colour)) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view(normal)) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&view(surface)) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

fn crop(atlas: &AtlasImage, r: &TileRect) -> Vec<u8> {
    let mut out = Vec::with_capacity((r.w * r.h * 4) as usize);
    for y in r.sy..r.sy + r.h {
        let start = ((y * atlas.width + r.sx) * 4) as usize;
        out.extend_from_slice(&atlas.rgba[start..start + (r.w * 4) as usize]);
    }
    out
}

fn measure(atlas: &AtlasImage, r: &TileRect) -> [f32; 3] {
    if r.sx + r.w > atlas.width || r.sy + r.h > atlas.height {
        return [0.3, 0.3, 0.3];
    }
    let px = crop(atlas, r);
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    for p in px.chunks_exact(4) {
        if p[3] < 128 {
            continue;
        }
        for c in 0..3 {
            sum[c] += p[c] as u64;
        }
        n += 1;
    }
    let n = n.max(1) as f32;
    let hex = |c: usize| ((sum[c] as f32 / n).round() as u32).min(255);
    let l = linear((hex(0) << 16) | (hex(1) << 8) | hex(2));
    // three's setRGB in sRGB goes through floats, not bytes: close enough at 8 bits
    [l[0], l[1], l[2]]
}
