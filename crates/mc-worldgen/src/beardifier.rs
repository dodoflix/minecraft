//! `net.minecraft.world.level.levelgen.Beardifier`: the density contribution of structures that adapt the
//! terrain (`beard_thin`, `beard_box`, `bury`, `encapsulate`), plugged into the density functions through
//! [`BEARDIFIER_KEY`](crate::densityfunction::sampler::BEARDIFIER_KEY).
//!
//! Vanilla reads `StructureStart`/`StructurePiece`/`PoolElementStructurePiece`/`JigsawJunction` from the
//! `StructureManager`; here the structure layer hands over the already-read facts as [`StructureStartInfo`]s.

use std::sync::LazyLock;

use mc_core::bounding_box::BoundingBox;
use mc_core::math::mth;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::chunk_pos::ChunkPos;

use crate::densityfunction::{DensityBuffer, DensitySampler, DensityVolume, SamplerContext};
use crate::util::java_pow;

/// `Beardifier.BEARD_KERNEL_RADIUS`.
pub const BEARD_KERNEL_RADIUS: i32 = 12;
const BEARD_KERNEL_SIZE: i32 = 24;

/// `TerrainAdjustment`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TerrainAdjustment {
    None,
    Bury,
    BeardThin,
    BeardBox,
    Encapsulate,
}

impl TerrainAdjustment {
    /// `TerrainAdjustment.CODEC` (serialized names).
    pub fn from_name(name: &str) -> Option<TerrainAdjustment> {
        Some(match name {
            "none" => TerrainAdjustment::None,
            "bury" => TerrainAdjustment::Bury,
            "beard_thin" => TerrainAdjustment::BeardThin,
            "beard_box" => TerrainAdjustment::BeardBox,
            "encapsulate" => TerrainAdjustment::Encapsulate,
            _ => return None,
        })
    }
}

/// The parts of `JigsawJunction` the beardifier reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JigsawJunction {
    pub source_x: i32,
    pub source_ground_y: i32,
    pub source_z: i32,
}

/// What differs between a plain `StructurePiece` and a `PoolElementStructurePiece` for the beardifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructurePieceKind {
    /// Any other `StructurePiece`: contributes its bounding box with ground level delta 0.
    Plain,
    /// `PoolElementStructurePiece`: `rigid` is `getElement().getProjection() == RIGID`; a rigid piece contributes
    /// its box with `getGroundLevelDelta()`; every piece contributes its junctions.
    PoolElement { rigid: bool, ground_level_delta: i32, junctions: Vec<JigsawJunction> },
}

/// A `StructurePiece` as the beardifier sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructurePieceInfo {
    pub bounding_box: BoundingBox,
    pub kind: StructurePieceKind,
}

/// A `StructureStart` as the beardifier sees it (`getStructure().terrainAdaptation()` + `getPieces()`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureStartInfo {
    pub terrain_adaptation: TerrainAdjustment,
    pub pieces: Vec<StructurePieceInfo>,
}

/// `Beardifier.Rigid`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rigid {
    pub bounding_box: BoundingBox,
    pub terrain_adjustment: TerrainAdjustment,
    pub ground_level_delta: i32,
}

/// `Beardifier`.
#[derive(Clone, Debug, Default)]
pub struct Beardifier {
    pieces: Vec<Rigid>,
    junctions: Vec<JigsawJunction>,
    affected_box: Option<BoundingBox>,
}

/// `BEARD_KERNEL`, indexed `zi * 24 * 24 + xi * 24 + yi`.
static BEARD_KERNEL: LazyLock<Box<[f32]>> = LazyLock::new(|| {
    let mut kernel = vec![0.0f32; (BEARD_KERNEL_SIZE * BEARD_KERNEL_SIZE * BEARD_KERNEL_SIZE) as usize];
    for zi in 0..BEARD_KERNEL_SIZE {
        for xi in 0..BEARD_KERNEL_SIZE {
            for yi in 0..BEARD_KERNEL_SIZE {
                kernel[(zi * 24 * 24 + xi * 24 + yi) as usize] = compute_beard_contribution(xi - 12, yi - 12, zi - 12) as f32;
            }
        }
    }
    kernel.into_boxed_slice()
});

fn compute_beard_contribution(dx: i32, dy: i32, dz: i32) -> f64 {
    let dy = f64::from(dy) + 0.5;
    let (dx, dz) = (f64::from(dx), f64::from(dz));
    let distance_sqr = dx * dx + dy * dy + dz * dz;
    java_pow(std::f64::consts::E, -distance_sqr / 16.0)
}

fn include_bounding_box(encompassing_box: Option<BoundingBox>, new_box: BoundingBox) -> Option<BoundingBox> {
    Some(match encompassing_box {
        None => new_box,
        Some(existing) => BoundingBox::encapsulating(&existing, &new_box),
    })
}

fn is_close_to_chunk(bounding_box: &BoundingBox, chunk_pos: ChunkPos, distance: i32) -> bool {
    let cx = chunk_pos.get_min_block_x();
    let cz = chunk_pos.get_min_block_z();
    bounding_box.intersects_xz(cx - distance, cz - distance, cx + 15 + distance, cz + 15 + distance)
}

impl Beardifier {
    /// `Beardifier.EMPTY`.
    pub const EMPTY: Beardifier = Beardifier { pieces: Vec::new(), junctions: Vec::new(), affected_box: None };

    /// `Beardifier(pieces, junctions, affectedBox)`.
    pub fn new(pieces: Vec<Rigid>, junctions: Vec<JigsawJunction>, affected_box: Option<BoundingBox>) -> Beardifier {
        Beardifier { pieces, junctions, affected_box }
    }

    /// `Beardifier.forStructuresInChunk(structureManager, chunkPos)`: `starts` are the structure starts reaching the
    /// chunk (`StructureManager.startsForStructure(x, z, ...)`); those with `terrainAdaptation() == NONE` are skipped.
    pub fn for_structures_in_chunk(chunk_pos: ChunkPos, starts: &[StructureStartInfo]) -> Beardifier {
        let structure_starts: Vec<&StructureStartInfo> =
            starts.iter().filter(|s| s.terrain_adaptation != TerrainAdjustment::None).collect();
        if structure_starts.is_empty() {
            return Beardifier::EMPTY;
        }
        let chunk_start_block_x = chunk_pos.get_min_block_x();
        let chunk_start_block_z = chunk_pos.get_min_block_z();
        let mut rigids = Vec::new();
        let mut junctions = Vec::new();
        let mut any_piece_bounding_box: Option<BoundingBox> = None;

        for start in structure_starts {
            let terrain_adjustment = start.terrain_adaptation;
            for piece in &start.pieces {
                if !is_close_to_chunk(&piece.bounding_box, chunk_pos, 12) {
                    continue;
                }
                match &piece.kind {
                    StructurePieceKind::PoolElement { rigid, ground_level_delta, junctions: piece_junctions } => {
                        if *rigid {
                            rigids.push(Rigid {
                                bounding_box: piece.bounding_box,
                                terrain_adjustment,
                                ground_level_delta: *ground_level_delta,
                            });
                            any_piece_bounding_box = include_bounding_box(any_piece_bounding_box, piece.bounding_box);
                        }
                        for junction in piece_junctions {
                            let junction_x = junction.source_x;
                            let junction_z = junction.source_z;
                            if junction_x > chunk_start_block_x - 12
                                && junction_z > chunk_start_block_z - 12
                                && junction_x < chunk_start_block_x + 15 + 12
                                && junction_z < chunk_start_block_z + 15 + 12
                            {
                                junctions.push(*junction);
                                let junction_box = BoundingBox::from_block_pos(&BlockPos::new(
                                    junction_x,
                                    junction.source_ground_y,
                                    junction_z,
                                ));
                                any_piece_bounding_box = include_bounding_box(any_piece_bounding_box, junction_box);
                            }
                        }
                    }
                    StructurePieceKind::Plain => {
                        rigids.push(Rigid { bounding_box: piece.bounding_box, terrain_adjustment, ground_level_delta: 0 });
                        any_piece_bounding_box = include_bounding_box(any_piece_bounding_box, piece.bounding_box);
                    }
                }
            }
        }

        match any_piece_bounding_box {
            None => Beardifier::EMPTY,
            Some(any_box) => Beardifier::new(rigids, junctions, Some(any_box.inflated_by(24))),
        }
    }

    /// `affectedBox` (`None` for [`Beardifier::EMPTY`]).
    pub fn affected_box(&self) -> Option<&BoundingBox> {
        self.affected_box.as_ref()
    }

    fn sample_value_unchecked(&self, block_x: i32, block_y: i32, block_z: i32) -> f32 {
        let mut noise_value = 0.0f32;

        for rigid in &self.pieces {
            let bx = &rigid.bounding_box;
            let ground_level_delta = rigid.ground_level_delta;
            let dx = 0.max((bx.min_x() - block_x).max(block_x - bx.max_x()));
            let dz = 0.max((bx.min_z() - block_z).max(block_z - bx.max_z()));
            let ground_y = bx.min_y() + ground_level_delta;
            let dy_to_ground = block_y - ground_y;

            let dy = match rigid.terrain_adjustment {
                TerrainAdjustment::None => 0,
                TerrainAdjustment::Bury | TerrainAdjustment::BeardThin => dy_to_ground,
                TerrainAdjustment::BeardBox => 0.max((ground_y - block_y).max(block_y - bx.max_y())),
                TerrainAdjustment::Encapsulate => 0.max((bx.min_y() - block_y).max(block_y - bx.max_y())),
            };

            noise_value += match rigid.terrain_adjustment {
                TerrainAdjustment::None => 0.0f32,
                TerrainAdjustment::Bury => get_bury_contribution(dx as f32, dy as f32 / 2.0, dz as f32),
                TerrainAdjustment::BeardThin | TerrainAdjustment::BeardBox => {
                    get_beard_contribution(dx, dy, dz, dy_to_ground) * 0.8
                }
                TerrainAdjustment::Encapsulate => {
                    get_bury_contribution(dx as f32 / 2.0, dy as f32 / 2.0, dz as f32 / 2.0) * 0.8
                }
            };
        }

        for junction in &self.junctions {
            let dx = block_x - junction.source_x;
            let dy = block_y - junction.source_ground_y;
            let dz = block_z - junction.source_z;
            noise_value += get_beard_contribution(dx, dy, dz, dy) * 0.4;
        }

        noise_value
    }
}

fn get_bury_contribution(dx: f32, dy: f32, dz: f32) -> f32 {
    let distance_sq = mth::length_squared_xyz_f32(dx, dy, dz);
    if distance_sq >= 36.0 { 0.0 } else { 1.0 - mth::sqrt(distance_sq) / 6.0 }
}

fn is_in_kernel_range(xi: i32) -> bool {
    (0..24).contains(&xi)
}

fn get_beard_contribution(dx: i32, dy: i32, dz: i32, y_to_ground: i32) -> f32 {
    let xi = dx + 12;
    let yi = dy + 12;
    let zi = dz + 12;
    if is_in_kernel_range(xi) && is_in_kernel_range(yi) && is_in_kernel_range(zi) {
        let dy_with_offset = y_to_ground as f32 + 0.5;
        let distance_sqr = mth::length_squared_xyz_f32(dx as f32, dy_with_offset, dz as f32);
        let value = -dy_with_offset * mth::fast_inv_sqrt(f64::from(distance_sqr / 2.0)) as f32 / 2.0;
        value * BEARD_KERNEL[(zi * 24 * 24 + xi * 24 + yi) as usize]
    } else {
        0.0
    }
}

impl DensitySampler for Beardifier {
    fn sample_volume(&self, _context: &SamplerContext, output_buffer: &mut DensityBuffer, volume: &DensityVolume) {
        output_buffer.fill(0.0);
        let Some(affected) = &self.affected_box else { return };
        if !affected.intersects_xyz(
            volume.min_block_x,
            volume.min_block_y,
            volume.min_block_z,
            volume.max_block_x(),
            volume.max_block_y(),
            volume.max_block_z(),
        ) {
            return;
        }
        let min_x = mth::floor_div(0.max(affected.min_x() - volume.min_block_x), volume.step_block_x);
        let min_y = mth::floor_div(0.max(affected.min_y() - volume.min_block_y), volume.step_block_y);
        let min_z = mth::floor_div(0.max(affected.min_z() - volume.min_block_z), volume.step_block_z);
        let max_x = (volume.size_x - 1).min(mth::floor_div(affected.max_x() - volume.min_block_x, volume.step_block_x));
        let max_y = (volume.size_y - 1).min(mth::floor_div(affected.max_y() - volume.min_block_y, volume.step_block_y));
        let max_z = (volume.size_z - 1).min(mth::floor_div(affected.max_z() - volume.min_block_z, volume.step_block_z));

        for z in min_z..=max_z {
            let block_z = volume.block_z(z);
            for x in min_x..=max_x {
                let block_x = volume.block_x(x);
                for y in min_y..=max_y {
                    let index = volume.index_unchecked(x, y, z);
                    let block_y = volume.block_y(y);
                    output_buffer.set(index, self.sample_value_unchecked(block_x, block_y, block_z));
                }
            }
        }
    }

    fn sample_value(&self, _context: &SamplerContext, block_x: i32, block_y: i32, block_z: i32) -> f32 {
        match &self.affected_box {
            Some(affected) if affected.is_inside_xyz(block_x, block_y, block_z) => {
                self.sample_value_unchecked(block_x, block_y, block_z)
            }
            _ => 0.0,
        }
    }
}
