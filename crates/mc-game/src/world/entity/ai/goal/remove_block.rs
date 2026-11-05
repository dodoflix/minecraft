//! `RemoveBlockGoal` (a `MoveToBlockGoal`); `ZombieAttackTurtleEggGoal` supplies [`RemoveBlockHooks`].

use super::move_to_block::{self, MoveToBlock, MoveToBlockData};
use super::{Goal, GoalFlags, reduced_tick_delay};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::level::BlockGetter;
use crate::world::level::LevelAccessor;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::{Item, ParticleType};
use mc_protocol::components::DataComponentPatch;
use mc_protocol::components::ItemStackTemplate;
use mc_protocol::packets::play::entity_data::ParticleOptions;

pub const WAIT_AFTER_BLOCK_FOUND: i32 = 20;

/// The methods `RemoveBlockGoal` subclasses override.
pub trait RemoveBlockHooks: Send + Sync {
    /// `acceptedDistance()`.
    fn accepted_distance(&self) -> f64 {
        1.0
    }
    /// `playDestroyProgressSound(level, pos)`.
    fn play_destroy_progress_sound(&self, _level: &mut ServerLevel, _pos: BlockPos) {}
    /// `playBreakSound(level, pos)`.
    fn play_break_sound(&self, _level: &mut ServerLevel, _pos: BlockPos) {}
}

/// The base class' no-op hooks.
pub struct NoRemoveBlockHooks;
impl RemoveBlockHooks for NoRemoveBlockHooks {}

pub struct RemoveBlockGoal {
    mtb: MoveToBlockData,
    block_to_remove: Block,
    ticks_since_reached_goal: i32,
    hooks: Box<dyn RemoveBlockHooks>,
}

impl RemoveBlockGoal {
    /// `RemoveBlockGoal(blockToRemove, mob, speedModifier, verticalSearchRange)`.
    pub fn new(block_to_remove: Block, speed_modifier: f64, vertical_search_range: i32) -> Self {
        Self::with_hooks(block_to_remove, speed_modifier, vertical_search_range, Box::new(NoRemoveBlockHooks))
    }

    pub fn with_hooks(block_to_remove: Block, speed_modifier: f64, vertical_search_range: i32, hooks: Box<dyn RemoveBlockHooks>) -> Self {
        Self { mtb: MoveToBlockData::with_vertical(speed_modifier, 24, vertical_search_range), block_to_remove, ticks_since_reached_goal: 0, hooks }
    }

    /// `getPosWithBlock(pos, level)`.
    fn get_pos_with_block(&self, pos: BlockPos, level: &ServerLevel) -> Option<BlockPos> {
        if level.get_block_state(pos).block() == self.block_to_remove {
            return Some(pos);
        }
        let neighbours = [pos.below(), pos.west(), pos.east(), pos.north(), pos.south(), pos.below().below()];
        neighbours.into_iter().find(|n| level.get_block_state(*n).block() == self.block_to_remove)
    }
}

impl MoveToBlock for RemoveBlockGoal {
    fn mtb(&self) -> &MoveToBlockData {
        &self.mtb
    }

    fn mtb_mut(&mut self) -> &mut MoveToBlockData {
        &mut self.mtb
    }

    fn accepted_distance(&self) -> f64 {
        self.hooks.accepted_distance()
    }

    fn is_valid_target(&self, _mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
        // `level.getChunk(x >> 4, z >> 4, FULL, false)` is null for unloaded chunks
        if level.chunk_source.get_chunk_now(pos.x() >> 4, pos.z() >> 4).is_none() {
            return false;
        }
        level.get_block_state(pos).block() == self.block_to_remove && level.get_block_state(pos.above()).is_air() && level.get_block_state(pos.above_n(2)).is_air()
    }
}

impl Goal for RemoveBlockGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !level.mob_griefing() {
            false
        } else if self.mtb.next_start_tick > 0 {
            self.mtb.next_start_tick -= 1;
            false
        } else if self.find_nearest_block(mob, level) {
            self.mtb.next_start_tick = reduced_tick_delay(20);
            true
        } else {
            self.mtb.next_start_tick = self.next_start_tick(mob);
            false
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        move_to_block::can_continue_to_use(self, mob, level)
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        // `super.stop()` is `Goal.stop()` (no-op)
        mob.base_mut().fall_distance = 1.0;
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        move_to_block::start(self, mob, level);
        self.ticks_since_reached_goal = 0;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        move_to_block::tick(self, mob, level);
        let mob_pos = mob.block_position();
        let eat_pos = self.get_pos_with_block(mob_pos, level);
        if let (true, Some(eat_pos)) = (self.is_reached_target(), eat_pos) {
            if self.ticks_since_reached_goal > 0 {
                let movement = mob.base().delta_movement();
                mob.base_mut().set_delta_movement(Vec3::new(movement.x, 0.3, movement.z));
                let random = &mut mob.base_mut().random;
                let (xd, yd, zd) = ((f64::from(random.next_float()) - 0.5) * 0.08, (f64::from(random.next_float()) - 0.5) * 0.08, (f64::from(random.next_float()) - 0.5) * 0.08);
                let egg = ParticleOptions::Item(ItemStackTemplate { item: Item::EGG, count: 1, components: DataComponentPatch::default() });
                level.send_particles(egg, f64::from(eat_pos.x()) + 0.5, f64::from(eat_pos.y()) + 0.7, f64::from(eat_pos.z()) + 0.5, 3, xd, yd, zd, f64::from(0.15f32));
            }
            if self.ticks_since_reached_goal % 2 == 0 {
                let movement = mob.base().delta_movement();
                mob.base_mut().set_delta_movement(Vec3::new(movement.x, -0.3, movement.z));
                if self.ticks_since_reached_goal % 6 == 0 {
                    self.hooks.play_destroy_progress_sound(level, self.mtb.block_pos);
                }
            }
            if self.ticks_since_reached_goal > 60 {
                level.remove_block(eat_pos, false);
                for _ in 0..20 {
                    let random = &mut mob.base_mut().random;
                    let (xa, ya, za) = (random.next_gaussian() * 0.02, random.next_gaussian() * 0.02, random.next_gaussian() * 0.02);
                    level.send_particles(ParticleOptions::Simple(ParticleType::POOF), f64::from(eat_pos.x()) + 0.5, f64::from(eat_pos.y()), f64::from(eat_pos.z()) + 0.5, 1, xa, ya, za, f64::from(0.15f32));
                }
                self.hooks.play_break_sound(level, eat_pos);
            }
            self.ticks_since_reached_goal += 1;
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[super::Flag::Move, super::Flag::Jump])
    }
}
