//! `net.minecraft.world.entity.vehicle.VehicleEntity`: the hurt-wobble synched data, `hurtServer` (damage accumulates, > 40 or a
//! destroying source breaks the vehicle, a creative player discards it), `destroy` (kill + drop item with the custom name).
//!
//! A concrete vehicle embeds an `EntityData`, calls [`define_synched_data`] first in its synched data definition, implements
//! [`VehicleEntity`] and forwards `Entity::hurt_server` / `ignore_explosion_in_level` / `get_dimension_changing_delay` here.

use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::item::spawn_at_location;
use crate::world::entity::synched_data::SynchedEntityDataBuilder;
use crate::world::entity::{Entity, EntityData};
use crate::world::item::ItemStack;
use crate::world::level::explosion::Explosion;
use mc_core::phys::Vec3;
use mc_data::{EntityType, GameEvent, Item};
use mc_protocol::components::CUSTOM_NAME;

define_accessor!(pub DATA_ID_HURT: Int = VehicleEntity);
define_accessor!(pub DATA_ID_HURTDIR: Int = VehicleEntity);
define_accessor!(pub DATA_ID_DAMAGE: Float = VehicleEntity);

/// `defineSynchedData`.
pub fn define_synched_data(builder: &mut SynchedEntityDataBuilder) {
    builder.define(DATA_ID_HURT, 0);
    builder.define(DATA_ID_HURTDIR, 1);
    builder.define(DATA_ID_DAMAGE, 0.0);
}

pub fn set_hurt_time(base: &mut EntityData, hurt_time: i32) {
    base.entity_data.set(DATA_ID_HURT, hurt_time);
}

pub fn set_hurt_dir(base: &mut EntityData, hurt_dir: i32) {
    base.entity_data.set(DATA_ID_HURTDIR, hurt_dir);
}

pub fn set_damage(base: &mut EntityData, damage: f32) {
    base.entity_data.set(DATA_ID_DAMAGE, damage);
}

pub fn get_damage(base: &EntityData) -> f32 {
    base.entity_data.get(DATA_ID_DAMAGE)
}

pub fn get_hurt_time(base: &EntityData) -> i32 {
    base.entity_data.get(DATA_ID_HURT)
}

pub fn get_hurt_dir(base: &EntityData) -> i32 {
    base.entity_data.get(DATA_ID_HURTDIR)
}

/// `Entity.getDimensionChangingDelay()` of vehicles.
pub const DIMENSION_CHANGING_DELAY: i32 = 10;

/// The abstract members of `VehicleEntity`.
pub trait VehicleEntity: Entity {
    /// `getDropItem()`.
    fn get_drop_item(&self) -> Item;

    /// `shouldSourceDestroy(source)`.
    fn should_source_destroy(&self, _level: &ServerLevel, _source: &DamageSource) -> bool {
        false
    }

    /// `destroy(level, source)` (chest boats and chest minecarts override it to drop their contents too).
    fn destroy(&mut self, level: &mut ServerLevel, _source: &DamageSource) {
        let item = self.get_drop_item();
        destroy_item(self, level, item);
    }
}

/// `VehicleEntity.hurtServer(level, source, damage)`.
pub fn hurt_server<E: VehicleEntity + ?Sized>(
    e: &mut E,
    level: &mut ServerLevel,
    source: &DamageSource,
    damage: f32,
) -> bool {
    if e.is_removed() {
        return true;
    }
    if e.is_invulnerable_to_base(level, source) {
        return false;
    }
    let hurt_dir = get_hurt_dir(e.base());
    set_hurt_dir(e.base_mut(), -hurt_dir);
    set_hurt_time(e.base_mut(), 10);
    e.mark_hurt();
    let new_damage = get_damage(e.base()) + damage * 10.0;
    set_damage(e.base_mut(), new_damage);
    level.game_event_from_entity(
        source.get_entity(),
        GameEvent::ENTITY_DAMAGE,
        e.base().position(),
    );
    let creative_player = source.is_creative_player();
    if (creative_player || !(get_damage(e.base()) > 40.0))
        && !e.should_source_destroy(level, source)
    {
        if creative_player {
            level.remove_held_entity(
                e.as_dyn_mut(),
                crate::world::entity::RemovalReason::Discarded,
            );
        }
    } else {
        e.destroy(level, source);
    }
    true
}

/// `VehicleEntity.destroy(level, dropItem)`: kill, then (rule `entity_drops`) drop the item carrying the custom name.
pub fn destroy_item<E: Entity + ?Sized>(e: &mut E, level: &mut ServerLevel, drop_item: Item) {
    e.kill_in_level(level);
    if level.entity_drops() {
        let mut stack = ItemStack::new(drop_item);
        if let Some(name) = e.base().get_custom_name() {
            stack.set(CUSTOM_NAME, name);
        }
        spawn_at_location(level, e.as_dyn(), stack, Vec3::ZERO);
    }
}

/// `instanceof Mob` (a living entity that is neither a player, an armor stand nor a mannequin).
pub fn is_mob(e: &dyn Entity) -> bool {
    e.living().is_some()
        && !e.is_player()
        && !matches!(
            e.get_type(),
            EntityType::ARMOR_STAND | EntityType::MANNEQUIN
        )
}

pub fn ignore_explosion(level: &ServerLevel, explosion: &dyn Explosion) -> bool {
    let Some(indirect) = explosion.get_indirect_source_entity() else {
        return false;
    };
    // the exploding entity usually runs outside the level
    let source = level.get_entity_or_lent(indirect);
    source.is_some_and(is_mob) && !level.mob_griefing()
}
