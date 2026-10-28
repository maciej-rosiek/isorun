// The level's geometry in chunks (`buildStatics`, `buildChunk` in render-iso-3d.ts), included into
// renderer.rs.

struct ChunkPart {
    ground: Vec<GroundLayer>,
    walls: Vec<WallBlock>,
    sprites: Vec<Sprite>,
}

fn part_at(parts: &mut HashMap<(i32, i32), ChunkPart>, x: i32, y: i32) -> &mut ChunkPart {
    parts.entry(chunk_id(x, y)).or_insert_with(|| ChunkPart { ground: Vec::new(), walls: Vec::new(), sprites: Vec::new() })
}

impl IsoRenderer {
    /// The chunks that may build differently since `built`: around every cell whose drawn content
    /// changed (as far as a water cell looks for its shore), and the cells whose cut-away or items changed.
    fn dirty_chunks(built: &Built, cells: &HashMap<CellKey, u64>, cut: &HashSet<CellKey>, standing: &HashMap<CellKey, u64>) -> HashSet<(i32, i32)> {
        let mut dirty = HashSet::new();
        let mut at = |key: CellKey, reach: i32| {
            let (x, y) = key_to_xy(key);
            for dy in [-reach, reach] {
                for dx in [-reach, reach] {
                    dirty.insert(chunk_id(x + dx, y + dy));
                }
            }
        };
        for (key, h) in cells {
            if built.cells.get(key) != Some(h) {
                at(*key, SHORE_SEARCH);
            }
        }
        for key in built.cells.keys() {
            if !cells.contains_key(key) {
                at(*key, SHORE_SEARCH);
            }
        }
        for key in cut.symmetric_difference(&built.cut) {
            at(*key, 0);
        }
        for (key, sig) in standing {
            if built.standing.get(key) != Some(sig) {
                at(*key, 0);
            }
        }
        for key in built.standing.keys() {
            if !standing.contains_key(key) {
                at(*key, 0);
            }
        }
        dirty
    }

    fn cut_away_key(&mut self, scene: &Scene) -> u64 {
        let focus = self.focus(scene);
        let cut = cut_away_cells(scene, &focus, self.level_yaw(), -self.level_pitch(), &self.view);
        let mut keys: Vec<CellKey> = cut.into_iter().collect();
        keys.sort_unstable();
        hash_of(&keys)
    }

    fn statics_key(&mut self, scene: &Scene) -> u64 {
        let revision = self.statics_revision(scene);
        let cut = self.cut_away_key(scene);
        let w = &self.lights.walls;
        hash_of(&(
            revision,
            cut,
            self.level_yaw().to_bits(),
            self.level_pitch().to_bits(),
            self.view.wall_height.to_bits(),
            (w.bevel.to_bits(), w.moss.to_bits(), w.moss_colour, self.voxel_key()),
            self.lights.fog.height.to_bits(),
            self.lights.fog.reach.to_bits(),
            self.lights.water.enabled,
            self.lights.lava.enabled,
            self.pages.as_ref().map(|p| p.pages.len()).unwrap_or(0) > 0,
            self.loot.hidden(),
        ))
    }

    fn voxel_key(&self) -> u64 {
        if !self.voxels.on() {
            return 0;
        }
        let v = self.lights.voxels;
        hash_of(&(self.voxels.key(), v.wall.to_bits(), v.floor.to_bits(), v.figure.to_bits()))
    }

    fn clear_statics(&mut self) {
        self.chunks.clear();
        self.sprite_chunks.clear();
        self.chunks_key = 0;
        self.built = None;
        self.tile_torches.clear();
        self.tile_torches_version += 1;
    }

    /// The level's geometry in chunks of `CHUNK_CELLS` square: only a chunk whose cells, cut-away
    /// walls or items changed is built again.
    fn build_statics(&mut self, scene: &Arc<Scene>, tiles: &Arc<dyn TileSource>) {
        let level_pitch = self.level_pitch();
        let level_yaw = self.level_yaw();
        let pose = self.upright(level_pitch);
        let w = &self.lights.walls;
        let chunks_key = hash_of(&(
            self.view.wall_height.to_bits(),
            (w.bevel.to_bits(), w.moss.to_bits(), w.moss_colour, self.voxel_key()),
            self.lights.water.enabled,
            self.lights.lava.enabled,
        ));
        if chunks_key != self.chunks_key {
            self.clear_statics();
            self.chunks_key = chunks_key;
        }
        let sprites_view = hash_of(&(level_yaw.to_bits(), pose.stretch.to_bits(), pose.lean.to_bits()));
        let turned = sprites_view != self.sprites_view;
        self.sprites_view = sprites_view;
        let focus = self.focus(scene);
        let cut = cut_away_cells(scene, &focus, level_yaw, -level_pitch, &self.view);
        let in_flight = self.loot.hidden();
        let mut standing = standing_by_cell(scene);
        for &(x, y) in &in_flight {
            if let Some(sig) = standing.get_mut(&cell_key(x, y)) {
                *sig = hash_of(&(*sig, "in flight"));
            }
        }
        let cells: HashMap<CellKey, u64> = scene.cells.iter().map(|(k, c)| (*k, drawn_hash(c))).collect();
        let dirty = self.built.as_ref().map(|b| Self::dirty_chunks(b, &cells, &cut, &standing));
        self.built = Some(Built { cells, cut, standing });
        let within = |x: i32, y: i32| dirty.as_ref().is_none_or(|d| d.contains(&chunk_id(x, y)));
        let plan = plan_scene(scene, level_yaw, -level_pitch, &self.view, &focus, &within, false);
        let sprite_plan = if turned && dirty.is_some() { Some(plan_scene(scene, level_yaw, -level_pitch, &self.view, &focus, &|_, _| true, true)) } else { None };
        let revision = self.statics_revision(scene);
        self.fog.build(&self.gpu, scene, revision, self.lights.fog.height, self.lights.fog.reach);
        self.liquids.build(&self.gpu, scene, revision);
        let forward = ground_basis(level_yaw).1;
        {
            let pages = self.pages.as_mut().unwrap();
            self.features.build(&self.gpu, scene, tiles.as_ref(), pages, self.view.wall_height, forward);
        }
        let mut grounds: HashMap<(i32, i32), ChunkPart> = HashMap::new();
        let mut sprites: HashMap<(i32, i32), ChunkPart> = HashMap::new();
        for g in &plan.ground {
            if !is_tile_feature(scene.cell(g.x, g.y)) {
                part_at(&mut grounds, g.x, g.y).ground.push(*g);
            }
        }
        for w in &plan.walls {
            if !is_tile_feature(scene.cell(w.x, w.y)) {
                part_at(&mut grounds, w.x, w.y).walls.push(w.clone());
            }
        }
        for s in &sprite_plan.as_ref().unwrap_or(&plan).sprites {
            let b = s.billboard.map(|i| &scene.billboards[i]);
            if b.is_some_and(|b| actor_id(b).is_some() || (b.kind == isorun_scene::BillboardKind::Item && in_flight.contains(&(b.x, b.y)))) {
                continue;
            }
            if b.is_none() && is_tile_feature(scene.cell(s.x, s.y)) {
                continue;
            }
            part_at(&mut sprites, s.x, s.y).sprites.push(s.clone());
        }
        let everywhere = dirty.is_none();
        let ground_key = |r: &IsoRenderer, p: &ChunkPart| {
            let ground: Vec<(i32, i32, TileId, u32)> = p.ground.iter().map(|g| (g.x, g.y, r.ground_tile(scene, g), g.lift)).collect();
            let walls: Vec<(i32, i32, u32, TileId, Vec<(Side, TileId)>)> = p.walls.iter().map(|w| (w.x, w.y, w.height.to_bits(), w.top, w.faces.iter().map(|f| (f.side, f.tile)).collect())).collect();
            hash_of(&(ground, walls))
        };
        let sprite_key = |p: &ChunkPart| {
            let s: Vec<(i32, i32, bool, bool, Vec<(u32, u32, u32, u32)>)> = p.sprites.iter().map(|s| (s.x, s.y, s.lying, s.pile, s.layers.iter().map(|l| (l.tile, l.ox.to_bits(), l.oy.to_bits(), l.ymax.map_or(u32::MAX, f32::to_bits))).collect())).collect();
            hash_of(&(sprites_view, s))
        };
        for (sprites_pass, parts) in [(false, grounds), (true, sprites)] {
            let mut ids: Vec<(i32, i32)> = parts.keys().copied().collect();
            ids.sort_unstable();
            for id in &ids {
                let p = &parts[id];
                let key = if sprites_pass { sprite_key(p) } else { ground_key(self, p) };
                let old = if sprites_pass { self.sprite_chunks.get(id) } else { self.chunks.get(id) };
                if old.is_some_and(|c| c.key == key) {
                    continue;
                }
                let chunk = self.build_chunk(scene, tiles.as_ref(), p, key);
                if sprites_pass {
                    self.sprite_chunks.insert(*id, chunk);
                } else {
                    self.chunks.insert(*id, chunk);
                }
            }
            let drop_all = everywhere || (sprites_pass && turned);
            let map = if sprites_pass { &mut self.sprite_chunks } else { &mut self.chunks };
            let gone: Vec<(i32, i32)> = map.keys().filter(|id| !parts.contains_key(id) && (drop_all || dirty.as_ref().is_some_and(|d| d.contains(id)))).copied().collect();
            for id in gone {
                map.remove(&id);
            }
        }
        self.tile_torches = self.chunks.values().flat_map(|c| c.torches.iter().copied()).collect();
        self.tile_torches.sort_by(|a, b| a.position.x.total_cmp(&b.position.x).then(a.position.z.total_cmp(&b.position.z)));
        self.tile_torches_version += 1;
    }

    fn add_wall_torch(&self, chunk: &mut Chunk, x: i32, y: i32, normal: (i32, i32)) {
        let height = TORCH_HEIGHT * (self.view.wall_height / 0.85).min(1.0);
        let (nx, ny) = (normal.0 as f32, normal.1 as f32);
        chunk.torches.push(Torch { position: Vec3::new(x as f32 + 0.5 + nx * 0.75, height + 0.2, y as f32 + 0.5 + ny * 0.75), cell: (x + normal.0, y + normal.1) });
    }

    fn build_chunk(&mut self, scene: &Scene, tiles: &dyn TileSource, part: &ChunkPart, key: u64) -> Chunk {
        let mut chunk = Chunk { key, meshes: Vec::new(), torches: Vec::new(), glows: false, voxels: None };
        let voxel_on = self.voxels.on();
        let relief = self.lights.voxels;
        let mut voxel_parts: Vec<VoxelPart> = Vec::new();
        let mut ground_batches: HashMap<u32, QuadBatch> = HashMap::new();
        let mut feature_batches: HashMap<u32, QuadBatch> = HashMap::new();
        let mut sprite_batches: HashMap<u32, QuadBatch> = HashMap::new();
        let mut mark_batches: HashMap<u32, QuadBatch> = HashMap::new();
        let mut pages_owned = self.pages.take().unwrap();
        let pages = &mut pages_owned;
        let gpu = &self.gpu;
        let texels = pages.page_texels();
        let level_yaw = self.level_yaw();
        let pose = self.upright(self.level_pitch());
        let mut quad = |pages: &mut Pages, batches: &mut HashMap<u32, QuadBatch>, lean: f32, tile: TileId, corners: [Point3; 4], texel_height: Option<f32>, shade: f32, glows: &mut bool| {
            let Some(rect) = tiles.tile(tile) else { return };
            let Some(placed) = pages.for_tile(&gpu.device, &gpu.page_layout, &gpu.page_sampler, tiles, &rect) else { return };
            if placed.label.is_some_and(|l| l.glow > 0) {
                *glows = true;
            }
            let batch = batches.entry(placed.page).or_insert_with(|| QuadBatch::new(texels, texels, lean, ATLAS_INSET));
            batch.add_grey(corners, placed.sx, placed.sy, rect.w as f32, texel_height.unwrap_or(rect.h as f32), shade);
        };
        let mut glows = false;
        let mut caps = QuadBatch::new(1.0, 1.0, 0.0, 0.0);
        let moss = linear(self.lights.walls.moss_colour);
        let moss_amount = self.lights.walls.moss;
        for g in &part.ground {
            let y = g.lift as f32 * LIFT_STEP;
            let (x0, z0) = (g.x as f32, g.y as f32);
            let tile = self.ground_tile(scene, g);
            let see_through = scene.cell(g.x, g.y).is_some_and(|c| c.translucent.contains(&tile));
            let batches = if see_through {
                &mut mark_batches
            } else if g.lift > 0 {
                &mut feature_batches
            } else {
                &mut ground_batches
            };
            let corners = [[x0, y, z0], [x0 + 1.0, y, z0], [x0 + 1.0, y, z0 + 1.0], [x0, y, z0 + 1.0]];
            let voxels = if voxel_on && !see_through && g.lift == 0 { self.voxels.part(gpu, pages, tiles, tile, VoxelKind::Floor, corners, None, relief.floor, 1.0) } else { None };
            match voxels {
                Some(p) => {
                    glows |= p.glows;
                    voxel_parts.push(p);
                }
                None => quad(pages, batches, 0.0, tile, corners, None, 1.0, &mut glows),
            }
        }
        let mut torch_faces = Vec::new();
        for w in &part.walls {
            let h = w.height;
            let torch_wall = scene.cell(w.x, w.y).is_some_and(|c| c.flags.torch_wall);
            let [r, g, b] = match tiles.tile(w.top) {
                Some(rect) => pages.average_colour(tiles, &rect),
                None => [0.3, 0.3, 0.3],
            };
            let lerp = |a: f32, b: f32| a + (b - a) * moss_amount;
            let cap_colour = [lerp(r * CAP_SHADE, moss[0]), lerp(g * CAP_SHADE, moss[1]), lerp(b * CAP_SHADE, moss[2])];
            let bevel = self.lights.walls.bevel.min(h / 2.0);
            let exposed = |s: Side| w.faces.iter().any(|f| f.side == s);
            let x0 = w.x as f32 + if exposed(Side::W) { bevel } else { 0.0 };
            let x1 = w.x as f32 + 1.0 - if exposed(Side::E) { bevel } else { 0.0 };
            let z0 = w.y as f32 + if exposed(Side::N) { bevel } else { 0.0 };
            let z1 = w.y as f32 + 1.0 - if exposed(Side::S) { bevel } else { 0.0 };
            caps.add([[x0, h, z0], [x1, h, z0], [x1, h, z1], [x0, h, z1]], 0.0, 0.0, 1.0, 1.0, cap_colour);
            let face_top = h - bevel;
            for face in &w.faces {
                let info = side(face.side);
                let (ax, ay) = ((w.x + info.from.0) as f32, (w.y + info.from.1) as f32);
                let (bx, by) = ((w.x + info.to.0) as f32, (w.y + info.to.1) as f32);
                let texel_height = tiles.tile(face.tile).map(|r| r.h as f32 * face_top.min(1.0));
                let corners = [[ax, face_top, ay], [bx, face_top, by], [bx, 0.0, by], [ax, 0.0, ay]];
                let voxels = if voxel_on { self.voxels.part(gpu, pages, tiles, face.tile, VoxelKind::Wall, corners, texel_height, relief.wall, 1.0) } else { None };
                match voxels {
                    Some(p) => {
                        glows |= p.glows;
                        voxel_parts.push(p);
                    }
                    None => quad(pages, &mut ground_batches, 0.0, face.tile, corners, texel_height, 1.0, &mut glows),
                }
                if bevel > 0.0 {
                    let cx = |x: f32| x.clamp(x0, x1);
                    let cz = |z: f32| z.clamp(z0, z1);
                    caps.add([[cx(ax), h, cz(ay)], [cx(bx), h, cz(by)], [bx, face_top, by], [ax, face_top, ay]], 0.0, 0.0, 1.0, 1.0, cap_colour);
                }
                if torch_wall {
                    torch_faces.push((w.x, w.y, info.normal));
                }
            }
        }
        let (right, forward) = ground_basis(level_yaw);
        for s in &part.sprites {
            let copies: &[(f32, f32, bool)] = if s.pile { &PILE_COPIES } else { &[] };
            let all: Vec<Option<(f32, f32, bool)>> = copies.iter().map(|c| Some(*c)).chain(std::iter::once(None)).collect();
            for copy in all {
                for (i, layer) in s.layers.iter().enumerate() {
                    let Some(rect) = tiles.tile(layer.tile) else { continue };
                    let f = tile_frame(&rect, layer.ox, layer.oy, layer.ymax);
                    if f.height <= 0.0 {
                        continue;
                    }
                    let scale = if copy.is_some() { PILE_SCALE } else { 1.0 };
                    let shade = if copy.is_some() { PILE_SHADE } else { 1.0 };
                    let (across, behind, mirror) = copy.unwrap_or((0.0, 0.0, false));
                    if s.lying {
                        let y = (if copy.is_some() { 4 + i } else { 10 + i }) as f32 * LIFT_STEP;
                        let cx = s.x as f32 + 0.5 + if copy.is_some() { right[0] * across + forward[0] * behind } else { 0.0 };
                        let cy = s.y as f32 + 0.5 + if copy.is_some() { right[1] * across + forward[1] * behind } else { 0.0 };
                        let x0 = cx + (f.left - 0.5) * scale;
                        let y0 = cy + (f.top - 0.5) * scale;
                        let (x1, y1) = (x0 + f.width * scale, y0 + f.height * scale);
                        let (l, r) = if mirror { (x1, x0) } else { (x0, x1) };
                        let depth = layer_depth(relief.figure, i) * scale;
                        let raised = y + depth;
                        let voxels = if voxel_on {
                            self.voxels.part(gpu, pages, tiles, layer.tile, VoxelKind::Figure, [[l, raised, y0], [r, raised, y0], [r, raised, y1], [l, raised, y1]], Some(f.texel_height), depth, shade)
                        } else {
                            None
                        };
                        match voxels {
                            Some(p) => {
                                glows |= p.glows;
                                voxel_parts.push(p);
                            }
                            None => quad(pages, &mut sprite_batches, SPRITE_LEAN, layer.tile, [[l, y, y0], [r, y, y0], [r, y, y1], [l, y, y1]], Some(f.texel_height), shade, &mut glows),
                        }
                    } else {
                        let cx = s.x as f32 + 0.5 + forward[0] * (BEHIND_ACTORS + behind) + right[0] * across;
                        let cy = s.y as f32 + 0.5 + forward[1] * (BEHIND_ACTORS + behind) + right[1] * across;
                        let left = (f.left - 0.5) * scale;
                        let right_edge = left + f.width * scale;
                        let top = (1.0 - f.top) * pose.stretch * scale;
                        let bottom = (1.0 - f.top - f.height) * pose.stretch * scale;
                        let (l, r) = if mirror { (right_edge, left) } else { (left, right_edge) };
                        let at = |a: f32, up: f32| [cx + right[0] * a + forward[0] * up * pose.lean.sin(), up * pose.lean.cos(), cy + right[1] * a + forward[1] * up * pose.lean.sin()];
                        let corners = [at(l, top), at(r, top), at(r, bottom), at(l, bottom)];
                        let depth = layer_depth(relief.figure, i) * scale;
                        let voxels = if voxel_on { self.voxels.part(gpu, pages, tiles, layer.tile, VoxelKind::Figure, corners, Some(f.texel_height), depth, shade) } else { None };
                        match voxels {
                            Some(p) => {
                                glows |= p.glows;
                                voxel_parts.push(p);
                            }
                            None => quad(pages, &mut sprite_batches, SPRITE_LEAN, layer.tile, corners, Some(f.texel_height), shade, &mut glows),
                        }
                    }
                }
            }
        }
        self.pages = Some(pages_owned);
        for (x, y, normal) in torch_faces {
            self.add_wall_torch(&mut chunk, x, y, normal);
        }
        chunk.glows = glows;
        let device = &self.gpu.device;
        chunk.voxels = self.voxels.chunk(device, voxel_parts);
        if !caps.is_empty() {
            chunk.meshes.push((Material::Cap, PageRef::Cap, Arc::new(GpuMesh::from_batch(device, &caps))));
        }
        let mut pages_sorted: Vec<(u32, QuadBatch)> = ground_batches.into_iter().collect();
        pages_sorted.sort_by_key(|p| p.0);
        for (page, batch) in pages_sorted {
            chunk.meshes.push((Material::Ground, PageRef::Page(page), Arc::new(GpuMesh::from_batch(device, &batch))));
        }
        let mut features_sorted: Vec<(u32, QuadBatch)> = feature_batches.into_iter().collect();
        features_sorted.sort_by_key(|p| p.0);
        for (page, batch) in features_sorted {
            chunk.meshes.push((Material::Feature, PageRef::Page(page), Arc::new(GpuMesh::from_batch(device, &batch))));
        }
        let mut marks_sorted: Vec<(u32, QuadBatch)> = mark_batches.into_iter().collect();
        marks_sorted.sort_by_key(|p| p.0);
        for (page, batch) in marks_sorted {
            chunk.meshes.push((Material::Mark, PageRef::Page(page), Arc::new(GpuMesh::from_batch(device, &batch))));
        }
        let mut sprites_sorted: Vec<(u32, QuadBatch)> = sprite_batches.into_iter().collect();
        sprites_sorted.sort_by_key(|p| p.0);
        for (page, batch) in sprites_sorted {
            chunk.meshes.push((Material::Sprite, PageRef::Page(page), Arc::new(GpuMesh::from_batch(device, &batch))));
        }
        chunk
    }

    /// Whether a chunk with a glowing tile is near enough to the middle of the view to be on the screen.
    fn glow_in_view(&self) -> bool {
        let hw = self.width / 2.0 / self.view.zoom;
        let hh = self.height / 2.0 / self.view.zoom / (-(self.cam.pitch as f32)).sin().max(0.2);
        let reach = (hw * hw + hh * hh).sqrt() + 1.0;
        let (x, y) = (self.cam.eye_x as f32 + 0.5, self.cam.eye_y as f32 + 0.5);
        self.chunks.iter().chain(self.sprite_chunks.iter()).any(|(id, c)| {
            if !c.glows {
                return false;
            }
            let a = chunk_area(*id);
            let dx = (a.x0 - x).max(0.0).max(x - a.x1);
            let dy = (a.y0 - y).max(0.0).max(y - a.y1);
            (dx * dx + dy * dy).sqrt() < reach
        })
    }
}
