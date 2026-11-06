//! `SpearApproach`: walk up to the target until the spear charge can start.

use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::EntityId;
use mc_game_core::world::entity::ai::behavior::position_tracker::{EntityTracker, PositionTracker};
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::SpearStatus;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, LOOK_TARGET, SPEAR_STATUS,
};
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::living_entity::using_item;
use mc_game_core::world::entity::mob::navigation;
use mc_protocol::components::KINETIC_WEAPON;

pub struct SpearApproach {
    speed_modifier_when_repositioning: f64,
    approach_distance_sq: f32,
}

impl SpearApproach {
    /// `new SpearApproach(speedModifierWhenRepositioning, approachDistance)`.
    pub fn new(
        speed_modifier_when_repositioning: f64,
        approach_distance: f32,
    ) -> BehaviorBase<SpearApproach> {
        BehaviorBase::new(
            entry_condition![SPEAR_STATUS => ValueAbsent],
            SpearApproach {
                speed_modifier_when_repositioning,
                approach_distance_sq: approach_distance * approach_distance,
            },
        )
    }
}

/// `getTarget(mob)`: the attack target.
pub(super) fn get_target(mob: &dyn Entity) -> Option<EntityId> {
    mob.brain().get_memory(&ATTACK_TARGET).map(|r| r.0)
}

/// `getTarget(mob) != null && mob.getMainHandItem().has(KINETIC_WEAPON)`.
pub(super) fn able_to_attack(mob: &dyn Entity) -> bool {
    get_target(mob).is_some()
        && mob
            .get_item_by_slot(EquipmentSlot::MainHand)
            .get(&KINETIC_WEAPON)
            .is_some()
}

/// `mob.getRootVehicle() instanceof Mob vehicleMob ? vehicleMob.chargeSpeedModifier() : 1.0F`.
pub(super) fn root_charge_speed_modifier(level: &ServerLevel, mob: &dyn Entity) -> f32 {
    let root = mob.get_root_vehicle(level);
    if root == mob.id() {
        mob.charge_speed_modifier()
    } else {
        level
            .get_entity(root)
            .filter(|e| e.mob().is_some())
            .map_or(1.0, |e| e.charge_speed_modifier())
    }
}

/// `mob.distanceToSqr(target.getX(), target.getY(), target.getZ())`.
pub(super) fn target_distance_sqr(level: &ServerLevel, mob: &dyn Entity) -> Option<f64> {
    let target = level.get_entity(get_target(mob)?)?;
    let p = target.position();
    Some(mob.base().distance_to_sqr_xyz(p.x, p.y, p.z))
}

impl Behavior for SpearApproach {
    fn check_extra_start_conditions(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        able_to_attack(body) && !using_item::is_using_item(body)
    }

    fn start(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if let Some(mob) = body.mob_mut() {
            mob.set_aggressive(true);
        }
        body.brain_mut()
            .set_memory(&SPEAR_STATUS, SpearStatus::Approach);
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        // `farEnough(mob)`
        able_to_attack(body)
            && target_distance_sqr(level, body)
                .is_some_and(|d| d > f64::from(self.approach_distance_sq))
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let Some(target) = get_target(body).and_then(|t| level.get_entity(t)) else {
            return;
        };
        let speed_modifier = root_charge_speed_modifier(level, body);
        let look = PositionTracker::Entity(EntityTracker::new(target, true));
        let target_pos = target.block_position();
        body.brain_mut().set_memory(&LOOK_TARGET, look);
        navigation::move_to_entity(
            body,
            level,
            target_pos,
            f64::from(speed_modifier) * self.speed_modifier_when_repositioning,
        );
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        navigation::stop(body, level);
        body.brain_mut()
            .set_memory(&SPEAR_STATUS, SpearStatus::Charging);
    }

    fn timed_out(&self, _timestamp: i64, _end_timestamp: i64) -> bool {
        false
    }
}
