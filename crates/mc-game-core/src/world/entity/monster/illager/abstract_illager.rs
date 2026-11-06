//! `net.minecraft.world.entity.monster.illager.AbstractIllager`: the shared behavior of pillagers, vindicators, evokers and
//! illusioners (it has no fields of its own).

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::mob;
use mc_data::EntityType;

/// `AbstractIllager.IllagerArmPose`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IllagerArmPose {
    Crossed,
    Attacking,
    Spellcasting,
    BowAndArrow,
    CrossbowHold,
    CrossbowCharge,
    Celebrating,
    Neutral,
}

/// `AbstractIllager.canAttack(target)`: an illager never attacks a baby villager (or wandering trader).
pub fn can_attack(e: &dyn Entity, level: &ServerLevel, target: &dyn Entity) -> bool {
    if is_abstract_villager(target) && target.is_baby() {
        return false;
    }
    mob::ext::can_attack_default(e, level, target)
}

fn is_abstract_villager(e: &dyn Entity) -> bool {
    matches!(
        e.get_type(),
        EntityType::VILLAGER | EntityType::WANDERING_TRADER
    )
}

/// `AbstractIllager.considersEntityAsAlly(other)`.
pub fn considers_entity_as_ally(e: &dyn Entity, level: &ServerLevel, other: &dyn Entity) -> bool {
    if mob::ext::considers_entity_as_ally_default(level, e, other) {
        true
    } else if !mc_data::tags::get::<EntityType>("illager_friends")
        .is_some_and(|t| t.contains(other.get_type()))
    {
        false
    } else {
        team_of(level, e).is_none() && team_of(level, other).is_none()
    }
}

fn team_of(level: &ServerLevel, e: &dyn Entity) -> Option<String> {
    level
        .scoreboard()
        .get_players_team(&e.get_scoreboard_name())
        .map(|t| t.get_name().to_string())
}

/// `isHolding`-style helper used by the pillager/illusioner arm poses: `getArmPose()` of the abstract class.
pub fn default_arm_pose() -> IllagerArmPose {
    IllagerArmPose::Crossed
}
