//! `FlintAndSteelItem.useOn`: lights campfires, candles and candle cakes, or places fire against the clicked face.

use super::{InteractionResult, ItemBehaviour, UseOnContext, hurt_and_break};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::level::block::impls::{
    base_fire_block, campfire_block, candle_block, candle_cake_block,
};
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::phys::Vec3;
use mc_data::{GameEvent, SoundEvent, props};
use mc_protocol::packets::play::SoundSource;

pub struct FlintAndSteelItem;

impl ItemBehaviour for FlintAndSteelItem {
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let player_id = Some(player.base_id());
        let pos = context.get_clicked_pos();
        let state = level.get_block_state(pos);
        let slot = context.hand.as_equipment_slot();
        if !campfire_block::can_light(state)
            && !candle_block::can_light(state)
            && !candle_cake_block::can_light(state)
        {
            let relative_pos = pos.relative(context.get_clicked_face());
            if !base_fire_block::can_be_placed_at(
                level,
                relative_pos,
                context.get_horizontal_direction(),
            ) {
                return InteractionResult::FAIL;
            }
            play_use_sound(level, player_id, relative_pos);
            let fire_state = base_fire_block::get_state(&*level, relative_pos);
            level.set_block_flags(relative_pos, fire_state, 11);
            level.game_event_from_entity_ref(
                &*player,
                GameEvent::BLOCK_PLACE,
                Vec3::at_center_of(&pos),
            );
            crate::advancements::triggers::CriteriaTriggers::PLACED_BLOCK.trigger(
                level,
                player,
                relative_pos,
                &context.item_stack,
            );
            hurt_and_break(&mut context.item_stack, 1, level, player, slot);
            InteractionResult::SUCCESS
        } else {
            play_use_sound(level, player_id, pos);
            level.set_block_flags(pos, state.set(props::LIT, true), 11);
            level.game_event_from_entity_ref(
                &*player,
                GameEvent::BLOCK_CHANGE,
                Vec3::at_center_of(&pos),
            );
            hurt_and_break(&mut context.item_stack, 1, level, player, slot);
            InteractionResult::SUCCESS
        }
    }
}

fn play_use_sound(
    level: &mut ServerLevel,
    player_id: Option<crate::world::entity::EntityId>,
    pos: mc_core::pos::BlockPos,
) {
    let pitch = level.random().next_float() * 0.4 + 0.8;
    level.play_sound(
        player_id,
        pos,
        SoundEvent::ITEM_FLINTANDSTEEL_USE,
        SoundSource::Blocks,
        1.0,
        pitch,
    );
}

register_item_class!("FlintAndSteelItem", |_| Box::new(FlintAndSteelItem));
