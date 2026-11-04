pub const FLOAT_MULTIPLIER: f32 = 5.9604645E-8;
/// Java: `double DOUBLE_MULTIPLIER = 1.110223E-16F` (a widened float, exactly 2^-53); the oracle shows `next_double` multiplies in double.
pub const DOUBLE_MULTIPLIER: f64 = 1.110223E-16_f32 as f64;

/// `BitRandomSource`: implementors provide `next(bits)`; the `bit_*` defaults are the vanilla
/// default methods, wired into `RandomSource` impls with `bit_random_source_methods!`.
pub trait BitRandomSource {
    fn next(&mut self, bits: i32) -> i32;

    fn bit_next_int(&mut self) -> i32 {
        self.next(32)
    }

    fn bit_next_int_bounded(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "Bound must be positive");
        if bound & (bound - 1) == 0 {
            ((bound as i64).wrapping_mul(self.next(31) as i64) >> 31) as i32
        } else {
            loop {
                let sample = self.next(31);
                let modulo = sample % bound;
                if sample.wrapping_sub(modulo).wrapping_add(bound - 1) >= 0 {
                    return modulo;
                }
            }
        }
    }

    fn bit_next_long(&mut self) -> i64 {
        let upper = self.next(32);
        let lower = self.next(32);
        ((upper as i64) << 32).wrapping_add(lower as i64)
    }

    fn bit_next_boolean(&mut self) -> bool {
        self.next(1) != 0
    }

    fn bit_next_float(&mut self) -> f32 {
        self.next(24) as f32 * FLOAT_MULTIPLIER
    }

    fn bit_next_double(&mut self) -> f64 {
        let upper = self.next(26);
        let lower = self.next(27);
        let combined = ((upper as i64) << 27).wrapping_add(lower as i64);
        combined as f64 * DOUBLE_MULTIPLIER
    }
}

macro_rules! bit_random_source_methods {
    () => {
        fn next_int(&mut self) -> i32 {
            $crate::random::BitRandomSource::bit_next_int(self)
        }
        fn next_int_bounded(&mut self, bound: i32) -> i32 {
            $crate::random::BitRandomSource::bit_next_int_bounded(self, bound)
        }
        fn next_long(&mut self) -> i64 {
            $crate::random::BitRandomSource::bit_next_long(self)
        }
        fn next_boolean(&mut self) -> bool {
            $crate::random::BitRandomSource::bit_next_boolean(self)
        }
        fn next_float(&mut self) -> f32 {
            $crate::random::BitRandomSource::bit_next_float(self)
        }
        fn next_double(&mut self) -> f64 {
            $crate::random::BitRandomSource::bit_next_double(self)
        }
    };
}
pub(crate) use bit_random_source_methods;
