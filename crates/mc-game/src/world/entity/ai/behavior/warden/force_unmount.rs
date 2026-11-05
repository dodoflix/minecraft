//! `ForceUnmount`.

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::behavior::{Behavior, BehaviorBase};

/// `ForceUnmount extends Behavior<LivingEntity>`.
pub struct ForceUnmount;

impl ForceUnmount {
    /// `new ForceUnmount()`.
    pub fn new() -> BehaviorBase<ForceUnmount> {
        BehaviorBase::new(Vec::new(), ForceUnmount)
    }
}

impl Behavior for ForceUnmount {
    fn check_extra_start_conditions(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity) -> bool {
        body.base().is_passenger()
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        level.un_ride(body);
    }
}
