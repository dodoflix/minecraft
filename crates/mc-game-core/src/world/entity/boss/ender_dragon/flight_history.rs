//! `net.minecraft.world.entity.boss.enderdragon.DragonFlightHistory`: the last 64 samples of the dragon's height and yaw, which
//! place its neck, head and tail.

const LENGTH: usize = 64;
const MASK: i32 = 63;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub y: f64,
    pub y_rot: f32,
}

pub struct DragonFlightHistory {
    samples: [Sample; LENGTH],
    head: i32,
}

impl Default for DragonFlightHistory {
    fn default() -> Self {
        Self {
            samples: [Sample { y: 0.0, y_rot: 0.0 }; LENGTH],
            head: -1,
        }
    }
}

impl DragonFlightHistory {
    /// `record(y, yRot)`: the first sample fills the whole history.
    pub fn record(&mut self, y: f64, y_rot: f32) {
        let sample = Sample { y, y_rot };
        if self.head < 0 {
            self.samples = [sample; LENGTH];
        }
        self.head += 1;
        if self.head == LENGTH as i32 {
            self.head = 0;
        }
        self.samples[self.head as usize] = sample;
    }

    /// `get(delay)`.
    pub fn get(&self, delay: i32) -> Sample {
        self.samples[((self.head - delay) & MASK) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_sample_fills_the_history_and_delays_wrap() {
        let mut history = DragonFlightHistory::default();
        assert_eq!(history.get(5), Sample { y: 0.0, y_rot: 0.0 });
        history.record(70.0, 10.0);
        assert_eq!(history.get(63).y, 70.0);
        history.record(71.0, 11.0);
        assert_eq!(history.get(0).y, 71.0);
        assert_eq!(history.get(1).y, 70.0);
        assert_eq!(history.get(64).y, 71.0);
    }
}
