//! Plays an offline game with no window: logs in, starts the game, sends keys and renders the
//! scene into a PNG. `cargo run -p isorun-app --example offline -- out.png [keys]`.

use isorun_client::session::{Session, SessionEvent};
use isorun_client::servers;
use isorun_webtiles::{cm, key_message, Phase};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn pump(s: &mut Session, secs: f64) {
    let end = Instant::now() + Duration::from_secs_f64(secs);
    while Instant::now() < end {
        for e in s.poll() {
            match e {
                SessionEvent::Closed(why) => eprintln!("closed: {why}"),
                SessionEvent::Gamedata(g) => eprintln!("gamedata: {g:?}"),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or("offline.png".into());
    let keys = args.get(2).cloned().unwrap_or("aaao".into());
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
    assert!(servers::offline_offered(), "no offline engine");
    let mut s = Session::new(servers::offline_server(), Some("tester".into()), wake);
    pump(&mut s, 0.5);
    s.send(cm::login("tester", ""));
    pump(&mut s, 0.5);
    let game = s.state.lobby.games.first().expect("no game links").clone();
    eprintln!("lobby: {:?} as {:?}", game, s.state.lobby.username);
    s.send(cm::play(&game.id));
    pump(&mut s, 4.0);
    eprintln!("after play: phase {:?} exit {:?} diags {:?} {:?}", s.state.phase, s.state.exit, s.diagnostics, s.state.diagnostics.iter().map(|d| d.text.clone()).collect::<Vec<_>>());
    for c in keys.chars() {
        s.send(key_message(c as i32));
        pump(&mut s, 1.0);
    }
    let t = Instant::now();
    while s.gamedata.is_none() && t.elapsed() < Duration::from_secs(120) {
        pump(&mut s, 0.1);
    }
    eprintln!("gamedata after {:?}", t.elapsed());
    pump(&mut s, 0.5);
    eprintln!("phase {:?}, gamedata {}, player at {:?}, {} billboards, {} cells, messages:", s.state.phase, s.gamedata.is_some(), s.scene.player, s.scene.billboards.len(), s.scene.cells.len());
    for m in s.state.messages.lines.iter().rev().take(5) {
        eprintln!("  {}", m.text);
    }
    assert_eq!(s.state.phase, Phase::Playing);
    let gd = s.gamedata.clone().expect("gamedata");

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() })).expect("device");
    let mut r = isorun_render::IsoRenderer::new(device.clone(), queue.clone());
    r.set_torch_model(isorun_client::testbed::TORCH_GLB).unwrap();
    r.set_tiles(gd);
    r.set_scene(s.scene.clone());
    let mut camera = isorun_render::IsoCamera::default();
    camera.snap_to(s.scene.player.0, s.scene.player.1);
    let (w, h) = (1280u32, 800u32);
    r.resize(w as f32, h as f32, 1.0);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    for _ in 0..12 {
        camera.update(1.0 / 60.0, s.scene.player.0, s.scene.player.1);
        r.follow(&camera);
        let mut encoder = device.create_command_encoder(&Default::default());
        r.render(&mut encoder, &view, wgpu::TextureFormat::Rgba8UnormSrgb);
        queue.submit([encoder.finish()]);
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        std::thread::sleep(Duration::from_millis(20));
    }
    let bytes_per_row = (w * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (bytes_per_row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bytes_per_row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let data = buffer.slice(..).get_mapped_range().unwrap();
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let start = (row * bytes_per_row) as usize;
        pixels.extend_from_slice(&data[start..start + (w * 4) as usize]);
    }
    image::save_buffer(&out, &pixels, w, h, image::ColorType::Rgba8).unwrap();
    eprintln!("wrote {out}");
    s.close();
    pump(&mut s, 0.5);
}
