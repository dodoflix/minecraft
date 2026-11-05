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
        EnchantmentEntityEffect::SpawnParticles(e) => {
            let particles = actors.with(level, entity, |lvl, t| {
                let movement = t.get_known_movement(lvl);
                let (w, h) = (t.base().get_bb_width(), t.base().get_bb_height());
                let pos = _position;
                let random = &mut t.base_mut().random;
                let x = e.horizontal_position.get_coordinate(pos.x, pos.x, w, random);
                let y = e.vertical_position.get_coordinate(pos.y, pos.y + f64::from(h / 2.0f32), h, random);
                let z = e.horizontal_position.get_coordinate(pos.z, pos.z, w, random);
                let vx = e.horizontal_velocity.get_velocity(movement.x, random);
                let vy = e.vertical_velocity.get_velocity(movement.y, random);
                let vz = e.horizontal_velocity.get_velocity(movement.z, random);
                let speed = e.speed.sample(random);
                (x, y, z, vx, vy, vz, speed)
            });
            if let Some((x, y, z, vx, vy, vz, speed)) = particles {
                level.send_particles(e.particle.clone(), x, y, z, 0, vx, vy, vz, f64::from(speed));
            }
        }
        // WAITING(EcExplosions): `ServerLevel.explode(..)` and entity spawning (`entity_factory::create` + `add_fresh_entity`)
        EnchantmentEntityEffect::Explode(_) | EnchantmentEntityEffect::SummonEntity(_) => {}
        _ => unreachable!("handled in apply"),
    }
}
