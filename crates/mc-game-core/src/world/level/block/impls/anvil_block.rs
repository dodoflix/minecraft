//! `net.minecraft.world.level.block.AnvilBlock` (extends `FallingBlock`).
use super::falling_block;
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::stats::stats::Stats;
use crate::world::damagesource::DamageSource;
use crate::world::entity::falling_block_entity::FallingBlockEntity;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::inventory::{AnvilMenu, ContainerLevelAccess, MenuProvider, SimpleMenuProvider};
use crate::world::item::InteractionResult;
use mc_core::phys::BlockHitResult;
use mc_text::Component;

pub struct AnvilBlock;

/// `AnvilBlock.damage(state)`: the next damage stage, `None` for a damaged anvil.
pub fn damage(state: BlockState) -> Option<BlockState> {
    let facing = state.get(props::HORIZONTAL_FACING);
    if state.is_block(Block::ANVIL) {
        Some(
            Block::CHIPPED_ANVIL
                .default_state()
                .set(props::HORIZONTAL_FACING, facing),
        )
    } else if state.is_block(Block::CHIPPED_ANVIL) {
        Some(
            Block::DAMAGED_ANVIL
                .default_state()
                .set(props::HORIZONTAL_FACING, facing),
        )
    } else {
        None
    }
}

impl BlockBehaviour for AnvilBlock {
    fn get_state_for_placement(
        &self,
        block: Block,
        _level: &mut ServerLevel,
        context: &crate::world::item::BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(block.default_state().set(
            props::HORIZONTAL_FACING,
            context.get_horizontal_direction().get_clockwise(),
        ))
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            falling_block::update_shape(state, level, pos, 2)
        }
    );
    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        falling_block::on_place(state, level, pos, 2);
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        falling_block::tick(state, level, pos);
    }
    fn falling(&self, entity: &mut FallingBlockEntity) {
        entity.set_hurts_entities(2.0, 40);
    }
    fn on_land(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _state: BlockState,
        _replaced: BlockState,
        entity: &FallingBlockEntity,
    ) {
        if !entity.base().is_silent() {
            level.level_event(None, 1031, pos, 0);
        }
    }
    fn on_broken_after_fall(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        entity: &FallingBlockEntity,
    ) {
        if !entity.base().is_silent() {
            level.level_event(None, 1029, pos, 0);
        }
    }
    fn get_fall_damage_source(
        &self,
        level: &ServerLevel,
        entity: &FallingBlockEntity,
    ) -> DamageSource {
        level.damage_sources().anvil(entity)
    }
    /// `useWithoutItem`: `player.openMenu(state.getMenuProvider(level, pos))` + `INTERACT_WITH_ANVIL`.
    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        if let Some(provider) = self.get_menu_provider(state, level, pos) {
            player.open_menu(level, provider.as_ref());
        }
        player.award_stat_once(level, Stats::INTERACT_WITH_ANVIL);
        InteractionResult::SUCCESS
    }
    fn get_menu_provider(
        &self,
        _state: BlockState,
        _level: &ServerLevel,
        pos: BlockPos,
    ) -> Option<Box<dyn MenuProvider>> {
        Some(Box::new(SimpleMenuProvider::new(
            Component::translatable("container.repair", Vec::new()),
            move |container_id, player| {
                Some(Box::new(AnvilMenu::new(
                    container_id,
                    player,
                    ContainerLevelAccess::create(pos),
                )))
            },
        )))
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
}

register_block_class!("AnvilBlock", |_| Box::new(AnvilBlock));
