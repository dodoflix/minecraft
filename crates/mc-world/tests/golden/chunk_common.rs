//! Shared helper for the chunk-level tests: a paletted-container factory over the vanilla biome list in `ref/`.
#![allow(
    dead_code,
    reason = "each test target uses a subset of the shared helpers"
)]

use mc_world::palette::{BiomeRegistry, PalettedContainerFactory};
use std::path::Path;

pub fn factory() -> PalettedContainerFactory {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ref/generated/data/minecraft/worldgen/biome");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("ref/ is missing: run scripts/setup-ref.sh")
        .map(|e| {
            format!(
                "minecraft:{}",
                e.unwrap()
                    .file_name()
                    .to_str()
                    .unwrap()
                    .trim_end_matches(".json")
            )
        })
        .collect();
    names.sort();
    PalettedContainerFactory::create(BiomeRegistry::new(names)).unwrap()
}
