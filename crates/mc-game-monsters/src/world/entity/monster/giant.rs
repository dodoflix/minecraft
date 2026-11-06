//! `Giant`: the unused giant zombie (a `Monster` without goals).

use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::EntityType;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::mob::{MobData, MobKind};
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::level::LevelReader;

/// `Giant`.
pub struct Giant {
    pub mob: MobData,
}

impl Giant {
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |_| {});
        let mut giant = Giant { mob };
        let navigation = StandardPathNavigation::ground(&giant);
        giant.mob.set_navigation(Box::new(navigation));
        giant
    }
}

impl Entity for Giant {
    mc_game_core::mob_entity_accessors!(mob);

    /// `getWalkTargetValue(pos, level)`: not negated like `Monster`'s.
    fn get_walk_target_value(&self, pos: BlockPos, level: &dyn LevelReader) -> f32 {
        level.get_pathfinding_cost_from_light_levels(pos)
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static GIANT: EntityRegistration = EntityRegistration {
    entity_type: EntityType::GIANT,
    create: |ty, id, registries| Box::new(Giant::new(ty, id, registries)),
};
