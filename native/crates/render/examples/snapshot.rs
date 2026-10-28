//! Renders the test bed's level to a PNG with no window, for comparing the look with the web
//! build's: `VK_ICD_FILENAMES=<swiftshader icd> cargo run -p isorun-render --example snapshot -- out.png`.
//! `SNAPSHOT_ROOT` is another folder with the test bed's files (`crates/client/assets`), as on a
//! phone: built for `aarch64-linux-android` and run from `adb shell`, it compiles the shaders
//! with the phone's own GPU driver.

use isorun_render::{ActorAction, AtlasImage, CameraAngles, IsoCamera, IsoRenderer, TileSource, DEFAULT_ANGLES, DEFAULT_VIEW};
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
    let env_f32 = |name: &str, default: f32| std::env::var(name).ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(default);
    let size: Vec<u32> = std::env::var("SIZE").unwrap_or_else(|_| "1280x800".into()).split('x').filter_map(|v| v.parse().ok()).collect();
    let (w, h) = (size.first().copied().unwrap_or(1280), size.get(1).copied().unwrap_or(800));
    // the test bed's level and atlas, as the client crate embeds them
    let root = std::env::var("SNAPSHOT_ROOT").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../client/assets").to_string());
    let root = root.as_str();
    let packed = Arc::new(load(root));
    let des = std::fs::read_to_string(format!("{root}/iso.des")).unwrap();
    let scene = Arc::new(testbed(&packed, &des));
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("adapter");
    eprintln!("adapter: {:?}", adapter.get_info().name);
    let timestamps = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
    let timed_gpu = adapter.features().contains(timestamps);
    let required_features = if timed_gpu { timestamps } else { wgpu::Features::empty() };
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), required_features, ..Default::default() }))
            .expect("device");
    let queries = timed_gpu.then(|| {
        let set = device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("frame"), ty: wgpu::QueryType::Timestamp, count: 2 });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        (set, resolve, read)
    });
    let mut gpu_ms = 0.0;
    device.on_uncaptured_error(std::sync::Arc::new(|e| panic!("wgpu: {e}")));
    let mut r = IsoRenderer::new(device.clone(), queue.clone());
    r.set_tiles(packed.clone());
    let detail = match std::env::var("DETAIL").as_deref() {
        Ok("off") => isorun_render::SurfaceDetail::Off,
        Ok("low") => isorun_render::SurfaceDetail::Low,
        Ok("high") => isorun_render::SurfaceDetail::High,
        _ => isorun_render::SurfaceDetail::Medium,
    };
    r.set_surface_detail(detail);
    let mut quality = isorun_render::quality(&std::env::var("QUALITY").unwrap_or_default()).unwrap_or(isorun_render::lights::QUALITY_MEDIUM);
    quality.adaptive = false;
    r.set_quality(quality);
    if let Ok(name) = std::env::var("LIGHTS") {
        r.set_lights(isorun_render::lights(&name).expect("no such look"));
    }
    r.set_voxels(std::env::var("VOXELS").is_ok_and(|v| v == "1"));
    r.set_scene(scene.clone());
    let angles = CameraAngles { rotation: DEFAULT_ANGLES.rotation + env_f32("TURN", 0.0), tilt: env_f32("TILT", DEFAULT_ANGLES.tilt), ..DEFAULT_ANGLES };
    let mut camera = IsoCamera::new(angles, DEFAULT_VIEW.zoom * env_f32("ZOOM", 1.0));
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
    let (mut encode_ms, mut frame_ms, mut timed) = (0.0, 0.0, 0u32);
    let event_frame = std::env::var("EVENT_FRAME").ok().and_then(|f| f.parse().ok()).unwrap_or(frames.saturating_sub(6));
    for i in 0..frames {
        if i == event_frame {
            if std::env::var("DIE").is_ok() {
                let near = scene
                    .billboards
                    .iter()
                    .filter(|b| b.kind == BillboardKind::Monster)
                    .min_by_key(|b| (b.x - scene.player.0).abs() + (b.y - scene.player.1).abs());
                if let Some(id) = near.and_then(|b| b.ref_id) {
                    r.act(id, ActorAction::Death, Some(scene.player), None);
                }
            }
            if std::env::var("BREAK").is_ok() || std::env::var("REBUILD").is_ok() {
                let mut changed = (*scene).clone();
                let (px, py) = scene.player;
                let wall = changed.cells.values().filter(|c| c.kind == CellKind::Wall).min_by_key(|c| (c.x - px).abs() + (c.y - py).abs()).map(|c| (c.x, c.y));
                let floor = changed.cells.values().find(|c| c.kind == CellKind::Floor).cloned();
                if let (Some(key), Some(floor)) = (wall.map(|(x, y)| cell_key(x, y)), floor) {
                    if let Some(cell) = changed.cells.get_mut(&key) {
                        let (x, y) = (cell.x, cell.y);
                        *cell = SceneCell { x, y, visibility: Visibility::Visible, ..floor };
                    }
                }
                changed.revision += 1;
                changed.layout_revision = changed.revision;
                r.set_scene(Arc::new(changed));
                let mut encoder = device.create_command_encoder(&Default::default());
                let t = std::time::Instant::now();
                r.render(&mut encoder, &view, wgpu::TextureFormat::Rgba8UnormSrgb);
                eprintln!("a wall cell gone: chunks rebuilt and the frame encoded in {:.2} ms", t.elapsed().as_secs_f64() * 1000.0);
                queue.submit([encoder.finish()]);
                let _ = device.poll(wgpu::PollType::wait_indefinitely());
                continue;
            }
        }
        camera.update(1.0 / 60.0, scene.player.0, scene.player.1);
        r.follow(&camera);
        let t = std::time::Instant::now();
        let mut encoder = device.create_command_encoder(&Default::default());
        if let Some((set, _, _)) = &queries {
            encoder.write_timestamp(set, 0);
        }
        r.render(&mut encoder, &view, wgpu::TextureFormat::Rgba8UnormSrgb);
        let encoded = t.elapsed().as_secs_f64() * 1000.0;
        if let Some((set, resolve, read)) = &queries {
            encoder.write_timestamp(set, 1);
            encoder.resolve_query_set(set, 0..2, resolve, 0);
            encoder.copy_buffer_to_buffer(resolve, 0, read, 0, 16);
        }
        queue.submit([encoder.finish()]);
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let total = t.elapsed().as_secs_f64() * 1000.0;
        if let Some((_, _, read)) = &queries {
            read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            let stamps: Vec<u64> = bytemuck::cast_slice(&read.slice(..).get_mapped_range().unwrap()).to_vec();
            read.unmap();
            if i >= frames / 2 {
                gpu_ms += u32::try_from(stamps[1].saturating_sub(stamps[0])).map(f64::from).unwrap_or(0.0) * f64::from(queue.get_timestamp_period()) / 1e6;
            }
        }
        if i == 0 {
            eprintln!("first frame (every chunk built): encoded in {encoded:.2} ms, {total:.2} ms with the GPU");
        } else if i >= frames / 2 {
            encode_ms += encoded;
            frame_ms += total;
            timed += 1;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let n = f64::from(timed.max(1));
    eprintln!(
        "{frames} frames in {:?}; the last {timed}: CPU {:.2} ms a frame, {:.2} ms with the GPU, GPU timestamps {:.2} ms",
        start.elapsed(),
        encode_ms / n,
        frame_ms / n,
        gpu_ms / n
    );
    let counts = r.frame_counts();
    let v = r.voxel_stats();
    eprintln!(
        "level meshes: {} draws, {} triangles; voxels: {} draws, {} triangles, {} instances, {} tile meshes, {} pieces",
        counts.draws, counts.triangles, v.draws, v.triangles, v.instances, v.meshes, v.pieces
    );
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
