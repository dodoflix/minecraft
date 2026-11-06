//! `net.minecraft.world.item.BowItem`.

use super::projectile_weapon_item::{ProjectileWeapon, draw, get_projectile, is_arrow, shoot};
use super::{
    APPROXIMATELY_INFINITE_USE_DURATION, InteractionHand, InteractionResult, ItemBehaviour,
    ItemStack,
};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::stats::Stat;
use crate::world::entity::player::ServerPlayer;
use crate::world::entity::projectile::{Projectile, shoot_from_rotation};
use crate::world::entity::{Entity, EntityId};
use crate::world::level::LevelAccessor as _;
use mc_core::random::RandomSource as _;
use mc_data::SoundEvent;
use mc_protocol::components::ItemUseAnimation;
use mc_protocol::packets::play::SoundSource;

pub const MAX_DRAW_DURATION: i32 = 20;
pub const DEFAULT_RANGE: i32 = 15;

pub struct BowItem {
    item: mc_data::Item,
}

/// `BowItem.getPowerForTime(timeHeld)`.
pub fn get_power_for_time(time_held: i32) -> f32 {
    let mut pow = time_held as f32 / 20.0;
    pow = (pow * pow + pow * 2.0) / 3.0;
    if pow > 1.0 {
        pow = 1.0;
    }
    pow
}

impl ProjectileWeapon for BowItem {
    fn supported_held_projectiles(&self, stack: &ItemStack) -> bool {
        is_arrow(stack)
    }

    fn shoot_projectile(
        &self,
        level: &mut ServerLevel,
        shooter: &mut dyn Entity,
        projectile: &mut dyn Projectile,
        _index: i32,
        power: f32,
        uncertainty: f32,
        angle: f32,
        _target: Option<EntityId>,
    ) {
        let movement = shooter.get_known_movement(level);
        let (x_rot, y_rot, on_ground) = (
            shooter.base().x_rot(),
            shooter.base().y_rot(),
            shooter.base().on_ground(),
        );
        shoot_from_rotation(
            projectile,
            movement,
            on_ground,
            x_rot,
            y_rot + angle,
            0.0,
            power,
            uncertainty,
        );
    }
}

impl ItemBehaviour for BowItem {
    fn release_using(
        &self,
        stack: &mut ItemStack,
        level: &mut ServerLevel,
        entity: &mut dyn Entity,
        remaining_time: i32,
    ) -> bool {
        let Some(player) = entity.as_any_mut().downcast_mut::<ServerPlayer>() else {
            return false;
        };
        let mut source = get_projectile(player, self);
        if source.stack.is_empty() {
            return false;
        }
        let time_held = self.get_use_duration(stack) - remaining_time;
        let pow = get_power_for_time(time_held);
        if f64::from(pow) < 0.1 {
            return false;
        }
        let fired = draw(level, stack, &mut source.stack, player);
        source.write_back(player);
        if !fired.is_empty() {
            let hand = player.get_used_item_hand();
            shoot(
                self,
                level,
                player,
                hand,
                stack,
                &fired,
                pow * 3.0,
                1.0,
                pow == 1.0,
                None,
            );
        }
        let pitch = 1.0 / (level.random().next_float() * 0.4 + 1.2) + pow * 0.5;
        let p = player.position();
        level.play_sound_at(
            None,
            p.x,
            p.y,
            p.z,
            SoundEvent::ENTITY_ARROW_SHOOT,
            SoundSource::Players,
            1.0,
            pitch,
        );
        player.award_stat_once(level, Stat::Used(self.item));
        true
    }

    fn get_use_duration(&self, _stack: &ItemStack) -> i32 {
        APPROXIMATELY_INFINITE_USE_DURATION
    }

    fn get_use_animation(&self, _stack: &ItemStack) -> ItemUseAnimation {
        ItemUseAnimation::Bow
    }

    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let stack = player.get_item_in_hand(hand);
        let found_projectile = !get_projectile(player, self).stack.is_empty();
        if !player.has_infinite_materials() && !found_projectile {
            InteractionResult::FAIL
        } else {
            player.start_using_item(level, hand);
            InteractionResult::CONSUME
        }
    }
}

register_item_class!("BowItem", |item| Box::new(BowItem { item }));
