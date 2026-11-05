//! `net.minecraft.stats.Stats`: the custom stats (`Stats.CUSTOM`) and their formatters.
use super::stat::Stat;
use super::stat_formatter::StatFormatter;
use mc_data::CustomStat;

/// `Stats.<NAME>` constants are `Stats.CUSTOM.get(<identifier>)`, i.e. [`Stat::Custom`].
pub struct Stats;

macro_rules! custom_stats {
    ($($name:ident = $formatter:ident;)*) => {
        impl Stats {
            $(pub const $name: Stat = Stat::Custom(CustomStat::$name);)*

            /// The formatter `makeCustomStat(id, formatter)` registered the custom stat with.
            pub fn formatter(stat: CustomStat) -> StatFormatter {
                match stat {
                    $(CustomStat::$name => StatFormatter::$formatter,)*
                    _ => StatFormatter::Default,
                }
            }
        }
    };
}

custom_stats! {
    LEAVE_GAME = Default;
    PLAY_TIME = Time;
    TOTAL_WORLD_TIME = Time;
    TIME_SINCE_DEATH = Time;
    TIME_SINCE_REST = Time;
    SNEAK_TIME = Time;
    WALK_ONE_CM = Distance;
    CROUCH_ONE_CM = Distance;
    SPRINT_ONE_CM = Distance;
    WALK_ON_WATER_ONE_CM = Distance;
    FALL_ONE_CM = Distance;
    CLIMB_ONE_CM = Distance;
    FLY_ONE_CM = Distance;
    WALK_UNDER_WATER_ONE_CM = Distance;
    MINECART_ONE_CM = Distance;
    BOAT_ONE_CM = Distance;
    PIG_ONE_CM = Distance;
    HAPPY_GHAST_ONE_CM = Distance;
    HORSE_ONE_CM = Distance;
    AVIATE_ONE_CM = Distance;
    SWIM_ONE_CM = Distance;
    STRIDER_ONE_CM = Distance;
    NAUTILUS_ONE_CM = Distance;
    JUMP = Default;
    DROP = Default;
    DAMAGE_DEALT = DivideByTen;
    DAMAGE_DEALT_ABSORBED = DivideByTen;
    DAMAGE_DEALT_RESISTED = DivideByTen;
    DAMAGE_TAKEN = DivideByTen;
    DAMAGE_BLOCKED_BY_SHIELD = DivideByTen;
    DAMAGE_ABSORBED = DivideByTen;
    DAMAGE_RESISTED = DivideByTen;
    DEATHS = Default;
    MOB_KILLS = Default;
    ANIMALS_BRED = Default;
    PLAYER_KILLS = Default;
    FISH_CAUGHT = Default;
    TALKED_TO_VILLAGER = Default;
    TRADED_WITH_VILLAGER = Default;
    EAT_CAKE_SLICE = Default;
    FILL_CAULDRON = Default;
    USE_CAULDRON = Default;
    CLEAN_ARMOR = Default;
    CLEAN_BANNER = Default;
    CLEAN_SHULKER_BOX = Default;
    INTERACT_WITH_BREWINGSTAND = Default;
    INTERACT_WITH_BEACON = Default;
    INSPECT_DROPPER = Default;
    INSPECT_HOPPER = Default;
    INSPECT_DISPENSER = Default;
    PLAY_NOTEBLOCK = Default;
    TUNE_NOTEBLOCK = Default;
    POT_FLOWER = Default;
    TRIGGER_TRAPPED_CHEST = Default;
    OPEN_ENDERCHEST = Default;
    ENCHANT_ITEM = Default;
    PLAY_RECORD = Default;
    INTERACT_WITH_FURNACE = Default;
    INTERACT_WITH_CRAFTING_TABLE = Default;
    OPEN_CHEST = Default;
    SLEEP_IN_BED = Default;
    SLEEP_IN_STRAW_BED = Default;
    OPEN_SHULKER_BOX = Default;
    OPEN_BARREL = Default;
    INTERACT_WITH_BLAST_FURNACE = Default;
    INTERACT_WITH_SMOKER = Default;
    INTERACT_WITH_LECTERN = Default;
    INTERACT_WITH_CAMPFIRE = Default;
    INTERACT_WITH_CARTOGRAPHY_TABLE = Default;
    INTERACT_WITH_LOOM = Default;
    INTERACT_WITH_STONECUTTER = Default;
    BELL_RING = Default;
    RAID_TRIGGER = Default;
    RAID_WIN = Default;
    INTERACT_WITH_ANVIL = Default;
    INTERACT_WITH_GRINDSTONE = Default;
    TARGET_HIT = Default;
    INTERACT_WITH_SMITHING_TABLE = Default;
}

impl Stats {
    /// `Stats.CROUCH_TIME` (the custom stat is registered as `sneak_time`).
    pub const CROUCH_TIME: Stat = Stats::SNEAK_TIME;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_custom_stat_has_its_formatter() {
        assert_eq!(Stats::formatter(CustomStat::WALK_ONE_CM), StatFormatter::Distance);
        assert_eq!(Stats::formatter(CustomStat::PLAY_TIME), StatFormatter::Time);
        assert_eq!(Stats::formatter(CustomStat::DAMAGE_DEALT), StatFormatter::DivideByTen);
        assert_eq!(Stats::formatter(CustomStat::JUMP), StatFormatter::Default);
        assert_eq!(CustomStat::iter().count(), 78);
    }
}
