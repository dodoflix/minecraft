//! Data fixing of region storage tags: `SimpleRegionStorage.upgradeChunkTag` (chunk, entity chunk and POI chunk tags) and
//! the data fix context tag of `ChunkMap.getChunkDataFixContextTag`.

use mc_datafix::{CURRENT_VERSION, DataFixTypes, data_fixer};
use mc_nbt::nbt_utils::{add_data_version, get_data_version_or};
use mc_nbt::{CompoundTag, Tag};
use std::fmt;

/// An update that failed (`ReportedException` titled `Updated chunk` with the category `Updated chunk details`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkUpgradeError {
    /// `Data version` of the details.
    pub data_version: i32,
    /// `Target version` of the details.
    pub target_version: i32,
    /// What the fixer threw.
    pub cause: String,
}

impl fmt::Display for ChunkUpgradeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.cause)
    }
}

impl std::error::Error for ChunkUpgradeError {}

/// `ChunkMap.getChunkDataFixContextTag(dimension, generatorIdentifier)`: the tag the chunk fixes read their dimension and
/// generator from.
pub fn chunk_data_fix_context_tag(dimension: &str, generator: Option<&str>) -> CompoundTag {
    let mut context = CompoundTag::new();
    context.put_string("dimension", dimension);
    if let Some(generator) = generator {
        context.put_string("generator", generator);
    }
    context
}

/// `SimpleRegionStorage.upgradeChunkTag(chunkTag, defaultVersion, dataFixContextTag, targetVersion)` for the tag type
/// `ty`: a tag already at `target_version` or newer is returned as it is (not stamped), an older one is fixed with the
/// context tag present during the update only and stamped with `target_version`.
pub fn upgrade_chunk_tag_to(
    ty: DataFixTypes,
    chunk_tag: CompoundTag,
    default_version: i32,
    data_fix_context: Option<&CompoundTag>,
    target_version: i32,
) -> Result<CompoundTag, ChunkUpgradeError> {
    upgrade_with(
        |tag, from, to| update(ty, tag, from, to),
        chunk_tag,
        default_version,
        data_fix_context,
        target_version,
    )
}

fn upgrade_with(
    update: impl FnOnce(&CompoundTag, i32, i32) -> Result<CompoundTag, Failure>,
    chunk_tag: CompoundTag,
    default_version: i32,
    data_fix_context: Option<&CompoundTag>,
    target_version: i32,
) -> Result<CompoundTag, ChunkUpgradeError> {
    let version = get_data_version_or(&chunk_tag, default_version);
    if version >= target_version {
        return Ok(chunk_tag);
    }
    let mut input = chunk_tag;
    if let Some(context) = data_fix_context {
        input.put("__context", context.clone());
    }
    let mut fixed = update(&input, version, target_version)
        .map_err(|failure| failure.into_error(version, target_version))?;
    if fixed.get("DataVersion").is_none() {
        // shortcut: the key added below is placed as if the fixer's result were a `HashMap` copy of its entries (what
        // `mergeToMap` leaves). That is the table of a rebuilt chunk; entity and POI tags without a `DataVersion`
        // whose root the fixer returned unrebuilt keep the table they were read with, and sort the key differently.
        fixed = fixed.into_shallow_copy();
    }
    fixed.remove("__context");
    add_data_version(&mut fixed, target_version);
    Ok(fixed)
}

/// `ChunkMap.upgradeChunkTag(tag)`: a chunk tag (`DataFixTypes.CHUNK`, default version -1) to the current version with
/// the data fix context of its dimension and generator.
pub fn upgrade_level_chunk_tag(
    chunk_tag: CompoundTag,
    context: &CompoundTag,
) -> Result<CompoundTag, ChunkUpgradeError> {
    upgrade_chunk_tag_to(
        DataFixTypes::CHUNK,
        chunk_tag,
        -1,
        Some(context),
        CURRENT_VERSION,
    )
}

/// What `new CollectFields(`DataVersion`, `Level.Structures.Starts`, `structures.starts`)` gathers while
/// `IOWorker.scanChunk` streams a chunk: those fields only, with the compounds on their way (empty when the rest of the
/// path is missing).
pub fn collect_scanned_fields(chunk: &CompoundTag) -> CompoundTag {
    fn path(tag: &CompoundTag, key: &str, leaf: &str) -> CompoundTag {
        let mut out = CompoundTag::new();
        if let Some(starts @ Tag::Compound(_)) = tag.get(leaf) {
            out.put(key, starts.clone());
        }
        out
    }
    let mut out = CompoundTag::new();
    for (key, value) in chunk.iter() {
        match (key.as_str(), value) {
            ("DataVersion", Tag::Int(_)) => {
                out.put(key.clone(), value.clone());
            }
            ("Level", Tag::Compound(level)) => {
                let mut collected = CompoundTag::new();
                if let Some(Tag::Compound(structures)) = level.get("Structures") {
                    collected.put("Structures", path(structures, "Starts", "Starts"));
                }
                out.put("Level", collected);
            }
            ("structures", Tag::Compound(structures)) => {
                out.put("structures", path(structures, "starts", "starts"));
            }
            _ => {}
        }
    }
    out
}

/// `StructureCheck.tryLoadFromStorage`: `DataFixTypes.CHUNK.updateToCurrentVersion(fixer, chunkTag, version)` of the
/// scanned fields of a chunk with the data fix context injected (kept in the result, the version not stamped).
pub fn update_scanned_chunk_tag(
    mut chunk_tag: CompoundTag,
    context: &CompoundTag,
) -> Result<CompoundTag, ChunkUpgradeError> {
    let version = get_data_version_or(&chunk_tag, -1);
    chunk_tag.put("__context", context.clone());
    if version >= CURRENT_VERSION {
        return Ok(chunk_tag);
    }
    update(DataFixTypes::CHUNK, &chunk_tag, version, CURRENT_VERSION)
        .map_err(|failure| failure.into_error(version, CURRENT_VERSION))
}

/// `PackedChunk.parse` of the POI storage: `upgradeChunkTag(tag, 1945)` (`DataFixTypes.POI_CHUNK`).
pub fn upgrade_poi_chunk_tag(chunk_tag: CompoundTag) -> Result<CompoundTag, ChunkUpgradeError> {
    upgrade_chunk_tag(DataFixTypes::POI_CHUNK, chunk_tag, 1945)
}

/// `upgradeChunkTag(chunkTag, defaultVersion)`: to the current version, without a context tag.
pub fn upgrade_chunk_tag(
    ty: DataFixTypes,
    chunk_tag: CompoundTag,
    default_version: i32,
) -> Result<CompoundTag, ChunkUpgradeError> {
    upgrade_chunk_tag_to(ty, chunk_tag, default_version, None, CURRENT_VERSION)
}

/// Why one update did not produce a tag.
struct Failure {
    cause: String,
}

impl Failure {
    fn thrown(cause: String) -> Self {
        Failure { cause }
    }

    fn into_error(self, data_version: i32, target_version: i32) -> ChunkUpgradeError {
        ChunkUpgradeError {
            data_version,
            target_version,
            cause: self.cause,
        }
    }
}

/// `DataFixTypes.update(fixer, tag, from, to)` with what the fixer throws as the error text.
fn update(ty: DataFixTypes, tag: &CompoundTag, from: i32, to: i32) -> Result<CompoundTag, Failure> {
    unwinding(|| ty.update_tag(data_fixer(), tag, from, to))
}

/// What `f` throws (a panic) as the error text.
fn unwinding(f: impl FnOnce() -> CompoundTag) -> Result<CompoundTag, Failure> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|payload| {
        Failure::thrown(
            payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("data fixing failed")
                .to_owned(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(version: Option<i32>) -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.put_string("Status", "minecraft:full");
        if let Some(version) = version {
            tag.put_int("DataVersion", version);
        }
        tag
    }

    #[test]
    fn a_failing_update_reports_both_versions_and_its_cause() {
        let error = upgrade_with(
            |_, _, _| Err(Failure::thrown("boom".to_owned())),
            tag(Some(4900)),
            -1,
            None,
            5023,
        )
        .unwrap_err();
        assert_eq!(
            error,
            ChunkUpgradeError {
                data_version: 4900,
                target_version: 5023,
                cause: "boom".to_owned(),
            }
        );
    }

    #[test]
    fn a_panicking_fix_is_an_error_not_a_crash() {
        let error = upgrade_with(
            |_, _, _| unwinding(|| panic!("java.lang.IllegalStateException: no value")),
            tag(Some(4900)),
            -1,
            None,
            5023,
        )
        .unwrap_err();
        assert_eq!(error.cause, "java.lang.IllegalStateException: no value");
    }

    #[test]
    fn the_context_is_present_for_the_update_and_gone_after() {
        let context = chunk_data_fix_context_tag("minecraft:overworld", Some("minecraft:noise"));
        let seen = std::cell::RefCell::new(None);
        let fixed = upgrade_with(
            |tag, _, _| {
                *seen.borrow_mut() = tag.get_compound("__context").cloned();
                Ok(tag.clone())
            },
            tag(Some(4900)),
            -1,
            Some(&context),
            5023,
        )
        .unwrap();
        assert_eq!(seen.into_inner(), Some(context));
        assert!(fixed.get("__context").is_none());
        assert_eq!(fixed.get_int("DataVersion"), Some(5023));
    }

    #[test]
    fn a_tag_at_or_above_the_target_is_returned_untouched() {
        for version in [Some(5023), Some(6000), None] {
            let input = tag(version);
            let default = if version.is_none() { 5023 } else { -1 };
            let out = upgrade_with(
                |_, _, _| panic!("no update runs"),
                input.clone(),
                default,
                Some(&CompoundTag::new()),
                5023,
            )
            .unwrap();
            assert_eq!(out, input, "{version:?}");
            assert_eq!(out.get_int("DataVersion"), version);
        }
    }
}
