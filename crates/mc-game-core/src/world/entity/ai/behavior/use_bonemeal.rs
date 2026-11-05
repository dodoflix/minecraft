//! `UseBonemeal`: villagers with bone meal grow crops around them.

use mc_core::random::RandomSource;
use crate::world::container::Container;
use super::position_tracker::{BlockPosTracker, PositionTracker};
use super::{Behavior, BehaviorBase, DEFAULT_DURATION};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::memory::memory_module_type::{LOOK_TARGET, WALK_TARGET};
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::npc::villager::villager_mut;
use crate::world::item::{ItemStack, bone_meal_item};
use crate::world::level::BlockGetter;
use crate::world::level::LevelAccessor as _;
use crate::world::level::block::impls::crop_block;
use mc_core::pos::BlockPos;
use mc_data::Item;

const BONEMEALING_DURATION: i32 = 80;

pub struct UseBonemeal {
    next_work_cycle_time: i64,
    last_bonemealing_session: i64,
    time_worked_so_far: i32,
    crop_pos: Option<BlockPos>,
}

impl UseBonemeal {
    pub fn new() -> BehaviorBase<UseBonemeal> {
        BehaviorBase::with_duration(
            entry_condition![LOOK_TARGET => ValueAbsent, WALK_TARGET => ValueAbsent],
            DEFAULT_DURATION,
            DEFAULT_DURATION,
            UseBonemeal { next_work_cycle_time: 0, last_bonemealing_session: 0, time_worked_so_far: 0, crop_pos: None },
        )
    }

    fn pick_next_target(&self, level: &mut ServerLevel, body: &dyn Entity) -> Option<BlockPos> {
        let mut result = None;
        let mut count = 0;
        let body_pos = body.block_position();
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let pos = body_pos.offset(x, y, z);
                    if valid_pos(pos, level) {
                        count += 1;
                        if level.random().next_int_bounded(count) == 0 {
                            result = Some(pos);
                        }
                    }
                }
            }
        }
        result
    }

    fn set_current_crop_as_target(&self, body: &mut dyn Entity) {
        if let Some(pos) = self.crop_pos {
            let tracker = PositionTracker::Block(BlockPosTracker::new(pos));
            body.brain_mut().set_memory(&LOOK_TARGET, tracker.clone());
            body.brain_mut().set_memory(&WALK_TARGET, WalkTarget::new(tracker, 0.5, 1));
        }
    }
}

fn valid_pos(block_pos: BlockPos, level: &ServerLevel) -> bool {
    crop_block::is_max_age(level.get_block_state(block_pos)) == Some(false)
}

impl Behavior for UseBonemeal {
    fn check_extra_start_conditions(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) -> bool {
        let tick_count = i64::from(body.base().tick_count);
        if body.base().tick_count % 10 == 0 && (self.last_bonemealing_session == 0 || self.last_bonemealing_session + 160 <= tick_count) {
            if villager_mut(body).av.inventory.count_item(Item::BONE_MEAL) <= 0 {
                return false;
            }
            self.crop_pos = self.pick_next_target(level, body);
            self.crop_pos.is_some()
        } else {
            false
        }
    }

    fn can_still_use(&mut self, _level: &mut ServerLevel, _body: &mut dyn Entity, _timestamp: i64) -> bool {
        self.time_worked_so_far < BONEMEALING_DURATION && self.crop_pos.is_some()
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.set_current_crop_as_target(body);
        body.set_item_slot(level, EquipmentSlot::MainHand, ItemStack::new(Item::BONE_MEAL));
        self.next_work_cycle_time = timestamp;
        self.time_worked_so_far = 0;
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.set_item_slot(level, EquipmentSlot::MainHand, ItemStack::empty());
        self.last_bonemealing_session = i64::from(body.base().tick_count);
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        let Some(target_pos) = self.crop_pos else { return };
        if timestamp >= self.next_work_cycle_time && target_pos.closer_to_center_than(&body.position(), 1.0) {
            let size = villager_mut(body).av.inventory.get_container_size();
            let mut bonemeal_slot = None;
            for i in 0..size {
                if villager_mut(body).av.inventory.get_item(i).item() == Item::BONE_MEAL && !villager_mut(body).av.inventory.get_item(i).clone().is_empty() {
                    bonemeal_slot = Some(i);
                    break;
                }
            }
            if let Some(slot) = bonemeal_slot {
                let mut stack = villager_mut(body).av.inventory.get_item(slot).clone();
                let grown = bone_meal_item::grow_crop(&mut stack, level, target_pos);
                villager_mut(body).av.inventory.set_item(slot, stack);
                if grown {
                    level.level_event(None, 1505, target_pos, 15);
                    self.crop_pos = self.pick_next_target(level, body);
                    self.set_current_crop_as_target(body);
                    self.next_work_cycle_time = timestamp + 40;
                }
            }
            self.time_worked_so_far += 1;
        }
    }
}
