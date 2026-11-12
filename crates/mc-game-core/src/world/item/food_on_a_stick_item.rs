//! `FoodOnAStickItem`: `use` boosts the pig or strider the player steers and damages the stick.

use super::{InteractionHand, InteractionResult, ItemBehaviour, hurt_and_convert_on_break};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::stats::Stat;
use crate::world::entity::mob::navigation::controlled_vehicle;
use crate::world::entity::{Entity, ServerPlayer};
use mc_data::{EntityType, Item};

struct FoodOnAStickItem {
    can_interact_with: EntityType,
    consume_item_damage: i32,
}

impl ItemBehaviour for FoodOnAStickItem {
    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let boosted = player.base().is_passenger()
            && controlled_vehicle(&*player, level).is_some_and(|id| {
                level
                    .get_entity_mut(id)
                    .is_some_and(|v| v.get_type() == self.can_interact_with && v.boost())
            });
        let mut stack = player.get_item_in_hand(hand);
        if !boosted {
            player.award_stat_once(level, Stat::Used(stack.item()));
            return InteractionResult::PASS;
        }
        let result = hurt_and_convert_on_break(
            &mut stack,
            self.consume_item_damage,
            Item::FISHING_ROD,
            level,
            player,
            hand.as_equipment_slot(),
        );
        // vanilla mutates the held stack in place
        player.set_item_in_hand(hand, result.clone());
        InteractionResult::SUCCESS_SERVER.held_item_transformed_to(result)
    }
}

register_item_class!("FoodOnAStickItem", |item| {
    let (can_interact_with, consume_item_damage) = if item == Item::CARROT_ON_A_STICK {
        (EntityType::PIG, 7)
    } else {
        (EntityType::STRIDER, 1)
    };
    Box::new(FoodOnAStickItem {
        can_interact_with,
        consume_item_damage,
    })
});
