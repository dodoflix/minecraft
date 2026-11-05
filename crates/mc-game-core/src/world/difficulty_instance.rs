//! `net.minecraft.world.DifficultyInstance` (regional difficulty).

use mc_core::math::mth;
use mc_world::level_storage::Difficulty;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DifficultyInstance {
    base: Difficulty,
    effective_difficulty: f32,
}

impl DifficultyInstance {
    pub fn new(base: Difficulty, total_game_time: i64, local_game_time: i64, moon_brightness: f32) -> Self {
        Self { base, effective_difficulty: Self::calculate_difficulty(base, total_game_time, local_game_time, moon_brightness) }
    }

    pub fn get_difficulty(&self) -> Difficulty {
        self.base
    }

    pub fn get_effective_difficulty(&self) -> f32 {
        self.effective_difficulty
    }

    pub fn is_hard(&self) -> bool {
        self.effective_difficulty >= Difficulty::Hard as i32 as f32
    }

    pub fn is_harder_than(&self, required_difficulty: f32) -> bool {
        self.effective_difficulty > required_difficulty
    }

    pub fn get_special_multiplier(&self) -> f32 {
        if self.effective_difficulty < 2.0 {
            0.0
        } else if self.effective_difficulty > 4.0 {
            1.0
        } else {
            (self.effective_difficulty - 2.0) / 2.0
        }
    }

    fn calculate_difficulty(base: Difficulty, total_game_time: i64, local_game_time: i64, moon_brightness: f32) -> f32 {
        if base == Difficulty::Peaceful {
            return 0.0;
        }
        let is_hard = base == Difficulty::Hard;
        let mut scale = 0.75f32;
        let global_scale = mth::clamp_f32((total_game_time as f32 + -72000.0f32) / 1440000.0f32, 0.0, 1.0) * 0.25f32;
        scale += global_scale;
        let mut local_scale = 0.0f32;
        local_scale += mth::clamp_f32(local_game_time as f32 / 3600000.0f32, 0.0, 1.0) * if is_hard { 1.0 } else { 0.75 };
        local_scale += mth::clamp_f32(moon_brightness * 0.25f32, 0.0, global_scale);
        if base == Difficulty::Easy {
            local_scale *= 0.5;
        }
        scale += local_scale;
        base.get_id() as f32 * scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaceful_is_zero_and_hard_grows_with_time() {
        assert_eq!(DifficultyInstance::new(Difficulty::Peaceful, 5_000_000, 5_000_000, 1.0).get_effective_difficulty(), 0.0);
        let early = DifficultyInstance::new(Difficulty::Hard, 0, 0, 0.0).get_effective_difficulty();
        assert_eq!(early, 2.25);
        let late = DifficultyInstance::new(Difficulty::Hard, 1_512_000, 3_600_000, 1.0).get_effective_difficulty();
        assert_eq!(late, 3.0 * (0.75 + 0.25 + 1.0 + 0.25));
        assert!(DifficultyInstance::new(Difficulty::Hard, 1_512_000, 3_600_000, 1.0).is_hard());
    }
}
