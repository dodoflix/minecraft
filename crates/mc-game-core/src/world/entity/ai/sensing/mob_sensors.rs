//! Sensors of single mob families: `AxolotlAttackablesSensor`, `FrogAttackablesSensor`, `WardenEntitySensor`,
//! `BreezeAttackEntitySensor`.

use super::nearest_living_entity_sensor::scan_nearest_living_entities;
use super::nearest_visible_living_entity_sensor::NearestVisibleLivingEntityMatcher;
use super::sensor::{Sensor, is_entity_attackable};
use super::{closer_than, distance_to_sqr};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::{
    HAS_HUNTING_COOLDOWN, NEAREST_ATTACKABLE, NEAREST_LIVING_ENTITIES, UNREACHABLE_TONGUE_TARGETS,
};
use crate::world::entity::ai::memory::{EntityRef, MemoryModuleType, MemoryTypeId};
use crate::world::entity::entity::Entity;
use mc_data::EntityType;

fn entity_type_tag(tag: &str, t: EntityType) -> bool {
    mc_data::tags::get::<EntityType>(tag).is_some_and(|tag| tag.contains(t))
}

/// `AxolotlAttackablesSensor` (a `NearestVisibleLivingEntitySensor`).
pub struct AxolotlAttackablesSensor;

impl NearestVisibleLivingEntityMatcher for AxolotlAttackablesSensor {
    fn is_matching_entity(&self, level: &ServerLevel, body: &dyn Entity, mob: &dyn Entity) -> bool {
        let is_close = distance_to_sqr(mob, body) <= 64.0;
        is_close
            && mob.is_in_water()
            && (entity_type_tag("axolotl_always_hostiles", mob.get_type())
                || (!body.brain().has_memory_value(HAS_HUNTING_COOLDOWN)
                    && entity_type_tag("axolotl_hunt_targets", mob.get_type())))
            && is_entity_attackable(level, body, mob)
    }

    fn get_memory_to_set(&self) -> MemoryModuleType<EntityRef> {
        NEAREST_ATTACKABLE
    }

    fn extra_requires(&self) -> Vec<MemoryTypeId> {
        vec![HAS_HUNTING_COOLDOWN.id()]
    }
}

/// `FrogAttackablesSensor` (a `NearestVisibleLivingEntitySensor`).
pub struct FrogAttackablesSensor;

/// `Frog.canEat(entity)`: slimes and magma cubes only when size 1, else `FROG_FOOD`.
pub fn frog_can_eat(entity: &dyn Entity) -> bool {
    match entity.cube_mob_size() {
        Some(size) if size != 1 => false,
        _ => entity_type_tag("frog_food", entity.get_type()),
    }
}

impl NearestVisibleLivingEntityMatcher for FrogAttackablesSensor {
    fn is_matching_entity(&self, level: &ServerLevel, body: &dyn Entity, mob: &dyn Entity) -> bool {
        if is_entity_attackable(level, body, mob)
            && frog_can_eat(mob)
            && !self.is_unreachable_attack_target(body, mob)
        {
            closer_than(mob, body, 10.0)
        } else {
            false
        }
    }

    fn get_memory_to_set(&self) -> MemoryModuleType<EntityRef> {
        NEAREST_ATTACKABLE
    }

    fn extra_requires(&self) -> Vec<MemoryTypeId> {
        vec![UNREACHABLE_TONGUE_TARGETS.id()]
    }
}

impl FrogAttackablesSensor {
    fn is_unreachable_attack_target(&self, body: &dyn Entity, mob: &dyn Entity) -> bool {
        body.brain()
            .get_memory(&UNREACHABLE_TONGUE_TARGETS)
            .is_some_and(|l| l.contains(&mob.uuid()))
    }
}

fn nearest_attackable_requires() -> Vec<MemoryTypeId> {
    vec![
        NEAREST_LIVING_ENTITIES.id(),
        crate::world::entity::ai::memory::memory_module_type::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
        NEAREST_ATTACKABLE.id(),
    ]
}

/// `WardenEntitySensor extends NearestLivingEntitySensor<Warden>`.
pub struct WardenEntitySensor;

impl Sensor for WardenEntitySensor {
    fn requires(&self) -> Vec<MemoryTypeId> {
        nearest_attackable_requires()
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        scan_nearest_living_entities(level, body);
        let closest = |test: &dyn Fn(&dyn Entity) -> bool| -> Option<EntityRef> {
            body.brain()
                .get_memory(&NEAREST_LIVING_ENTITIES)?
                .iter()
                .copied()
                .find(|r| {
                    level
                        .get_entity(r.0)
                        .is_some_and(|e| body.can_target_entity(level, e) && test(e))
                })
        };
        let target = closest(&|e| e.get_type() == EntityType::PLAYER)
            .or_else(|| closest(&|e| e.get_type() != EntityType::PLAYER));
        body.brain_mut().set_memory_opt(&NEAREST_ATTACKABLE, target);
    }
}

/// `BreezeAttackEntitySensor extends NearestLivingEntitySensor<Breeze>`.
pub struct BreezeAttackEntitySensor;

/// `EntitySelector.NO_CREATIVE_OR_SPECTATOR`.
pub fn no_creative_or_spectator(entity: &dyn Entity) -> bool {
    !entity.is_player()
        || (!entity.is_spectator() && !entity.player().is_some_and(|p| p.abilities.instabuild))
}

impl Sensor for BreezeAttackEntitySensor {
    fn requires(&self) -> Vec<MemoryTypeId> {
        nearest_attackable_requires()
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        scan_nearest_living_entities(level, body);
        let target = body
            .brain()
            .get_memory(&NEAREST_LIVING_ENTITIES)
            .and_then(|l| {
                l.iter().copied().find(|r| {
                    level.get_entity(r.0).is_some_and(|e| {
                        no_creative_or_spectator(e) && is_entity_attackable(level, &*body, e)
                    })
                })
            });
        body.brain_mut().set_memory_opt(&NEAREST_ATTACKABLE, target);
    }
}
