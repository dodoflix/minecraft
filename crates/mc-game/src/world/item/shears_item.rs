//! `ShearsItem`: `mineBlock` (no durability on fire) and `useOn` on a growing plant head (kelp/vines age to max).

use super::{InteractionResult, ItemBehaviour, ItemStack, UseOnContext, hurt_and_break};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::ServerPlayer;
use crate::world::level::block::{BlockStateExt, helpers};
use crate::world::level::gameevent::GameEventContext;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{GameEvent, SoundEvent, props};
use mc_protocol::components::TOOL;
use mc_protocol::packets::play::SoundSource;

/// `GrowingPlantHeadBlock.MAX_AGE`.
const MAX_AGE: i32 = 25;

pub struct ShearsItem;

impl ItemBehaviour for ShearsItem {
    fn mine_block(&self, stack: &mut ItemStack, level: &mut ServerLevel, state: BlockState, _pos: BlockPos, owner: &mut ServerPlayer) -> bool {
        let Some(tool) = stack.get(&TOOL).cloned() else { return false };
        if !state.is_tag("fire") && tool.damage_per_block > 0 {
            hurt_and_break(stack, tool.damage_per_block, level, owner, EquipmentSlot::MainHand);
        }
        true
    }

    fn use_on(&self, context: &mut UseOnContext, level: &mut ServerLevel, player: &mut ServerPlayer) -> InteractionResult {
        let pos = context.get_clicked_pos();
        let state = level.get_block_state(pos);
        if helpers::is_instance_of(state, "GrowingPlantHeadBlock") && state.get(props::AGE_25) != MAX_AGE {
            // CriteriaTriggers::ITEM_USED_ON_BLOCK.trigger(level, player, pos, &context.item_stack) (ImAdvancements)
            let player_id = Some(player.base_id());
            level.play_sound(player_id, pos, SoundEvent::BLOCK_GROWING_PLANT_CROP, SoundSource::Blocks, 1.0, 1.0);
            let new_state = state.set(props::AGE_25, MAX_AGE);
            level.set_block_and_update(pos, new_state);
            level.game_event(GameEvent::BLOCK_CHANGE, Vec3::at_center_of(&pos), &GameEventContext::of(player_id, Some(new_state)));
            hurt_and_break(&mut context.item_stack, 1, level, player, context.hand.as_equipment_slot());
            InteractionResult::SUCCESS
        } else {
            super::PlainItem.use_on(context, level, player)
        }
    }
}

register_item_class!("ShearsItem", |_| Box::new(ShearsItem));
