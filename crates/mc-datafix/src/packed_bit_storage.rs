//! `net.minecraft.util.datafix.PackedBitStorage`: the pre-1.16 packed long array whose entries straddle two longs.

use mc_core::math::mth::round_toward_i32;

/// `Validate.inclusiveBetween(start, end, value)` of commons-lang3 (an `IllegalArgumentException`).
#[track_caller]
fn inclusive_between(start: i64, end: i64, value: i64) {
    assert!(
        (start..=end).contains(&value),
        "java.lang.IllegalArgumentException: The value {value} is not in the specified inclusive range of {start} to {end}"
    );
}

/// `ceil(size * bits / 64)` longs (the Java `int` product wraps).
fn required_length(bits: i32, size: i32) -> i32 {
    round_toward_i32(size.wrapping_mul(bits), 64) / 64
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedBitStorage {
    data: Vec<i64>,
    bits: i32,
    mask: i64,
    size: i32,
}

impl PackedBitStorage {
    /// `new PackedBitStorage(bits, size)`.
    #[track_caller]
    pub fn new(bits: i32, size: i32) -> PackedBitStorage {
        let length = required_length(bits, size);
        assert!(
            length >= 0,
            "java.lang.NegativeArraySizeException: {length}"
        );
        PackedBitStorage::from_data(bits, size, vec![0; length as usize])
    }

    /// `new PackedBitStorage(bits, size, data)`; panics (`IllegalArgumentException`) on a bad `bits` or `data` length.
    #[track_caller]
    pub fn from_data(bits: i32, size: i32, data: Vec<i64>) -> PackedBitStorage {
        inclusive_between(1, 32, i64::from(bits));
        let required = required_length(bits, size);
        assert!(
            i64::try_from(data.len()) == Ok(i64::from(required)),
            "java.lang.IllegalArgumentException: Invalid length given for storage, got: {} but expected: {required}",
            data.len()
        );
        PackedBitStorage {
            data,
            bits,
            mask: (1i64 << bits) - 1,
            size,
        }
    }

    /// The first and last long an entry touches and its bit offset in the first.
    fn locate(&self, index: i32) -> (usize, usize, i32) {
        let position = index.wrapping_mul(self.bits);
        let start_data = position >> 6;
        let end_data = index
            .wrapping_add(1)
            .wrapping_mul(self.bits)
            .wrapping_sub(1)
            >> 6;
        (
            start_data as usize,
            end_data as usize,
            position ^ (start_data << 6),
        )
    }

    /// `set(index, value)`.
    #[track_caller]
    pub fn set(&mut self, index: i32, value: i32) {
        inclusive_between(0, i64::from(self.size.wrapping_sub(1)), i64::from(index));
        inclusive_between(0, self.mask, i64::from(value));
        let (start_data, end_data, start_bit) = self.locate(index);
        let value = i64::from(value) & self.mask;
        self.data[start_data] =
            self.data[start_data] & !(self.mask << start_bit) | value << start_bit;
        if start_data != end_data {
            let shift_bits = 64 - start_bit;
            let wanted_bits = self.bits - shift_bits;
            self.data[end_data] = (((self.data[end_data] as u64) >> wanted_bits) << wanted_bits)
                as i64
                | value >> shift_bits;
        }
    }

    /// `get(index)`.
    #[track_caller]
    pub fn get(&self, index: i32) -> i32 {
        inclusive_between(0, i64::from(self.size.wrapping_sub(1)), i64::from(index));
        let (start_data, end_data, start_bit) = self.locate(index);
        if start_data == end_data {
            (((self.data[start_data] as u64) >> start_bit) as i64 & self.mask) as i32
        } else {
            let shift_bits = 64 - start_bit;
            ((((self.data[start_data] as u64) >> start_bit) as i64
                | self.data[end_data] << shift_bits)
                & self.mask) as i32
        }
    }

    /// `getRaw()`.
    pub fn raw(&self) -> &[i64] {
        &self.data
    }

    /// `getRaw()`, consuming the storage.
    pub fn into_raw(self) -> Vec<i64> {
        self.data
    }

    /// `getBits()`.
    pub fn bits(&self) -> i32 {
        self.bits
    }
}
