//! `WindChargeItem`: `use` throws a `WindCharge` from the player, `ProjectileItem` is the dispenser's entry point.

use super::{
    DispenseConfig, InteractionHand, InteractionResult, ItemBehaviour, ItemStack, ItemStackExt,
    PositionFunction, ProjectileItem, thrown_items::play_throw_sound,
};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::projectile::Projectile;
use crate::world::entity::projectile::hurting::windcharge::WindCharge;
use crate::world::entity::projectile::spawn::spawn_projectile_from_rotation;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::LevelAccessor;
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_data::SoundEvent;
use mc_protocol::packets::play::{SoundSource, Stat};

/// `WindChargeItem.PROJECTILE_SHOOT_POWER`.
const PROJECTILE_SHOOT_POWER: f32 = 1.5;

struct WindChargeItem;

impl ItemBehaviour for WindChargeItem {
    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let mut stack = player.get_item_in_hand(hand);
        spawn_projectile_from_rotation(
            level,
            // vanilla's creator ignores `source` and captures the player: same entity, eye height over the feet column
            |level, source, _item| {
                let eye = source.base().get_eye_position();
                Box::new(WindCharge::from_player(level, source, eye.x, eye.y, eye.z))
            },
            &stack,
            player,
            0.0,
            PROJECTILE_SHOOT_POWER,
            1.0,
        );
        play_throw_sound(
            level,
            player,
            SoundEvent::ENTITY_WIND_CHARGE_THROW,
            SoundSource::Neutral,
        );
        player.award_stat_once(level, Stat::Used(stack.item()));
        let infinite = player.player.abilities.instabuild;
        stack.consume(1, infinite);
        player.set_item_in_hand(hand, stack);
        InteractionResult::SUCCESS
    }

    fn as_projectile_item(&self) -> Option<&dyn ProjectileItem> {
        Some(self)
    }
}

impl ProjectileItem for WindChargeItem {
    fn as_projectile(
        &self,
        level: &mut ServerLevel,
        position: Vec3,
        _item_stack: &ItemStack,
        direction: Direction,
    ) -> Box<dyn Entity> {
        let random = level.random();
        let dir_x = random.triangle(f64::from(direction.get_step_x()), 0.11485000000000001);
        let dir_y = random.triangle(f64::from(direction.get_step_y()), 0.11485000000000001);
        let dir_z = random.triangle(f64::from(direction.get_step_z()), 0.11485000000000001);
        let dir = Vec3::new(dir_x, dir_y, dir_z);
        let mut charge = WindCharge::with_direction(level, position.x, position.y, position.z, dir);
        charge.base_mut().set_delta_movement(dir);
        Box::new(charge)
    }

    /// The charge flies along the direction it was created with: nothing is shot.
    fn shoot(&self, _: &mut dyn Projectile, _: f64, _: f64, _: f64, _: f32, _: f32) {}

    fn create_dispense_config(&self) -> DispenseConfig {
        DispenseConfig {
            position_function: PositionFunction::Dispenser {
                scale: 1.0,
                offset: Vec3::ZERO,
            },
            uncertainty: 6.6666665,
            power: 1.0,
            override_dispense_event: Some(1051),
        }
    }
}

register_item_class!("WindChargeItem", |_| Box::new(WindChargeItem));
