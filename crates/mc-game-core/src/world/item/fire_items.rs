//! `FlintAndSteelItem` and `FireChargeItem` (`useOn`: light campfires/candles/candle cakes or place fire).

use super::{InteractionResult, ItemBehaviour, ItemStackExt, UseOnContext, hurt_and_break};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::level::LevelAccessor;
use crate::world::level::block::impls::{base_fire_block, campfire_block, candle_block, candle_cake_block};
use crate::world::level::block::BlockStateExt;
use crate::world::level::gameevent::GameEventContext;
use mc_core::phys::Vec3;
use mc_data::{GameEvent, SoundEvent, props};
use mc_protocol::packets::play::SoundSource;

/// `CampfireBlock.canLight(state) || CandleBlock.canLight(state) || CandleCakeBlock.canLight(state)`.
fn can_light(state: mc_data::block::BlockState) -> bool {
    campfire_block::can_light(state) || candle_block::can_light(state) || candle_cake_block::can_light(state)
}

pub struct FlintAndSteelItem;

impl ItemBehaviour for FlintAndSteelItem {
    fn use_on(&self, context: &mut UseOnContext, level: &mut ServerLevel, player: &mut ServerPlayer) -> InteractionResult {
        let pos = context.get_clicked_pos();
        let state = level.get_block_state(pos);
        let player_id = player.base_id();
        if !can_light(state) {
            let relative_pos = pos.relative(context.get_clicked_face());
            if base_fire_block::can_be_placed_at(level, relative_pos, context.get_horizontal_direction()) {
                let pitch = LevelAccessor::random(level).next_float() * 0.4 + 0.8;
                level.play_sound(Some(player_id), relative_pos, SoundEvent::ITEM_FLINTANDSTEEL_USE, SoundSource::Blocks, 1.0, pitch);
                let fire_state = base_fire_block::get_state(&*level, relative_pos);
                level.set_block_flags(relative_pos, fire_state, 11);
                level.game_event(GameEvent::BLOCK_PLACE, Vec3::at_center_of(&pos), &GameEventContext::of_entity(Some(player_id)));
                crate::advancements::triggers::CriteriaTriggers::PLACED_BLOCK.trigger(level, player, relative_pos, &context.item_stack);
                hurt_and_break(&mut context.item_stack, 1, level, player, context.hand.as_equipment_slot());
                InteractionResult::SUCCESS
            } else {
                InteractionResult::FAIL
            }
        } else {
            let pitch = LevelAccessor::random(level).next_float() * 0.4 + 0.8;
            level.play_sound(Some(player_id), pos, SoundEvent::ITEM_FLINTANDSTEEL_USE, SoundSource::Blocks, 1.0, pitch);
            level.set_block_flags(pos, state.set(props::LIT, true), 11);
            level.game_event(GameEvent::BLOCK_CHANGE, Vec3::at_center_of(&pos), &GameEventContext::of_entity(Some(player_id)));
            hurt_and_break(&mut context.item_stack, 1, level, player, context.hand.as_equipment_slot());
            InteractionResult::SUCCESS
        }
    }
}

pub struct FireChargeItem;

impl FireChargeItem {
    /// `playSound(level, pos)`.
    fn play_sound(level: &mut ServerLevel, pos: mc_core::pos::BlockPos) {
        let random = LevelAccessor::random(level);
        let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
        level.play_sound(None, pos, SoundEvent::ITEM_FIRECHARGE_USE, SoundSource::Blocks, 1.0, pitch);
    }
}

impl ItemBehaviour for FireChargeItem {
    fn use_on(&self, context: &mut UseOnContext, level: &mut ServerLevel, player: &mut ServerPlayer) -> InteractionResult {
        let mut pos = context.get_clicked_pos();
        let block_state = level.get_block_state(pos);
        let player_id = Some(player.base_id());
        let used;
        if !can_light(block_state) {
            pos = pos.relative(context.get_clicked_face());
            if base_fire_block::can_be_placed_at(level, pos, context.get_horizontal_direction()) {
                Self::play_sound(level, pos);
                let fire_state = base_fire_block::get_state(&*level, pos);
                level.set_block_and_update(pos, fire_state);
                level.game_event(GameEvent::BLOCK_PLACE, Vec3::at_center_of(&pos), &GameEventContext::of_entity(player_id));
                used = true;
            } else {
                used = false;
            }
        } else {
            Self::play_sound(level, pos);
            level.set_block_and_update(pos, block_state.set(props::LIT, true));
            level.game_event(GameEvent::BLOCK_CHANGE, Vec3::at_center_of(&pos), &GameEventContext::of_entity(player_id));
            used = true;
        }
        if used {
            context.item_stack.shrink(1);
            InteractionResult::SUCCESS
        } else {
            InteractionResult::FAIL
        }
    }
}

register_item_class!("FlintAndSteelItem", |_| Box::new(FlintAndSteelItem));
register_item_class!("FireChargeItem", |_| Box::new(FireChargeItem));
