//! `net.minecraft.world.level.chunk.{CarvingMask, CarverOutput}`: the bitset of blocks a carver removed in one chunk.
//! Not persisted in 26.3 (only used while `NoiseBasedChunkGenerator.applyCarvers` runs).

/// `CarverOutput`.
pub trait CarverOutput {
    fn carve(&mut self, x: i32, y: i32, z: i32);
    fn min_y(&self) -> i32;
    fn max_y(&self) -> i32;
}

/// `CarvingMask.Filter`: `test(x, y, z)`.
pub type Filter<'a> = dyn Fn(i32, i32, i32) -> bool + 'a;

#[derive(Clone, Debug)]
pub struct CarvingMask {
    min_y: i32,
    max_y: i32,
    height: i32,
    /// `java.util.BitSet` words; grows on `set` like the JDK class.
    mask: Vec<u64>,
}

impl CarvingMask {
    pub fn new(min_y: i32, max_y: i32) -> CarvingMask {
        let height = max_y - min_y + 1;
        CarvingMask {
            min_y,
            max_y,
            height,
            mask: vec![0; (256 * height as usize).div_ceil(64)],
        }
    }

    fn get_index(&self, x: i32, y: i32, z: i32) -> usize {
        usize::try_from(y - self.min_y + (z + (x << 4)) * self.height)
            .expect("CarvingMask index out of range (vanilla: IndexOutOfBoundsException)")
    }

    /// `CarvingMask.visit(visitor)`: runs of set bits are split into per-column `(x, z, bottomY, topY)` segments.
    pub fn visit(&self, mut visitor: impl FnMut(i32, i32, i32, i32)) {
        let mut start = self.next_set_bit(0);
        while let Some(start_index) = start {
            let end_index = self.next_clear_bit(start_index) - 1;
            self.visit_segment(&mut visitor, start_index, end_index);
            start = self.next_set_bit(end_index + 1);
        }
    }

    fn visit_segment(
        &self,
        visitor: &mut impl FnMut(i32, i32, i32, i32),
        start_index: usize,
        end_index: usize,
    ) {
        let height = self.height as usize;
        for column in start_index / height..=end_index / height {
            let column_x = (column >> 4 & 15) as i32;
            let column_z = (column & 15) as i32;
            let column_base_index = column * height;
            let bottom_y = start_index.saturating_sub(column_base_index) as i32 + self.min_y;
            let top_y = (end_index - column_base_index).min(height - 1) as i32 + self.min_y;
            visitor(column_x, column_z, bottom_y, top_y);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.mask.iter().all(|&w| w == 0)
    }

    /// `BitSet.nextSetBit`.
    fn next_set_bit(&self, from: usize) -> Option<usize> {
        let mut word = from / 64;
        if word >= self.mask.len() {
            return None;
        }
        let mut bits = self.mask[word] & (!0u64 << (from % 64));
        loop {
            if bits != 0 {
                return Some(word * 64 + bits.trailing_zeros() as usize);
            }
            word += 1;
            bits = *self.mask.get(word)?;
        }
    }

    /// `BitSet.nextClearBit` (bits past the last word are clear).
    fn next_clear_bit(&self, from: usize) -> usize {
        let mut word = from / 64;
        if word >= self.mask.len() {
            return from;
        }
        let mut bits = !self.mask[word] & (!0u64 << (from % 64));
        loop {
            if bits != 0 {
                return word * 64 + bits.trailing_zeros() as usize;
            }
            word += 1;
            if word >= self.mask.len() {
                return word * 64;
            }
            bits = !self.mask[word];
        }
    }
}

impl CarverOutput for CarvingMask {
    fn carve(&mut self, x: i32, y: i32, z: i32) {
        let index = self.get_index(x, y, z);
        if index / 64 >= self.mask.len() {
            self.mask.resize(index / 64 + 1, 0);
        }
        self.mask[index / 64] |= 1 << (index % 64);
    }

    fn min_y(&self) -> i32 {
        self.min_y
    }

    fn max_y(&self) -> i32 {
        self.max_y
    }
}
