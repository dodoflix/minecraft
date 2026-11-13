//! `Collections.shuffle(list)` without a random source: vanilla draws from the process-wide, unseeded `java.util.Random`,
//! never from the level or entity random, so the order differs from run to run.

use mc_core::random::{RandomSource, SingleThreadedRandomSource};
use std::cell::RefCell;
use std::hash::{BuildHasher, Hasher};

thread_local! {
    static SHUFFLE_RANDOM: RefCell<SingleThreadedRandomSource> = {
        let seed = std::collections::hash_map::RandomState::new().build_hasher().finish() as i64;
        RefCell::new(SingleThreadedRandomSource::new(seed))
    };
}

/// `Collections.shuffle(list)` (same algorithm: `swap(list, i - 1, rnd.nextInt(i))` from the end).
pub fn shuffle_unseeded<T>(list: &mut [T]) {
    SHUFFLE_RANDOM.with_borrow_mut(|random| {
        for n in (2..=list.len()).rev() {
            list.swap(n - 1, random.next_int_bounded(n as i32) as usize);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::shuffle_unseeded;

    #[test]
    fn keeps_every_element() {
        let mut v: Vec<i32> = (0..27).collect();
        shuffle_unseeded(&mut v);
        v.sort_unstable();
        assert_eq!(v, (0..27).collect::<Vec<_>>());
    }
}
