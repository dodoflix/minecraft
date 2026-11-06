//! `StrongholdStructure` (`minecraft:stronghold`).

use super::stronghold_pieces::StartPiece;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::structure::pieces::StructurePiecesBuilder;
use mc_game_core::world::level::levelgen::structure::structure::{
    GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings,
};
use std::sync::Arc;

pub struct StrongholdStructure {
    settings: StructureSettings,
}

impl StrongholdStructure {
    pub fn new(settings: StructureSettings) -> StrongholdStructure {
        StrongholdStructure { settings }
    }

    /// `simpleCodec(StrongholdStructure::new)`.
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Structure>> {
        Ok(Arc::new(StrongholdStructure::new(
            StructureSettings::decode(fields, ctx)?,
        )))
    }
}

/// `generatePieces(builder, context)`: regenerates (seed + tries) until the layout contains a portal room.
fn generate_pieces(builder: &mut StructurePiecesBuilder, context: &mut GenerationContext<'_>) {
    let chunk_pos = context.chunk_pos;
    let sea_level = context.env.get_sea_level();
    let min_y = context.env.get_min_y();
    let mut tries = 0i64;
    loop {
        builder.clear();
        context.random.set_large_feature_seed(
            context.seed.wrapping_add(tries),
            chunk_pos.x,
            chunk_pos.z,
        );
        tries += 1;
        // `resetPieces()` happens with the creation of the start piece's state
        let start_room = StartPiece::new(
            &mut context.random,
            chunk_pos.get_block_x(2),
            chunk_pos.get_block_z(2),
        );
        // `builder.addPiece(startRoom); startRoom.addChildren(startRoom, builder, random)`
        builder.add_piece_and_children(Box::new(start_room), None, &mut context.random);
        // the start piece is expanded next to the builder: its stand-in has the same box for the collision checks
        let mut start_room = builder.take_out(0);

        loop {
            let state = &mut start_room
                .downcast_mut::<StartPiece>()
                .expect("start piece")
                .state;
            if !state.has_pending_children() {
                break;
            }
            let pos = context
                .random
                .next_int_bounded(state.pending_children_len() as i32)
                as usize;
            let index = state.remove_pending_child(pos);
            let mut piece = builder.take_out(index);
            piece.add_children(Some(&mut *start_room), builder, &mut context.random);
            builder.put_back(index, piece);
        }

        let has_portal_room = start_room
            .downcast_ref::<StartPiece>()
            .expect("start piece")
            .state
            .has_portal_room();
        builder.put_back(0, start_room);
        builder.move_below_sea_level(sea_level, min_y, &mut context.random, 10);
        if !(builder.is_empty() || !has_portal_room) {
            break;
        }
    }
}

impl Structure for StrongholdStructure {
    fn settings(&self) -> &StructureSettings {
        &self.settings
    }

    fn find_generation_point<'a>(
        &'a self,
        context: &mut GenerationContext<'_>,
    ) -> Option<GenerationStub<'a>> {
        Some(GenerationStub::new(
            context.chunk_pos.get_world_position(),
            generate_pieces,
        ))
    }

    fn type_name(&self) -> &'static str {
        "stronghold"
    }
}

#[linkme::distributed_slice(STRUCTURE_TYPES)]
static STRONGHOLD: TypeEntry<dyn Structure> = TypeEntry {
    name: "stronghold",
    decode: StrongholdStructure::decode,
};
