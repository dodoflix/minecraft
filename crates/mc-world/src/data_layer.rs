//! `net.minecraft.world.level.chunk.DataLayer`: a 4096-entry nibble array (light levels), lazily materialized.

use std::borrow::Cow;
use std::fmt;

pub const LAYER_COUNT: i32 = 16;
pub const LAYER_SIZE: i32 = 128;
pub const SIZE: usize = 2048;

/// `new DataLayer(byte[])` with `data.length != 2048` (vanilla throws `IllegalArgumentException`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InvalidSize(pub usize);

impl fmt::Display for InvalidSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DataLayer should be 2048 bytes not: {}", self.0)
    }
}

impl std::error::Error for InvalidSize {}

/// Until a nibble is set (or `get_data` is called) the layer is "filled with `default_value`" and owns no array.
#[derive(Clone, Debug)]
pub struct DataLayer {
    data: Option<Box<[u8; SIZE]>>,
    default_value: i32,
}

impl Default for DataLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl DataLayer {
    /// `new DataLayer()`.
    pub fn new() -> DataLayer {
        Self::filled(0)
    }

    /// `new DataLayer(int defaultValue)`: e.g. `filled(15)` for full skylight.
    pub fn filled(default_value: i32) -> DataLayer {
        DataLayer {
            data: None,
            default_value,
        }
    }

    /// `new DataLayer(byte[])`.
    pub fn from_bytes(data: Vec<u8>) -> Result<DataLayer, InvalidSize> {
        let data: Box<[u8; SIZE]> = data
            .into_boxed_slice()
            .try_into()
            .map_err(|d: Box<[u8]>| InvalidSize(d.len()))?;
        Ok(DataLayer {
            data: Some(data),
            default_value: 0,
        })
    }

    fn get_index(x: i32, y: i32, z: i32) -> usize {
        (y << 8 | z << 4 | x) as usize
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> i32 {
        self.get_at(Self::get_index(x, y, z))
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, val: i32) {
        self.set_at(Self::get_index(x, y, z), val);
    }

    fn get_at(&self, index: usize) -> i32 {
        match &self.data {
            None => self.default_value,
            Some(data) => i32::from(data[index >> 1] >> (4 * (index & 1)) & 15),
        }
    }

    fn set_at(&mut self, index: usize, val: i32) {
        let nibble = 4 * (index & 1);
        let byte = &mut self.get_data()[index >> 1];
        *byte = *byte & !(15 << nibble) | ((val & 15) as u8) << nibble;
    }

    /// `fill(value)`: drops the array and makes the layer homogeneous again.
    pub fn fill(&mut self, value: i32) {
        self.default_value = value;
        self.data = None;
    }

    /// `packFilled`: `(byte) value | value << 4` in Java `int` arithmetic, narrowed to a byte.
    fn pack_filled(value: i32) -> u8 {
        (i32::from(value as i8) | value.wrapping_shl(4)) as u8
    }

    /// `getData()`: materializes the 2048-byte array (filled with the default value's nibble pair) if needed.
    pub fn get_data(&mut self) -> &mut [u8; SIZE] {
        let default_value = self.default_value;
        self.data.get_or_insert_with(|| {
            let mut data = Box::new([0u8; SIZE]);
            if default_value != 0 {
                data.fill(Self::pack_filled(default_value));
            }
            data
        })
    }

    /// `getData()` without mutating `self`: borrows the array, or builds the filled one on the fly.
    pub fn data(&self) -> Cow<'_, [u8; SIZE]> {
        match &self.data {
            Some(data) => Cow::Borrowed(&**data),
            None => {
                let mut copy = Self::filled(self.default_value);
                Cow::Owned(*copy.get_data())
            }
        }
    }

    /// `copy()`.
    pub fn copy(&self) -> DataLayer {
        self.clone()
    }

    /// `layerToString(layer)`; like vanilla it ignores `layer` and always dumps the first 256 entries.
    pub fn layer_to_string(&self, _layer: i32) -> String {
        let mut out = String::new();
        for i in 0..256usize {
            out.push_str(&format!("{:x}", self.get_at(i) as u32));
            if i & 15 == 15 {
                out.push('\n');
            }
        }
        out
    }

    /// `isDefinitelyHomogenous()`.
    pub fn is_definitely_homogenous(&self) -> bool {
        self.data.is_none()
    }

    /// `isDefinitelyFilledWith(value)`.
    pub fn is_definitely_filled_with(&self, value: i32) -> bool {
        self.data.is_none() && self.default_value == value
    }

    /// `isEmpty()`: no array and default value 0.
    pub fn is_empty(&self) -> bool {
        self.data.is_none() && self.default_value == 0
    }
}

/// `toString()`.
impl fmt::Display for DataLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..4096usize {
            write!(f, "{:x}", self.get_at(i) as u32)?;
            if i & 15 == 15 {
                f.write_str("\n")?;
            }
            if i & 0xFF == 255 {
                f.write_str("\n")?;
            }
        }
        Ok(())
    }
}
