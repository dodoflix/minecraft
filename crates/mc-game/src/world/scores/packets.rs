//! Payloads of the scoreboard packets `ServerScoreboard` broadcasts, as plain data (the network layer builds the
//! real `Clientbound*` packets from them; this module does not depend on the play packets).

use mc_text::Component;

use super::number_format::NumberFormat;
use super::objective_criteria::RenderType;
use super::team::{CollisionRule, DisplaySlot, TeamColor, Visibility};

/// `ClientboundSetObjectivePacket(objective, method)`: method 0 = add, 1 = remove, 2 = change.
#[derive(Debug, Clone, PartialEq)]
pub struct SetObjective {
    pub objective_name: String,
    pub display_name: Component,
    pub render_type: RenderType,
    pub number_format: Option<NumberFormat>,
    pub method: u8,
}

/// `ClientboundSetPlayerTeamPacket.Parameters`.
#[derive(Debug, Clone, PartialEq)]
pub struct TeamParameters {
    pub display_name: Component,
    pub player_prefix: Component,
    pub player_suffix: Component,
    pub name_tag_visibility: Visibility,
    pub collision_rule: CollisionRule,
    pub color: Option<TeamColor>,
    pub options: u8,
}

/// `ClientboundSetPlayerTeamPacket`: method 0 add, 1 remove, 2 change, 3 join (players added), 4 leave.
#[derive(Debug, Clone, PartialEq)]
pub struct SetPlayerTeam {
    pub name: String,
    pub method: u8,
    pub parameters: Option<TeamParameters>,
    pub players: Vec<String>,
}

/// One packet broadcast to every player.
#[derive(Debug, Clone, PartialEq)]
pub enum ScoreboardPacket {
    /// `ClientboundSetScorePacket`.
    SetScore { owner: String, objective_name: String, value: i32, display: Option<Component>, number_format: Option<NumberFormat> },
    /// `ClientboundResetScorePacket` (`objective_name == None`: every score of the owner).
    ResetScore { owner: String, objective_name: Option<String> },
    /// `ClientboundSetDisplayObjectivePacket` (`None` objective is the empty name).
    SetDisplayObjective { slot: DisplaySlot, objective_name: Option<String> },
    SetObjective(SetObjective),
    SetPlayerTeam(SetPlayerTeam),
}

/// What `ServerScoreboard` asks the server to do, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum ScoreboardOutput {
    /// `playerList.broadcastAll(packet)`.
    Broadcast(ScoreboardPacket),
    /// `level.getWaypointManager().remakeConnections(player)` for each named online player (in every level).
    RemakeWaypointConnections(Vec<String>),
}
