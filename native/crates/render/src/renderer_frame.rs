// A frame (`render` in render-iso-3d.ts): the clock, the actors and the layers moved on, the
// camera and the lights placed, then the scene pass and the screen effects.
// Included into renderer.rs.

/// The planes of a view-projection, for culling what is off the screen.
fn frustum_planes(m: Mat4) -> [Vec4; 6] {
    let r = |i: usize| m.row(i);
    [r(3) + r(0), r(3) - r(0), r(3) + r(1), r(3) - r(1), r(2), r(3) - r(2)]
}

fn box_visible(planes: &[Vec4; 6], min: Vec3, max: Vec3) -> bool {
    planes.iter().all(|p| {
        let v = Vec3::new(if p.x >= 0.0 { max.x } else { min.x }, if p.y >= 0.0 { max.y } else { min.y }, if p.z >= 0.0 { max.z } else { min.z });
        p.truncate().dot(v) + p.w >= 0.0
    })
}

impl IsoRenderer {
    fn ensure_pipelines(&mut self, samples: u32) {
        if self.pipelines.as_ref().is_some_and(|p| p.samples == samples) {
            return;
        }
        let mut world = HashMap::new();
        for m in [Material::Ground, Material::Sprite, Material::Actor, Material::Cap, Material::Clay] {
            world.insert(m, self.gpu.world_pipeline(m, samples));
        }
        self.pipelines = Some(Pipelines { samples, world });
        self.fog.prepare(&self.gpu, samples);
        self.liquids.prepare(&self.gpu, samples);
        self.combat.prepare(&self.gpu, samples);
        self.steps.prepare(&self.gpu, samples);
        self.loot.prepare(&self.gpu, samples);
        self.overlay.prepare(&self.gpu, samples);
    }

    fn land_impacts(&mut self, scene: &Scene, now: f64) {
        let mut i = 0;
        while i < self.impacts.len() {
            if self.impacts[i].time > self.clock {
                i += 1;
                continue;
            }
            let impact = self.impacts.remove(i);
            if let Some(token) = impact.token {
                self.events.shown.push(token);
            }
            if matches!(impact.action, ActorAction::Attack | ActorAction::Miss) {
                continue;
            }
            let Some(at) = self.actors.cell_of(impact.id) else { continue };
            let kind = if impact.action == ActorAction::Death {
                ImpactKind::Death
            } else if impact.id == PLAYER_ACTOR {
                ImpactKind::Hurt
            } else {
                ImpactKind::Hit
            };
            let stop = self.combat.impact(kind, at, impact.from, &|x, y| ground_floor(scene, x, y));
            self.stop_until = self.stop_until.max(now + stop);
        }
    }

    /// The camera for this frame: ortho or perspective, shaken by the hits, lifted by the centre lift.
    fn place_camera(&mut self) {
        let elevation = -(self.cam.pitch as f32);
        let (right, forward) = ground_basis(self.cam.yaw as f32);
        let shake = self.combat.shake();
        let mut target = Vec3::new(self.cam.eye_x as f32 + 0.5, 0.0, self.cam.eye_y as f32 + 0.5);
        target.x += right[0] * shake.x + forward[0] * shake.y;
        target.z += right[1] * shake.x + forward[1] * shake.y;
        let hw = self.width / 2.0 / self.view.zoom / shake.zoom;
        let hh = self.height / 2.0 / self.view.zoom / shake.zoom;
        let fov = self.view.fov;
        let perspective = fov > 0.0;
        let distance = if perspective { hh / (fov.to_radians() / 2.0).tan() } else { CAMERA_DISTANCE };
        let eye = Vec3::new(target.x - forward[0] * elevation.cos() * distance, elevation.sin() * distance, target.z - forward[1] * elevation.cos() * distance);
        let look = Mat4::look_at_rh(eye, target, Vec3::Y);
        let view = Mat4::from_rotation_z(-shake.roll) * look;
        // the centre lift (three's setViewOffset): the frustum moves down, the view's content up
        let lift = self.centre_lift / self.height;
        let proj = if perspective {
            let (near, far) = ((distance - PERSPECTIVE_DEPTH).max(0.1), distance + PERSPECTIVE_DEPTH);
            let top = near * (fov.to_radians() / 2.0).tan();
            let right_edge = top * self.width / self.height;
            let shift = 2.0 * top * lift;
            Mat4::frustum_rh(-right_edge, right_edge, -top - shift, top - shift, near, far)
        } else {
            let shift = 2.0 * hh * lift;
            Mat4::orthographic_rh(-hw, hw, -hh - shift, hh - shift, 0.1, CAMERA_DISTANCE * 2.0)
        };
        self.camera_view = view;
        self.camera_proj = proj;
        self.camera_pos = eye;
        self.perspective = perspective;
    }

    /// Every torch lights the level from the baked torch light map.
    fn place_lights(&mut self) {
        if let Some(scene) = self.scene.clone() {
            self.torch_light.bake(&scene, &self.tile_torches, self.tile_torches_version, self.lights.torch.distance, self.lights.torch.decay);
        }
        self.torch_light.upload(&self.gpu);
    }

    fn frame_uniform(&self, now: f64) -> FrameUniform {
        let view = self.camera_view;
        let lin = |hex: u32, k: f32| {
            let l = linear(hex);
            [l[0] * k, l[1] * k, l[2] * k, 1.0]
        };
        let to_view = |v: Vec3| (view * v.extend(0.0)).truncate();
        let detail = self.detail.detail();
        let s = &self.lights.shading;
        let gate = |on: bool, v: f32| if on { v } else { 0.0 };
        let mut lights = [LightUniform::default(); MAX_LIGHTS];
        let spec = self.lights.torch;
        let colour = linear(spec.colour);
        if let Some((p, intensity)) = self.combat.light() {
            let vp = (view * p.extend(1.0)).truncate();
            let c = linear(0xffd8a8);
            lights[TORCH_LIGHTS] = LightUniform { position: [vp.x, vp.y, vp.z, 4.0], colour: [c[0] * intensity, c[1] * intensity, c[2] * intensity, 1.0], world: [p.x, p.y, p.z, 0.0] };
        }
        let features = &self.features;
        let centre = (self.cam.eye_x as f32 + 0.5, self.cam.eye_y as f32 + 0.5);
        for (slot, (p, c, intensity)) in self.loot.lights(centre, TORCH_LIGHTS, &|x, y| features.ground_at(x, y)).into_iter().enumerate() {
            let vp = (view * p.extend(1.0)).truncate();
            lights[slot] = LightUniform { position: [vp.x, vp.y, vp.z, LOOT_LIGHT_REACH], colour: [c[0] * intensity, c[1] * intensity, c[2] * intensity, 1.0], world: [p.x, p.y, p.z, 0.0] };
        }
        let up = to_view(Vec3::Y);
        FrameUniform {
            view_proj: (self.camera_proj * view).to_cols_array_2d(),
            view: view.to_cols_array_2d(),
            proj: self.camera_proj.to_cols_array_2d(),
            inv_view: view.inverse().to_cols_array_2d(),
            camera: [self.camera_pos.x, self.camera_pos.y, self.camera_pos.z, self.perspective as u8 as f32],
            sky: lin(self.lights.ambient_sky, self.lights.ambient_intensity),
            ground: lin(self.lights.ambient_ground, self.lights.ambient_intensity),
            shading0: [s.wrap, s.bands, s.relief, s.depth],
            shading1: [s.sprites, gate(detail.gloss, s.gloss), gate(detail.rim, s.rim), gate(detail.cavity, s.cavity)],
            shading2: [gate(detail.variation, s.variation), gate(detail.wetness, s.wetness), gate(detail.glow, s.glow), gate(detail.sparkle, s.sparkle)],
            shading3: [detail.depth_steps as f32, detail.liquids as u8 as f32, detail.statuses as u8 as f32, (now - self.start) as f32],
            viewport: [self.width, self.height, MAX_LIGHTS as f32, 0.0],
            up: [up.x, up.y, up.z, 0.0],
            torch_light: [colour[0] * spec.intensity, colour[1] * spec.intensity, colour[2] * spec.intensity, self.tile_torches.first().map_or(1.0, |t| t.position.y)],
            lights,
        }
    }

    /// Draw a frame into `output` (a view of `format` at `render_size()`): returns false when
    /// there is nothing to draw yet (no scene, no tiles).
    pub fn render(&mut self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView, format: wgpu::TextureFormat) -> bool {
        let (Some(scene), Some(tiles)) = (self.scene.clone(), self.tiles.clone()) else { return false };
        if self.pages.is_none() {
            return false;
        }
        self.frame_number += 1;
        let now = now_seconds();
        if self.last_frame > 0.0 && self.quality.adaptive {
            self.resolution.frame(((now - self.last_frame) * 1000.0) as f32);
        }
        let dt = if self.last_frame > 0.0 { (now - self.last_frame).min(0.1) } else { 0.0 };
        self.last_frame = now;
        {
            let features = &self.features;
            let pose = self.upright(self.cam.pitch as f32);
            self.loot.advance(self.clock, self.cam.yaw as f32, pose, &|x, y| features.ground_at(x, y));
            self.events.landings.extend(self.loot.take_landings());
        }
        let static_key = self.statics_key(&scene);
        if static_key != self.static_key {
            self.build_statics(&scene, &tiles);
            self.static_key = static_key;
        }
        self.features.update(dt as f32);
        self.events.doors.extend(self.features.take_doors());
        if now >= self.stop_until {
            self.clock += dt * self.animation_speed;
        }
        self.land_impacts(&scene, now);
        let pose = self.upright(self.cam.pitch as f32);
        let features = &self.features;
        let hops = self.actors.update(self.clock, self.cam.yaw as f32, pose, &|x, y| features.ground_at(x, y));
        for hop in hops {
            self.steps.step(hop.from, hop.to, hop.seconds / self.animation_speed, &|x, y| ground_floor(&scene, x, y));
            self.events.hops.push(hop);
        }
        self.fog.update(dt as f32);
        self.place_camera();
        let rotation = self.camera_view.inverse();
        let (cam_right, cam_up) = (rotation.transform_vector3(Vec3::X), rotation.transform_vector3(Vec3::Y));
        self.combat.update(&self.gpu, dt as f32, cam_right, cam_up, &|x, y| ground_floor(&scene, x, y));
        self.steps.update(&self.gpu, dt as f32, cam_right, cam_up);
        {
            let features = &self.features;
            self.loot.update(&self.gpu, cam_right, pose.stretch, &|x, y| features.ground_at(x, y));
        }
        let danger = self.danger * (1.0 - DANGER_PULSE + DANGER_PULSE * (now as f32 * DANGER_PULSE_SPEED).sin());
        self.place_lights();
        let aim_colour_of = |r: &IsoRenderer, intent| {
            let c = if intent == crate::plan::AimIntent::Move { r.lights.aim.colour } else { aim_colour(intent) };
            let l = linear(c);
            [l[0] * r.lights.aim.brightness, l[1] * r.lights.aim.brightness, l[2] * r.lights.aim.brightness]
        };
        let focus = self.focus(&scene);
        let (level_yaw, level_pitch) = (self.level_yaw(), self.level_pitch());
        let lift_on = |x: i32, y: i32| if is_pointable(scene.cell(x, y)) { AIM_LIFT } else { wall_height_at(x, y, &focus, level_yaw, -level_pitch, &self.view) + AIM_LIFT };
        let aim = self.aim.filter(|_| scene.player_on_level).map(|a| (a, aim_colour_of(self, a.intent)));
        let route = self.route.as_ref().map(|r| (r.steps.iter().take(ROUTE_STEPS).copied().collect::<Vec<_>>(), r.target, lift_on(r.target.0, r.target.1), aim_colour_of(self, r.intent)));
        let cursor = self.cursor.filter(|c| c.mode != isorun_scene::CursorMode::Map).map(|c| ((c.x, c.y), lift_on(c.x, c.y), aim_colour_of(self, if c.mode == isorun_scene::CursorMode::Target { crate::plan::AimIntent::Attack } else { crate::plan::AimIntent::Move })));
        self.overlay.place(&self.gpu, scene.player, aim, route, cursor);

        let (width, height) = self.render_size();
        let samples = 1;
        self.ensure_pipelines(samples);
        self.post.ensure(&self.gpu, width, height, samples);
        {
            let pages = self.pages.as_mut().unwrap();
            pages.upload(&self.gpu.device, &self.gpu.queue, encoder, &self.mipmapper);
        }
        // what is drawn this frame
        let view_proj = self.camera_proj * self.camera_view;
        let planes = frustum_planes(view_proj);
        let mut items: Vec<DrawItem> = Vec::new();
        // the level's chunks by material, then nearest first, as three sorts its opaque objects
        // (painterSortStable): what stands in front is drawn first, so the depth test can spare
        // the shading of what it hides
        let mut chunk_items: Vec<(u8, f32, DrawItem)> = Vec::new();
        for (id, chunk) in self.chunks.iter().chain(self.sprite_chunks.iter()) {
            let a = chunk_area(*id);
            let visible = box_visible(&planes, Vec3::new(a.x0 - 1.0, -1.5, a.y0 - 1.0), Vec3::new(a.x1 + 1.0, 3.0, a.y1 + 1.0));
            let centre = self.camera_view.transform_point3(Vec3::new((a.x0 + a.x1) * 0.5, 0.0, (a.y0 + a.y1) * 0.5));
            if !visible {
                continue;
            }
            for (material, page, mesh) in &chunk.meshes {
                chunk_items.push((*material as u8, -centre.z, DrawItem { material: *material, page: *page, mesh: mesh.clone(), uniform: DrawUniform::plain() }));
            }
        }
        chunk_items.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        items.extend(chunk_items.into_iter().map(|(_, _, item)| item));
        self.features.draws(&mut items);
        self.actors.draws(&mut items);
        self.loot.draws(&mut items);
        // the uniforms of every draw, in one buffer
        self.draws.clear();
        let offsets: Vec<u32> = items.iter().map(|i| self.draws.push(&i.uniform)).collect();
        self.draws.upload(&self.gpu.device, &self.gpu.queue, &self.gpu.draw_layout);
        let frame = self.frame_uniform(now);
        self.gpu.queue.write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame));
        let page_group = |r: &IsoRenderer, p: PageRef| -> Option<wgpu::BindGroup> {
            match p {
                PageRef::Page(i) => r.pages.as_ref().and_then(|pages| pages.bind_group(i)).cloned(),
                PageRef::Cap => Some(r.cap_group.clone()),
                PageRef::Door => Some(r.door_group.clone()),
            }
        };
        let groups: Vec<Option<wgpu::BindGroup>> = items.iter().map(|i| page_group(self, i.page)).collect();

        let pipelines = self.pipelines.as_ref().unwrap();
        self.fog.upload(&self.gpu);
        // the scene pass
        let targets = self.post.targets.as_ref().unwrap();
        let bg = linear(self.lights.background);
        let views: Vec<wgpu::TextureView> = [&targets.colour, &targets.normal, &targets.emission].iter().map(|t| t.create_view(&wgpu::TextureViewDescriptor::default())).collect();
        let depth_view = targets.depth.create_view(&wgpu::TextureViewDescriptor::default());
        {
            let clear = |c: wgpu::Color| wgpu::Operations { load: wgpu::LoadOp::Clear(c), store: wgpu::StoreOp::Store };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment { view: &views[0], depth_slice: None, resolve_target: None, ops: clear(wgpu::Color { r: bg[0] as f64, g: bg[1] as f64, b: bg[2] as f64, a: 1.0 }) }),
                    Some(wgpu::RenderPassColorAttachment { view: &views[1], depth_slice: None, resolve_target: None, ops: clear(wgpu::Color { r: 0.5, g: 0.5, b: 1.0, a: 1.0 }) }),
                    Some(wgpu::RenderPassColorAttachment { view: &views[2], depth_slice: None, resolve_target: None, ops: clear(wgpu::Color::TRANSPARENT) }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &depth_view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.frame_group, &[]);
            let mut current: Option<Material> = None;
            for (k, item) in items.iter().enumerate() {
                if !box_visible_item(&planes, item) {
                    continue;
                }
                let Some(group) = &groups[k] else { continue };
                if current != Some(item.material) {
                    pass.set_pipeline(&pipelines.world[&item.material]);
                    current = Some(item.material);
                }
                pass.set_bind_group(1, group, &[]);
                pass.set_bind_group(2, &self.draws.bind_group, &[offsets[k]]);
                item.mesh.draw(&mut pass);
            }
            self.combat.draw_opaque(&mut pass, &self.frame_group);
            self.overlay.draw(&mut pass, &self.frame_group);
            // what is blended, by three's render order
            self.combat.draw_sparks(&mut pass, &self.frame_group);
            self.steps.draw_prints(&mut pass, &self.frame_group);
            self.combat.draw_splats(&mut pass, &self.frame_group);
            self.liquids.draw(&mut pass, &self.frame_group);
            // over the liquids, which do not keep the depth of what they cover
            self.loot.draw(&mut pass, &self.frame_group);
            self.fog.draw(&mut pass, &self.frame_group);
            self.steps.draw_puffs(&mut pass, &self.frame_group);
        }
        let emission = self.liquids.glowing() || (self.pages.as_ref().is_some_and(|p| p.glowing) && self.detail.detail().glow && self.lights.shading.glow > 0.0 && (self.glow_in_view() || self.actors.glowing() || self.features.glowing));
        let origin = view_proj.project_point3(Vec3::ZERO);
        let ratio = self.pixel_ratio();
        let post_frame = PostFrame {
            proj: self.camera_proj,
            pixel_scale: self.view.zoom / DEFAULT_VIEW.zoom * ratio,
            pixel_origin: [(origin.x + 1.0) / 2.0 * width as f32, (1.0 - origin.y) / 2.0 * height as f32],
            danger,
            emission,
            perspective: self.perspective,
        };
        self.post.render(&self.gpu, encoder, &self.lights, &post_frame, output, format);
        true
    }
}

fn box_visible_item(_planes: &[Vec4; 6], _item: &DrawItem) -> bool {
    true
}
