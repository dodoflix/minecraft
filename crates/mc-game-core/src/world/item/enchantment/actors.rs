//! [`Actors`]: the entities an enchantment pipeline touches that are *not* in the level's storage.
//!
//! The entity that is currently running (an attacking player, a ticking mob) is taken out of the level while its code
//! executes, so a lookup by id would miss it. Callers hand those entities in explicitly; every other entity is taken out
//! of / looked up in the level on demand. Effects address entities by [`EntityId`] and never hold two `&mut` at once.

use mc_core::random::RandomSource;

use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, EntityId};

/// Entities lent to a pipeline run (absent from the level for its duration).
#[derive(Default)]
pub struct Actors<'a> {
    explicit: Vec<&'a mut dyn Entity>,
}

impl<'a> Actors<'a> {
    /// No explicit entities: everything is resolved through the level.
    pub fn none() -> Actors<'a> {
        Actors {
            explicit: Vec::new(),
        }
    }

    /// `entity` is the running entity.
    pub fn of(entity: &'a mut dyn Entity) -> Actors<'a> {
        Actors {
            explicit: vec![entity],
        }
    }

    /// Adds another entity that is out of the level.
    pub fn and(mut self, entity: &'a mut dyn Entity) -> Actors<'a> {
        self.explicit.push(entity);
        self
    }

    /// Adds an out-of-level entity in place.
    pub fn push(&mut self, entity: &'a mut dyn Entity) {
        self.explicit.push(entity);
    }

    /// Shared view of `id`: an explicit entity or one stored in the level.
    pub fn get<'s>(&'s self, level: &'s ServerLevel, id: EntityId) -> Option<&'s dyn Entity> {
        match self.explicit.iter().find(|e| e.id() == id) {
            Some(e) => Some(&**e),
            None => level.get_entity(id),
        }
    }

    /// `id` when it is one of the explicit (out of the level) entities.
    pub fn explicit(&self, id: EntityId) -> Option<&dyn Entity> {
        self.explicit.iter().find(|e| e.id() == id).map(|e| &**e)
    }

    /// Runs `f` with `id` mutably borrowed (taken out of the level unless explicit). The explicit entities other than
    /// `id` are lent to the level meanwhile, so whatever `f` runs finds them by id (a thorns hit credited to the
    /// wearer, who is out of the level).
    pub fn with<R>(
        &mut self,
        level: &mut ServerLevel,
        id: EntityId,
        f: impl FnOnce(&mut ServerLevel, &mut dyn Entity) -> R,
    ) -> Option<R> {
        let mut f = Some(f);
        match self.explicit.iter().position(|e| e.id() == id) {
            Some(at) => {
                let (before, rest) = self.explicit.split_at_mut(at);
                let (target, after) = rest.split_first_mut().expect("position found it");
                Some(lend_all(level, before, &mut |level| {
                    lend_all(level, &mut *after, &mut |level| {
                        (f.take().expect("called once"))(level, &mut **target)
                    })
                }))
            }
            None => lend_all(level, &mut self.explicit, &mut |level| {
                level.with_entity(id, f.take().expect("called once"))
            }),
        }
    }

    /// `entity.getRandom()` (falls back to the level random when the entity is gone).
    pub fn with_random<R>(
        &mut self,
        level: &mut ServerLevel,
        id: EntityId,
        f: impl FnOnce(&mut dyn RandomSource) -> R,
    ) -> R {
        let mut f = Some(f);
        if let Some(r) = self.with(level, id, |_, e| {
            (f.take().expect("called once"))(&mut e.base_mut().random)
        }) {
            return r;
        }
        use crate::world::level::LevelAccessor;
        (f.take().expect("not consumed when the entity was missing"))(level.random())
    }
}

/// Lends every entity of `entities` to `level` while `f` runs.
fn lend_all<R>(
    level: &mut ServerLevel,
    entities: &mut [&mut dyn Entity],
    f: &mut dyn FnMut(&mut ServerLevel) -> R,
) -> R {
    match entities.split_first_mut() {
        None => f(level),
        Some((first, rest)) => level.lend(&mut **first, |level| lend_all(level, rest, f)),
    }
}
