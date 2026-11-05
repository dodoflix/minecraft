//! Entity effects whose world-side services belong to other slices (explosions, particles, entity spawning, functions).

use mc_core::phys::Vec3;

use super::EnchantmentEntityEffect;
use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::item::enchantment::actors::Actors;
use crate::world::item::enchantment::enchanted_item_in_use::EnchantedItemInUse;

pub fn apply(
    effect: &EnchantmentEntityEffect,
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    enchantment_level: i32,
    _item: &mut EnchantedItemInUse,
    entity: EntityId,
    _position: Vec3,
) {
    match effect {
        // `Player.causeFoodExhaustion(amount)`
        EnchantmentEntityEffect::ApplyExhaustion { amount } => {
            let amount = amount.calculate(enchantment_level);
            actors.with(level, entity, |_, e| {
                if let Some(p) = e.player_mut() {
                    if !p.abilities.invulnerable {
                        p.food_data.add_exhaustion(amount);
                    }
                }
            });
        }
        // `Enchantment run_function effect failed for non-existent function`: no data pack functions are loaded in this server
        EnchantmentEntityEffect::RunFunction { function } => {
            log::error!("Enchantment run_function effect failed for non-existent function {function}");
        }
        // WAITING(EcExplosions): `ServerLevel.explode(..)`; WAITING(EcEntityBase): `sendParticles`; WAITING(EcExplosions/EcLevel): entity spawning
        EnchantmentEntityEffect::Explode(_) | EnchantmentEntityEffect::SpawnParticles(_) | EnchantmentEntityEffect::SummonEntity(_) => {}
        _ => unreachable!("handled in apply"),
    }
}
