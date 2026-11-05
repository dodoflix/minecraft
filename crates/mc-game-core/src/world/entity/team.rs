//! `Entity.getTeam()` / `Entity.doTeamsAllowDamage(other)`.

use super::Entity;
use crate::server::level::ServerLevel;

/// `source.doTeamsAllowDamage(other)`: no team = allowed, else allowed against non-allied teams or with friendly fire.
pub fn do_teams_allow_damage(level: &ServerLevel, source: &dyn Entity, other: &dyn Entity) -> bool {
    let scoreboard = level.scoreboard();
    let Some(team) = scoreboard.get_players_team(&source.get_scoreboard_name()) else { return true };
    let other_team = scoreboard.get_players_team(&other.get_scoreboard_name());
    !team.is_allied_to(other_team) || team.is_allow_friendly_fire()
}
