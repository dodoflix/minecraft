//! `net.minecraft.world.level.block.CopperGolemStatueBlock` and `WeatheringCopperGolemStatueBlock` (whose `randomTick`
//! is dispatched by `weathering_copper::random_tick`); the statue's name lives in
//! `block_entity::copper_golem_statue_block_entity`.
use crate::world::block_entity::copper_golem_statue_block_entity::{
    CopperGolemStatueBlockEntity, has_golem_factory,
};
use mc_core::phys::BlockHitResult;
use mc_data::enums::CopperGolemStatueBlockPose;
use mc_data::fluid::Fluid;
use mc_data::{GameEvent, Item, SoundEvent};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::{Entity as _, ServerPlayer};
use mc_game_core::world::item::{
    BlockPlaceContext, InteractionHand, InteractionResult, ItemStack, hurt_and_break,
};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::weathering_copper::{self, WeatherState};
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::{register_block_class, update_shape};
use mc_protocol::packets::play::level::SoundSource;

/// `CopperGolemStatueBlock` (`weathering == false`) / `WeatheringCopperGolemStatueBlock` (`true`).
pub struct CopperGolemStatueBlock {
    weathering: bool,
}

fn is_axe(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("axes").is_some_and(|tag| tag.contains(stack.item()))
}

/// `Pose.getNextPose()` (`BY_ID` with the `ZERO` out-of-bounds strategy: wraps after the last pose).
fn next_pose(pose: CopperGolemStatueBlockPose) -> CopperGolemStatueBlockPose {
    CopperGolemStatueBlockPose::from_ordinal(pose.ordinal() + 1)
        .unwrap_or(CopperGolemStatueBlockPose::Standing)
}

/// `updatePose(level, state, pos, player)`.
fn update_pose(level: &mut ServerLevel, state: BlockState, pos: BlockPos, player: &ServerPlayer) {
    level.play_sound(
        None,
        pos,
        SoundEvent::ENTITY_COPPER_GOLEM_BECOME_STATUE,
        SoundSource::Blocks,
        1.0,
        1.0,
    );
    level.set_block_and_update(
        pos,
        state.set(
            props::COPPER_GOLEM_POSE,
            next_pose(state.get(props::COPPER_GOLEM_POSE)),
        ),
    );
    level.game_event_at_block(
        GameEvent::BLOCK_CHANGE,
        pos,
        &GameEventContext::of_entity(Some(player.id())),
    );
}

fn has_statue_entity(level: &ServerLevel, pos: BlockPos) -> bool {
    level
        .get_block_entity(pos)
        .is_some_and(|be| be.as_any().is::<CopperGolemStatueBlockEntity>())
}

impl CopperGolemStatueBlock {
    /// `WeatheringCopperGolemStatueBlock.useItemOn`.
    fn use_item_on_weathering(
        stack: &ItemStack,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        if !has_statue_entity(level, pos) {
            return InteractionResult::PASS;
        }
        if !is_axe(stack) {
            if stack.item() == Item::HONEYCOMB {
                return InteractionResult::PASS;
            }
            update_pose(level, state, pos, player);
            return InteractionResult::SUCCESS;
        }
        // Not implemented: without the `CopperGolem` entity nothing can be spawned, so the axe is left undamaged.
        if weathering_copper::age(state.block()) == Some(WeatherState::Unaffected)
            && has_golem_factory()
        {
            let golem = level
                .get_block_entity(pos)
                .and_then(|be| be.as_any().downcast_ref::<CopperGolemStatueBlockEntity>())
                .and_then(|be| be.remove_statue(&*level, state));
            let mut used = stack.clone();
            hurt_and_break(&mut used, 1, level, player, hand.as_equipment_slot());
            player.set_item_in_hand(hand, used);
            if let Some(golem) = golem {
                level.add_fresh_entity(golem);
                level.remove_block(pos, false);
                return InteractionResult::SUCCESS;
            }
        }
        InteractionResult::PASS
    }
}

impl BlockBehaviour for CopperGolemStatueBlock {
    mc_game_core::simple_waterlogged!();

    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let replaced_fluid_state = level.get_fluid_state(context.get_clicked_pos());
        Some(
            block
                .default_state()
                .set(
                    props::HORIZONTAL_FACING,
                    context.get_horizontal_direction().get_opposite(),
                )
                .set(
                    props::WATERLOGGED,
                    replaced_fluid_state.get_type() == Fluid::WATER,
                ),
        )
    }

    fn use_item_on(
        &self,
        stack: &ItemStack,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        if self.weathering {
            return Self::use_item_on_weathering(stack, state, level, pos, player, hand);
        }
        if is_axe(stack) {
            return InteractionResult::PASS;
        }
        update_pose(level, state, pos, player);
        InteractionResult::SUCCESS
    }

    fn is_pathfindable(&self, state: BlockState, ty: mc_data::block::PathComputationType) -> bool {
        ty == mc_data::block::PathComputationType::Water && state.get_fluid_state().is_tag("water")
    }

    fn should_changed_state_keep_block_entity(&self, old_state: BlockState) -> bool {
        old_state.is_tag("copper_golem_statues")
    }

    fn get_analog_output_signal(
        &self,
        state: BlockState,
        _level: &ServerLevel,
        _pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        state.get(props::COPPER_GOLEM_POSE).ordinal() as i32 + 1
    }

    fn get_clone_item_stack(
        &self,
        state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
        _include_data: bool,
    ) -> ItemStack {
        let base = state
            .block()
            .item()
            .map(ItemStack::new)
            .unwrap_or_else(ItemStack::empty);
        match level
            .get_block_entity(pos)
            .and_then(|be| be.as_any().downcast_ref::<CopperGolemStatueBlockEntity>())
        {
            Some(be) => be.get_item(base, state.get(props::COPPER_GOLEM_POSE)),
            None => base,
        }
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        level.update_neighbour_for_output_signal(pos, state.block());
    }

    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            state
        }
    );

    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("CopperGolemStatueBlock", |_| Box::new(
    CopperGolemStatueBlock { weathering: false }
));
register_block_class!("WeatheringCopperGolemStatueBlock", |_| Box::new(
    CopperGolemStatueBlock { weathering: true }
));
