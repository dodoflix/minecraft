//! The static spawn helpers of `Projectile`: `spawnProjectileFromRotation`, `spawnProjectileUsingShoot`,
//! `spawnProjectile` and `applyOnProjectileSpawned`.

use super::{Projectile, shoot, shoot_from_rotation};
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::ItemStack;
use crate::world::item::enchantment::actors::Actors;

/// `Projectile.spawnProjectile(projectile, level, itemStack, shootFunction)`: shoots, adds the entity to the level and
/// applies the `on_projectile_spawned` enchantment effects of `item_stack`. `None` when the level refused the entity.
pub fn spawn_projectile(level: &mut ServerLevel, mut projectile: Box<dyn Entity>, item_stack: &ItemStack, shoot_fn: impl FnOnce(&mut dyn Projectile)) -> Option<EntityId> {
    if let Some(p) = projectile.as_projectile_mut() {
        shoot_fn(p);
    }
    let id = projectile.id();
    if !level.add_fresh_entity(projectile) {
        return None;
    }
    apply_on_projectile_spawned(level, id, item_stack);
    Some(id)
}

/// `spawnProjectile(projectile, level, itemStack)` (no shoot function).
pub fn spawn_projectile_plain(level: &mut ServerLevel, projectile: Box<dyn Entity>, item_stack: &ItemStack) -> Option<EntityId> {
    spawn_projectile(level, projectile, item_stack, |_| {})
}

/// `Projectile.spawnProjectileFromRotation(creator, level, itemStack, source, yOffset, pow, uncertainty)`:
/// `creator(level, source, itemStack)` builds the projectile.
pub fn spawn_projectile_from_rotation(
    level: &mut ServerLevel,
    creator: impl FnOnce(&mut ServerLevel, &dyn Entity, &ItemStack) -> Box<dyn Entity>,
    item_stack: &ItemStack,
    source: &dyn Entity,
    y_offset: f32,
    pow: f32,
    uncertainty: f32,
) -> Option<EntityId> {
    let projectile = creator(level, source, item_stack);
    let movement = source.get_known_movement(level);
    let (x_rot, y_rot, on_ground) = (source.base().x_rot(), source.base().y_rot(), source.base().on_ground());
    spawn_projectile(level, projectile, item_stack, |p| shoot_from_rotation(p, movement, on_ground, x_rot, y_rot, y_offset, pow, uncertainty))
}

/// `Projectile.spawnProjectileUsingShoot(projectile, level, itemStack, targetX, targetY, targetZ, pow, uncertainty)`.
pub fn spawn_projectile_using_shoot(level: &mut ServerLevel, projectile: Box<dyn Entity>, item_stack: &ItemStack, target_x: f64, target_y: f64, target_z: f64, pow: f32, uncertainty: f32) -> Option<EntityId> {
    spawn_projectile(level, projectile, item_stack, |p| shoot(p, target_x, target_y, target_z, pow, uncertainty))
}

/// `Projectile.applyOnProjectileSpawned(level, pickupItemStack)` for the projectile `id` that is in the level.
pub fn apply_on_projectile_spawned(level: &mut ServerLevel, id: EntityId, pickup_item_stack: &ItemStack) {
    let (owner, weapon) = match level.get_entity(id).and_then(|e| e.as_projectile()) {
        Some(p) => (p.get_owner(level).filter(|o| o.living().is_some()).map(|o| o.id()), p.get_weapon_item()),
        None => return,
    };
    let mut pickup = pickup_item_stack.clone();
    crate::world::item::enchantment_helper::on_projectile_spawned(level, &mut Actors::none(), &mut pickup, id, owner, Box::new(|_, _| {}));
    // `AbstractArrow`: the weapon's enchantments too when it is another item than the pickup item
    if let Some(mut weapon) = weapon
        && !weapon.is_empty()
        && pickup_item_stack.item() != weapon.item()
    {
        let on_break = Box::new(move |level: &mut ServerLevel, broken: &ItemStack| {
            level.with_entity(id, |_, e| {
                if let Some(p) = e.as_projectile_mut() {
                    p.on_item_break(broken);
                }
            });
        });
        crate::world::item::enchantment_helper::on_projectile_spawned(level, &mut Actors::none(), &mut weapon, id, owner, on_break);
    }
}
