//! `net.minecraft.util.worldupdate` and the driver loop of `Main.forceUpgrade`: `--forceUpgrade`, `--eraseCache` and
//! `--recreateRegionFiles` upgrade every chunk, entity chunk and point-of-interest chunk of the world to the current data
//! version before the server starts.

mod region_storage_upgrader;
mod status;
mod upgrader;

use crate::registry::{RegistryAccess, keys};
use mc_world::level_storage::LevelStorageAccess;
use std::time::Duration;

pub use region_storage_upgrader::{Builder, RegionStorageUpgrader, TagModifier, UpgradeError};
pub use upgrader::{
    UpgradeDimension, WorldUpgrader, verify_chunk_pos, verify_chunk_pos_and_erase_cache,
};

/// The dimensions of the `dimension` registry with their folders and generator type ids (what `WorldUpgrader`'s
/// constructor reads from the registry access).
pub fn upgrade_dimensions(
    access: &LevelStorageAccess,
    registries: &RegistryAccess,
) -> Result<Vec<UpgradeDimension>, String> {
    registries
        .registry_or_throw(keys::LEVEL_STEM)
        .entries()
        .iter()
        .map(|entry| {
            let stem = registries
                .level_stem(&entry.id)
                .ok_or_else(|| format!("The dimension {} is not a level stem", entry.id))?;
            Ok(UpgradeDimension {
                key: entry.id.clone(),
                dir: access
                    .get_dimension_path(&entry.id)
                    .map_err(|e| e.to_string())?,
                generator: Some(stem.generator.type_name()),
            })
        })
        .collect()
}

/// `Main.forceUpgrade(storageSource, fixerUpper, eraseCache, () -> true, registryAccess, recreateRegionFiles)`: starts the
/// upgrade and logs its stage and the percentage of chunks done once a second until it is finished.
pub fn force_upgrade(
    access: &LevelStorageAccess,
    dimensions: Vec<UpgradeDimension>,
    erase_cache: bool,
    recreate_region_files: bool,
) {
    log::info!("Forcing world upgrade!");
    let upgrader = WorldUpgrader::new(
        access.get_level_id().to_owned(),
        dimensions,
        erase_cache,
        recreate_region_files,
    );
    let mut last_status = None;
    while !upgrader.is_finished() {
        let stage = upgrader.get_status();
        // vanilla compares the status components by reference: one key is one component
        if last_status != Some(stage) {
            last_status = Some(stage);
            log::info!("{}", status::text(stage));
        }
        let total_chunks = upgrader.get_total_chunks();
        if total_chunks > 0 {
            let done = upgrader.get_converted() + upgrader.get_skipped();
            log::info!(
                "{}% completed ({} / {} chunks)...",
                (done as f32 / total_chunks as f32 * 100.0).floor() as i32,
                done,
                total_chunks
            );
        }
        std::thread::sleep(Duration::from_millis(1000));
    }
}

#[cfg(test)]
mod tests;
