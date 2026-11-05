//! Oracle tests (T2): checks against the vanilla JVM or the vanilla-generated reference world; one module per topic.
//! Every test is `#[ignore]`d; run them with `cargo test -p mc-world --test oracle -- --ignored`.

mod chunk_common;
mod chunk_vanilla;
mod chunk_worldref;
// shared with the golden target: the helpers are the same code, not a second copy
#[path = "../golden/height_status_common/mod.rs"]
mod height_status_common;
mod heightmap_worldref;
mod level_storage_vanilla;
mod level_storage_world;
#[path = "../golden/region_common/mod.rs"]
mod region_common;
mod region_file;
mod region_lz4;
mod region_oracle;
mod region_storage;
mod region_worldref;
mod ticks_worldref;
