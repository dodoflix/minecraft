//! `DesertPyramidStructure`.

use super::desert_pyramid_piece::DesertPyramidPiece;
use mc_game_core::world::level::BlockGetter as _;
use mc_game_core::world::level::LevelAccessor as _;
use mc_game_core::world::level::levelgen::block_entity_data::base_tag;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::tree::shuffle;
use mc_game_core::world::level::levelgen::structure::manager::StructureManager;
use mc_game_core::world::level::levelgen::structure::pieces::PiecesContainer;
use crate::world::level::levelgen::structure::single_piece_structure::SinglePieceStructure;
use mc_game_core::world::level::levelgen::structure::structure::{GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use linkme::distributed_slice;
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::{RandomSource, create_thread_local_instance};
use mc_data::block::Block;
use std::sync::Arc;

pub struct DesertPyramidStructure {
    inner: SinglePieceStructure,
}

impl DesertPyramidStructure {
    /// `DesertPyramidStructure(settings)`.
    pub fn new(settings: StructureSettings) -> DesertPyramidStructure {
        DesertPyramidStructure { inner: SinglePieceStructure::new(|random, west, north| Box::new(DesertPyramidPiece::new(random, west, north)), 21, 21, settings) }
    }
}

#[distributed_slice(STRUCTURE_TYPES)]
static DESERT_PYRAMID: TypeEntry<dyn Structure> = TypeEntry {
    name: "desert_pyramid",
    decode: |fields, ctx| Ok(Arc::new(DesertPyramidStructure::new(StructureSettings::decode(fields, ctx)?))),
};

/// `DesertPyramidStructure.placeSuspiciousSand`: the block entity is the `BrushableBlockEntity` that
/// `setLootTable(DESERT_PYRAMID_ARCHAEOLOGY, pos.asLong())` configured.
fn place_suspicious_sand(chunk_bb: &BoundingBox, level: &mut dyn WorldGenLevel, block_pos: BlockPos) {
    if chunk_bb.is_inside(&block_pos) {
        level.set_block_flags(block_pos, Block::SUSPICIOUS_SAND.default_state(), 2);
        if level.get_block_state(block_pos).block() != Block::SUSPICIOUS_SAND {
            return;
        }
        let mut tag = base_tag("minecraft:brushable_block", block_pos);
        tag.put_string("LootTable", "minecraft:archaeology/desert_pyramid");
        let seed = block_pos.as_long();
        if seed != 0 {
            tag.put_long("LootTableSeed", seed);
        }
        level.set_block_entity_nbt(tag);
    }
}

impl Structure for DesertPyramidStructure {
    fn settings(&self) -> &StructureSettings {
        &self.inner.settings
    }

    fn find_generation_point<'a>(&'a self, context: &mut GenerationContext<'_>) -> Option<GenerationStub<'a>> {
        self.inner.find_generation_point(context)
    }

    fn type_name(&self) -> &'static str {
        "desert_pyramid"
    }

    fn after_place(
        &self,
        level: &mut dyn WorldGenLevel,
        _structure_manager: &StructureManager,
        _generator: &GenEnv,
        _random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
        _chunk_pos: ChunkPos,
        pieces: &PiecesContainer,
    ) {
        // `SortedArraySet.create(Vec3i::compareTo)`: ascending by (y, z, x), no duplicates.
        let mut unique_sand_placements: Vec<BlockPos> = Vec::new();
        for piece in &pieces.pieces {
            if let Some(desert_pyramid_piece) = (**piece).downcast_ref::<DesertPyramidPiece>() {
                unique_sand_placements.extend_from_slice(desert_pyramid_piece.get_potential_suspicious_sand_world_positions());
                place_suspicious_sand(chunk_bb, level, desert_pyramid_piece.get_random_collapsed_roof_pos());
            }
        }
        unique_sand_placements.sort_by_key(|p| (p.y(), p.z(), p.x()));
        unique_sand_placements.dedup();

        let mut shuffled_sand_placements = unique_sand_placements.clone();
        let mut positional_random = create_thread_local_instance(level.get_seed()).fork_positional().at_pos(pieces.calculate_bounding_box().get_center());
        shuffle(&mut shuffled_sand_placements, &mut *positional_random);
        let mut suspicious_sand_to_place = (unique_sand_placements.len() as i32).min(positional_random.next_int_between(5, 8));

        for block_pos in shuffled_sand_placements {
            if suspicious_sand_to_place > 0 {
                suspicious_sand_to_place -= 1;
                place_suspicious_sand(chunk_bb, level, block_pos);
            } else if chunk_bb.is_inside(&block_pos) {
                level.set_block_flags(block_pos, Block::SAND.default_state(), 2);
            }
        }
    }
}
