//! `WorkAtPoi`: stand at the job site, play the work sound, restock.

use super::position_tracker::{BlockPosTracker, PositionTracker};
use super::{Behavior, BehaviorBase, DEFAULT_DURATION};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::memory_module_type::{
    JOB_SITE, LAST_WORKED_AT_POI, LOOK_TARGET,
};
use crate::world::entity::npc::villager::villager_mut;
use crate::world::entity::npc::villager::villager_profession as profession_data;
use crate::world::level::LevelAccessor as _;
use mc_core::random::RandomSource;

const CHECK_COOLDOWN: i64 = 300;
const DISTANCE: f64 = 1.73;

/// `useWorkstation(level, body)` of the concrete class.
pub type UseWorkstation = fn(&mut ServerLevel, &mut dyn Entity);

pub struct WorkAtPoi {
    last_check: i64,
    use_workstation: UseWorkstation,
}

fn no_workstation(_: &mut ServerLevel, _: &mut dyn Entity) {}

impl WorkAtPoi {
    /// `new WorkAtPoi()`.
    pub fn new() -> BehaviorBase<WorkAtPoi> {
        Self::with_workstation(no_workstation)
    }

    /// The constructor of a subclass that overrides `useWorkstation`.
    pub fn with_workstation(use_workstation: UseWorkstation) -> BehaviorBase<WorkAtPoi> {
        BehaviorBase::with_duration(
            entry_condition![JOB_SITE => ValuePresent, LOOK_TARGET => Registered],
            DEFAULT_DURATION,
            DEFAULT_DURATION,
            WorkAtPoi {
                last_check: 0,
                use_workstation,
            },
        )
    }
}

impl Behavior for WorkAtPoi {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        if level.game_time() - self.last_check < CHECK_COOLDOWN {
            false
        } else if level.random().next_int_bounded(2) != 0 {
            false
        } else {
            self.last_check = level.game_time();
            let Some(target) = body.brain().get_memory(&JOB_SITE) else {
                return false;
            };
            target.dimension == level.dimension
                && target.pos.closer_to_center_than(&body.position(), DISTANCE)
        }
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        body.brain_mut().set_memory(&LAST_WORKED_AT_POI, timestamp);
        if let Some(pos) = body.brain().get_memory(&JOB_SITE).map(|g| g.pos) {
            body.brain_mut().set_memory(
                &LOOK_TARGET,
                PositionTracker::Block(BlockPosTracker::new(pos)),
            );
        }
        // `body.playWorkSound()`
        let villager = villager_mut(body);
        if let Some(sound) =
            profession_data::data(villager.get_villager_data().profession).work_sound
        {
            crate::world::entity::living_entity::damage::make_sound(villager, level, sound);
        }
        (self.use_workstation)(level, body);
        let villager = villager_mut(body);
        if villager.should_restock(level) {
            villager.restock(level);
        }
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        match body.brain().get_memory(&JOB_SITE) {
            None => false,
            Some(target) => {
                target.dimension == level.dimension
                    && target.pos.closer_to_center_than(&body.position(), DISTANCE)
            }
        }
    }
}
