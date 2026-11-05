//! Collision pushing between entities: `LivingEntity.pushEntities` (incl. cramming damage), `doPush`, `isPushable`.

use super::util::{self, is_position_entity_ticking};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::random::RandomSource;

/// `LivingEntity.isPushable()`.
pub fn living_is_pushable(entity: &dyn Entity, level: &ServerLevel) -> bool {
    if !entity.is_always_ticking() && !is_position_entity_ticking(level, entity.base().block_position()) {
        false
    } else {
        entity.is_alive() && !entity.is_spectator() && !entity.on_climbable(level)
    }
}

/// `EntitySelector.pushableBy(entity)` over `Level.getPushableEntities(entity, box)` (`NO_SPECTATORS`, `isPushable()` and the
/// scoreboard team collision rules of both entities).
fn get_pushable_entities(level: &ServerLevel, entity: &dyn Entity) -> Vec<crate::world::entity::EntityId> {
    use crate::world::scores::CollisionRule;
    let scoreboard = level.scoreboard();
    let own_team = scoreboard.get_player_team(&entity.get_scoreboard_name());
    let own_rule = own_team.map_or(CollisionRule::Always, |t| t.get_collision_rule());
    if own_rule == CollisionRule::Never {
        return Vec::new();
    }
    level.get_entities(Some(entity.id()), &entity.base().bounding_box(), |other| {
        if other.is_spectator() || !other.is_pushable(level) {
            return false;
        }
        let their_team = scoreboard.get_player_team(&other.get_scoreboard_name());
        let their_rule = their_team.map_or(CollisionRule::Always, |t| t.get_collision_rule());
        if their_rule == CollisionRule::Never {
            return false;
        }
        let same_team = own_team.is_some_and(|t| t.is_allied_to(their_team));
        if (own_rule == CollisionRule::PushOwnTeam || their_rule == CollisionRule::PushOwnTeam) && same_team {
            false
        } else {
            (own_rule != CollisionRule::PushOtherTeams && their_rule != CollisionRule::PushOtherTeams) || same_team
        }
    })
}

/// `LivingEntity.pushEntities()`.
pub fn push_entities(entity: &mut dyn Entity, level: &mut ServerLevel) {
    let pushable = get_pushable_entities(level, entity);
    if pushable.is_empty() {
        return;
    }
    let max_cramming = level.max_entity_cramming();
    if max_cramming > 0 && pushable.len() as i32 > max_cramming - 1 && entity.base_mut().random.next_int_bounded(4) == 0 {
        let count = pushable.iter().filter(|&&id| level.get_entity(id).is_some_and(|e| !e.base().is_passenger())).count() as i32;
        if count > max_cramming - 1 {
            util::hurt(level, entity, |sources| sources.cramming(), 6.0);
        }
    }
    for id in pushable {
        level.with_entity(id, |level, other| entity.do_push(level, other));
    }
}

/// `LivingEntity.doPush(entity)` = `entity.push(this)`.
pub fn do_push(level: &mut ServerLevel, this: &mut dyn Entity, other: &mut dyn Entity) {
    other.push_entity(level, this);
}
