//! `LookAtPlayerGoal`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::goal::target::EntityClass;
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::mob::{self, ext, tick};
use crate::world::entity::{Entity, EntityId};
use mc_core::random::RandomSource;
use std::sync::Arc;

pub const DEFAULT_PROBABILITY: f32 = 0.02;

/// `lookAtType`: `Player.class` or any living entity class (an `instanceof` test).
#[derive(Clone)]
pub enum LookAtType {
    Player,
    Class(EntityClass),
}

pub struct LookAtPlayerGoal {
    /// `lookAt`.
    pub look_at: Option<EntityId>,
    pub look_distance: f32,
    pub look_time: i32,
    pub probability: f32,
    only_horizontal: bool,
    pub look_at_type: LookAtType,
    pub look_at_context: TargetingConditions,
    /// `setFlags(...)` (`LOOK`; `InteractGoal` adds `MOVE`).
    flags: GoalFlags,
}

impl LookAtPlayerGoal {
    /// `LookAtPlayerGoal(mob, Player.class, lookDistance)`.
    pub fn new_player(mob: &dyn Entity, look_distance: f32) -> Self {
        Self::build(
            mob,
            LookAtType::Player,
            look_distance,
            DEFAULT_PROBABILITY,
            false,
        )
    }

    pub fn new(mob: &dyn Entity, look_at_type: LookAtType, look_distance: f32) -> Self {
        Self::build(mob, look_at_type, look_distance, DEFAULT_PROBABILITY, false)
    }

    pub fn with_probability(
        mob: &dyn Entity,
        look_at_type: LookAtType,
        look_distance: f32,
        probability: f32,
    ) -> Self {
        Self::build(mob, look_at_type, look_distance, probability, false)
    }

    /// `setFlags(flags)` of a subclass constructor.
    pub fn set_flags(&mut self, flags: GoalFlags) {
        self.flags = flags;
    }

    pub fn build(
        mob: &dyn Entity,
        look_at_type: LookAtType,
        look_distance: f32,
        probability: f32,
        only_horizontal: bool,
    ) -> Self {
        let look_at_context = if matches!(look_at_type, LookAtType::Player) {
            // `EntitySelector.notRiding(mob)`
            let mob_id = mob.id();
            TargetingConditions::for_non_combat()
                .range(f64::from(look_distance))
                .selector(Some(Arc::new(
                    move |target: &dyn Entity, level: &ServerLevel| {
                        not_riding(level, mob_id, target)
                    },
                )))
        } else {
            TargetingConditions::for_non_combat().range(f64::from(look_distance))
        };
        Self {
            look_at: None,
            look_distance,
            look_time: 0,
            probability,
            only_horizontal,
            look_at_type,
            look_at_context,
            flags: GoalFlags::of(&[Flag::Look]),
        }
    }
}

/// `EntitySelector.notRiding(entity)`.
fn not_riding(level: &ServerLevel, mob_id: EntityId, input: &dyn Entity) -> bool {
    let mut cur = input.base().vehicle;
    while let Some(v) = cur {
        if v == mob_id {
            return false;
        }
        cur = level
            .entity_manager
            .get_any(v)
            .and_then(|e| e.base().vehicle);
    }
    true
}

impl Goal for LookAtPlayerGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if mob.base_mut().random.next_float() >= self.probability {
            return false;
        }
        if let Some(target) = tick::get_target_checked(mob, level) {
            self.look_at = Some(target);
        }
        let pos = mob.position();
        let eye_y = mob.base().get_eye_y();
        self.look_at = match &self.look_at_type {
            LookAtType::Player => {
                ext::get_nearest_player(level, &self.look_at_context, mob, pos.x, eye_y, pos.z)
            }
            LookAtType::Class(test) => {
                let bb = mob.base().bounding_box().inflate(
                    f64::from(self.look_distance),
                    3.0,
                    f64::from(self.look_distance),
                );
                let candidates =
                    level.get_entities_of_class(&bb, |e| e.living().is_some() && test(e), |_| true);
                ext::get_nearest_entity(
                    level,
                    &candidates,
                    &self.look_at_context,
                    mob,
                    pos.x,
                    eye_y,
                    pos.z,
                )
            }
        };
        self.look_at.is_some()
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(target) = self.look_at.and_then(|t| level.get_entity(t)) else {
            return false;
        };
        if !target.is_alive() {
            return false;
        }
        if mob.base().distance_to_sqr_vec(target.position())
            > f64::from(self.look_distance * self.look_distance)
        {
            return false;
        }
        self.look_time > 0
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        let roll = 40 + mob.base_mut().random.next_int_bounded(40);
        self.look_time = self.adjusted_tick_delay(roll);
    }

    fn stop(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.look_at = None;
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(target) = self.look_at.and_then(|t| level.get_entity(t)) else {
            return;
        };
        if target.is_alive() {
            let target_y = if self.only_horizontal {
                mob.base().get_eye_y()
            } else {
                target.base().get_eye_y()
            };
            mob::look_at_xyz(mob, target.base().x(), target_y, target.base().z());
            self.look_time -= 1;
        }
    }

    fn flags(&self) -> GoalFlags {
        self.flags
    }
}
