//! `NameTagItem`: names a living entity (`interactLivingEntity`).

use super::{InteractionHand, InteractionResult, ItemBehaviour, ItemStack, ItemStackExt};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, ServerPlayer};
use mc_protocol::components::CUSTOM_NAME;

pub struct NameTagItem;

impl ItemBehaviour for NameTagItem {
    fn interact_living_entity(
        &self,
        _level: &mut ServerLevel,
        stack: &mut ItemStack,
        _player: &mut ServerPlayer,
        target: &mut dyn Entity,
        _hand: InteractionHand,
    ) -> InteractionResult {
        let Some(custom_name) = stack.get(&CUSTOM_NAME).cloned() else {
            return InteractionResult::PASS;
        };
        if !target.get_type().data().serializable {
            return InteractionResult::PASS;
        }
        if target.is_alive() {
            target.set_custom_name(Some(custom_name));
            if target.mob().is_some() {
                target.set_persistence_required();
            }
            stack.consume(1, false);
        }
        InteractionResult::SUCCESS
    }
}

register_item_class!("NameTagItem", |_| Box::new(NameTagItem));
