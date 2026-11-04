//! `levelgen/blending` + `BelowZeroRetrogen`: blending of new terrain into old (pre-1.18) chunks.

mod blender;
mod below_zero_retrogen;
pub mod data;
mod fastutil;
pub mod region;

pub use below_zero_retrogen::{apply_bedrock_mask, get_biome_resolver as get_below_zero_biome_resolver, replace_old_bedrock};
pub use blender::{
    BlendingOutput, Blender, CarvingFilter, DistanceGetter, OutputBuffer, generate_border_ticks, make_old_chunk_distance_getter,
};
pub use data::{BlendingDataExt, get_or_update_blending_data, side_by_generation_age};
pub use region::{BlendingChunk, BlendingRegion, ChunkView};
