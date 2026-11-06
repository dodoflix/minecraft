//! `net.minecraft.world.level.chunk.storage.RegionStorageInfo`.

use mc_core::ident::Identifier;

/// Identifies a region storage (level folder name, dimension key, storage type such as `chunk`,
/// `entities`, `poi`); only used for reporting in vanilla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionStorageInfo {
    pub level: String,
    /// `ResourceKey<Level>` location, e.g. `minecraft:overworld`.
    pub dimension: Identifier,
    pub r#type: String,
}

impl RegionStorageInfo {
    pub fn new(level: impl Into<String>, dimension: Identifier, r#type: impl Into<String>) -> Self {
        Self {
            level: level.into(),
            dimension,
            r#type: r#type.into(),
        }
    }

    pub fn with_type_suffix(&self, suffix: &str) -> Self {
        Self {
            level: self.level.clone(),
            dimension: self.dimension.clone(),
            r#type: format!("{}{}", self.r#type, suffix),
        }
    }
}
