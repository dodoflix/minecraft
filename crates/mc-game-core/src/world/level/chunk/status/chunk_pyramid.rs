//! `ChunkPyramid`: the generation and loading step lists.
use super::chunk_step::{ChunkStatusTask as T, ChunkStepBuilder};
use super::ChunkStep;
use mc_world::status::ChunkStatus as S;
use std::sync::LazyLock;

#[derive(Clone, Debug)]
pub struct ChunkPyramid {
    steps: Vec<ChunkStep>,
}

impl ChunkPyramid {
    /// `getStepTo(status)`.
    pub fn get_step_to(&self, status: S) -> &ChunkStep {
        &self.steps[status.index() as usize]
    }

    pub fn steps(&self) -> &[ChunkStep] {
        &self.steps
    }

    fn build(specs: Vec<(S, Box<dyn Fn(ChunkStepBuilder) -> ChunkStepBuilder>)>) -> Self {
        let mut steps: Vec<ChunkStep> = Vec::new();
        for (status, op) in specs {
            let builder = ChunkStepBuilder::new(status, steps.last());
            steps.push(op(builder).build());
        }
        ChunkPyramid { steps }
    }
}

type Op = Box<dyn Fn(ChunkStepBuilder) -> ChunkStepBuilder>;

fn op(f: impl Fn(ChunkStepBuilder) -> ChunkStepBuilder + 'static) -> Op {
    Box::new(f)
}

/// `ChunkPyramid.GENERATION_PYRAMID`.
pub static GENERATION_PYRAMID: LazyLock<ChunkPyramid> = LazyLock::new(|| {
    ChunkPyramid::build(vec![
        (S::Empty, op(|s| s)),
        (S::StructureStarts, op(|s| s.set_task(T::GenerateStructureStarts))),
        (S::StructureReferences, op(|s| s.add_requirement(S::StructureStarts, 8).set_task(T::GenerateStructureReferences))),
        (S::Biomes, op(|s| s.add_requirement(S::StructureStarts, 8).set_task(T::GenerateBiomes))),
        (
            S::Terrain,
            op(|s| s.add_requirement(S::StructureStarts, 8).add_requirement(S::Biomes, 1).block_state_write_radius(0).set_task(T::BuildTerrain)),
        ),
        (
            S::Features,
            op(|s| s.add_requirement(S::StructureStarts, 8).add_requirement(S::Terrain, 1).block_state_write_radius(1).set_task(T::GenerateFeatures)),
        ),
        (S::InitializeLight, op(|s| s.set_task(T::InitializeLight))),
        (S::Light, op(|s| s.add_requirement(S::InitializeLight, 1).set_task(T::Light))),
        (S::Spawn, op(|s| s.add_requirement(S::Biomes, 1).set_task(T::GenerateSpawn))),
        (S::Full, op(|s| s.set_task(T::Full))),
    ])
});

/// `ChunkPyramid.LOADING_PYRAMID`.
pub static LOADING_PYRAMID: LazyLock<ChunkPyramid> = LazyLock::new(|| {
    ChunkPyramid::build(vec![
        (S::Empty, op(|s| s)),
        (S::StructureStarts, op(|s| s.set_task(T::LoadStructureStarts))),
        (S::StructureReferences, op(|s| s)),
        (S::Biomes, op(|s| s)),
        (S::Terrain, op(|s| s)),
        (S::Features, op(|s| s)),
        (S::InitializeLight, op(|s| s.set_task(T::InitializeLight))),
        (S::Light, op(|s| s.add_requirement(S::InitializeLight, 1).set_task(T::Light))),
        (S::Spawn, op(|s| s)),
        (S::Full, op(|s| s.set_task(T::Full))),
    ])
});

/// `ChunkPyramid.MAX_CHUNK_COORDINATE_VALUE`.
pub fn max_chunk_coordinate_value() -> i32 {
    let safety_margin_chunks = (32 + GENERATION_PYRAMID.get_step_to(S::Full).accumulated_dependencies.size() as i32 + 1) * 2;
    (mc_core::pos::block_pos::MAX_HORIZONTAL_COORDINATE >> 4) - safety_margin_chunks
}
