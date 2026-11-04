use super::RandomSource;

/// `MarsagliaPolarGaussian`. Vanilla holds the source; here the source is passed per call
/// (owners keep this state next to themselves).
#[derive(Clone, Copy, Debug, Default)]
pub struct MarsagliaPolarGaussian {
    next_next_gaussian: f64,
    have_next_next_gaussian: bool,
}

impl MarsagliaPolarGaussian {
    pub const fn new() -> Self {
        Self { next_next_gaussian: 0.0, have_next_next_gaussian: false }
    }

    pub fn reset(&mut self) {
        self.have_next_next_gaussian = false;
    }

    pub fn next_gaussian<R: RandomSource + ?Sized>(&mut self, random: &mut R) -> f64 {
        if self.have_next_next_gaussian {
            self.have_next_next_gaussian = false;
            return self.next_next_gaussian;
        }
        let (x, y, radius_squared) = loop {
            let x = 2.0 * random.next_double() - 1.0;
            let y = 2.0 * random.next_double() - 1.0;
            let r = x * x + y * y;
            if !(r >= 1.0 || r == 0.0) {
                break (x, y, r);
            }
        };
        let multiplier = (-2.0 * radius_squared.ln() / radius_squared).sqrt();
        self.next_next_gaussian = y * multiplier;
        self.have_next_next_gaussian = true;
        x * multiplier
    }
}
