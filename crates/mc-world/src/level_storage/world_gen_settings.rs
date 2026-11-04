//! `WorldOptions` + `WorldGenSettings` (`data/minecraft/world_gen_settings.dat`): seed, structure / bonus
//! chest flags and the raw `dimensions` map (its `LevelStem` values need the registries; they stay NBT).

use std::io;
use std::path::Path;

use mc_core::ident::Identifier;
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
        WorldOptions { seed, generate_structures, generate_bonus_chest, legacy_custom_options: None }
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
        Some(java_parse_long(trimmed).unwrap_or_else(|| mc_core::math::java::java_string_hash(trimmed) as i64))
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
        let seed = c.get("seed").ok_or("No key seed")?.as_long().ok_or("Not a number")?;
        let flag = |key: &str, default: bool| match c.get(key) {
            None => Ok(default),
            Some(t) => t.as_number().map(|n| n.double_value() != 0.0).ok_or_else(|| format!("{key}: Not a number")),
        };
        let generate_structures = flag("generate_structures", true)?;
        let generate_bonus_chest = flag("bonus_chest", false)?;
        // lenientOptionalFieldOf: a non-string value is treated as absent
        let legacy_custom_options = c.get("legacy_custom_options").and_then(Tag::as_string).map(str::to_owned);
        let dimensions = c.get("dimensions").ok_or("No key dimensions")?.as_compound().ok_or("Not a map")?;
        for key in dimensions.keys() {
            Identifier::read(key)?;
        }
        Ok(WorldGenSettings {
            options: WorldOptions { seed, generate_structures, generate_bonus_chest, legacy_custom_options },
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
    /// `<world>/data/minecraft/world_gen_settings.dat`. The error string is the `DataResult` message.
    pub fn read_existing(world_dir: &Path) -> Result<WorldGenSettings, String> {
        let data_dir = world_dir.join("data");
        let path = SavedDataStorage::new(&data_dir).get_data_file(&world_gen_settings_id()).map_err(|e| e.to_string())?;
        let file = read_compressed(&path, &mut NbtAccounter::unlimited_heap()).map_err(|e| e.to_string())?;
        WorldGenSettings::decode(&Tag::Compound(file.get_compound_or_empty("data").clone()))
    }

    /// `LevelStorageSource.writeWorldGenSettings`.
    pub fn write(&self, world_dir: &Path) -> io::Result<()> {
        SavedDataStorage::new(world_dir.join("data")).save(&world_gen_settings_id(), self.encode())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_parsing() {
        assert_eq!(WorldOptions::parse_seed("  "), None);
        assert_eq!(WorldOptions::parse_seed(" 12345 "), Some(12345));
        assert_eq!(WorldOptions::parse_seed("-9223372036854775808"), Some(i64::MIN));
        assert_eq!(WorldOptions::parse_seed("hello"), Some(99162322));
        assert_eq!(WorldOptions::parse_seed("9223372036854775808"), Some(mc_core::math::java::java_string_hash("9223372036854775808") as i64));
    }
}
