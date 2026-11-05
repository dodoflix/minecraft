//! `net.minecraft.world.level.block.SoulFireBlock`.
use super::prelude::*;

pub struct SoulFireBlock;

/// `SoulFireBlock.canSurviveOnBlock`.
pub fn can_survive_on_block(state: BlockState) -> bool {
    state.is_tag("soul_fire_base_blocks")
}

impl BlockBehaviour for SoulFireBlock {
    fn get_state_for_placement(&self, _block: Block, level: &mut crate::server::level::ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
        Some(super::base_fire_block::get_state(&*level, context.get_clicked_pos()))
    }
    fn player_will_destroy(&self, state: BlockState, level: &mut crate::server::level::ServerLevel, pos: BlockPos, player: &mut crate::world::entity::ServerPlayer) -> BlockState {
        super::base_fire_block::player_will_destroy(state, level, pos, player)
    }
    fn entity_inside(
        &self,
        _state: BlockState,
        _level: &mut crate::server::level::ServerLevel,
        _pos: BlockPos,
        _entity: &mut dyn crate::world::entity::Entity,
        effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        super::base_fire_block::entity_inside(effect_applier, 2.0);
    }
    fn on_place(&self, state: BlockState, level: &mut crate::server::level::ServerLevel, pos: BlockPos, old_state: BlockState, _moved_by_piston: bool) {
        super::base_fire_block::on_place(state, level, pos, old_state);
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        can_survive_on_block(level.get_block_state(pos.below()))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if self.can_survive(state, &*level, pos) { Block::SOUL_FIRE.default_state() } else { air() }
    });
}

register_block_class!("SoulFireBlock", |_| Box::new(SoulFireBlock));
