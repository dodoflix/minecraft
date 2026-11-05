//! `Player.awardStat/resetStat` + `ServerPlayer`'s overrides: the player's [`ServerStatsCounter`] and the scoreboard
//! objectives of the stat criteria.
//!
//! [`ServerStatsCounter`]: crate::stats::ServerStatsCounter

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::stats::Stat;

impl ServerPlayer {
    /// `awardStat(stat)` (count 1).
    pub fn award_stat_once(&mut self, level: &mut ServerLevel, stat: Stat) {
        self.award_stat(level, stat, 1);
    }

    /// `ServerPlayer.awardStat(stat, count)`.
    pub fn award_stat(&mut self, level: &mut ServerLevel, stat: Stat, count: i32) {
        self.stats.lock().increment(stat, count);
        let _ = level; // SEAM(scoreboard): `level.getScoreboard().forAllObjectives(stat, this, score -> score.add(count))`
    }

    /// `ServerPlayer.resetStat(stat)`.
    pub fn reset_stat(&mut self, level: &mut ServerLevel, stat: Stat) {
        self.stats.lock().set_value(stat, 0);
        let _ = level; // SEAM(scoreboard): `forAllObjectives(stat, this, ScoreAccess::reset)`
    }
}
