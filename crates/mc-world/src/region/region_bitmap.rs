//! `net.minecraft.world.level.chunk.storage.RegionBitmap`: used-sector tracking (a `java.util.BitSet`).

#[derive(Debug, Clone, Default)]
pub struct RegionBitmap {
    words: Vec<u64>,
}

impl RegionBitmap {
    pub fn new() -> Self {
        Self::default()
    }

    fn is_set(&self, bit: usize) -> bool {
        self.words.get(bit / 64).is_some_and(|w| w >> (bit % 64) & 1 != 0)
    }

    /// `BitSet.nextClearBit`
    fn next_clear_bit(&self, from: usize) -> usize {
        let mut bit = from;
        while self.is_set(bit) {
            bit += 1;
        }
        bit
    }

    /// `BitSet.nextSetBit`: `None` is Java's `-1`.
    fn next_set_bit(&self, from: usize) -> Option<usize> {
        (from..self.words.len() * 64).find(|&b| self.is_set(b))
    }

    pub fn force(&mut self, position: usize, size: usize) {
        let end = position + size;
        if self.words.len() * 64 < end {
            self.words.resize(end.div_ceil(64), 0);
        }
        for bit in position..end {
            self.words[bit / 64] |= 1 << (bit % 64);
        }
    }

    pub fn free(&mut self, position: usize, size: usize) {
        for bit in position..(position + size).min(self.words.len() * 64) {
            self.words[bit / 64] &= !(1 << (bit % 64));
        }
    }

    /// First-fit allocation of `size` consecutive sectors.
    pub fn allocate(&mut self, size: usize) -> usize {
        let mut current = 0;
        loop {
            let free_start = self.next_clear_bit(current);
            match self.next_set_bit(free_start) {
                Some(free_end) if free_end - free_start < size => current = free_end,
                _ => {
                    self.force(free_start, size);
                    return free_start;
                }
            }
        }
    }

    /// `getUsed()`: set sector indices, ascending.
    pub fn get_used(&self) -> Vec<usize> {
        (0..self.words.len() * 64).filter(|&b| self.is_set(b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocate_first_fit_and_free() {
        let mut b = RegionBitmap::new();
        b.force(0, 2);
        assert_eq!(b.allocate(3), 2);
        assert_eq!(b.allocate(1), 5);
        b.free(2, 3);
        assert_eq!(b.allocate(4), 6); // hole of 3 too small
        assert_eq!(b.allocate(2), 2);
        assert_eq!(b.allocate(1), 4);
        assert_eq!(b.get_used(), (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn spans_word_boundary() {
        let mut b = RegionBitmap::new();
        b.force(0, 62);
        assert_eq!(b.allocate(5), 62);
        assert!(b.is_set(66) && !b.is_set(67));
        b.free(60, 100);
        assert_eq!(b.get_used(), (0..60).collect::<Vec<_>>());
    }
}
