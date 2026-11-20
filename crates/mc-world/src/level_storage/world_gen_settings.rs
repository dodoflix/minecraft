//! `WorldOptions` + `WorldGenSettings` (`data/minecraft/world_gen_settings.dat`): seed, structure / bonus
//! chest flags and the raw `dimensions` map (its `LevelStem` values need the registries; they stay NBT).

use std::io;
use std::path::Path;

use mc_core::ident::Identifier;
use mc_core::java_exception::JavaException;
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

    /// `WorldOptions.parseSeed`: `None` for blank, the long when numeric, else `String.hashCode()`.
    pub fn parse_seed(seed_string: &str) -> Option<i64> {
        let trimmed = seed_string.trim_matches(|c: char| c <= ' ');
        if trimmed.is_empty() {
            return None;
        }
        Some(
            java_parse_long(trimmed)
                .unwrap_or_else(|| mc_core::math::java::java_string_hash(trimmed) as i64),
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

/// `WorldGenSettings` (the saved-data compound).
#[derive(Clone, PartialEq, Debug)]
pub struct WorldGenSettings {
    pub options: WorldOptions,
    /// `dimensions`: `{ "<namespace>:<path>": LevelStem compound }`, kept raw.
    pub dimensions: CompoundTag,
}

impl WorldGenSettings {
    /// `WorldGenSettings.CODEC` decode (`Err` mirrors a failed `DataResult`).
    pub fn decode(tag: &Tag) -> Result<WorldGenSettings, String> {
        let c = tag.as_compound().ok_or("Not a map")?;
        let seed = c
            .get("seed")
            .ok_or("No key seed")?
            .as_long()
            .ok_or("Not a number")?;
        let flag = |key: &str, default: bool| match c.get(key) {
            None => Ok(default),
            Some(t) => t
                .as_number()
                .map(|n| n.double_value() != 0.0)
                .ok_or_else(|| format!("{key}: Not a number")),
        };
        let generate_structures = flag("generate_structures", true)?;
        let generate_bonus_chest = flag("bonus_chest", false)?;
        // lenientOptionalFieldOf: a non-string value is treated as absent
        let legacy_custom_options = c
            .get("legacy_custom_options")
            .and_then(Tag::as_string)
            .map(str::to_owned);
        let dimensions = c
            .get("dimensions")
            .ok_or("No key dimensions")?
            .as_compound()
            .ok_or("Not a map")?;
        for key in dimensions.keys() {
            Identifier::read(key.as_lossy_str())?;
        }
        Ok(WorldGenSettings {
            options: WorldOptions {
                seed,
                generate_structures,
                generate_bonus_chest,
                legacy_custom_options,
            },
            dimensions: dimensions.clone(),
        })
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
    pub fn read_existing(world_dir: &Path) -> Result<WorldGenSettings, ReadError> {
        WorldGenSettings::read_existing_in_data_dir(&world_dir.join("data"))
    }

    /// [`Self::read_existing`] with the `data` folder (`LevelResource.DATA`) given.
    pub fn read_existing_in_data_dir(data_dir: &Path) -> Result<WorldGenSettings, ReadError> {
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
        WorldGenSettings::decode(&Tag::Compound(file.get_compound_or_empty("data").clone()))
            .map_err(ReadError::Error)
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
        let read = || WorldGenSettings::read_existing_in_data_dir(dir.path());
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
        assert_eq!(WorldOptions::parse_seed("  "), None);
        assert_eq!(WorldOptions::parse_seed(" 12345 "), Some(12345));
        assert_eq!(
            WorldOptions::parse_seed("-9223372036854775808"),
            Some(i64::MIN)
        );
        assert_eq!(WorldOptions::parse_seed("hello"), Some(99162322));
        assert_eq!(
            WorldOptions::parse_seed("9223372036854775808"),
            Some(mc_core::math::java::java_string_hash("9223372036854775808") as i64)
        );
    }
}
