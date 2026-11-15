//! `Entity.getTeam()` / `Entity.doTeamsAllowDamage(other)`.

use super::Entity;
use crate::server::level::ServerLevel;
use crate::world::scores::{PlayerTeam, Scoreboard};

/// `Entity.getTeam()` (a tamed pet without a team of its own plays for its root owner's), looked up in `scoreboard`.
pub fn get_team<'a>(
    level: &ServerLevel,
    scoreboard: &'a Scoreboard,
    e: &dyn Entity,
) -> Option<&'a PlayerTeam> {
    if let Some(team) = scoreboard.get_players_team(&e.get_scoreboard_name()) {
        return Some(team);
    }
    if e.mob().and_then(|m| m.tamable.as_ref()).is_some() && super::mob::tamable::is_tame(e) {
        let owner = level.get_entity(super::mob::tamable::get_root_owner(level, e)?)?;
        return scoreboard.get_players_team(&owner.get_scoreboard_name());
    }
    None
}

/// `Entity.getTeam().getName()`.
pub fn get_team_name(level: &ServerLevel, e: &dyn Entity) -> Option<String> {
    get_team(level, &level.scoreboard(), e).map(|t| t.get_name().to_owned())
}

/// `source.doTeamsAllowDamage(other)`: no team = allowed, else allowed against non-allied teams or with friendly fire.
pub fn do_teams_allow_damage(level: &ServerLevel, source: &dyn Entity, other: &dyn Entity) -> bool {
    let scoreboard = level.scoreboard();
    let Some(team) = get_team(level, &scoreboard, source) else {
        return true;
    };
    let other_team = get_team(level, &scoreboard, other);
    !team.is_allied_to(other_team) || team.is_allow_friendly_fire()
}
