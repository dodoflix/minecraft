//! The JSON codec primitives shared with the gameplay-side worldgen data (features, structures): DFU-style
//! `fieldOf`/`optionalFieldOf` record access, `Codec.INT/FLOAT/...` parsers and the error type with field paths.

pub use crate::data::LoadError;
pub use crate::densityfunction::codec::{
    Fields, LoadResult, parse_bool, parse_double_range, parse_f32, parse_f64, parse_identifier, parse_int,
    parse_int_range, parse_list, parse_noise_value, parse_positive_int, parse_str,
};
