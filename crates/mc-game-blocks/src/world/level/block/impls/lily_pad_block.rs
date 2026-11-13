//! `net.minecraft.world.level.block.LilyPadBlock` (extends `VegetationBlock`).
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::MayPlaceOn;
use mc_game_core::{register_block_class, update_shape};

pub struct LilyPadBlock;

impl MayPlaceOn for LilyPadBlock {
    fn may_place_on(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        let fluid_state = level.get_fluid_state(pos);
        let fluid_above = level.get_fluid_state(pos.above());
        (fluid_state.is_tag("supports_lily_pad") || state.is_tag("supports_lily_pad"))
            && fluid_above.is_empty()
    }
}

/// `entity instanceof AbstractBoat` (`Boat`, `ChestBoat`, `Raft`, `ChestRaft`).
fn is_abstract_boat(ty: mc_data::EntityType) -> bool {
    let name = ty.name();
    name.ends_with("_boat") || name.ends_with("_raft")
}

impl BlockBehaviour for LilyPadBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        mc_game_core::world::level::block::impls::vegetation_block::can_survive(self, level, pos)
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            mc_game_core::world::level::block::impls::vegetation_block::update_shape(
                state, &*level, pos,
            )
        }
    );
    fn entity_inside(
        &self,
        _state: BlockState,
        level: &mut mc_game_core::server::level::ServerLevel,
        pos: BlockPos,
        entity: &mut dyn mc_game_core::world::entity::Entity,
        _effect_applier: &mut dyn mc_game_core::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        if is_abstract_boat(entity.get_type()) {
            let id = entity.id();
            level.lend(&mut *entity, |level| {
                level.destroy_block(pos, true, Some(id), 512)
            });
        }
    }
}

register_block_class!("LilyPadBlock", |_| Box::new(LilyPadBlock));
