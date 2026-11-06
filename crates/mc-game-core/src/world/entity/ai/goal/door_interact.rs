//! `DoorInteractGoal` (abstract; `OpenDoorGoal` and `BreakDoorGoal` embed a [`DoorInteractData`] and call these functions
//! for `super.xyz()`).

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::mob::navigation;
use crate::world::level::BlockGetter;
use crate::world::level::block::helpers::is_instance_of;
use crate::world::level::block::impls::door_block;
use mc_core::pos::BlockPos;
use mc_data::props;

/// The fields of `DoorInteractGoal`.
#[derive(Clone, Debug)]
pub struct DoorInteractData {
    pub door_pos: BlockPos,
    pub has_door: bool,
    passed: bool,
    door_open_dir_x: f32,
    door_open_dir_z: f32,
}

impl DoorInteractData {
    /// `DoorInteractGoal(mob)`.
    ///
    /// # Panics
    /// `IllegalArgumentException("Unsupported mob type for DoorInteractGoal")` when the navigation is not a ground navigation.
    pub fn new(mob: &dyn Entity) -> Self {
        assert!(
            mob.mob()
                .and_then(|m| m.navigation.as_deref())
                .is_some_and(|n| n.can_navigate_ground()),
            "Unsupported mob type for DoorInteractGoal"
        );
        Self {
            door_pos: BlockPos::ZERO,
            has_door: false,
            passed: false,
            door_open_dir_x: 0.0,
            door_open_dir_z: 0.0,
        }
    }

    /// `isOpen()`.
    pub fn is_open(&mut self, level: &ServerLevel) -> bool {
        if !self.has_door {
            false
        } else {
            let state = level.get_block_state(self.door_pos);
            if !is_instance_of(state, "DoorBlock") {
                self.has_door = false;
                false
            } else {
                state.get(props::OPEN)
            }
        }
    }

    /// `setOpen(open)`.
    pub fn set_open(&self, mob: &dyn Entity, level: &mut ServerLevel, open: bool) {
        if self.has_door {
            let state = level.get_block_state(self.door_pos);
            if is_instance_of(state, "DoorBlock") {
                door_block::set_open_at(Some(mob.id()), level, state, self.door_pos, open);
            }
        }
    }

    /// `canUse()`.
    pub fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !navigation::can_navigate_ground(mob, level) {
            return false;
        }
        if !mob.base().horizontal_collision {
            return false;
        }
        let Some(path) = navigation::with_navigation_ref(mob, level, |n| n.get_path()).flatten()
        else {
            return false;
        };
        if path.is_done() {
            return false;
        }
        let limit = ((path.get_next_node_index() + 2) as usize).min(path.get_node_count());
        for i in 0..limit {
            let node = path.get_node(i);
            self.door_pos = BlockPos::new(node.x, node.y + 1, node.z);
            if !(mob.base().distance_to_sqr_xyz(
                f64::from(self.door_pos.x()),
                mob.base().y(),
                f64::from(self.door_pos.z()),
            ) > 2.25)
            {
                self.has_door = door_block::is_wooden_door(level.get_block_state(self.door_pos));
                if self.has_door {
                    return true;
                }
            }
        }
        self.door_pos = mob.block_position().above();
        self.has_door = door_block::is_wooden_door(level.get_block_state(self.door_pos));
        self.has_door
    }

    /// `canContinueToUse()`.
    pub fn can_continue_to_use(&self) -> bool {
        !self.passed
    }

    /// `start()`.
    pub fn start(&mut self, mob: &dyn Entity) {
        self.passed = false;
        self.door_open_dir_x = (f64::from(self.door_pos.x()) + 0.5 - mob.base().x()) as f32;
        self.door_open_dir_z = (f64::from(self.door_pos.z()) + 0.5 - mob.base().z()) as f32;
    }

    /// `tick()`.
    pub fn tick(&mut self, mob: &dyn Entity) {
        let new_door_dir_x = (f64::from(self.door_pos.x()) + 0.5 - mob.base().x()) as f32;
        let new_door_dir_z = (f64::from(self.door_pos.z()) + 0.5 - mob.base().z()) as f32;
        let dot = self.door_open_dir_x * new_door_dir_x + self.door_open_dir_z * new_door_dir_z;
        if dot < 0.0 {
            self.passed = true;
        }
    }
}
