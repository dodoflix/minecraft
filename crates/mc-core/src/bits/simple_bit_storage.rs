use std::fmt;

use super::{BitStorage, inclusive_between};

/// `{divide_mul, divide_add, divide_shift}` per values-per-long (1..=64): `index / values_per_long` as multiply-shift.
const MAGIC: [i32; 192] = [
    -1,
    -1,
    0,
    i32::MIN,
    0,
    0,
    1431655765,
    1431655765,
    0,
    i32::MIN,
    0,
    1,
    858993459,
    858993459,
    0,
    715827882,
    715827882,
    0,
    613566756,
    613566756,
    0,
    i32::MIN,
    0,
    2,
    477218588,
    477218588,
    0,
    429496729,
    429496729,
    0,
    390451572,
    390451572,
    0,
    357913941,
    357913941,
    0,
    330382099,
    330382099,
    0,
    306783378,
    306783378,
    0,
    286331153,
    286331153,
    0,
    i32::MIN,
    0,
    3,
    252645135,
    252645135,
    0,
    238609294,
    238609294,
    0,
    226050910,
    226050910,
    0,
    214748364,
    214748364,
    0,
    204522252,
    204522252,
    0,
    195225786,
    195225786,
    0,
    186737708,
    186737708,
    0,
    178956970,
    178956970,
    0,
    171798691,
    171798691,
    0,
    165191049,
    165191049,
    0,
    159072862,
    159072862,
    0,
    153391689,
    153391689,
    0,
    148102320,
    148102320,
    0,
    143165576,
    143165576,
    0,
    138547332,
    138547332,
    0,
    i32::MIN,
    0,
    4,
    130150524,
    130150524,
    0,
    126322567,
    126322567,
    0,
    122713351,
    122713351,
    0,
    119304647,
    119304647,
    0,
    116080197,
    116080197,
    0,
    113025455,
    113025455,
    0,
    110127366,
    110127366,
    0,
    107374182,
    107374182,
    0,
    104755299,
    104755299,
    0,
    102261126,
    102261126,
    0,
    99882960,
    99882960,
    0,
    97612893,
    97612893,
    0,
    95443717,
    95443717,
    0,
    93368854,
    93368854,
    0,
    91382282,
    91382282,
    0,
    89478485,
    89478485,
    0,
    87652393,
    87652393,
    0,
    85899345,
    85899345,
    0,
    84215045,
    84215045,
    0,
    82595524,
    82595524,
    0,
    81037118,
    81037118,
    0,
    79536431,
    79536431,
    0,
    78090314,
    78090314,
    0,
    76695844,
    76695844,
    0,
    75350303,
    75350303,
    0,
    74051160,
    74051160,
    0,
    72796055,
    72796055,
    0,
    71582788,
    71582788,
    0,
    70409299,
    70409299,
    0,
    69273666,
    69273666,
    0,
    68174084,
    68174084,
    0,
    i32::MIN,
    0,
    5,
];

/// `SimpleBitStorage.InitializationException`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitializationException(pub String);

impl fmt::Display for InitializationException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InitializationException {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimpleBitStorage {
    data: Vec<i64>,
    bits: i32,
    mask: i64,
    size: usize,
    values_per_long: i32,
    divide_mul: i32,
    divide_add: i32,
    divide_shift: i32,
}

impl SimpleBitStorage {
    /// `new SimpleBitStorage(bits, size)`; panics unless `1 <= bits <= 32`.
    pub fn new(bits: i32, size: usize) -> Self {
        Self::with_optional_data(bits, size, None).expect("no data given")
    }

    /// `new SimpleBitStorage(bits, size, long[] data)`; errors if `data.len()` is not the required length.
    pub fn with_data(
        bits: i32,
        size: usize,
        data: Vec<i64>,
    ) -> Result<Self, InitializationException> {
        Self::with_optional_data(bits, size, Some(data))
    }

    /// `new SimpleBitStorage(bits, size, int[] values)`
    pub fn from_values(bits: i32, size: usize, values: &[i32]) -> Self {
        let mut s = Self::new(bits, size);
        let vpl = s.values_per_long as usize;
        let mut output_index = 0;
        let mut input_offset = 0;
        while input_offset + vpl <= size {
            let mut packed = 0i64;
            for index_in_long in (0..vpl).rev() {
                packed <<= bits;
                packed |= i64::from(values[input_offset + index_in_long]) & s.mask;
            }
            s.data[output_index] = packed;
            output_index += 1;
            input_offset += vpl;
        }
        let remainder = size - input_offset;
        if remainder > 0 {
            let mut last = 0i64;
            for index_in_long in (0..remainder).rev() {
                last <<= bits;
                last |= i64::from(values[input_offset + index_in_long]) & s.mask;
            }
            s.data[output_index] = last;
        }
        s
    }

    fn with_optional_data(
        bits: i32,
        size: usize,
        data: Option<Vec<i64>>,
    ) -> Result<Self, InitializationException> {
        inclusive_between(1, 32, bits.into());
        let mask = (1i64 << bits) - 1;
        let values_per_long = 64 / bits;
        let row = 3 * (values_per_long as usize - 1);
        let required_length = (size + values_per_long as usize - 1) / values_per_long as usize;
        let data = match data {
            Some(d) if d.len() != required_length => {
                return Err(InitializationException(format!(
                    "Invalid length given for storage, got: {} but expected: {}",
                    d.len(),
                    required_length
                )));
            }
            Some(d) => d,
            None => vec![0; required_length],
        };
        Ok(Self {
            data,
            bits,
            mask,
            size,
            values_per_long,
            divide_mul: MAGIC[row],
            divide_add: MAGIC[row + 1],
            divide_shift: MAGIC[row + 2],
        })
    }

    fn cell_index(&self, bit_index: usize) -> usize {
        let mul = i64::from(self.divide_mul as u32);
        let add = i64::from(self.divide_add as u32);
        // the `int` index is widened to long before the multiply, as in vanilla
        ((bit_index as i64).wrapping_mul(mul).wrapping_add(add) >> 32 >> self.divide_shift) as i32
            as usize
    }

    fn check(&self, index: usize, value: Option<i32>) {
        inclusive_between(0, self.size as i64 - 1, index as i64);
        if let Some(v) = value {
            inclusive_between(0, self.mask, v.into());
        }
    }
}

impl BitStorage for SimpleBitStorage {
    fn get_and_set(&mut self, index: usize, value: i32) -> i32 {
        self.check(index, Some(value));
        let cell_index = self.cell_index(index);
        let cell_value = self.data[cell_index];
        let bit_index = (index - cell_index * self.values_per_long as usize) as i32 * self.bits;
        let old = (cell_value >> bit_index & self.mask) as i32;
        self.data[cell_index] =
            cell_value & !(self.mask << bit_index) | (i64::from(value) & self.mask) << bit_index;
        old
    }

    fn set(&mut self, index: usize, value: i32) {
        self.check(index, Some(value));
        let cell_index = self.cell_index(index);
        let cell_value = self.data[cell_index];
        let bit_index = (index - cell_index * self.values_per_long as usize) as i32 * self.bits;
        self.data[cell_index] =
            cell_value & !(self.mask << bit_index) | (i64::from(value) & self.mask) << bit_index;
    }

    fn fill(&mut self, value: i32) {
        inclusive_between(0, self.mask, value.into());
        let mut cell_value = i64::from(value) & self.mask;
        let mut offset = self.bits;
        loop {
            cell_value |= cell_value << offset;
            offset <<= 1;
            if offset >= 64 {
                break;
            }
        }
        self.data.fill(cell_value);
    }

    fn get(&self, index: usize) -> i32 {
        self.check(index, None);
        let cell_index = self.cell_index(index);
        let cell_value = self.data[cell_index];
        let bit_index = (index - cell_index * self.values_per_long as usize) as i32 * self.bits;
        (cell_value >> bit_index & self.mask) as i32
    }

    fn get_raw(&self) -> &[i64] {
        &self.data
    }

    fn get_size(&self) -> usize {
        self.size
    }

    fn get_bits(&self) -> i32 {
        self.bits
    }

    fn get_all(&self, output: &mut dyn FnMut(i32)) {
        let mut count = 0;
        for &cell in &self.data {
            let mut cell_value = cell;
            for _ in 0..self.values_per_long {
                output((cell_value & self.mask) as i32);
                cell_value >>= self.bits;
                count += 1;
                if count >= self.size {
                    return;
                }
            }
        }
    }

    fn unpack(&self, output: &mut [i32]) {
        let data_length = self.data.len();
        let vpl = self.values_per_long as usize;
        let mut output_offset = 0;
        for i in 0..data_length.saturating_sub(1) {
            let mut cell_value = self.data[i];
            for index_in_long in 0..vpl {
                output[output_offset + index_in_long] = (cell_value & self.mask) as i32;
                cell_value >>= self.bits;
            }
            output_offset += vpl;
        }
        let remainder = self.size - output_offset;
        if remainder > 0 {
            let mut cell_value = self.data[data_length - 1];
            for index_in_long in 0..remainder {
                output[output_offset + index_in_long] = (cell_value & self.mask) as i32;
                cell_value >>= self.bits;
            }
        }
    }

    fn copy(&self) -> Box<dyn BitStorage> {
        Box::new(self.clone())
    }
}
