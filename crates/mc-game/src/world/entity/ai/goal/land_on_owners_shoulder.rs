//! `LandOnOwnersShoulderGoal`.

use super::{Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::mob::{leashable, tamable};
use crate::world::entity::{Entity, EntityId, ServerPlayer};

/// `ShoulderRidingEntity` (`canSitOnShoulder`, `setEntityOnShoulder`), exposed by parrots through
/// `Entity::as_shoulder_riding{,_mut}`.
pub trait ShoulderRidingAccess {
    /// `canSitOnShoulder()`.
    fn can_sit_on_shoulder(&self) -> bool;
    /// `setEntityOnShoulder(owner)`: whether the entity was put on the shoulder.
    fn set_entity_on_shoulder(&mut self, level: &mut ServerLevel, owner: EntityId) -> bool;
}

#[derive(Default)]
pub struct LandOnOwnersShoulderGoal {
    is_sitting_on_shoulder: bool,
}

impl LandOnOwnersShoulderGoal {
    pub fn new() -> Self {
        Self::default()
    }

    /// `getOwner() instanceof ServerPlayer`.
    fn owner_player(entity: &dyn Entity, level: &ServerLevel) -> Option<EntityId> {
        let owner = tamable::get_owner(entity, level)?;
        level.get_entity(owner).filter(|o| o.as_any().is::<ServerPlayer>()).map(|_| owner)
    }
}

impl Goal for LandOnOwnersShoulderGoal {
    fn can_use(&mut self, entity: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(owner) = Self::owner_player(entity, level).and_then(|o| level.get_entity(o)) else { return false };
        let flying = owner.player().is_some_and(|p| p.abilities.flying);
        let owner_that_can_be_sat_on = !owner.is_spectator() && !flying && !owner.is_in_water() && !owner.base().is_in_powder_snow;
        !tamable::is_ordered_to_sit(entity) && owner_that_can_be_sat_on && entity.as_shoulder_riding().is_some_and(|s| s.can_sit_on_shoulder())
    }

    fn is_interruptable(&self) -> bool {
        !self.is_sitting_on_shoulder
    }

    fn start(&mut self, _entity: &mut dyn Entity, _level: &mut ServerLevel) {
        self.is_sitting_on_shoulder = false;
    }

    fn tick(&mut self, entity: &mut dyn Entity, level: &mut ServerLevel) {
        if !self.is_sitting_on_shoulder && !tamable::is_in_sitting_pose(entity) && !leashable::is_leashed(entity) {
            if let Some(owner) = Self::owner_player(entity, level) {
                if level.get_entity(owner).is_some_and(|o| entity.base().bounding_box().intersects_aabb(&o.base().bounding_box())) {
                    if let Some(s) = entity.as_shoulder_riding_mut() {
                        self.is_sitting_on_shoulder = s.set_entity_on_shoulder(level, owner);
                    }
                }
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}
