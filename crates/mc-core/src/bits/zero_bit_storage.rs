use super::{BitStorage, inclusive_between};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZeroBitStorage {
    size: usize,
}

impl ZeroBitStorage {
    pub const RAW: [i64; 0] = [];

    pub fn new(size: usize) -> Self {
        Self { size }
    }

    fn check_index(&self, index: usize) {
        inclusive_between(0, self.size as i64 - 1, index as i64);
    }
}

impl BitStorage for ZeroBitStorage {
    fn get_and_set(&mut self, index: usize, value: i32) -> i32 {
        self.check_index(index);
        inclusive_between(0, 0, value.into());
        0
    }

    fn set(&mut self, index: usize, value: i32) {
        self.check_index(index);
        inclusive_between(0, 0, value.into());
    }

    fn fill(&mut self, value: i32) {
        inclusive_between(0, 0, value.into());
    }

    fn get(&self, index: usize) -> i32 {
        self.check_index(index);
        0
    }

    fn get_raw(&self) -> &[i64] {
        &Self::RAW
    }

    fn get_size(&self) -> usize {
        self.size
    }

    fn get_bits(&self) -> i32 {
        0
    }

    fn get_all(&self, output: &mut dyn FnMut(i32)) {
        for _ in 0..self.size {
            output(0);
        }
    }

    fn unpack(&self, output: &mut [i32]) {
        output[..self.size].fill(0);
    }

    fn copy(&self) -> Box<dyn BitStorage> {
        Box::new(*self)
    }
}
