//! `JumpControl`.

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use std::any::Any;

#[derive(Default)]
pub struct JumpControlBase {
    pub jump: bool,
}

/// `JumpControl` (subclass: `Rabbit.RabbitJumpControl`).
pub trait JumpControl: Any {
    fn base(&self) -> &JumpControlBase;
    fn base_mut(&mut self) -> &mut JumpControlBase;

    /// `jump()`.
    fn jump(&mut self) {
        self.base_mut().jump = true;
    }

    /// `tick()`.
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        base_tick(self.base_mut(), mob, level);
    }
}

/// `JumpControl.tick()`.
pub fn base_tick(base: &mut JumpControlBase, mob: &mut dyn Entity, level: &mut ServerLevel) {
    mob.set_jumping(level, base.jump);
    base.jump = false;
}

/// The plain vanilla `JumpControl`.
#[derive(Default)]
pub struct StandardJumpControl {
    base: JumpControlBase,
}

impl JumpControl for StandardJumpControl {
    fn base(&self) -> &JumpControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut JumpControlBase {
        &mut self.base
    }
}
