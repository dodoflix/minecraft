//! The goals nested in `Rabbit`: `RabbitAvoidEntityGoal`, `RabbitPanicGoal`, `RaidGardenGoal`.

use super::rabbit::Rabbit;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::{GameEvent, props};
use mc_game_ai::world::entity::ai::goal::{AvoidEntityGoal, EntityTest};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::move_to_block::{self, MoveToBlock, MoveToBlockData};
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags, PanicGoal};
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::block::helpers::is_instance_of;
use mc_game_core::world::level::level_access::{BlockGetter, LevelAccessor as _};
use mc_protocol::components::RabbitVariant;
use std::cell::Cell;

/// `Rabbit.RabbitAvoidEntityGoal`: never while a killer bunny.
pub struct RabbitAvoidEntityGoal {
    inner: AvoidEntityGoal,
}

impl RabbitAvoidEntityGoal {
    pub fn new(
        avoid_class: EntityTest,
        max_dist: f32,
        walk_speed_modifier: f64,
        sprint_speed_modifier: f64,
    ) -> Self {
        Self {
            inner: AvoidEntityGoal::new(
                avoid_class,
                max_dist,
                walk_speed_modifier,
                sprint_speed_modifier,
            ),
        }
    }
}

impl Goal for RabbitAvoidEntityGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        mob.as_any()
            .downcast_ref::<Rabbit>()
            .is_some_and(|r| r.get_variant() != RabbitVariant::Evil)
            && self.inner.can_use_impl(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.tick(mob, level);
    }

    fn name(&self) -> &'static str {
        "Rabbit$RabbitAvoidEntityGoal"
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Rabbit.RabbitPanicGoal`: keeps the rabbit's jump speed at the panic speed.
pub struct RabbitPanicGoal {
    inner: PanicGoal,
}

impl RabbitPanicGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            inner: PanicGoal::new(speed_modifier),
        }
    }
}

impl Goal for RabbitPanicGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_use_impl(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let speed = self.inner.speed_modifier;
        if let Some(rabbit) = mob.as_any_mut().downcast_mut::<Rabbit>() {
            rabbit.set_speed_modifier(level, speed);
        }
    }

    fn name(&self) -> &'static str {
        "Rabbit$RabbitPanicGoal"
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }

    fn as_panic_goal(&self) -> Option<&PanicGoal> {
        Some(&self.inner)
    }
}

/// `Rabbit.RaidGardenGoal`.
pub struct RaidGardenGoal {
    data: MoveToBlockData,
    wants_to_raid: bool,
    can_raid: Cell<bool>,
}

impl RaidGardenGoal {
    pub fn new() -> Self {
        Self {
            data: MoveToBlockData::new(f64::from(0.7f32), 16),
            wants_to_raid: false,
            can_raid: Cell::new(false),
        }
    }
}

impl Default for RaidGardenGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl MoveToBlock for RaidGardenGoal {
    fn mtb(&self) -> &MoveToBlockData {
        &self.data
    }

    fn mtb_mut(&mut self) -> &mut MoveToBlockData {
        &mut self.data
    }

    fn is_valid_target(&self, _mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
        let state = level.get_block_state(pos);
        if state.is_tag("supports_crops") && self.wants_to_raid && !self.can_raid.get() {
            let above = level.get_block_state(pos.above());
            if is_instance_of(above, "CarrotBlock") && above.get(props::AGE_7) >= 7 {
                self.can_raid.set(true);
                return true;
            }
        }
        false
    }
}

impl Goal for RaidGardenGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if self.data.next_start_tick <= 0 {
            if !level.mob_griefing() {
                return false;
            }
            self.can_raid.set(false);
            self.wants_to_raid = mob
                .as_any()
                .downcast_ref::<Rabbit>()
                .is_some_and(Rabbit::wants_more_food);
        }
        move_to_block::can_use(self, mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_raid.get() && move_to_block::can_continue_to_use(self, mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        move_to_block::start(self, mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        move_to_block::tick(self, mob, level);
        let block_pos = self.data.block_pos;
        let max_head_x_rot = mob.get_max_head_x_rot() as f32;
        mob.mob_mut()
            .expect("mob")
            .look_control_mut()
            .set_look_at_full(
                f64::from(block_pos.x()) + 0.5,
                f64::from(block_pos.y() + 1),
                f64::from(block_pos.z()) + 0.5,
                10.0,
                max_head_x_rot,
            );
        if self.is_reached_target() {
            let crops_pos = block_pos.above();
            let state = level.get_block_state(crops_pos);
            if self.can_raid.get() && is_instance_of(state, "CarrotBlock") {
                let age = state.get(props::AGE_7);
                if age == 0 {
                    level.set_block_flags(crops_pos, Block::AIR.default_state(), 2);
                    level.expose(&*mob, |level| {
                        level.destroy_block_impl(crops_pos, true, Some(mob.id()), 512)
                    });
                } else {
                    level.set_block_flags(crops_pos, state.set(props::AGE_7, age - 1), 2);
                    level.game_event_from_entity_ref_in_state(
                        &*mob,
                        None,
                        GameEvent::BLOCK_CHANGE,
                        mc_core::phys::Vec3::at_center_of(&crops_pos),
                    );
                    level.level_event_impl(None, 2001, crops_pos, state.id() as i32);
                }
                if let Some(rabbit) = mob.as_any_mut().downcast_mut::<Rabbit>() {
                    rabbit.more_carrot_ticks = 40;
                }
            }
            self.can_raid.set(false);
            self.data.next_start_tick = 10;
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "Rabbit$RaidGardenGoal"
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Jump])
    }
}
