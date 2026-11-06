//! `net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator` (26.3: `buildTerrain` = fill + surface + carvers).

use std::collections::HashMap;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_core::pos::quart_pos;
use mc_core::random::{LegacyRandomSource, WorldgenRandom, generate_unique_seed};
use mc_data::block::{Block, BlockState};
use mc_world::carving_mask::CarvingMask;
use mc_world::chunk::{LevelHeight, ProtoChunk, SectionsView};
use mc_world::heightmap::HeightmapType;
use mc_world::palette::BiomeId;

use crate::aquifer::{FluidPicker, FluidStatus};
use crate::beardifier::Beardifier;
use crate::biome::{BiomeGenerationSettings, BiomeRegistry, BiomeSource, NoiseSpawnFinder};
use crate::blending::{self, Blender};
use crate::carver::{self, WorldCarver};
use crate::data::{LoadError, WorldgenRegistries};
use crate::densityfunction::{DensityVolume, SamplerContext};
use crate::generator::{BiomeFn, BuildTerrainContext, ChunkGenerator, NoiseColumn};
use crate::material::MaterialRule;
use crate::noise_chunk::NoiseChunk;
use crate::noise_settings::{NoiseGeneratorSettings, NoiseSettings};
use crate::random_state::RandomState;
use crate::world_generation_context::WorldGenerationContext;

type LoadResult<T> = Result<T, LoadError>;

/// `NoiseBasedChunkGenerator`.
pub struct NoiseBasedChunkGenerator {
    biome_source: Arc<dyn BiomeSource>,
    settings: Arc<NoiseGeneratorSettings>,
    registries: Arc<WorldgenRegistries>,
    biomes: Arc<BiomeRegistry>,
    /// `settings.materialRule().value()`.
    material_rule: MaterialRule,
    /// `globalFluidPicker` (`createFluidPicker(settings)`).
    global_fluid_picker: FluidPicker,
    default_block: BlockState,
    /// Every carver referenced by a biome's generation settings, resolved once.
    carvers: HashMap<Identifier, Arc<dyn WorldCarver>>,
}

impl NoiseBasedChunkGenerator {
    /// `new NoiseBasedChunkGenerator(biomeSource, settings)`. Resolves the settings' material rule and the carvers of
    /// every biome (vanilla resolves both while loading the registries).
    pub fn new(
        biome_source: Arc<dyn BiomeSource>,
        settings: Arc<NoiseGeneratorSettings>,
        registries: Arc<WorldgenRegistries>,
        biomes: Arc<BiomeRegistry>,
    ) -> LoadResult<NoiseBasedChunkGenerator> {
        let default_block = settings.default_block.resolve()?;
        let global_fluid_picker = create_fluid_picker(&settings)?;
        let material_rule = MaterialRule::from_json(&settings.material_rule, &registries, &biomes)?;
        let mut carvers = HashMap::new();
        // the registry's biomes, and the inline (`Holder.Direct`) ones the biome source uses
        let inline = biome_source
            .possible_biomes()
            .iter()
            .copied()
            .filter(|b| BiomeRegistry::is_inline(*b));
        for biome in biomes.ids().chain(inline) {
            for id in biomes.get(biome).generation_settings().get_carvers() {
                if !carvers.contains_key(id) {
                    carvers.insert(id.clone(), carver::load_carver(&registries, id)?);
                }
            }
        }
        Ok(NoiseBasedChunkGenerator {
            biome_source,
            settings,
            registries,
            biomes,
            material_rule,
            global_fluid_picker,
            default_block,
            carvers,
        })
    }

    /// The biome source as the shared handle the generator was created with.
    pub fn biome_source_arc(&self) -> &Arc<dyn BiomeSource> {
        &self.biome_source
    }

    /// `generatorSettings()`.
    pub fn settings(&self) -> &Arc<NoiseGeneratorSettings> {
        &self.settings
    }

    pub fn registries(&self) -> &Arc<WorldgenRegistries> {
        &self.registries
    }

    pub fn biomes(&self) -> &Arc<BiomeRegistry> {
        &self.biomes
    }

    /// `createNoiseChunk(chunk, structureManager, blender, randomState, noiseSettings)`.
    pub fn create_noise_chunk(
        &self,
        chunk_pos: ChunkPos,
        ctx: &BuildTerrainContext<'_>,
        noise_settings: NoiseSettings,
    ) -> NoiseChunk {
        let beardifier = Beardifier::for_structures_in_chunk(chunk_pos, ctx.structure_starts);
        NoiseChunk::new(
            ctx.random_state,
            Some(beardifier),
            &self.settings,
            self.global_fluid_picker.clone(),
            ctx.blender,
            chunk_volume(chunk_pos, noise_settings),
        )
    }

    /// `iterateNoiseColumn(heightAccessor, randomState, blockX, blockZ, columnReference, tester)`.
    fn iterate_noise_column(
        &self,
        height_accessor: LevelHeight,
        random_state: &Arc<RandomState>,
        block_x: i32,
        block_z: i32,
        want_column: bool,
        tester: Option<&dyn Fn(BlockState) -> bool>,
    ) -> (Option<i32>, Option<NoiseColumn>) {
        let noise_settings = self
            .settings
            .noise
            .clamp_to_height_accessor(height_accessor);
        if noise_settings.height <= 0 {
            return (None, None);
        }
        let volume = DensityVolume::with_unit_step(
            1,
            noise_settings.height,
            1,
            block_x,
            noise_settings.min_y,
            block_z,
        );
        let mut write_to = want_column.then(|| vec![self.default_block; volume.size_y as usize]);

        let mut noise_chunk = NoiseChunk::new(
            random_state,
            None,
            &self.settings,
            self.global_fluid_picker.clone(),
            &Blender::empty(),
            volume,
        );
        let samplers = noise_chunk.caching_samplers_rc();
        let final_density = samplers.get(&self.settings.noise_router.final_density);
        let density_buffer = final_density.sample_volume_scoped(noise_chunk.volume());
        let aquifer = noise_chunk.aquifer();
        let mut result = None;
        for y in (0..volume.size_y).rev() {
            let density = density_buffer.get(volume.index_unchecked(0, y, 0));
            let block_y = volume.block_y(y);
            let base_state =
                aquifer.compute_substance(block_x, block_y, block_z, f64::from(density));
            let state = base_state.unwrap_or(self.default_block);
            if let Some(write_to) = &mut write_to {
                write_to[y as usize] = state;
            }
            if let Some(tester) = tester
                && tester(state)
            {
                result = Some(block_y + 1);
                break;
            }
        }
        let column = write_to.map(|states| NoiseColumn::new(volume.min_block_y, states));
        (result, column)
    }

    /// `buildSurface(...)`.
    fn build_surface(
        &self,
        chunk: &mut ProtoChunk,
        noise_chunk: &NoiseChunk,
        ctx: &BuildTerrainContext<'_>,
    ) {
        let context = self.generation_context(chunk);
        ctx.random_state.surface_system().build_surface(
            ctx.random_state,
            ctx.biome_manager,
            &context,
            &self.biomes,
            chunk,
            noise_chunk.volume(),
            noise_chunk.caching_samplers(),
            &self.material_rule,
            ctx.possible_biomes,
        );
    }

    /// `new WorldGenerationContext(this, chunk.getHeightAccessorForGeneration())`.
    fn generation_context(&self, chunk: &ProtoChunk) -> WorldGenerationContext {
        WorldGenerationContext::new(
            self.get_min_y(),
            self.get_gen_depth(),
            self.get_sea_level(),
            chunk.get_height_accessor_for_generation(),
        )
    }

    /// `getBiomeGenerationSettingsForCarver(biomeResolver, sourcePos)`'s biome.
    fn carver_biome(resolver: &dyn crate::biome::BiomeResolver, source_pos: ChunkPos) -> BiomeId {
        let quart_x = quart_pos::from_block(source_pos.get_min_block_x());
        let quart_z = quart_pos::from_block(source_pos.get_min_block_z());
        resolver.get_noise_biome(quart_x, 0, quart_z)
    }

    /// `generateCarvers(chunk, blender, noiseChunk, randomState, biomeManager, carverBiomeRegion, materialRule)`.
    fn generate_carvers(
        &self,
        chunk: &mut ProtoChunk,
        noise_chunk: &mut NoiseChunk,
        ctx: &BuildTerrainContext<'_>,
    ) {
        let random_state = ctx.random_state;
        let biome_resolver = self.biome_source.create_uncached_resolver(random_state);
        let mut random =
            WorldgenRandom::new(Box::new(LegacyRandomSource::new(generate_unique_seed())));
        let pos = chunk.pos();
        let context = self.generation_context(chunk);
        let protected_blocks_on_top = if chunk.is_upgrading() { 0 } else { 7 };
        let max_y = context.get_min_gen_y() + context.get_gen_depth() - 1 - protected_blocks_on_top;
        let mut mask = CarvingMask::new(context.get_min_gen_y() + 1, max_y);

        for dx in -8..=8 {
            for dz in -8..=8 {
                let source_pos = ChunkPos::new(pos.x + dx, pos.z + dz);
                let source_biome = match ctx.carver_biome_cache {
                    Some(cache) => cache.carver_biome(source_pos, &mut || {
                        Self::carver_biome(&*biome_resolver, source_pos)
                    }),
                    None => Self::carver_biome(&*biome_resolver, source_pos),
                };
                let source_settings = self.get_biome_generation_settings(source_biome);

                for (index, carver_id) in source_settings.get_carvers().iter().enumerate() {
                    let carver = &self.carvers[carver_id];
                    random.set_large_feature_seed(
                        random_state.seed().wrapping_add(index as i64),
                        source_pos.x,
                        source_pos.z,
                    );
                    if carver.is_start_chunk(&mut random) {
                        carver.carve(&context, &mut random, pos, source_pos, &mut mask);
                    }
                }
            }
        }

        if !mask.is_empty() {
            let correct_biome_manager = ctx.biome_manager.with_different_source(&*biome_resolver);
            let carving_filter = ctx.blender.get_carving_filter();
            self.apply_carving_mask(
                chunk,
                &mask,
                random_state,
                &context,
                noise_chunk,
                &|pos| correct_biome_manager.get_biome(pos),
                carving_filter.as_deref(),
            );
        }
    }

    /// `applyCarvingMask(chunk, mask, randomState, materialRule, context, noiseChunk, biomeGetter, filter)`.
    fn apply_carving_mask(
        &self,
        chunk: &mut ProtoChunk,
        mask: &CarvingMask,
        random_state: &Arc<RandomState>,
        context: &WorldGenerationContext,
        noise_chunk: &mut NoiseChunk,
        biome_getter: &dyn Fn(BlockPos) -> BiomeId,
        filter: Option<&(dyn Fn(i32, i32, i32) -> bool + Send + Sync)>,
    ) {
        let chunk_pos = chunk.pos();
        let samplers = noise_chunk.caching_samplers_rc();
        let aquifer = noise_chunk.aquifer();
        let uncarvable = mc_data::tags::get::<Block>("uncarvable");
        let is_uncarvable = |state: BlockState| {
            uncarvable
                .as_ref()
                .is_some_and(|tag| tag.contains(state.block()))
        };
        mask.visit(|x, z, bottom_y, top_y| {
            let mut has_grass = false;
            let world_x = chunk_pos.get_block_x(x);
            let world_z = chunk_pos.get_block_z(z);

            for world_y in (bottom_y..=top_y).rev() {
                if filter.is_some_and(|f| !f(x, world_y, z)) {
                    continue;
                }
                let block_pos = BlockPos::new(world_x, world_y, world_z);
                let block_state = chunk.get_block_state(block_pos);
                if is_uncarvable(block_state) {
                    continue;
                }
                if block_state.block() == Block::GRASS_BLOCK
                    || block_state.block() == Block::MYCELIUM
                {
                    has_grass = true;
                }

                let Some(state) = aquifer.compute_substance(world_x, world_y, world_z, 0.0) else {
                    continue;
                };
                chunk.set_block_state(block_pos, state, None);
                if aquifer.should_schedule_fluid_update() && !state.get_fluid_state().is_empty() {
                    chunk.mark_pos_for_post_processing(block_pos);
                }

                if has_grass {
                    let helper_pos = block_pos.below();
                    if chunk.get_block_state(helper_pos).block() == Block::DIRT
                        && let Some(top_material) = random_state.surface_system().top_material(
                            &self.material_rule,
                            random_state,
                            context,
                            &self.biomes,
                            biome_getter,
                            chunk,
                            &samplers,
                            helper_pos,
                            !state.get_fluid_state().is_empty(),
                        )
                    {
                        chunk.set_block_state(helper_pos, top_material, None);
                        if !top_material.get_fluid_state().is_empty() {
                            chunk.mark_pos_for_post_processing(helper_pos);
                        }
                    }
                }
            }
        });
    }

    /// `doFill(noiseChunk, chunk)`.
    pub fn do_fill(&self, noise_chunk: &mut NoiseChunk, chunk: &mut ProtoChunk) {
        chunk
            .access
            .get_or_create_heightmap_unprimed(HeightmapType::OceanFloorWg);
        chunk
            .access
            .get_or_create_heightmap_unprimed(HeightmapType::WorldSurfaceWg);
        let volume = *noise_chunk.volume();
        let samplers = noise_chunk.caching_samplers_rc();
        let final_density = samplers.get(&self.settings.noise_router.final_density);
        let density_buffer = final_density.sample_volume_scoped(&volume);
        let aquifer = noise_chunk.aquifer();
        let air = Block::AIR.default_state();

        for z in 0..volume.size_z {
            let block_z = volume.block_z(z);
            for x in 0..volume.size_x {
                let block_x = volume.block_x(x);
                for y in (0..volume.size_y).rev() {
                    let block_y = volume.block_y(y);
                    let density = density_buffer.get(volume.index_unchecked(x, y, z));
                    let state = aquifer
                        .compute_substance(block_x, block_y, block_z, f64::from(density))
                        .unwrap_or(self.default_block);
                    if state != air {
                        let access = &mut chunk.access;
                        let section_index = access.level_height.section_index(block_y) as usize;
                        access.sections[section_index].set_block_state(x, block_y & 15, z, state);
                        let view = SectionsView {
                            level_height: access.level_height,
                            sections: &access.sections,
                            void_air_outside: true,
                        };
                        for ty in [HeightmapType::OceanFloorWg, HeightmapType::WorldSurfaceWg] {
                            if let Some(heightmap) = access.heightmaps.get_mut(&ty) {
                                heightmap.update(x, block_y, z, state, &view);
                            }
                        }
                        if aquifer.should_schedule_fluid_update()
                            && !state.get_fluid_state().is_empty()
                        {
                            chunk.mark_pos_for_post_processing(BlockPos::new(
                                block_x, block_y, block_z,
                            ));
                        }
                    }
                }
            }
        }
    }
}

/// `createFluidPicker(settings)`.
pub fn create_fluid_picker(settings: &NoiseGeneratorSettings) -> LoadResult<FluidPicker> {
    let lava_status = FluidStatus::new(-54, Block::LAVA.default_state());
    let sea_level = settings.sea_level;
    let sea_status = FluidStatus::new(sea_level, settings.default_fluid.resolve()?);
    Ok(Arc::new(move |_x, y, _z| {
        if y < (-54).min(sea_level) {
            lava_status
        } else {
            sea_status
        }
    }))
}

/// `DEBUG_DENSITY_FORMAT` (`DecimalFormat("0.000", Locale.ROOT)`) applied to a float widened to double.
fn format_density(value: f32) -> String {
    let value = f64::from(value);
    if value.is_nan() {
        "NaN".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 {
            "\u{221e}".to_owned()
        } else {
            "-\u{221e}".to_owned()
        }
    } else {
        format!("{value:.3}")
    }
}

/// `chunkVolume(chunk, noiseSettings)`.
fn chunk_volume(pos: ChunkPos, noise_settings: NoiseSettings) -> DensityVolume {
    DensityVolume::with_unit_step(
        16,
        noise_settings.height,
        16,
        pos.get_min_block_x(),
        noise_settings.min_y,
        pos.get_min_block_z(),
    )
}

impl ChunkGenerator for NoiseBasedChunkGenerator {
    fn biome_source(&self) -> &dyn BiomeSource {
        &*self.biome_source
    }

    fn get_biome_generation_settings(&self, biome: BiomeId) -> &BiomeGenerationSettings {
        self.biomes.get(biome).generation_settings()
    }

    fn spawns_original_mobs(&self) -> bool {
        !self.settings.disable_mob_generation
    }

    fn get_origin(&self, random_state: &Arc<RandomState>) -> ChunkPos {
        let spawn_target = &self.settings.spawn_target;
        if spawn_target.is_empty() {
            return ChunkPos::ZERO;
        }
        let samplers =
            random_state.samplers_with_context(SamplerContext::builder().enable_caches().build());
        ChunkPos::containing(&NoiseSpawnFinder::find_spawn_position(
            spawn_target,
            &samplers,
        ))
    }

    fn decorate_biome_resolver<'a>(
        &'a self,
        blender: &'a Blender,
        chunk: &ProtoChunk,
        biome_resolver: BiomeFn<'a>,
    ) -> BiomeFn<'a> {
        let blended = blender.get_biome_resolver(biome_resolver);
        let retained: Vec<BiomeId> = mc_world::below_zero_retrogen::RETAINED_RETROGEN_BIOMES
            .iter()
            .filter_map(|name| self.biomes.id_of_name(name))
            .collect();
        blending::get_below_zero_biome_resolver(
            blended,
            chunk,
            Box::new(move |biome| retained.contains(&biome)),
        )
    }

    fn add_debug_screen_info(
        &self,
        result: &mut Vec<String>,
        random_state: &Arc<RandomState>,
        feet_pos: BlockPos,
        sampler_context: SamplerContext,
    ) {
        let functions = &self.settings.debug_functions;
        if functions.is_empty() {
            return;
        }
        let samplers = random_state.samplers_with_context(sampler_context);
        let mut builder = String::from("Density ");
        for (label, function) in functions {
            builder.push_str(label);
            builder.push_str(": ");
            builder.push_str(&format_density(samplers.sample_value(
                function,
                feet_pos.x(),
                feet_pos.y(),
                feet_pos.z(),
            )));
            builder.push(' ');
        }
        builder.pop();
        result.push(builder);
    }

    fn build_terrain(&self, chunk: &mut ProtoChunk, ctx: &BuildTerrainContext<'_>) {
        let noise_settings = self
            .settings
            .noise
            .clamp_to_height_accessor(chunk.get_height_accessor_for_generation());
        if noise_settings.height <= 0 {
            return;
        }
        let mut noise_chunk = self.create_noise_chunk(chunk.pos(), ctx, noise_settings);
        self.do_fill(&mut noise_chunk, chunk);
        self.build_surface(chunk, &noise_chunk, ctx);
        self.generate_carvers(chunk, &mut noise_chunk, ctx);
    }

    fn get_gen_depth(&self) -> i32 {
        self.settings.noise.height
    }

    fn get_sea_level(&self) -> i32 {
        self.settings.sea_level
    }

    fn get_min_y(&self) -> i32 {
        self.settings.noise.min_y
    }

    fn get_base_height(
        &self,
        x: i32,
        z: i32,
        ty: HeightmapType,
        height_accessor: LevelHeight,
        random_state: &Arc<RandomState>,
    ) -> i32 {
        self.iterate_noise_column(
            height_accessor,
            random_state,
            x,
            z,
            false,
            Some(&|state| ty.is_opaque(state)),
        )
        .0
        .unwrap_or(height_accessor.min_y)
    }

    fn get_base_column(
        &self,
        x: i32,
        z: i32,
        height_accessor: LevelHeight,
        random_state: &Arc<RandomState>,
    ) -> NoiseColumn {
        // Vanilla returns `result.get()`, which is null when the noise height is empty; an empty column answers air.
        self.iterate_noise_column(height_accessor, random_state, x, z, true, None)
            .1
            .unwrap_or_else(|| NoiseColumn::new(height_accessor.min_y, Vec::new()))
    }
}
