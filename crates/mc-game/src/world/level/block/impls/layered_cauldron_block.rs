//! `net.minecraft.world.level.block.LayeredCauldronBlock` (water and powder snow cauldrons).
use super::abstract_cauldron_block;
use super::cauldron_block::should_handle_precipitation;
use super::cauldron_interactions::CauldronKind;
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::inside_block_effects::{InsideBlockEffectApplier, InsideBlockEffectType};
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::precipitation::Precipitation;
use mc_data::GameEvent;

pub struct LayeredCauldronBlock {
    precipitation_type: Precipitation,
    interactions: CauldronKind,
}

impl LayeredCauldronBlock {
    /// `Blocks.WATER_CAULDRON` (rain, `WATER`) / `Blocks.POWDER_SNOW_CAULDRON` (snow, `POWDER_SNOW`).
    fn new(block: Block) -> LayeredCauldronBlock {
        if block == Block::POWDER_SNOW_CAULDRON {
            LayeredCauldronBlock { precipitation_type: Precipitation::Snow, interactions: CauldronKind::PowderSnow }
        } else {
            LayeredCauldronBlock { precipitation_type: Precipitation::Rain, interactions: CauldronKind::Water }
        }
    }
}

/// `handleEntityOnFireInside(state, level, pos)`.
fn handle_entity_on_fire_inside(precipitation_type: Precipitation, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    if precipitation_type == Precipitation::Snow {
        lower_fill_level(Block::WATER_CAULDRON.default_state().set(props::LEVEL_CAULDRON, state.get(props::LEVEL_CAULDRON)), level, pos);
    } else {
        lower_fill_level(state, level, pos);
    }
}

/// `LayeredCauldronBlock.lowerFillLevel(state, level, pos)`.
pub fn lower_fill_level(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    let new_level = state.get(props::LEVEL_CAULDRON) - 1;
    let new_state = if new_level == 0 { Block::CAULDRON.default_state() } else { state.set(props::LEVEL_CAULDRON, new_level) };
    level.set_block_and_update(pos, new_state);
    level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::of_state(Some(new_state)));
}

impl BlockBehaviour for LayeredCauldronBlock {
    fn use_item_on(&self, stack: &ItemStack, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, hand: InteractionHand, _hit: &mc_core::phys::BlockHitResult) -> InteractionResult {
        abstract_cauldron_block::use_item_on(self.interactions, stack, state, level, pos, player, hand)
    }
    fn is_pathfindable(&self, _state: BlockState, _ty: mc_data::block::PathComputationType) -> bool {
        false
    }
    fn get_analog_output_signal(&self, state: BlockState, _level: &ServerLevel, _pos: BlockPos, _direction: Direction) -> i32 {
        state.get(props::LEVEL_CAULDRON)
    }
    fn handle_precipitation(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, precipitation: Precipitation) {
        if should_handle_precipitation(level, precipitation) && state.get(props::LEVEL_CAULDRON) != 3 && precipitation == self.precipitation_type {
            let new_state = state.cycle(props::LEVEL_CAULDRON);
            level.set_block_and_update(pos, new_state);
            level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::of_state(Some(new_state)));
        }
    }
    fn entity_inside(&self, state: BlockState, _level: &mut ServerLevel, pos: BlockPos, _entity: &mut dyn Entity, effect_applier: &mut dyn InsideBlockEffectApplier, _is_precise: bool) {
        let precipitation_type = self.precipitation_type;
        effect_applier.run_before(
            InsideBlockEffectType::Extinguish,
            Box::new(move |entity, level| {
                if entity.is_on_fire() && entity.may_interact(level, pos) {
                    handle_entity_on_fire_inside(precipitation_type, state, level, pos);
                }
            }),
        );
        effect_applier.apply(InsideBlockEffectType::Extinguish);
    }
}

register_block_class!("LayeredCauldronBlock", |block| Box::new(LayeredCauldronBlock::new(block)));
