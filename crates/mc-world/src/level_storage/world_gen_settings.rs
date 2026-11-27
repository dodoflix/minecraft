//! `WorldOptions` + `WorldGenSettings` (`data/minecraft/world_gen_settings.dat`): seed, structure / bonus
//! chest flags and the `dimensions` map (its `LevelStem` values stay NBT; the caller supplies the stem codec).

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use mc_core::ident::Identifier;
use mc_core::java_exception::JavaException;
use mc_core::java_string::JavaString;
use mc_dfu::extra::{IDENTIFIER, optional_always_present_field_of};
use mc_dfu::{BOOL, Codec, DataResult, Dyn, Dynamic, LONG, NBT, PASSTHROUGH, STRING};
use mc_nbt::{CompoundTag, NbtAccounter, Tag};

use super::file_util::read_compressed;
use super::level_data::SpecialWorldProperty;
use super::saved_data_storage::SavedDataStorage;

/// `WorldGenSettings.TYPE.id()`.
pub fn world_gen_settings_id() -> Identifier {
    Identifier::with_default_namespace("world_gen_settings").expect("valid id")
}

/// `WorldOptions`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WorldOptions {
    pub seed: i64,
    pub generate_structures: bool,
    pub generate_bonus_chest: bool,
    pub legacy_custom_options: Option<String>,
}

impl WorldOptions {
    pub fn new(seed: i64, generate_structures: bool, generate_bonus_chest: bool) -> WorldOptions {
        WorldOptions {
            seed,
            generate_structures,
            generate_bonus_chest,
            legacy_custom_options: None,
        }
    }

    pub fn is_old_customized_world(&self) -> bool {
        self.legacy_custom_options.is_some()
    }

    /// `WorldOptions.parseSeed`: `None` for blank, the long when numeric, else `String.hashCode()` (of the trimmed string,
    /// which may hold a lone surrogate).
    pub fn parse_seed(seed_string: &JavaString) -> Option<i64> {
        let units: Vec<u16> = seed_string.encode_utf16().collect();
        // `String.trim`: every unit up to U+0020 at either end
        let start = units.iter().position(|&u| u > 0x20)?;
        let end = units.iter().rposition(|&u| u > 0x20)? + 1;
        let trimmed = JavaString::from_utf16_units(&units[start..end]);
        Some(
            java_parse_long(trimmed.as_lossy_str())
                .unwrap_or_else(|| i64::from(trimmed.java_hash_code())),
        )
    }
}

/// `Long.parseLong` (optional sign, ASCII digits, no overflow).
fn java_parse_long(s: &str) -> Option<i64> {
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// The serialized shape of [`WorldGenSettings`]: `WorldOptions.CODEC` and `WorldDimensions.CODEC` side by side.
struct Packed {
    seed: i64,
    generate_structures: bool,
    bonus_chest: bool,
    legacy_custom_options: Option<String>,
    dimensions: Vec<(Identifier, Tag)>,
}

/// `WorldGenSettings` (the saved-data compound).
#[derive(Clone, PartialEq, Debug)]
pub struct WorldGenSettings {
    pub options: WorldOptions,
    /// `dimensions`: `{ "<namespace>:<path>": LevelStem compound }`, kept raw.
    pub dimensions: CompoundTag,
}

impl WorldGenSettings {
    /// `WorldGenSettings.CODEC` with `stem` as the `LevelStem` codec of the `dimensions` values. `overworld_missing`
    /// is set when a decoded `dimensions` map (complete or partial) has no overworld stem: the `WorldDimensions`
    /// constructor throws there, whatever else failed.
    fn codec_with(
        stem: &Codec<Tag>,
        overworld_missing: Arc<AtomicBool>,
    ) -> Codec<WorldGenSettings> {
        mc_dfu::record!(Packed {
            seed: LONG.field_of("seed"),
            generate_structures: optional_always_present_field_of(
                &BOOL,
                "generate_structures",
                true
            ),
            bonus_chest: optional_always_present_field_of(&BOOL, "bonus_chest", false),
            legacy_custom_options: STRING.lenient_optional_field_of("legacy_custom_options"),
            dimensions: IDENTIFIER.unbounded_map(stem).field_of("dimensions").xmap(
                move |stems| {
                    if !stems
                        .iter()
                        .any(|(id, _)| id.to_string() == "minecraft:overworld")
                    {
                        overworld_missing.store(true, Ordering::Relaxed);
                    }
                    stems
                },
                |stems: &Vec<(Identifier, Tag)>| stems.clone(),
            ),
        })
        .codec()
        .xmap(
            |p| {
                let mut dimensions = CompoundTag::new();
                for (id, stem) in p.dimensions {
                    dimensions.put(id.to_string(), stem);
                }
                WorldGenSettings {
                    options: WorldOptions {
                        seed: p.seed,
                        generate_structures: p.generate_structures,
                        generate_bonus_chest: p.bonus_chest,
                        legacy_custom_options: p.legacy_custom_options,
                    },
                    dimensions,
                }
            },
            |settings: &WorldGenSettings| Packed {
                seed: settings.options.seed,
                generate_structures: settings.options.generate_structures,
                bonus_chest: settings.options.generate_bonus_chest,
                legacy_custom_options: settings.options.legacy_custom_options.clone(),
                dimensions: settings
                    .dimensions
                    .iter()
                    .filter_map(|(k, v)| {
                        Identifier::read(k.as_lossy_str())
                            .ok()
                            .map(|id| (id, v.clone()))
                    })
                    .collect(),
            },
        )
    }

    /// `WorldGenSettings.CODEC`.
    pub fn codec(stem: &Codec<Tag>) -> Codec<WorldGenSettings> {
        WorldGenSettings::codec_with(stem, Arc::default())
    }

    /// The `LevelStem` codec for callers without registries: a stem stays the tag it is, unchecked.
    pub fn raw_stem_codec() -> Codec<Tag> {
        PASSTHROUGH.flat_xmap(
            |d| match d.convert_value(&NBT).to_tag() {
                Ok(tag) => DataResult::success(tag),
                Err(e) => DataResult::error(move || e.clone()),
            },
            |tag: &Tag| DataResult::success(Dynamic::new(NBT, Dyn::from(tag))),
        )
    }

    /// `WorldGenSettings.CODEC` decode with `stem` as the `LevelStem` codec. A `DataResult` error is
    /// [`ReadError::Error`]; a decoded `dimensions` map without an overworld stem makes the `WorldDimensions`
    /// constructor throw ([`ReadError::Thrown`]).
    pub fn decode(tag: &Tag, stem: &Codec<Tag>) -> Result<WorldGenSettings, ReadError> {
        let overworld_missing = Arc::new(AtomicBool::new(false));
        let result = mc_dfu::parse_nbt(
            &WorldGenSettings::codec_with(stem, overworld_missing.clone()),
            tag,
        );
        if overworld_missing.load(Ordering::Relaxed) {
            return Err(ReadError::Thrown(
                "java.lang.IllegalStateException: Overworld settings missing".to_owned(),
            ));
        }
        result.into_result().map_err(ReadError::Error)
    }

    /// `WorldDimensions.specialWorldProperty` for the stored overworld stem (`DebugLevelSource` / `FlatLevelSource`
    /// generator type). Without an overworld entry vanilla consults the data pack's dimensions, which are not
    /// available here: `None` is reported.
    pub fn special_world_property(&self) -> SpecialWorldProperty {
        let generator_type = self
            .dimensions
            .get_compound("minecraft:overworld")
            .and_then(|stem| stem.get_compound("generator"))
            .and_then(|g| g.get_string("type"))
            .and_then(Identifier::try_parse)
            .map(|id| id.to_string());
        match generator_type.as_deref() {
            Some("minecraft:debug") => SpecialWorldProperty::Debug,
            Some("minecraft:flat") => SpecialWorldProperty::Flat,
            _ => SpecialWorldProperty::None,
        }
    }

    /// `WorldGenSettings.CODEC` encode.
    pub fn encode(&self) -> Tag {
        let mut c = CompoundTag::new();
        c.put_long("seed", self.options.seed);
        c.put_boolean("generate_structures", self.options.generate_structures);
        c.put_boolean("bonus_chest", self.options.generate_bonus_chest);
        if let Some(legacy) = &self.options.legacy_custom_options {
            c.put_string("legacy_custom_options", legacy.clone());
        }
        c.put("dimensions", Tag::Compound(self.dimensions.clone()));
        Tag::Compound(c)
    }

    /// `LevelStorageSource.readExistingSavedData(access, registries, WorldGenSettings.TYPE)`: reads
    /// `<world>/data/minecraft/world_gen_settings.dat`.
    pub fn read_existing(
        world_dir: &Path,
        stem: &Codec<Tag>,
    ) -> Result<WorldGenSettings, ReadError> {
        WorldGenSettings::read_existing_in_data_dir(&world_dir.join("data"), stem)
    }

    /// [`Self::read_existing`] with the `data` folder (`LevelResource.DATA`) given.
    pub fn read_existing_in_data_dir(
        data_dir: &Path,
        stem: &Codec<Tag>,
    ) -> Result<WorldGenSettings, ReadError> {
        let path = SavedDataStorage::new(data_dir)
            .get_data_file(&world_gen_settings_id())
            .map_err(|e| ReadError::Thrown(e.to_string()))?;
        let file = read_compressed(&path, &mut NbtAccounter::unlimited_heap()).map_err(|e| {
            let exception = JavaException::of_io(&e);
            if exception.unchecked {
                ReadError::Thrown(exception.to_string())
            } else {
                // `DataResult.error(e::getMessage)`: a missing message prints as `null`
                ReadError::Error(exception.message.unwrap_or_else(|| "null".to_owned()))
            }
        })?;
        WorldGenSettings::decode(
            &Tag::Compound(file.get_compound_or_empty("data").clone()),
            stem,
        )
    }

    /// `LevelStorageSource.writeWorldGenSettings`.
    pub fn write(&self, world_dir: &Path) -> io::Result<()> {
        SavedDataStorage::new(world_dir.join("data")).save(&world_gen_settings_id(), self.encode())
    }
}

/// Why [`WorldGenSettings::read_existing`] failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// A `DataResult` error: the message. Vanilla logs it and falls back to the default settings.
    Error(String),
    /// An unchecked exception (`<class>: <message>`) that nothing in vanilla catches.
    Thrown(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (ReadError::Error(m) | ReadError::Thrown(m)) = self;
        f.write_str(m)
    }
}

impl std::error::Error for ReadError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// `readExistingSavedData`: an `IOException` is a `DataResult` error with `getMessage()`, an unchecked exception
    /// is thrown.
    #[test]
    fn a_damaged_file_is_an_error_or_an_exception_like_in_java() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("minecraft/world_gen_settings.dat");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let read = || {
            WorldGenSettings::read_existing_in_data_dir(
                dir.path(),
                &WorldGenSettings::raw_stem_codec(),
            )
        };
        assert_eq!(
            read().unwrap_err(),
            ReadError::Error(file.display().to_string())
        );
        let mut tag = CompoundTag::new();
        tag.put("data", Tag::Compound(CompoundTag::new()));
        let full = mc_nbt::compress::write_compressed(&tag);
        for (bytes, expected) in [
            (vec![0x1f], ReadError::Error("null".to_owned())),
            (
                vec![1, 2, 3],
                ReadError::Error("Not in GZIP format".to_owned()),
            ),
            (
                full[..full.len() / 2].to_vec(),
                ReadError::Thrown(
                    "net.minecraft.nbt.ReportedNbtException: Loading NBT data".to_owned(),
                ),
            ),
            (
                full[..12].to_vec(),
                ReadError::Error("Unexpected end of ZLIB input stream".to_owned()),
            ),
        ] {
            std::fs::write(&file, bytes).unwrap();
            assert_eq!(read().unwrap_err(), expected);
        }
    }

    #[test]
    fn seed_parsing() {
        let seed = |s: &str| WorldOptions::parse_seed(&s.into());
        assert_eq!(seed("  "), None);
        assert_eq!(seed(" 12345 "), Some(12345));
        assert_eq!(seed("-9223372036854775808"), Some(i64::MIN));
        assert_eq!(seed("hello"), Some(99162322));
        assert_eq!(
            seed("9223372036854775808"),
            Some(mc_core::math::java::java_string_hash("9223372036854775808") as i64)
        );
        // a lone surrogate hashes as its unit, the whitespace around it is trimmed
        let lone = JavaString::from_utf16_units(&[0x20, 0xD800, 0x20]);
        assert_eq!(WorldOptions::parse_seed(&lone), Some(0xD800));
    }
}
