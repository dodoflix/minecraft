//! `Player.awardStat/resetStat` + `ServerPlayer`'s overrides: the player's [`ServerStatsCounter`] and the scoreboard
//! objectives of the stat criteria.
//!
//! [`ServerStatsCounter`]: crate::stats::ServerStatsCounter

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::stats::Stat;
use crate::stats::stat::build_name;
use crate::world::scores::{ObjectiveCriteria, score::ScoreHolder};
use mc_text::Component;

/// The player as an owner of scores (`Entity.getScoreboardName()` of a player is its profile name).
impl ScoreHolder for ServerPlayer {
    fn get_scoreboard_name(&self) -> String {
        self.player.game_profile.name.clone()
    }

    fn get_display_name(&self) -> Option<Component> {
        Some(self.get_display_name())
    }
}

impl ServerPlayer {
    /// `awardStat(stat)` (count 1).
    pub fn award_stat_once(&mut self, level: &ServerLevel, stat: Stat) {
        self.award_stat(level, stat, 1);
    }

    /// `ServerPlayer.awardStat(stat, count)`.
    pub fn award_stat(&mut self, level: &ServerLevel, stat: Stat, count: i32) {
        self.stats.lock().increment(stat, count);
        let criteria = ObjectiveCriteria::stat(build_name(&stat));
        level.scoreboard_mut().for_all_objectives(&criteria, &*self, |score| {
            let _ = score.add(count);
        });
    }

    /// `ServerPlayer.resetStat(stat)`.
    pub fn reset_stat(&mut self, level: &ServerLevel, stat: Stat) {
        self.stats.lock().set_value(stat, 0);
        let criteria = ObjectiveCriteria::stat(build_name(&stat));
        level.scoreboard_mut().for_all_objectives(&criteria, &*self, |score| {
            let _ = score.reset();
        });
    }

    /// `updateScoreForCriteria(criteria, value)`.
    pub fn update_score_for_criteria(&self, level: &ServerLevel, criteria: &ObjectiveCriteria, value: i32) {
        level.scoreboard_mut().for_all_objectives(criteria, self, |score| {
            let _ = score.set(value);
        });
    }
}
