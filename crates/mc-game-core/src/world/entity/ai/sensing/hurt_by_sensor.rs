//! `HurtBySensor`: `HURT_BY` / `HURT_BY_ENTITY` from the body's last damage source.

use super::sensor::Sensor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::{HURT_BY, HURT_BY_ENTITY};
use crate::world::entity::ai::memory::{EntityRef, MemoryTypeId, SharedDamageSource};
use crate::world::entity::entity::Entity;
use std::sync::Arc;

pub struct HurtBySensor;

/// `LivingEntity.getLastDamageSource()` (`getLastDamageSource(40)`).
pub fn get_last_damage_source(
    level: &ServerLevel,
    body: &dyn Entity,
) -> Option<Arc<crate::world::damagesource::DamageSource>> {
    let l = body.living()?;
    if level.game_time() - l.last_damage_stamp > 40 {
        None
    } else {
        l.last_damage_source.clone().map(Arc::new)
    }
}

impl Sensor for HurtBySensor {
    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![HURT_BY.id(), HURT_BY_ENTITY.id()]
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        let source = get_last_damage_source(level, &*body);
        match source {
            Some(source) => {
                // `damageSource.getEntity() instanceof LivingEntity`
                let causing = source
                    .causing_entity
                    .filter(|&id| level.get_entity(id).is_some_and(|e| e.living().is_some()));
                let brain = body.brain_mut();
                brain.set_memory(&HURT_BY, SharedDamageSource(source));
                if let Some(id) = causing {
                    brain.set_memory(&HURT_BY_ENTITY, EntityRef(id));
                }
            }
            None => body.brain_mut().erase_memory(&HURT_BY),
        }
        if let Some(&EntityRef(hurt_by)) = body.brain().get_memory(&HURT_BY_ENTITY) {
            // `!isAlive() || level() != level` (a removed entity is gone from this level)
            if !level.get_entity(hurt_by).is_some_and(|e| e.is_alive()) {
                body.brain_mut().erase_memory(&HURT_BY_ENTITY);
            }
        }
    }
}
