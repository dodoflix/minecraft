//! `net.minecraft.world.item.enchantment.Enchantments`: the resource keys of the vanilla enchantments.

use mc_core::ident::Identifier;

pub const PROTECTION: &str = "minecraft:protection";
pub const FIRE_PROTECTION: &str = "minecraft:fire_protection";
pub const FEATHER_FALLING: &str = "minecraft:feather_falling";
pub const BLAST_PROTECTION: &str = "minecraft:blast_protection";
pub const PROJECTILE_PROTECTION: &str = "minecraft:projectile_protection";
pub const RESPIRATION: &str = "minecraft:respiration";
pub const AQUA_AFFINITY: &str = "minecraft:aqua_affinity";
pub const THORNS: &str = "minecraft:thorns";
pub const DEPTH_STRIDER: &str = "minecraft:depth_strider";
pub const FROST_WALKER: &str = "minecraft:frost_walker";
pub const BINDING_CURSE: &str = "minecraft:binding_curse";
pub const SOUL_SPEED: &str = "minecraft:soul_speed";
pub const SWIFT_SNEAK: &str = "minecraft:swift_sneak";
pub const SHARPNESS: &str = "minecraft:sharpness";
pub const SMITE: &str = "minecraft:smite";
pub const BANE_OF_ARTHROPODS: &str = "minecraft:bane_of_arthropods";
pub const KNOCKBACK: &str = "minecraft:knockback";
pub const FIRE_ASPECT: &str = "minecraft:fire_aspect";
pub const LOOTING: &str = "minecraft:looting";
pub const SWEEPING_EDGE: &str = "minecraft:sweeping_edge";
pub const EFFICIENCY: &str = "minecraft:efficiency";
pub const SILK_TOUCH: &str = "minecraft:silk_touch";
pub const UNBREAKING: &str = "minecraft:unbreaking";
pub const FORTUNE: &str = "minecraft:fortune";
pub const POWER: &str = "minecraft:power";
pub const PUNCH: &str = "minecraft:punch";
pub const FLAME: &str = "minecraft:flame";
pub const INFINITY: &str = "minecraft:infinity";
pub const LUCK_OF_THE_SEA: &str = "minecraft:luck_of_the_sea";
pub const LURE: &str = "minecraft:lure";
pub const LOYALTY: &str = "minecraft:loyalty";
pub const IMPALING: &str = "minecraft:impaling";
pub const RIPTIDE: &str = "minecraft:riptide";
pub const CHANNELING: &str = "minecraft:channeling";
pub const MULTISHOT: &str = "minecraft:multishot";
pub const QUICK_CHARGE: &str = "minecraft:quick_charge";
pub const PIERCING: &str = "minecraft:piercing";
pub const DENSITY: &str = "minecraft:density";
pub const BREACH: &str = "minecraft:breach";
pub const WIND_BURST: &str = "minecraft:wind_burst";
pub const LUNGE: &str = "minecraft:lunge";
pub const MENDING: &str = "minecraft:mending";
pub const VANISHING_CURSE: &str = "minecraft:vanishing_curse";

/// Parses one of the constants above.
pub fn id(key: &str) -> Identifier {
    Identifier::parse(key).expect("valid id")
}
