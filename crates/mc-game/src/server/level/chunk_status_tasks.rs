//! `ChunkStatusTasks`: the generation step bodies that run on the worldgen thread. A [`GenBatch`] is one scheduled
//! layer of a `ChunkGenerationTask` (the steps of one status for the chunks of one layer); the chunks it works on are
//! lent by the game thread for the duration of the batch.
//!
//! Steps that run on the game thread instead (`passThrough`, `loadStructureStarts`, `initializeLight`, `light`, `full`)
//! are handled by the chunk map.

use super::world_gen::{GenChunk, WorldGenContext, WorldGenRegion};
use crate::crash_report::CrashReport;
use crate::world::level::chunk::status::{ChunkStatusTask, ChunkStep};
use mc_core::pos::ChunkPos;
use mc_world::chunk::{ProtoChunk, StructureData};
use mc_world::heightmap::HeightmapType;
use mc_world::status::ChunkStatus;
use mc_worldgen::blending::{self, BlendingChunk, BlendingRegion};
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
}

pub struct BatchResult {
    pub id: u64,
    /// The lent chunks, to be put back into their holders.
    pub chunks: Vec<(i64, Box<ProtoChunk>)>,
    /// `(chunk, status)` pairs whose step finished.
    pub completed: Vec<(i64, ChunkStatus)>,
}

/// Run the jobs of a batch one after another (the worldgen executor is serial). A panic in a step is vanilla's
/// uncaught exception of `ChunkMap.applyStep`: the report ("Exception generating new chunk", or the step's own
/// `ReportedException` such as "Feature placement") crashes the server through the game thread.
pub fn run_batch(ctx: &WorldGenContext, batch: GenBatch) -> Result<BatchResult, CrashReport> {
    let GenBatch { id, jobs, mut chunks, structure_views } = batch;
    let mut completed = Vec::with_capacity(jobs.len());
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
        if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(|| run_step(ctx, step, &mut region))) {
            let mut report = CrashReport::for_throwable(payload, "Exception generating new chunk");
            report
                .add_category("Chunk to be generated")
                .set_detail("Status being generated", step.target_status.name())
                .set_detail("Location", format!("{},{}", job.pos.x, job.pos.z))
                .set_detail("Position hash", job.pos.pack())
                .set_detail("Generator", ctx.generator.type_name_for_data_fixer().map_or_else(|| "unknown".to_owned(), |id| id.to_string()));
            return Err(report);
        }
        chunks = region.into_chunks();
        completed.push((job.pos.pack(), step.target_status));
    }
    let chunks = chunks
        .into_iter()
        .filter_map(|(key, c)| match c {
            GenChunk::Loaned(chunk) => Some((key, chunk)),
            GenChunk::Snapshot(_) => None,
        })
        .collect();
    Ok(BatchResult { id, chunks, completed })
}

/// `ChunkStep.apply` for a chunk that is still before the target status.
fn run_step(ctx: &WorldGenContext, step: &ChunkStep, region: &mut WorldGenRegion) {
    let center = region.get_center();
    match step.task {
        ChunkStatusTask::GenerateStructureStarts => {
            let chunk = region.get_chunk_mut(center.x, center.z).expect("center chunk is lent");
            if ctx.generate_structures {
                ctx.hooks.create_structures(ctx, chunk);
            }
            ctx.hooks.on_structure_starts_available(chunk);
        }
        ChunkStatusTask::GenerateStructureReferences => ctx.hooks.create_references(ctx, region),
        ChunkStatusTask::GenerateBiomes => {
            let mut chunk = region.take_chunk(center).expect("center chunk is lent");
            ctx.generator.create_biomes(chunk.proto_mut(), region);
            region.put_chunk(center, chunk);
        }
        ChunkStatusTask::BuildTerrain => {
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
                &[HeightmapType::MotionBlocking, HeightmapType::MotionBlockingNoLeaves, HeightmapType::OceanFloor, HeightmapType::WorldSurface],
            );
            region.put_chunk(center, chunk);
        }
        ChunkStatusTask::GenerateFeatures => {
            ctx.hooks.apply_biome_decoration(ctx, region);
            blending::generate_border_ticks(&RegionBlending(RefCell::new(&mut *region)), center);
        }
        ChunkStatusTask::GenerateSpawn => {
            let upgrading = region.get_chunk(center.x, center.z).is_some_and(ProtoChunk::is_upgrading);
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

    fn is_old_chunk_around(&self, _pos: ChunkPos, _range: i32) -> bool {
        unreachable!("only Blender.of asks for old chunks (it is not wired: Blender::empty() everywhere)")
    }

    fn with_chunk(&self, chunk_x: i32, chunk_z: i32, f: &mut dyn FnMut(&mut dyn BlendingChunk)) {
        let mut region = self.0.borrow_mut();
        let chunk = region.gen_chunk_mut(chunk_x, chunk_z).unwrap_or_else(|| panic!("Requested chunk unavailable during world generation: {chunk_x},{chunk_z}"));
        f(chunk);
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
