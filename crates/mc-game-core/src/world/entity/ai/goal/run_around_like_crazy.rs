//! `RunAroundLikeCrazyGoal`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::util::default_random_pos;
use crate::world::entity::mob::navigation;
use crate::world::entity::Entity;
use mc_core::random::RandomSource;

pub struct RunAroundLikeCrazyGoal {
    speed_modifier: f64,
    pos_x: f64,
    pos_y: f64,
    pos_z: f64,
}

impl RunAroundLikeCrazyGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self { speed_modifier, pos_x: 0.0, pos_y: 0.0, pos_z: 0.0 }
    }
}

impl Goal for RunAroundLikeCrazyGoal {
    fn can_use(&mut self, horse: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(h) = horse.as_abstract_horse() else { return false };
        if !h.is_mob_controlled(level) && !h.is_tamed() && horse.base().is_vehicle() {
            let Some(pos) = default_random_pos::get_pos(horse, level, 5, 4) else { return false };
            self.pos_x = pos.x;
            self.pos_y = pos.y;
            self.pos_z = pos.z;
            true
        } else {
            false
        }
    }

    fn start(&mut self, horse: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::move_to(horse, level, self.pos_x, self.pos_y, self.pos_z, self.speed_modifier);
    }

    fn can_continue_to_use(&mut self, horse: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        horse.as_abstract_horse().is_some_and(|h| !h.is_tamed()) && !navigation::is_done(horse, level) && horse.base().is_vehicle()
    }

    fn tick(&mut self, horse: &mut dyn Entity, level: &mut ServerLevel) {
        let tamed = horse.as_abstract_horse().is_some_and(|h| h.is_tamed());
        let bound = self.adjusted_tick_delay(50);
        if !tamed && horse.base_mut().random.next_int_bounded(bound) == 0 {
            let Some(passenger_id) = horse.base().passengers.first().copied() else { return };
            if level.get_entity(passenger_id).is_some_and(|p| p.is_player()) {
                let (temper, max_temper) = horse.as_abstract_horse().map_or((0, 0), |h| (h.get_temper(), h.get_max_temper()));
                if max_temper > 0 && horse.base_mut().random.next_int_bounded(max_temper) < temper {
                    if let Some(h) = horse.as_abstract_horse_mut() {
                        h.tame_with_name(level, passenger_id);
                    }
                    return;
                }
                if let Some(h) = horse.as_abstract_horse_mut() {
                    h.modify_temper(5);
                }
            }
            level.eject_passengers(horse);
            if let Some(h) = horse.as_abstract_horse_mut() {
                h.make_mad(level);
            }
            level.broadcast_entity_event(horse.id(), 6);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}
