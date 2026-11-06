//! `DimensionPadding`.

use mc_game_core::world::level::levelgen::codec::*;
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub struct DimensionPadding {
    pub bottom: i32,
    pub top: i32,
    /// Vanilla compares against the `ZERO` constant by reference (`dimensionPadding == DimensionPadding.ZERO`): only the
    /// default instance is the constant, a decoded `0` is a fresh instance.
    is_zero_constant: bool,
}

impl DimensionPadding {
    /// `DimensionPadding.ZERO`.
    pub const ZERO: DimensionPadding = DimensionPadding {
        bottom: 0,
        top: 0,
        is_zero_constant: true,
    };

    /// `new DimensionPadding(bottom, top)`.
    pub const fn new(bottom: i32, top: i32) -> DimensionPadding {
        DimensionPadding {
            bottom,
            top,
            is_zero_constant: false,
        }
    }

    /// `this == DimensionPadding.ZERO`.
    pub fn is_zero_constant(&self) -> bool {
        self.is_zero_constant
    }

    /// `hasEqualTopAndBottom()`.
    pub fn has_equal_top_and_bottom(&self) -> bool {
        self.top == self.bottom
    }

    /// `ExtraCodecs.NON_NEGATIVE_INT`.
    fn parse_non_negative(value: &Value) -> LoadResult<i32> {
        let v = parse_int(value)?;
        if v < 0 {
            Err(LoadError::new(format!("Value must be non-negative: {v}")))
        } else {
            Ok(v)
        }
    }

    /// `DimensionPadding.CODEC`: `Codec.either(NON_NEGATIVE_INT, RECORD_CODEC)`.
    pub fn from_json(value: &Value) -> LoadResult<DimensionPadding> {
        let first = match Self::parse_non_negative(value) {
            Ok(v) => return Ok(DimensionPadding::new(v, v)),
            Err(e) => e,
        };
        let second = Fields::new(value).map(|f| {
            // `lenientOptionalFieldOf`: an invalid value falls back to the default
            let lenient = |name: &str| {
                f.get(name)
                    .and_then(|v| Self::parse_non_negative(v).ok())
                    .unwrap_or(0)
            };
            DimensionPadding::new(lenient("bottom"), lenient("top"))
        });
        second.map_err(|second| {
            LoadError::new(format!(
                "Failed to parse either. First: {first}; Second: {second}"
            ))
        })
    }
}
