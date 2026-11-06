//! `NetherFortressStructure` (`minecraft:fortress`).

use super::nether_fortress_pieces::StartPiece;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::ids::EntityType;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::structure::pieces::StructurePiecesBuilder;
use mc_game_core::world::level::levelgen::structure::structure::{
    GenerationContext, GenerationStub, STRUCTURE_TYPES, SpawnerData, Structure, StructureSettings,
};
use mc_worldgen::provider::{IntProvider, Weighted, WeightedList};
use std::sync::{Arc, LazyLock};

/// `FORTRESS_ENEMIES`: what `NaturalSpawner` spawns on nether bricks inside a fortress piece.
pub fn fortress_enemies() -> &'static WeightedList<SpawnerData> {
    static FORTRESS_ENEMIES: LazyLock<WeightedList<SpawnerData>> = LazyLock::new(|| {
        let entry = |name: &str, count: IntProvider, weight: i32| Weighted {
            value: SpawnerData {
                entity_type: EntityType::by_name(name).expect("vanilla entity type"),
                count,
            },
            weight,
        };
        WeightedList::of(vec![
            entry(
                "minecraft:blaze",
                IntProvider::Uniform {
                    min_inclusive: 2,
                    max_inclusive: 3,
                },
                10,
            ),
            entry("minecraft:zombified_piglin", IntProvider::constant(4), 5),
            entry("minecraft:wither_skeleton", IntProvider::constant(5), 8),
            entry("minecraft:skeleton", IntProvider::constant(5), 2),
            entry("minecraft:magma_cube", IntProvider::constant(4), 3),
        ])
        .expect("fortress enemy weights")
    });
    &FORTRESS_ENEMIES
}

pub struct NetherFortressStructure {
    settings: StructureSettings,
}

impl NetherFortressStructure {
    pub fn new(settings: StructureSettings) -> NetherFortressStructure {
        NetherFortressStructure { settings }
    }

    /// `simpleCodec(NetherFortressStructure::new)`.
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Structure>> {
        Ok(Arc::new(NetherFortressStructure::new(
            StructureSettings::decode(fields, ctx)?,
        )))
    }
}

/// `generatePieces(builder, context)`.
fn generate_pieces(builder: &mut StructurePiecesBuilder, context: &mut GenerationContext<'_>) {
    let chunk_pos = context.chunk_pos;
    let start = StartPiece::new(
        &mut context.random,
        chunk_pos.get_block_x(2),
        chunk_pos.get_block_z(2),
    );
    // `builder.addPiece(start); start.addChildren(start, builder, random)`
    builder.add_piece_and_children(Box::new(start), None, &mut context.random);
    // the start piece is expanded next to the builder: its stand-in has the same box for the collision checks
    let mut start = builder.take_out(0);

    loop {
        let pending_children = &mut start
            .downcast_mut::<StartPiece>()
            .expect("start piece")
            .pending_children;
        if pending_children.is_empty() {
            break;
        }
        let pos = context
            .random
            .next_int_bounded(pending_children.len() as i32) as usize;
        let index = pending_children.remove(pos);
        let mut piece = builder.take_out(index);
        piece.add_children(Some(&mut *start), builder, &mut context.random);
        builder.put_back(index, piece);
    }

    builder.put_back(0, start);
    builder.move_inside_heights(&mut context.random, 48, 70);
}

impl Structure for NetherFortressStructure {
    fn settings(&self) -> &StructureSettings {
        &self.settings
    }

    fn find_generation_point<'a>(
        &'a self,
        context: &mut GenerationContext<'_>,
    ) -> Option<GenerationStub<'a>> {
        let chunk_pos = context.chunk_pos;
        let start_pos = BlockPos::new(chunk_pos.get_min_block_x(), 64, chunk_pos.get_min_block_z());
        Some(GenerationStub::new(start_pos, generate_pieces))
    }

    fn type_name(&self) -> &'static str {
        "fortress"
    }
}

#[linkme::distributed_slice(STRUCTURE_TYPES)]
static FORTRESS: TypeEntry<dyn Structure> = TypeEntry {
    name: "fortress",
    decode: NetherFortressStructure::decode,
};
