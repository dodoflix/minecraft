//! `net.minecraft.world.scores`: the scoreboard model (objectives, teams, scores, display slots, criteria, number
//! formats), `ServerScoreboard` semantics and the `scoreboard.dat` saved data.
//!
//! Objectives and teams are addressed by name (vanilla compares them by identity, but names are unique within a
//! scoreboard and every holder of a removed objective is cleaned up by `remove_objective`). Packet payloads the server
//! sends on changes are modelled as data in [`packets`]; the network layer converts them.
pub mod number_format;
pub mod objective_criteria;
pub mod packets;
pub mod score;
pub mod scoreboard;
pub mod scoreboard_save_data;
pub mod team;

pub use number_format::NumberFormat;
pub use objective_criteria::{ObjectiveCriteria, RenderType};
pub use score::{PlayerScoreEntry, Score, ScoreHolder};
pub use scoreboard::{Objective, PlayerTeam, ScoreAccess, Scoreboard, ScoreboardError, ServerScoreboard, TeamMut};
pub use scoreboard_save_data::ScoreboardSaveData;
pub use team::{CollisionRule, DisplaySlot, TeamColor, Visibility};
