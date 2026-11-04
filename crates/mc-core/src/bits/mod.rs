//! Port of `net.minecraft.util.{BitStorage, SimpleBitStorage, ZeroBitStorage}`.
//! Out-of-range indices/values panic where vanilla's `Validate.inclusiveBetween` throws `IllegalArgumentException`.
mod simple_bit_storage;
mod zero_bit_storage;

pub use simple_bit_storage::{InitializationException, SimpleBitStorage};
pub use zero_bit_storage::ZeroBitStorage;

pub trait BitStorage: Send + Sync {
    fn get_and_set(&mut self, index: usize, value: i32) -> i32;
    fn set(&mut self, index: usize, value: i32);
    fn fill(&mut self, value: i32);
    fn get(&self, index: usize) -> i32;
    fn get_raw(&self) -> &[i64];
    fn get_size(&self) -> usize;
    fn get_bits(&self) -> i32;
    /// `getAll(IntConsumer)`
    fn get_all(&self, output: &mut dyn FnMut(i32));
    /// `unpack(int[])`: writes `get_size()` values to the start of `output`.
    fn unpack(&self, output: &mut [i32]);
    fn copy(&self) -> Box<dyn BitStorage>;
}

/// `Validate.inclusiveBetween(start, end, value)`
pub(crate) fn inclusive_between(start: i64, end: i64, value: i64) {
    assert!(
        start <= value && value <= end,
        "The value {value} is not in the specified inclusive range of {start} to {end}"
    );
}
