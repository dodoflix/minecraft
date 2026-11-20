//! `net.minecraft.util.worldupdate.WorldUpgrader`: a `World Upgrader #n` thread that upgrades the entities, points of
//! interest and chunks of every dimension, while the caller polls the progress.

use super::region_storage_upgrader::{Builder, UpgradeError};
use super::status;
use mc_core::ident::Identifier;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_datafix::{CURRENT_VERSION, DataFixTypes};
use mc_nbt::{CompoundTag, Tag};
use mc_world::filefix::{Status, Type, UpgradeProgress};
use mc_world::region::upgrade::chunk_data_fix_context_tag;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::JoinHandle;
use std::time::Instant;

/// `THREAD_FACTORY`: the `%d` of the thread name counts up from 0 over all upgraders of the process.
static THREAD_COUNT: AtomicU32 = AtomicU32::new(0);

/// One dimension of the world: its key, its folder and the type id of its chunk generator
/// (`generator.getTypeNameForDataFixer()`).
#[derive(Clone)]
pub struct UpgradeDimension {
    pub key: Identifier,
    pub dir: PathBuf,
    pub generator: Option<Identifier>,
}

/// What the upgrade thread owns.
struct Work {
    level_id: String,
    levels: Vec<UpgradeDimension>,
    erase_cache: bool,
    recreate_region_files: bool,
    upgrade_progress: Arc<UpgradeProgress>,
}

pub struct WorldUpgrader {
    upgrade_progress: Arc<UpgradeProgress>,
    thread: Option<JoinHandle<()>>,
}

/// `getDataFixContextTag(dimensions, dimension)`.
fn get_data_fix_context_tag(level: &UpgradeDimension) -> CompoundTag {
    chunk_data_fix_context_tag(
        &level.key.to_string(),
        level.generator.as_ref().map(ToString::to_string).as_deref(),
    )
}

/// `verifyChunkPosAndEraseCache(pos, upgradedTag)`: `--eraseCache` drops the heightmaps and the light of a chunk.
pub fn verify_chunk_pos_and_erase_cache(pos: ChunkPos, upgraded_tag: &mut CompoundTag) -> bool {
    verify_chunk_pos(pos, upgraded_tag);
    let mut changed = upgraded_tag.contains("Heightmaps");
    upgraded_tag.remove("Heightmaps");
    changed = changed || upgraded_tag.contains("isLightOn");
    upgraded_tag.remove("isLightOn");
    if let Some(sections) = upgraded_tag.get_list_mut("sections") {
        for section in sections.iter_mut() {
            if let Tag::Compound(section) = section {
                changed = changed || section.contains("BlockLight");
                section.remove("BlockLight");
                changed = changed || section.contains("SkyLight");
                section.remove("SkyLight");
            }
        }
    }
    changed
}

/// `verifyChunkPos(pos, upgradedTag)`: warns about a chunk that is stored in the wrong slot; never changes it.
pub fn verify_chunk_pos(pos: ChunkPos, upgraded_tag: &mut CompoundTag) -> bool {
    let stored_pos = ChunkPos::new(
        upgraded_tag.get_int_or("xPos", 0),
        upgraded_tag.get_int_or("zPos", 0),
    );
    if stored_pos != pos {
        log::warn!("Chunk {pos} has invalid position {stored_pos}");
    }
    false
}

impl WorldUpgrader {
    /// `new WorldUpgrader(levelSource, dataFixer, registryAccess, eraseCache, recreateRegionFiles)`: starts the thread.
    pub fn new(
        level_id: String,
        levels: Vec<UpgradeDimension>,
        erase_cache: bool,
        recreate_region_files: bool,
    ) -> WorldUpgrader {
        let upgrade_progress = Arc::new(UpgradeProgress::default());
        let work = Work {
            level_id,
            levels,
            erase_cache,
            recreate_region_files,
            upgrade_progress: upgrade_progress.clone(),
        };
        let name = format!(
            "World Upgrader #{}",
            THREAD_COUNT.fetch_add(1, Ordering::Relaxed)
        );
        let thread = std::thread::Builder::new()
            .name(name)
            .spawn(move || {
                // the uncaught exception handler of the thread
                let failure = match catch_unwind(AssertUnwindSafe(|| work.work())) {
                    Ok(Ok(())) => return,
                    Ok(Err(e)) => e.to_string(),
                    Err(payload) => payload
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| payload.downcast_ref::<&str>().copied())
                        .unwrap_or("the world upgrade panicked")
                        .to_owned(),
                };
                log::error!("Error upgrading world\n{failure}");
                work.upgrade_progress.set_status(Status::Failed);
                work.upgrade_progress.set_finished(true);
            })
            .expect("spawn the world upgrader thread");
        WorldUpgrader {
            upgrade_progress,
            thread: Some(thread),
        }
    }

    /// `cancel()`: stops after the file being upgraded and waits for the thread.
    pub fn cancel(&mut self) {
        self.upgrade_progress.set_canceled();
        self.join();
    }

    fn join(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    pub fn is_finished(&self) -> bool {
        self.upgrade_progress.is_finished()
    }

    pub fn get_total_progress(&self) -> f32 {
        self.upgrade_progress.total_progress()
    }

    pub fn dimension_progress(&self, dimension: &Identifier) -> f32 {
        self.upgrade_progress.dimension_progress(dimension)
    }

    pub fn get_total_chunks(&self) -> i32 {
        self.upgrade_progress.total_chunks()
    }

    pub fn get_converted(&self) -> i32 {
        self.upgrade_progress.converted()
    }

    pub fn get_skipped(&self) -> i32 {
        self.upgrade_progress.skipped()
    }

    /// `getStatus()`: the translation key of the stage line (see [`status::translate`]).
    pub fn get_status(&self) -> &'static str {
        status::translate(&self.upgrade_progress)
    }
}

impl Drop for WorldUpgrader {
    fn drop(&mut self) {
        // the thread is a daemon in vanilla; it ends right after it flags the upgrade finished
        if self.is_finished() {
            self.join();
        }
    }
}

impl Work {
    fn work(&self) -> Result<(), UpgradeError> {
        let started = Instant::now();
        let current_version = CURRENT_VERSION;
        log::info!("Upgrading entities");
        self.upgrade_levels(
            DataFixTypes::ENTITY_CHUNK,
            Builder::default()
                .set_type_and_folder_name("entities")
                .set_recreate_region_files(self.recreate_region_files)
                .track_progress(self.upgrade_progress.clone()),
            |builder, _| builder,
        )?;
        log::info!("Upgrading POIs");
        self.upgrade_levels(
            DataFixTypes::POI_CHUNK,
            Builder::default()
                .set_type_and_folder_name("poi")
                .set_default_version(1945)
                .set_recreate_region_files(self.recreate_region_files)
                .track_progress(self.upgrade_progress.clone()),
            |builder, _| builder,
        )?;
        log::info!("Upgrading blocks");
        self.upgrade_levels(
            DataFixTypes::CHUNK,
            Builder::default()
                .set_type("chunk")
                .set_folder_name("region")
                .set_recreate_region_files(self.recreate_region_files)
                .track_progress(self.upgrade_progress.clone()),
            |builder, level| {
                builder
                    .set_data_fix_context_tag(Some(get_data_fix_context_tag(level)))
                    .add_tag_modifier(
                        current_version,
                        if self.erase_cache {
                            verify_chunk_pos_and_erase_cache
                        } else {
                            verify_chunk_pos
                        },
                    )
            },
        )?;
        // `overworldSavedDataStorage.saveAndJoin()`: nothing is ever read through that storage, so there is nothing to save
        log::info!(
            "World optimization finished after {} seconds",
            started.elapsed().as_millis() / 1000
        );
        self.upgrade_progress.set_finished(true);
        Ok(())
    }

    fn upgrade_levels(
        &self,
        data_fix_type: DataFixTypes,
        builder: Builder,
        level_specific_builder: impl Fn(Builder, &UpgradeDimension) -> Builder,
    ) -> Result<(), UpgradeError> {
        let mut upgraders = Vec::new();
        self.upgrade_progress.reset(data_fix_type);
        self.upgrade_progress.set_type(Type::Regions);
        let builder = builder.set_data_fix_type(data_fix_type);
        let mut previous_copies_file_amounts = 0;
        for level in &self.levels {
            let mut upgrader =
                level_specific_builder(builder.clone(), level).build(previous_copies_file_amounts);
            upgrader.init(&level.key, &self.level_id, &level.dir)?;
            previous_copies_file_amounts += upgrader.file_amount();
            upgraders.push(upgrader);
        }
        for upgrader in &mut upgraders {
            upgrader.upgrade()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(x: i32, z: i32) -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.put_int("xPos", x);
        tag.put_int("zPos", z);
        tag
    }

    #[test]
    fn erase_cache_removes_the_cached_fields_and_reports_whether_it_did() {
        let mut tag = chunk(1, 2);
        tag.put_byte("isLightOn", 1);
        let mut sections = mc_nbt::ListTag::new();
        let mut section = CompoundTag::new();
        section.put_byte("Y", 0);
        section.put_byte_array("SkyLight", vec![0; 4]);
        sections.push(section);
        sections.push(CompoundTag::new());
        tag.put("sections", sections);
        assert!(verify_chunk_pos_and_erase_cache(
            ChunkPos::new(1, 2),
            &mut tag
        ));
        assert!(tag.get("isLightOn").is_none());
        let sections = tag.get_list("sections").unwrap();
        assert!(sections.get_compound(0).unwrap().get("SkyLight").is_none());
        assert!(sections.get_compound(0).unwrap().get("Y").is_some());
        assert!(!verify_chunk_pos_and_erase_cache(
            ChunkPos::new(1, 2),
            &mut tag
        ));
    }

    #[test]
    fn a_misplaced_chunk_is_not_a_change() {
        let mut tag = chunk(5, 5);
        assert!(!verify_chunk_pos(ChunkPos::new(1, 2), &mut tag));
        assert_eq!(tag, chunk(5, 5));
    }
}
