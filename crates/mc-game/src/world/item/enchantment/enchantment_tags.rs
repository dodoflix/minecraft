//! `net.minecraft.tags.EnchantmentTags`: tag ids of the `enchantment` registry (`tags/enchantment/<path>.json`), without `#`.

use mc_core::ident::Identifier;

pub const TOOLTIP_ORDER: &str = "minecraft:tooltip_order";
pub const ARMOR_EXCLUSIVE: &str = "minecraft:exclusive_set/armor";
pub const BOOTS_EXCLUSIVE: &str = "minecraft:exclusive_set/boots";
pub const BOW_EXCLUSIVE: &str = "minecraft:exclusive_set/bow";
pub const CROSSBOW_EXCLUSIVE: &str = "minecraft:exclusive_set/crossbow";
pub const DAMAGE_EXCLUSIVE: &str = "minecraft:exclusive_set/damage";
pub const MINING_EXCLUSIVE: &str = "minecraft:exclusive_set/mining";
pub const RIPTIDE_EXCLUSIVE: &str = "minecraft:exclusive_set/riptide";
pub const TRADEABLE: &str = "minecraft:tradeable";
pub const DOUBLE_TRADE_PRICE: &str = "minecraft:double_trade_price";
pub const IN_ENCHANTING_TABLE: &str = "minecraft:in_enchanting_table";
pub const ON_MOB_SPAWN_EQUIPMENT: &str = "minecraft:on_mob_spawn_equipment";
pub const ON_TRADED_EQUIPMENT: &str = "minecraft:on_traded_equipment";
pub const ON_RANDOM_LOOT: &str = "minecraft:on_random_loot";
pub const CURSE: &str = "minecraft:curse";
pub const SMELTS_LOOT: &str = "minecraft:smelts_loot";
pub const PREVENTS_BEE_SPAWNS_WHEN_MINING: &str = "minecraft:prevents_bee_spawns_when_mining";
pub const PREVENTS_DECORATED_POT_SHATTERING: &str = "minecraft:prevents_decorated_pot_shattering";
pub const PREVENTS_ICE_MELTING: &str = "minecraft:prevents_ice_melting";
pub const PREVENTS_INFESTED_SPAWNS: &str = "minecraft:prevents_infested_spawns";
pub const TREASURE: &str = "minecraft:treasure";
pub const NON_TREASURE: &str = "minecraft:non_treasure";
pub const TRADES_DESERT_COMMON: &str = "minecraft:trades/desert_common";
pub const TRADES_JUNGLE_COMMON: &str = "minecraft:trades/jungle_common";
pub const TRADES_PLAINS_COMMON: &str = "minecraft:trades/plains_common";
pub const TRADES_SAVANNA_COMMON: &str = "minecraft:trades/savanna_common";
pub const TRADES_SNOW_COMMON: &str = "minecraft:trades/snow_common";
pub const TRADES_SWAMP_COMMON: &str = "minecraft:trades/swamp_common";
pub const TRADES_TAIGA_COMMON: &str = "minecraft:trades/taiga_common";

/// Parses one of the constants above.
pub fn id(key: &str) -> Identifier {
    Identifier::parse(key).expect("valid id")
}

/// Every constant above (`EnchantmentTags`' fields).
pub const ALL: &[&str] = &[TOOLTIP_ORDER, ARMOR_EXCLUSIVE, BOOTS_EXCLUSIVE, BOW_EXCLUSIVE, CROSSBOW_EXCLUSIVE, DAMAGE_EXCLUSIVE, MINING_EXCLUSIVE, RIPTIDE_EXCLUSIVE, TRADEABLE, DOUBLE_TRADE_PRICE, IN_ENCHANTING_TABLE, ON_MOB_SPAWN_EQUIPMENT, ON_TRADED_EQUIPMENT, ON_RANDOM_LOOT, CURSE, SMELTS_LOOT, PREVENTS_BEE_SPAWNS_WHEN_MINING, PREVENTS_DECORATED_POT_SHATTERING, PREVENTS_ICE_MELTING, PREVENTS_INFESTED_SPAWNS, TREASURE, NON_TREASURE, TRADES_DESERT_COMMON, TRADES_JUNGLE_COMMON, TRADES_PLAINS_COMMON, TRADES_SAVANNA_COMMON, TRADES_SNOW_COMMON, TRADES_SWAMP_COMMON, TRADES_TAIGA_COMMON];
