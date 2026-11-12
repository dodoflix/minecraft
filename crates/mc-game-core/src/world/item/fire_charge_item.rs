//! `FireChargeItem.useOn`: lights campfires, candles and candle cakes, or places fire against the clicked face.
//!
//! Not implemented: the `ProjectileItem` half (`asProjectile` building a `SmallFireball`, the dispenser config with
//! uncertainty 6.6666665 and event 1018).

use super::{InteractionResult, ItemBehaviour, UseOnContext};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::level::block::impls::{
    base_fire_block, campfire_block, candle_block, candle_cake_block,
};
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::{GameEvent, SoundEvent, props};
use mc_protocol::packets::play::SoundSource;

pub struct FireChargeItem;

impl ItemBehaviour for FireChargeItem {
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let player_id = Some(player.base_id());
        let mut pos = context.get_clicked_pos();
        let state = level.get_block_state(pos);
        if !campfire_block::can_light(state)
            && !candle_block::can_light(state)
            && !candle_cake_block::can_light(state)
        {
            pos = pos.relative(context.get_clicked_face());
            if !base_fire_block::can_be_placed_at(level, pos, context.get_horizontal_direction()) {
                return InteractionResult::FAIL;
            }
            play_sound(level, pos);
            let fire_state = base_fire_block::get_state(&*level, pos);
            level.set_block_and_update(pos, fire_state);
            level.game_event(
                GameEvent::BLOCK_PLACE,
                Vec3::at_center_of(&pos),
                &GameEventContext::of(player_id, None),
            );
        } else {
            play_sound(level, pos);
            level.set_block_and_update(pos, state.set(props::LIT, true));
            level.game_event(
                GameEvent::BLOCK_CHANGE,
                Vec3::at_center_of(&pos),
                &GameEventContext::of(player_id, None),
            );
        }
        context.item_stack.shrink(1);
        InteractionResult::SUCCESS
    }
}

fn play_sound(level: &mut ServerLevel, pos: BlockPos) {
    let random = level.random();
    let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
    level.play_sound(
        None,
        pos,
        SoundEvent::ITEM_FIRECHARGE_USE,
        SoundSource::Blocks,
        1.0,
        pitch,
    );
}

register_item_class!("FireChargeItem", |_| Box::new(FireChargeItem));
