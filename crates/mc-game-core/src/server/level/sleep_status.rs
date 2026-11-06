//! `SleepStatus` (`net.minecraft.server.players.SleepStatus`): how many of a level's players are asleep.

/// What `SleepStatus.update` needs to know about a player.
#[derive(Clone, Copy, Debug)]
pub struct SleepInfo {
    pub spectator: bool,
    pub sleeping: bool,
    /// `Player.isSleepingLongEnough()`.
    pub sleeping_long_enough: bool,
}

#[derive(Default, Debug)]
pub struct SleepStatus {
    active_players: i32,
    sleeping_players: i32,
}

impl SleepStatus {
    pub fn are_enough_sleeping(&self, sleep_percentage_needed: i32) -> bool {
        self.sleeping_players >= self.sleepers_needed(sleep_percentage_needed)
    }

    pub fn are_enough_deep_sleeping(
        &self,
        sleep_percentage_needed: i32,
        players: &[SleepInfo],
    ) -> bool {
        let deep_sleepers = players.iter().filter(|p| p.sleeping_long_enough).count() as i32;
        deep_sleepers >= self.sleepers_needed(sleep_percentage_needed)
    }

    /// `Math.max(1, Mth.ceil(activePlayers * percentage / 100.0F))` (float arithmetic).
    pub fn sleepers_needed(&self, sleep_percentage_needed: i32) -> i32 {
        let value = (self.active_players.wrapping_mul(sleep_percentage_needed)) as f32 / 100.0f32;
        mc_core::math::mth::ceil_f32(value).max(1)
    }

    pub fn remove_all_sleepers(&mut self) {
        self.sleeping_players = 0;
    }

    pub fn amount_sleeping(&self) -> i32 {
        self.sleeping_players
    }

    /// `update(players)`: returns whether the sleeping status message must be announced.
    pub fn update(&mut self, players: &[SleepInfo]) -> bool {
        let (old_active, old_sleeping) = (self.active_players, self.sleeping_players);
        self.active_players = 0;
        self.sleeping_players = 0;
        for player in players {
            if !player.spectator {
                self.active_players += 1;
                if player.sleeping {
                    self.sleeping_players += 1;
                }
            }
        }
        (old_sleeping > 0 || self.sleeping_players > 0)
            && (old_active != self.active_players || old_sleeping != self.sleeping_players)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(spectator: bool, sleeping: bool) -> SleepInfo {
        SleepInfo {
            spectator,
            sleeping,
            sleeping_long_enough: sleeping,
        }
    }

    #[test]
    fn percentage_rounds_up_and_needs_at_least_one() {
        let mut s = SleepStatus::default();
        assert!(s.update(&[p(false, true), p(false, false), p(false, false)]));
        assert_eq!(s.sleepers_needed(100), 3);
        assert_eq!(s.sleepers_needed(50), 2);
        assert_eq!(s.sleepers_needed(0), 1);
        assert!(!s.are_enough_sleeping(50));
        assert!(s.are_enough_sleeping(30));
    }

    #[test]
    fn spectators_do_not_count_and_update_reports_changes() {
        let mut s = SleepStatus::default();
        assert!(
            !s.update(&[p(false, false), p(true, true)]),
            "nobody sleeping: no announcement"
        );
        assert!(s.update(&[p(false, true), p(true, true)]));
        assert!(!s.update(&[p(false, true), p(true, true)]), "unchanged");
        assert_eq!(s.amount_sleeping(), 1);
    }
}
