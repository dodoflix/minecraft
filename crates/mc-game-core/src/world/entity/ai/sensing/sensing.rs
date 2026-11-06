//! `Sensing`: per-tick cache of `hasLineOfSight` results (cleared every `serverAiStep`).

use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, EntityId};
use std::cell::RefCell;
use std::collections::HashSet;

#[derive(Default)]
pub struct Sensing {
    seen: RefCell<HashSet<EntityId>>,
    unseen: RefCell<HashSet<EntityId>>,
}

impl Sensing {
    /// `tick()`.
    pub fn tick(&mut self) {
        self.seen.get_mut().clear();
        self.unseen.get_mut().clear();
    }

    /// `hasLineOfSight(target)`; `mob` owns this cache.
    pub fn has_line_of_sight(
        &self,
        level: &ServerLevel,
        mob: &dyn Entity,
        target: &dyn Entity,
    ) -> bool {
        let id = target.id();
        if self.seen.borrow().contains(&id) {
            return true;
        }
        if self.unseen.borrow().contains(&id) {
            return false;
        }
        let result = crate::world::entity::mob::ext::has_line_of_sight(level, mob, target);
        if result {
            self.seen.borrow_mut().insert(id);
        } else {
            self.unseen.borrow_mut().insert(id);
        }
        result
    }
}
