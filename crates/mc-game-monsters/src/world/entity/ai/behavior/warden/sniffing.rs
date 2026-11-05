//! `Sniffing`.

use super::{warden, warden_mut};
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, DISTURBANCE_LOCATION, IS_SNIFFING, LOOK_TARGET, NEAREST_ATTACKABLE, SNIFF_COOLDOWN, WALK_TARGET,
};
use crate::world::entity::monster::warden::warden_ai;
use mc_data::SoundEvent;
use mc_data::enums::Pose;

const ANGER_FROM_SNIFFING_MAX_DISTANCE_XZ: f64 = 6.0;
const ANGER_FROM_SNIFFING_MAX_DISTANCE_Y: f64 = 20.0;

/// `Sniffing<E extends Warden>`.
pub struct Sniffing;

impl Sniffing {
    /// `new Sniffing(ticks)`.
    pub fn new(ticks: i32) -> BehaviorBase<Sniffing> {
        BehaviorBase::with_timeout(
            entry_condition![
                IS_SNIFFING => ValuePresent,
                ATTACK_TARGET => ValueAbsent,
                WALK_TARGET => ValueAbsent,
                LOOK_TARGET => Registered,
                NEAREST_ATTACKABLE => Registered,
                DISTURBANCE_LOCATION => Registered,
                SNIFF_COOLDOWN => Registered
            ],
            ticks,
            Sniffing,
        )
    }
}

impl Behavior for Sniffing {
    fn can_still_use(&mut self, _level: &mut ServerLevel, _body: &mut dyn Entity, _timestamp: i64) -> bool {
        true
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.play_sound(level, SoundEvent::ENTITY_WARDEN_SNIFF, 5.0, 1.0);
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if body.base().get_pose() == Pose::Sniffing {
            body.set_pose(Pose::Standing);
        }
        body.brain_mut().erase_memory(&IS_SNIFFING);
        let attackable = body
            .brain()
            .get_memory(&NEAREST_ATTACKABLE)
            .map(|r| r.0)
            .filter(|id| level.get_entity(*id).is_some_and(|e| warden(body).can_target_entity_ref(level, e)));
        if let Some(entity_id) = attackable {
            let (close, block_pos) = {
                let entity = level.get_entity(entity_id).expect("checked above");
                (crate::world::entity::monster::warden::closer_than_xz_y(body, entity, ANGER_FROM_SNIFFING_MAX_DISTANCE_XZ, ANGER_FROM_SNIFFING_MAX_DISTANCE_Y), entity.block_position())
            };
            if close {
                warden_mut(body).increase_anger_at_default(level, Some(entity_id));
            }
            if !body.brain().has_memory_value(DISTURBANCE_LOCATION) {
                warden_ai::set_disturbance_location(level, warden_mut(body), block_pos);
            }
        }
    }
}
