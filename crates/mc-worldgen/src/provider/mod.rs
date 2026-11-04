//! Value providers: `net.minecraft.util.valueproviders.{IntProvider, FloatProvider}` (+ their `*Providers`
//! codecs), `levelgen.heightproviders.HeightProvider` and `util.random.{Weighted, WeightedList}`.
//!
//! Every provider samples from a `&mut dyn RandomSource` with exactly vanilla's RNG call order.

pub mod float_provider;
pub mod height_provider;
pub mod int_provider;
pub mod weighted;

pub use float_provider::FloatProvider;
pub use height_provider::HeightProvider;
pub use int_provider::IntProvider;
pub use weighted::{Weighted, WeightedList};

use serde_json::Value;

use crate::data::LoadError;
use crate::densityfunction::codec::{Fields, LoadResult, parse_identifier};

/// `registry.byNameCodec().dispatch(...)`: the object's `type` names a `minecraft:` registry entry whose
/// `MapCodec` then reads the same object. Returns the entry path and the object's fields.
pub(crate) fn dispatch_type(value: &Value) -> LoadResult<(String, Fields<'_>)> {
    let fields = Fields::new(value)?;
    let ty = fields.required("type", parse_identifier)?;
    if ty.namespace() != "minecraft" {
        return Err(LoadError::new(format!("Unknown registry key: {ty}")).at("type"));
    }
    Ok((ty.path().to_string(), fields))
}

pub(crate) fn unknown_type(path: &str) -> LoadError {
    LoadError::new(format!("Unknown registry key: minecraft:{path}")).at("type")
}
