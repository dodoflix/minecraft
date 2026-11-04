//! Static vanilla data for the 26.3 server: registries (name ↔ protocol id) with typed id newtypes, blocks and block
//! states (global state ids, typed properties, per-state data, collision/occlusion shapes as raw data), fluids, items,
//! entity types, the small code-defined registries, plus the embedded vanilla data pack, tags and the `en_us` language file.
//!
//! Everything is generated at build time by `build.rs` from `ref/extracted` (see `tools/extractor`) — run
//! `scripts/setup-ref.sh` before building. The extract schema is documented in `STATUS.md`.

#[macro_use]
pub mod registry;
pub mod block;
pub mod datapack;
pub mod entity_type;
pub mod enums;
pub mod fluid;
pub mod ids;
pub mod item;
pub mod lang;
pub mod misc;
pub mod props;
pub mod registry_data;
pub mod shape;
pub mod voxel;
pub mod tags;

pub use ids::*;
