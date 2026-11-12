//! `net.minecraft.world.item.FireworkRocketItem`.

use super::{
    DispenseConfig, InteractionHand, InteractionResult, ItemBehaviour, ItemStack, ItemStackExt,
    PositionFunction, ProjectileItem, UseOnContext,
};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::projectile::firework_rocket::FireworkRocketEntity;
use crate::world::entity::projectile::spawn::spawn_projectile_plain;
use crate::world::entity::{Entity, ServerPlayer};
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_data::{Item, SoundEvent};
use mc_protocol::packets::play::{SoundSource, Stat};

/// `ROCKET_PLACEMENT_OFFSET`.
const ROCKET_PLACEMENT_OFFSET: f64 = 0.15;

struct FireworkRocketItem;

impl ItemBehaviour for FireworkRocketItem {
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        if player.living().is_some_and(|l| l.is_fall_flying()) {
            return InteractionResult::PASS;
        }
        let item_stack = context.item_stack.clone();
        let click_location = context.get_click_location();
        let direction = context.get_clicked_face();
        let rocket = FireworkRocketEntity::with_owner_at(
            level,
            Some(&*player),
            click_location.x + f64::from(direction.get_step_x()) * ROCKET_PLACEMENT_OFFSET,
            click_location.y + f64::from(direction.get_step_y()) * ROCKET_PLACEMENT_OFFSET,
            click_location.z + f64::from(direction.get_step_z()) * ROCKET_PLACEMENT_OFFSET,
            &item_stack,
        );
        spawn_projectile_plain(level, Box::new(rocket), &item_stack);
        context.item_stack.shrink(1);
        InteractionResult::SUCCESS
    }

    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        if !player.living().is_some_and(|l| l.is_fall_flying()) {
            return InteractionResult::PASS;
        }
        let mut item_stack = player.get_item_in_hand(hand);
        let player_id = player.base_id();
        if player.drop_all_leash_connections(level, None) {
            let p = player.base().position();
            level.play_sound_from_entity_at(
                None,
                player_id,
                p,
                SoundEvent::ITEM_LEAD_BREAK,
                SoundSource::Neutral,
                1.0,
                1.0,
            );
        }
        let rocket = FireworkRocketEntity::stuck_to(level, &item_stack, &*player);
        spawn_projectile_plain(level, Box::new(rocket), &item_stack);
        let infinite = player.player.abilities.instabuild;
        item_stack.consume(1, infinite);
        player.set_item_in_hand(hand, item_stack.clone());
        // `this`, not the stack's item: the consumed last rocket leaves an empty stack
        player.award_stat_once(level, Stat::Used(Item::FIREWORK_ROCKET));
        InteractionResult::SUCCESS
    }

    fn as_projectile_item(&self) -> Option<&dyn ProjectileItem> {
        Some(self)
    }
}

impl ProjectileItem for FireworkRocketItem {
    fn as_projectile(
        &self,
        level: &mut ServerLevel,
        position: Vec3,
        item_stack: &ItemStack,
        _direction: Direction,
    ) -> Box<dyn Entity> {
        Box::new(FireworkRocketEntity::shot_at_angle(
            level,
            &item_stack.copy_with_count(1),
            position.x,
            position.y,
            position.z,
            true,
        ))
    }

    fn create_dispense_config(&self) -> DispenseConfig {
        DispenseConfig {
            position_function: PositionFunction::JustOutsideOfBlock,
            uncertainty: 1.0,
            power: 0.5,
            override_dispense_event: Some(1004),
        }
    }
}

register_item_class!("FireworkRocketItem", |_| Box::new(FireworkRocketItem));
