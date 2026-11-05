//! `PlayerSensor`: nearby players (sorted), the visible ones and the attackable ones.

use super::sensor::{Sensor, is_entity_attackable, is_entity_targetable};
use super::{closer_than, distance_to_sqr};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::{NEAREST_PLAYERS, NEAREST_VISIBLE_ATTACKABLE_PLAYER, NEAREST_VISIBLE_ATTACKABLE_PLAYERS, NEAREST_VISIBLE_PLAYER};
use crate::world::entity::ai::memory::{EntityRef, MemoryTypeId};
use crate::world::entity::entity::Entity;
use mc_data::Attribute;

pub struct PlayerSensor;

impl PlayerSensor {
    /// `getFollowDistance(body)`.
    fn get_follow_distance(body: &dyn Entity) -> f64 {
        body.living().map_or(0.0, |l| l.get_attribute_value(Attribute::FOLLOW_RANGE))
    }
}

impl Sensor for PlayerSensor {
    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![NEAREST_PLAYERS.id(), NEAREST_VISIBLE_PLAYER.id(), NEAREST_VISIBLE_ATTACKABLE_PLAYER.id(), NEAREST_VISIBLE_ATTACKABLE_PLAYERS.id()]
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        let follow = Self::get_follow_distance(&*body);
        let body_ref: &dyn Entity = &*body;
        // `EntitySelector.NO_SPECTATORS`, `closerThan`, then a stable sort by squared distance
        let mut players: Vec<EntityRef> = level
            .players()
            .iter()
            .filter_map(|&id| level.get_entity(id))
            .filter(|p| !p.is_spectator() && closer_than(body_ref, *p, follow))
            .map(|p| EntityRef(p.id()))
            .collect();
        let distance = |r: &EntityRef| level.get_entity(r.0).map_or(f64::NAN, |p| distance_to_sqr(body_ref, p));
        players.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
        let visible: Vec<EntityRef> = players.iter().copied().filter(|r| level.get_entity(r.0).is_some_and(|p| is_entity_targetable(level, body_ref, p))).collect();
        let attackable: Vec<EntityRef> = visible.iter().copied().filter(|r| level.get_entity(r.0).is_some_and(|p| is_entity_attackable(level, body_ref, p))).collect();
        let brain = body.brain_mut();
        brain.set_memory(&NEAREST_PLAYERS, players);
        brain.set_memory_opt(&NEAREST_VISIBLE_PLAYER, visible.first().copied());
        brain.set_memory(&NEAREST_VISIBLE_ATTACKABLE_PLAYERS, attackable.clone());
        brain.set_memory_opt(&NEAREST_VISIBLE_ATTACKABLE_PLAYER, attackable.first().copied());
    }
}
