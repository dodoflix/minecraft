//! `TemptGoal` (+ `TemptGoal.ForNonPathfinders`).

use mc_core::random::RandomSource;
use super::{Flag, Goal, GoalFlags, reduced_tick_delay};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::living_entity::util::l;
use crate::world::entity::mob::{self, ext, navigation};
use crate::world::entity::{Entity, EntityId};
use crate::world::item::ItemStack;
use mc_data::Attribute;
use std::sync::Arc;

pub const DEFAULT_STOP_DISTANCE: f64 = 2.5;

/// `Predicate<ItemStack>`.
pub type ItemPredicate = Arc<dyn Fn(&ItemStack) -> bool + Send + Sync>;

pub struct TemptGoal {
    targeting_conditions: TargetingConditions,
    speed_modifier: f64,
    px: f64,
    py: f64,
    pz: f64,
    p_rot_x: f64,
    p_rot_y: f64,
    player: Option<EntityId>,
    calm_down: i32,
    is_running: bool,
    can_scare: bool,
    stop_distance: f64,
    /// `ForNonPathfinders`: wait/wander through the move control instead of the navigation.
    non_pathfinder: bool,
}

impl TemptGoal {
    /// `TemptGoal(mob, speedModifier, items, canScare)`.
    pub fn new(speed_modifier: f64, items: ItemPredicate, can_scare: bool) -> Self {
        Self::with_stop_distance(speed_modifier, items, can_scare, DEFAULT_STOP_DISTANCE)
    }

    /// `TemptGoal(mob, speedModifier, items, canScare, stopDistance)`.
    pub fn with_stop_distance(speed_modifier: f64, items: ItemPredicate, can_scare: bool, stop_distance: f64) -> Self {
        Self::build(speed_modifier, items, can_scare, stop_distance, false)
    }

    /// `TemptGoal.ForNonPathfinders(mob, speedModifier, items, canScare, stopDistance)`.
    pub fn for_non_pathfinders(speed_modifier: f64, items: ItemPredicate, can_scare: bool, stop_distance: f64) -> Self {
        Self::build(speed_modifier, items, can_scare, stop_distance, true)
    }

    fn build(speed_modifier: f64, items: ItemPredicate, can_scare: bool, stop_distance: f64, non_pathfinder: bool) -> Self {
        // `shouldFollow(player)`: main hand or offhand matches
        let selector = Arc::new(move |target: &dyn Entity, _level: &ServerLevel| items(&target.get_item_by_slot(EquipmentSlot::MainHand)) || items(&target.get_item_by_slot(EquipmentSlot::OffHand)));
        Self {
            targeting_conditions: TargetingConditions::for_non_combat().ignore_line_of_sight().selector(Some(selector)),
            speed_modifier,
            px: 0.0,
            py: 0.0,
            pz: 0.0,
            p_rot_x: 0.0,
            p_rot_y: 0.0,
            player: None,
            calm_down: 0,
            is_running: false,
            can_scare,
            stop_distance,
            non_pathfinder,
        }
    }

    /// `isRunning()`.
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// `player` (the tempting player of the current `canUse`).
    pub fn player(&self) -> Option<EntityId> {
        self.player
    }

    /// `canContinueToUse()` up to the final `return this.canUse()` (subclasses override `canScare()`/`canUse()`):
    /// `false` when the scare conditions end the goal.
    pub fn scare_check(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel, can_scare: bool) -> bool {
        if can_scare {
            let Some(player) = self.player.and_then(|p| level.get_entity(p)) else { return false };
            if mob.base().distance_to_sqr_vec(player.position()) < 36.0 {
                if player.base().distance_to_sqr_xyz(self.px, self.py, self.pz) > 0.010000000000000002 {
                    return false;
                }
                if (f64::from(player.base().x_rot()) - self.p_rot_x).abs() > 5.0 || (f64::from(player.base().y_rot()) - self.p_rot_y).abs() > 5.0 {
                    return false;
                }
            } else {
                self.px = player.base().x();
                self.py = player.base().y();
                self.pz = player.base().z();
            }
            self.p_rot_x = f64::from(player.base().x_rot());
            self.p_rot_y = f64::from(player.base().y_rot());
        }
        true
    }

    /// `canScare()`.
    pub fn can_scare(&self) -> bool {
        self.can_scare
    }

    fn stop_navigation(&self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if self.non_pathfinder {
            if let Some(m) = mob.mob_mut() {
                m.move_control_mut().set_wait();
            }
        } else {
            navigation::stop(mob, level);
        }
    }

    fn navigate_towards(&self, mob: &mut dyn Entity, level: &mut ServerLevel, player: EntityId) {
        let Some(p) = level.get_entity(player) else { return };
        if self.non_pathfinder {
            let eye = p.base().get_eye_position();
            let pos = mob.position();
            let r = mob.base_mut().random.next_double();
            let target = eye.subtract(pos).scale(r).add(pos);
            mob::set_wanted_position(mob, target.x, target.y, target.z, self.speed_modifier);
        } else {
            let pos = p.block_position();
            navigation::move_to_entity(mob, level, pos, self.speed_modifier);
        }
    }
}

impl Goal for TemptGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if self.calm_down > 0 {
            self.calm_down -= 1;
            return false;
        }
        // `targetingConditions.range(...)` mutates the shared conditions
        self.targeting_conditions = self.targeting_conditions.clone().range(l(mob).get_attribute_value(Attribute::TEMPT_RANGE));
        let pos = mob.position();
        let players: Vec<EntityId> = level.players().to_vec();
        self.player = ext::get_nearest_entity(level, &players, &self.targeting_conditions, mob, pos.x, pos.y, pos.z);
        self.player.is_some()
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.scare_check(mob, level, self.can_scare) && self.can_use(mob, level)
    }

    fn start(&mut self, _mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(player) = self.player.and_then(|p| level.get_entity(p)) {
            self.px = player.base().x();
            self.py = player.base().y();
            self.pz = player.base().z();
        }
        self.is_running = true;
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.player = None;
        self.stop_navigation(mob, level);
        self.calm_down = reduced_tick_delay(100);
        self.is_running = false;
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(player_id) = self.player else { return };
        let (y_max, x_max) = (mob.get_max_head_y_rot() + 20, mob.get_max_head_x_rot());
        let Some(player) = level.get_entity(player_id) else { return };
        mob::look_at_entity_full(mob, player, y_max as f32, x_max as f32);
        if mob.base().distance_to_sqr_vec(player.position()) < self.stop_distance * self.stop_distance {
            self.stop_navigation(mob, level);
        } else {
            self.navigate_towards(mob, level, player_id);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}
