//! Renders the test bed's level to a PNG with no window, for comparing the look with the web
//! build's: `VK_ICD_FILENAMES=<swiftshader icd> cargo run -p isorun-render --example snapshot -- out.png`.
//! `SNAPSHOT_ROOT` is another folder with the test bed's files (`crates/client/assets`), as on a
//! phone: built for `aarch64-linux-android` and run from `adb shell`, it compiles the shaders
//! with the phone's own GPU driver.

use isorun_render::{AtlasImage, IsoCamera, IsoRenderer, TileSource};
use isorun_scene::*;
use std::sync::Arc;

struct Packed {
    atlas: isorun_vault::PackedAtlas,
    image: Arc<AtlasImage>,
}

impl TileSource for Packed {
    fn tile(&self, id: TileId) -> Option<TileRect> {
        self.atlas.tile(id).cloned()
    }
    fn atlas(&self, _: &str) -> Option<Arc<AtlasImage>> {
        Some(self.image.clone())
    }
}

fn load(root: &str) -> Packed {
    let atlas = isorun_vault::PackedAtlas::from_json(&std::fs::read_to_string(format!("{root}/atlas.json")).unwrap()).unwrap();
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(format!("{root}/atlas.png")).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => buf[..info.buffer_size()].chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        other => panic!("{other:?}"),
    };
    Packed { atlas, image: Arc::new(AtlasImage { width: info.width, height: info.height, rgba }) }
}

/// The test bed's level as testbed/iso.ts prepares it: the human is the player, the monsters get
/// ids and a status each.
fn testbed(p: &Packed, des: &str) -> Scene {
    let vault = isorun_vault::parse_des(des).unwrap();
    let mut scene = isorun_vault::vault_scene(&vault, &p.atlas).unwrap();
    let statuses = |name: &str| match name {
        "orc" => vec![SpriteEffect::Wet],
        "jackal" => vec![SpriteEffect::Glowing],
        "goblin" => vec![SpriteEffect::Corroded],
        "kobold" => vec![SpriteEffect::Poisoned],
        "skeletal warrior" => vec![SpriteEffect::Petrified],
        "wight" => vec![SpriteEffect::Summoned],
        "necromancer" => vec![SpriteEffect::Burning],
        "ogre" => vec![SpriteEffect::Berserk],
        _ => vec![],
    };
    let mut next = 1;
    for b in &mut scene.billboards {
        if b.kind != BillboardKind::Monster {
            continue;
        }
        b.scenery = false;
        b.effects = statuses(b.name.as_deref().unwrap_or(""));
        b.ref_id = Some(next);
        next += 1;
    }
    let human = scene.billboards.iter_mut().find(|b| b.name.as_deref() == Some("human")).expect("no human");
    human.kind = BillboardKind::Player;
    human.attitude = Some(Attitude::Friendly);
    human.ref_id = Some(PLAYER_REF);
    scene.player = (human.x, human.y);
    scene.player_on_level = true;
    scene.revision += 1;
    scene
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or_else(|| "snapshot.png".into());
    let (w, h) = (1280u32, 800u32);
    // the test bed's level and atlas, as the client crate embeds them
    let root = std::env::var("SNAPSHOT_ROOT").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../client/assets").to_string());
    let root = root.as_str();
    let packed = Arc::new(load(root));
    let des = std::fs::read_to_string(format!("{root}/iso.des")).unwrap();
    let scene = Arc::new(testbed(&packed, &des));
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("adapter");
    eprintln!("adapter: {:?}", adapter.get_info().name);
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() })).expect("device");
    device.on_uncaptured_error(std::sync::Arc::new(|e| panic!("wgpu: {e}")));
    let mut r = IsoRenderer::new(device.clone(), queue.clone());
    let glb = std::fs::read(format!("{root}/torch_mounted.glb")).unwrap();
    r.set_torch_model(&glb).unwrap();
    r.set_tiles(packed.clone());
    let detail = match std::env::var("DETAIL").as_deref() {
        Ok("off") => isorun_render::SurfaceDetail::Off,
        Ok("low") => isorun_render::SurfaceDetail::Low,
        Ok("high") => isorun_render::SurfaceDetail::High,
        _ => isorun_render::SurfaceDetail::Medium,
    };
    r.set_surface_detail(detail);
    if let Ok(q) = std::env::var("QUALITY") {
        let quality = isorun_render::quality(&q).unwrap_or(isorun_render::lights::QUALITY_MEDIUM);
        r.set_quality(quality);
    }
    if let Ok(name) = std::env::var("LIGHTS") {
        r.set_lights(isorun_render::lights(&name).expect("no such look"));
    }
    r.set_scene(scene.clone());
    let mut camera = IsoCamera::default();
    camera.snap_to(scene.player.0, scene.player.1);
    r.resize(w as f32, h as f32, 1.0);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("out"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let frames: usize = std::env::var("FRAMES").ok().and_then(|f| f.parse().ok()).unwrap_or(30);
    let start = std::time::Instant::now();
    for _ in 0..frames {
        camera.update(1.0 / 60.0, scene.player.0, scene.player.1);
        r.follow(&camera);
        let mut encoder = device.create_command_encoder(&Default::default());
        r.render(&mut encoder, &view, wgpu::TextureFormat::Rgba8UnormSrgb);
        queue.submit([encoder.finish()]);
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    eprintln!("{frames} frames in {:?}", start.elapsed());
    let (rw, rh) = r.render_size();
    let bytes_per_row = (rw * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (bytes_per_row * rh) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bytes_per_row), rows_per_image: Some(rh) },
        },
        wgpu::Extent3d { width: rw, height: rh, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let data = buffer.slice(..).get_mapped_range().unwrap();
    let mut pixels = Vec::with_capacity((rw * rh * 4) as usize);
    for row in 0..rh {
        let start = (row * bytes_per_row) as usize;
        pixels.extend_from_slice(&data[start..start + (rw * 4) as usize]);
    }
    let file = std::fs::File::create(&out).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), rw, rh);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&pixels).unwrap();
    eprintln!("wrote {out} ({rw}x{rh})");
}
