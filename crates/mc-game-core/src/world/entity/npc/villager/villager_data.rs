//! `VillagerData`: type + profession + level (the synched value is `mc_protocol`'s `VillagerData`).

pub use mc_protocol::packets::play::VillagerData;

use mc_data::{VillagerProfession, VillagerType};
use mc_nbt::CompoundTag;

pub const MIN_VILLAGER_LEVEL: i32 = 1;
pub const MAX_VILLAGER_LEVEL: i32 = 5;
const NEXT_LEVEL_XP_THRESHOLDS: [i32; 5] = [0, 10, 70, 150, 250];

/// `VillagerData.DEFAULT_TYPE` & co. as an extension of the shared record type.
pub trait VillagerDataExt: Sized {
    const DEFAULT_TYPE: VillagerType = VillagerType::PLAINS;
    /// `new VillagerData(type, profession, level)` (`level = max(1, level)`).
    fn new(r#type: VillagerType, profession: VillagerProfession, level: i32) -> Self;
    fn with_type(self, r#type: VillagerType) -> Self;
    fn with_profession(self, profession: VillagerProfession) -> Self;
    fn with_level(self, level: i32) -> Self;
    /// `VillagerData.CODEC` (encode).
    fn save(&self) -> CompoundTag;
    /// `VillagerData.CODEC` (decode; absent / invalid fields fall back to the defaults).
    fn load(tag: &CompoundTag) -> Self;
}

impl VillagerDataExt for VillagerData {
    fn new(r#type: VillagerType, profession: VillagerProfession, level: i32) -> Self {
        VillagerData { r#type, profession, level: level.max(1) }
    }
    fn with_type(self, r#type: VillagerType) -> Self {
        Self::new(r#type, self.profession, self.level)
    }
    fn with_profession(self, profession: VillagerProfession) -> Self {
        Self::new(self.r#type, profession, self.level)
    }
    fn with_level(self, level: i32) -> Self {
        Self::new(self.r#type, self.profession, level)
    }
    fn save(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.put_string("type", self.r#type.name());
        tag.put_string("profession", self.profession.name());
        tag.put_int("level", self.level);
        tag
    }
    fn load(tag: &CompoundTag) -> Self {
        let r#type = tag.get_string("type").and_then(VillagerType::by_name).unwrap_or(Self::DEFAULT_TYPE);
        let profession = tag.get_string("profession").and_then(VillagerProfession::by_name).unwrap_or(VillagerProfession::NONE);
        let level = tag.get_int_or("level", 1);
        Self::new(r#type, profession, level)
    }
}

/// `VillagerData.getMinXpPerLevel(level)`.
pub fn get_min_xp_per_level(level: i32) -> i32 {
    if can_level_up(level) { NEXT_LEVEL_XP_THRESHOLDS[(level - 1) as usize] } else { 0 }
}

/// `VillagerData.getMaxXpPerLevel(level)`.
pub fn get_max_xp_per_level(level: i32) -> i32 {
    if can_level_up(level) { NEXT_LEVEL_XP_THRESHOLDS[level as usize] } else { 0 }
}

/// `VillagerData.canLevelUp(currentLevel)`.
pub fn can_level_up(current_level: i32) -> bool {
    (1..5).contains(&current_level)
}

/// `VillagerData.DEFAULT_TYPE`.
pub const DEFAULT_TYPE: VillagerType = VillagerType::PLAINS;
