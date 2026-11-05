//! `MobCategory` accessors (the constants live in `mc_data`): per-chunk caps, friendliness/persistence and the despawn
//! distances the spawner and `Mob.checkDespawn` read.

use mc_data::enums::MobCategory;

/// `MobCategory.getMaxInstancesPerChunk()` & co.
pub trait MobCategoryExt: Copy {
    fn get_name(self) -> &'static str;
    fn get_max_instances_per_chunk(self) -> i32;
    fn is_friendly(self) -> bool;
    fn is_persistent(self) -> bool;
    fn get_despawn_distance(self) -> i32;
    fn get_no_despawn_distance(self) -> i32;
}

impl MobCategoryExt for MobCategory {
    fn get_name(self) -> &'static str {
        self.data().serialized_name
    }
    fn get_max_instances_per_chunk(self) -> i32 {
        self.data().max_instances_per_chunk
    }
    fn is_friendly(self) -> bool {
        self.data().friendly
    }
    fn is_persistent(self) -> bool {
        self.data().persistent
    }
    fn get_despawn_distance(self) -> i32 {
        self.data().despawn_distance
    }
    fn get_no_despawn_distance(self) -> i32 {
        self.data().no_despawn_distance
    }
}

/// `NaturalSpawner.SPAWNING_CATEGORIES`: every category but `MISC`, in declaration order.
pub const SPAWNING_CATEGORIES: [MobCategory; 7] = [
    MobCategory::Monster,
    MobCategory::Creature,
    MobCategory::Ambient,
    MobCategory::Axolotls,
    MobCategory::UndergroundWaterCreature,
    MobCategory::WaterCreature,
    MobCategory::WaterAmbient,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The constants of vanilla's `MobCategory` enum.
    #[test]
    fn constants_match_vanilla() {
        let expected = [
            (MobCategory::Monster, 70, false, false, 128),
            (MobCategory::Creature, 10, true, true, 128),
            (MobCategory::Ambient, 15, true, false, 128),
            (MobCategory::Axolotls, 5, true, false, 128),
            (MobCategory::UndergroundWaterCreature, 5, true, false, 128),
            (MobCategory::WaterCreature, 5, true, false, 128),
            (MobCategory::WaterAmbient, 20, true, false, 64),
            (MobCategory::Misc, -1, true, true, 128),
        ];
        for (c, max, friendly, persistent, despawn) in expected {
            assert_eq!((c.get_max_instances_per_chunk(), c.is_friendly(), c.is_persistent(), c.get_despawn_distance(), c.get_no_despawn_distance()), (max, friendly, persistent, despawn, 32), "{c:?}");
        }
        assert_eq!(SPAWNING_CATEGORIES.len(), MobCategory::ALL.len() - 1);
    }
}
