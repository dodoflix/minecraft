//! `Entity.getTeam()` / `Entity.doTeamsAllowDamage(other)`.

use super::Entity;
use super::mob::ext::team_member_name;
use crate::server::level::ServerLevel;
use crate::world::scores::{PlayerTeam, Scoreboard};

/// `Entity.getTeam()` (a tamed pet without a team of its own plays for its root owner's), looked up in `scoreboard`.
pub fn get_team<'a>(
    level: &ServerLevel,
    scoreboard: &'a Scoreboard,
    e: &dyn Entity,
) -> Option<&'a PlayerTeam> {
    scoreboard.get_players_team(&team_member_name(level, e)?)
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
