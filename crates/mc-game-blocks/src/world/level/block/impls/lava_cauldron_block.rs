//! `net.minecraft.world.level.block.LavaCauldronBlock`.
use super::abstract_cauldron_block;
use super::cauldron_interactions::CauldronKind;
use mc_core::phys::shapes::{Shapes, VoxelShape};
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::inside_block_effects::{
    InsideBlockEffectApplier, InsideBlockEffectType,
};
use mc_game_core::world::entity::{Entity, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::shapes::{column, voxel_shape};
use std::sync::LazyLock;

/// `LavaCauldronBlock.FILLED_SHAPE`.
static FILLED_SHAPE: LazyLock<VoxelShape> = LazyLock::new(|| column(12.0, 4.0, 15.0));

pub struct LavaCauldronBlock;

impl BlockBehaviour for LavaCauldronBlock {
    fn get_interaction_shape(
        &self,
        _state: BlockState,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
    ) -> VoxelShape {
        abstract_cauldron_block::interaction_shape()
    }
    fn get_entity_inside_collision_shape(
        &self,
        state: BlockState,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
        _entity: &dyn Entity,
    ) -> VoxelShape {
        Shapes::or(&voxel_shape(state.shape()), &FILLED_SHAPE)
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        abstract_cauldron_block::tick(self, state, level, pos);
    }
    fn use_item_on(
        &self,
        stack: &ItemStack,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        abstract_cauldron_block::use_item_on(
            CauldronKind::Lava,
            stack,
            state,
            level,
            pos,
            player,
            hand,
        )
    }
    fn entity_inside(
        &self,
        _state: BlockState,
        _level: &mut ServerLevel,
        _pos: BlockPos,
        _entity: &mut dyn Entity,
        effect_applier: &mut dyn InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        effect_applier.apply(InsideBlockEffectType::ClearFreeze);
        effect_applier.apply(InsideBlockEffectType::LavaIgnite);
        effect_applier.run_after(
            InsideBlockEffectType::LavaIgnite,
            Box::new(|entity, level| entity.lava_hurt(level)),
        );
    }
    fn get_analog_output_signal(
        &self,
        _state: BlockState,
        _level: &ServerLevel,
        _pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        3
    }
    fn is_pathfindable(
        &self,
        _state: BlockState,
        _ty: mc_data::block::PathComputationType,
    ) -> bool {
        false
    }
}

register_block_class!("LavaCauldronBlock", |_| Box::new(LavaCauldronBlock));
