//! `ChunkStatusTasks`: the generation step bodies that run on the worldgen thread. A [`GenBatch`] is one scheduled
//! layer of a `ChunkGenerationTask` (the steps of one status for the chunks of one layer); the chunks it works on are
//! lent by the game thread for the duration of the batch.
//!
//! Steps that run on the game thread instead (`passThrough`, `loadStructureStarts`, `initializeLight`, `light`, `full`)
//! are handled by the chunk map.

use super::world_gen::{GenChunk, LightEvent, OldChunk, WorldGenContext, WorldGenRegion};
use crate::crash_report::CrashReport;
use crate::world::entity::ai::village::poi::PoiTypeHolder;
use crate::world::level::chunk::status::{ChunkStatusTask, ChunkStep};
use mc_core::pos::ChunkPos;
use mc_world::blending_data::BlendingData;
use mc_world::chunk::{ProtoChunk, StructureData};
use mc_world::heightmap::HeightmapType;
use mc_world::status::ChunkStatus;
use mc_worldgen::blending::{self, Blender, BlendingChunk, BlendingRegion, ChunkView};
use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;

pub struct GenJob {
    pub pos: ChunkPos,
    pub step: ChunkStep,
}

pub struct GenBatch {
    pub id: u64,
    pub jobs: Vec<GenJob>,
    pub chunks: HashMap<i64, GenChunk>,
    pub structure_views: HashMap<i64, Arc<StructureData>>,
    /// Snapshots of the chunks with blending data around the jobs that are not lent (see
    /// [`WorldGenRegion::old_chunks`]).
    pub old_chunks: HashMap<i64, OldChunk>,
}

pub struct BatchResult {
    pub id: u64,
    /// The lent chunks, to be put back into their holders.
    pub chunks: Vec<(i64, Box<ProtoChunk>)>,
    /// `(chunk, status)` pairs whose step finished.
    pub completed: Vec<(i64, ChunkStatus)>,
    /// `WorldGenRegion.setBlock` POI updates of the batch's steps (see `WorldGenRegion::poi_changes`).
    pub poi_changes: Vec<(
        mc_core::pos::BlockPos,
        Option<PoiTypeHolder>,
        Option<PoiTypeHolder>,
    )>,
    /// Light engine calls of the batch's block writes (see `WorldGenRegion::light_events`), in call order.
    pub light_events: Vec<LightEvent>,
    /// Raw section writes that reached read-only snapshots, in call order (see `WorldGenRegion::section_writes`).
    pub section_writes: Vec<(mc_core::pos::BlockPos, mc_data::block::BlockState)>,
    /// Structure start rewrites for chunks the batch did not lend (`WorldGenRegion::update_structure_start`).
    pub start_updates: Vec<(ChunkPos, String, mc_nbt::CompoundTag)>,
    /// The blending data `BlendingData.calculateData` filled in chunks the batch only saw as snapshots (the game thread
    /// puts it into the chunks).
    pub blending_updates: Vec<(ChunkPos, BlendingData)>,
}

/// Run the jobs of a batch one after another (the worldgen executor is serial). A panic in a step is vanilla's
/// uncaught exception of `ChunkMap.applyStep`: the report ("Exception generating new chunk", or the step's own
/// `ReportedException` such as "Feature placement") crashes the server through the game thread.
pub fn run_batch(ctx: &WorldGenContext, batch: GenBatch) -> Result<BatchResult, CrashReport> {
    let GenBatch {
        id,
        jobs,
        mut chunks,
        structure_views,
        mut old_chunks,
    } = batch;
    let mut completed = Vec::with_capacity(jobs.len());
    let mut poi_changes = Vec::new();
    let mut light_events = Vec::new();
    let mut section_writes = Vec::new();
    let mut start_updates = Vec::new();
    for job in &jobs {
        let step = &job.step;
        let mut region = WorldGenRegion::new(
            job.pos,
            step.direct_dependencies.get_radius(),
            step.block_state_write_radius,
            std::mem::take(&mut chunks),
            structure_views.clone(),
            ctx.level_height,
        );
        region.old_chunk_scan = ctx.old_chunk_scan.clone();
        region.old_chunks = std::mem::take(&mut old_chunks);
        if let Err(payload) =
            panic::catch_unwind(AssertUnwindSafe(|| run_step(ctx, step, &mut region)))
        {
            let mut report = CrashReport::for_throwable(payload, "Exception generating new chunk");
            report
                .add_category("Chunk to be generated")
                .set_detail("Status being generated", step.target_status.name())
                .set_detail("Location", format!("{},{}", job.pos.x, job.pos.z))
                .set_detail("Position hash", job.pos.pack())
                .set_detail(
                    "Generator",
                    ctx.generator
                        .type_name_for_data_fixer()
                        .map_or_else(|| "unknown".to_owned(), |id| id.to_string()),
                );
            return Err(report);
        }
        poi_changes.append(&mut region.poi_changes);
        light_events.append(&mut region.light_events);
        section_writes.append(&mut region.section_writes);
        start_updates.append(&mut region.start_updates);
        old_chunks = std::mem::take(&mut region.old_chunks);
        chunks = region.into_chunks();
        completed.push((job.pos.pack(), step.target_status));
    }
    let mut blending_updates = Vec::new();
    let chunks = chunks
        .into_iter()
        .filter_map(|(key, c)| match c {
            GenChunk::Loaned(chunk) => Some((key, chunk)),
            GenChunk::Snapshot(chunk) => {
                blending_updates.extend(calculated_blending_data(&chunk));
                None
            }
            GenChunk::Shared(_) => None,
        })
        .collect();
    blending_updates.extend(
        old_chunks
            .values()
            .filter(|old| !old.calculated_before)
            .filter_map(|old| calculated_blending_data(&old.chunk)),
    );
    Ok(BatchResult {
        id,
        chunks,
        completed,
        poi_changes,
        light_events,
        section_writes,
        start_updates,
        blending_updates,
    })
}

/// The blending data a snapshot computed (`getOrUpdateBlendingData` mutates the chunk's own object in vanilla).
fn calculated_blending_data(chunk: &ProtoChunk) -> Option<(ChunkPos, BlendingData)> {
    chunk
        .access
        .blending_data
        .as_ref()
        .filter(|data| data.has_calculated_data)
        .map(|data| (chunk.pos(), data.clone()))
}

/// `Blender.of(region)` with the center chunk still in the region.
fn blender_of(region: &mut WorldGenRegion) -> Blender {
    Blender::of(Some(&RegionBlending(RefCell::new(region))))
}

/// `ChunkStep.apply` for a chunk that is still before the target status.
fn run_step(ctx: &WorldGenContext, step: &ChunkStep, region: &mut WorldGenRegion) {
    let center = region.get_center();
    match step.task {
        ChunkStatusTask::GenerateStructureStarts => {
            let chunk = region
                .get_chunk_mut(center.x, center.z)
                .expect("center chunk is lent");
            if ctx.generate_structures {
                ctx.hooks.create_structures(ctx, chunk);
            }
            ctx.hooks.on_structure_starts_available(chunk);
        }
        ChunkStatusTask::GenerateStructureReferences => ctx.hooks.create_references(ctx, region),
        ChunkStatusTask::GenerateBiomes => {
            region.blender = blender_of(region);
            let mut chunk = region.take_chunk(center).expect("center chunk is lent");
            ctx.generator.create_biomes(chunk.proto_mut(), region);
            region.put_chunk(center, chunk);
        }
        ChunkStatusTask::BuildTerrain => {
            region.blender = blender_of(region);
            let mut chunk = region.take_chunk(center).expect("center chunk is lent");
            ctx.generator.build_terrain(chunk.proto_mut(), region);
            if let Some(retrogen) = chunk.proto().get_below_zero_retrogen().cloned() {
                blending::replace_old_bedrock(chunk.proto_mut());
                if retrogen.has_bedrock_holes() {
                    blending::apply_bedrock_mask(&retrogen, chunk.proto_mut());
                }
            }
            chunk.proto_mut().access.prime_heightmaps(
                true,
                &[
                    HeightmapType::MotionBlocking,
                    HeightmapType::MotionBlockingNoLeaves,
                    HeightmapType::OceanFloor,
                    HeightmapType::WorldSurface,
                ],
            );
            region.put_chunk(center, chunk);
        }
        ChunkStatusTask::GenerateFeatures => {
            ctx.hooks.apply_biome_decoration(ctx, region);
            blending::generate_border_ticks(&RegionBlending(RefCell::new(&mut *region)), center);
        }
        ChunkStatusTask::GenerateSpawn => {
            let upgrading = region
                .get_chunk(center.x, center.z)
                .is_some_and(ProtoChunk::is_upgrading);
            if !upgrading {
                ctx.hooks.spawn_original_mobs(ctx, region);
            }
        }
        other => unreachable!("{other:?} does not run on the worldgen thread"),
    }
    let chunk = region.get_chunk_mut_unchecked(center);
    if chunk.get_persisted_status().is_before(step.target_status) {
        chunk.set_persisted_status(step.target_status);
    }
}

/// The part of `WorldGenRegion` `Blender.generateBorderTicks` uses (`getChunk` of the center and its four neighbours).
struct RegionBlending<'a>(RefCell<&'a mut WorldGenRegion>);

impl BlendingRegion for RegionBlending<'_> {
    fn get_center(&self) -> ChunkPos {
        self.0.borrow().center
    }

    fn is_old_chunk_around(&self, pos: ChunkPos, range: i32) -> bool {
        self.0.borrow().is_old_chunk_around(pos, range)
    }

    fn with_chunk(&self, chunk_x: i32, chunk_z: i32, f: &mut dyn FnMut(&mut dyn BlendingChunk)) {
        let mut region = self.0.borrow_mut();
        // a snapshot of an old chunk beats the biome-only view of the same chunk
        if let Some(old) = region
            .old_chunks
            .get_mut(&ChunkPos::pack_xz(chunk_x, chunk_z))
        {
            f(&mut *old.chunk);
        } else if let Some(chunk) = region.gen_chunk_mut(chunk_x, chunk_z) {
            f(chunk);
        } else {
            // not lent and not old: all `Blender` asks of such a chunk is `getBlendingData() == null`
            f(&mut NotOldChunk);
        }
    }
}

/// A chunk of the region the batch did not receive: it has no blending data.
struct NotOldChunk;

impl ChunkView for NotOldChunk {
    fn get_block_state(&self, _: mc_core::pos::BlockPos) -> mc_data::block::BlockState {
        unreachable!("a chunk without blending data is not read")
    }
    fn get_fluid_state(&self, _: mc_core::pos::BlockPos) -> mc_data::fluid::FluidState {
        unreachable!("a chunk without blending data is not read")
    }
    fn has_primed_heightmap(&self, _: HeightmapType) -> bool {
        unreachable!("a chunk without blending data is not read")
    }
    fn get_height(&self, _: HeightmapType, _: i32, _: i32) -> i32 {
        unreachable!("a chunk without blending data is not read")
    }
    fn get_noise_biome(&self, _: i32, _: i32, _: i32) -> mc_world::palette::BiomeId {
        unreachable!("a chunk without blending data is not read")
    }
}

impl BlendingChunk for NotOldChunk {
    fn is_old_noise_generation(&self) -> bool {
        false
    }
    fn get_highest_generated_status(&self) -> ChunkStatus {
        unreachable!("a chunk without blending data is not read")
    }
    fn blending_data(&self) -> Option<&BlendingData> {
        None
    }
    fn update_blending_data(&mut self, _: &mut dyn FnMut(&mut BlendingData, &dyn ChunkView)) {}
    fn get_height_primed(&mut self, _: HeightmapType, _: i32, _: i32) -> i32 {
        unreachable!("a chunk without blending data is not read")
    }
    fn mark_pos_for_post_processing(&mut self, _: mc_core::pos::BlockPos) {
        unreachable!("only chunks of the generated area get border ticks")
    }
}

/// Whether a step runs on the worldgen thread.
pub fn runs_on_worldgen_thread(task: ChunkStatusTask) -> bool {
    matches!(
        task,
        ChunkStatusTask::GenerateStructureStarts
            | ChunkStatusTask::GenerateStructureReferences
            | ChunkStatusTask::GenerateBiomes
            | ChunkStatusTask::BuildTerrain
            | ChunkStatusTask::GenerateFeatures
            | ChunkStatusTask::GenerateSpawn
    )
}

/// Whether vanilla runs the step on `Util.backgroundExecutor()` (a `supplyAsync` inside the step) instead of the worldgen
/// executor: the step only writes its own chunk and reads biomes and structure starts of the others.
pub fn runs_on_background_executor(task: ChunkStatusTask) -> bool {
    matches!(
        task,
        ChunkStatusTask::GenerateBiomes | ChunkStatusTask::BuildTerrain
    )
}
