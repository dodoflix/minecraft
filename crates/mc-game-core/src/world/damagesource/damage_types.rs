//! `DamageTypes` (the vanilla damage type keys) and `DamageTypeTags`: full identifiers, usable with `DamageSource::is`/`is_tag`.

/// `DamageTypes` keys (`ResourceKey<DamageType>`), as full identifiers.
pub struct DamageTypes;

impl DamageTypes {
    pub const IN_FIRE: &'static str = "minecraft:in_fire";
    pub const CAMPFIRE: &'static str = "minecraft:campfire";
    pub const LIGHTNING_BOLT: &'static str = "minecraft:lightning_bolt";
    pub const ON_FIRE: &'static str = "minecraft:on_fire";
    pub const LAVA: &'static str = "minecraft:lava";
    pub const HOT_FLOOR: &'static str = "minecraft:hot_floor";
    pub const SULFUR_CUBE_HOT: &'static str = "minecraft:sulfur_cube_hot";
    pub const IN_WALL: &'static str = "minecraft:in_wall";
    pub const CRAMMING: &'static str = "minecraft:cramming";
    pub const DROWN: &'static str = "minecraft:drown";
    pub const STARVE: &'static str = "minecraft:starve";
    pub const CACTUS: &'static str = "minecraft:cactus";
    pub const FALL: &'static str = "minecraft:fall";
    pub const ENDER_PEARL: &'static str = "minecraft:ender_pearl";
    pub const FLY_INTO_WALL: &'static str = "minecraft:fly_into_wall";
    pub const FELL_OUT_OF_WORLD: &'static str = "minecraft:out_of_world";
    pub const GENERIC: &'static str = "minecraft:generic";
    pub const MAGIC: &'static str = "minecraft:magic";
    pub const WITHER: &'static str = "minecraft:wither";
    pub const DRAGON_BREATH: &'static str = "minecraft:dragon_breath";
    pub const DRY_OUT: &'static str = "minecraft:dry_out";
    pub const SWEET_BERRY_BUSH: &'static str = "minecraft:sweet_berry_bush";
    pub const FREEZE: &'static str = "minecraft:freeze";
    pub const STALAGMITE: &'static str = "minecraft:stalagmite";
    pub const FALLING_BLOCK: &'static str = "minecraft:falling_block";
    pub const FALLING_ANVIL: &'static str = "minecraft:falling_anvil";
    pub const FALLING_STALACTITE: &'static str = "minecraft:falling_stalactite";
    pub const STING: &'static str = "minecraft:sting";
    pub const MOB_ATTACK: &'static str = "minecraft:mob_attack";
    pub const MOB_ATTACK_NO_AGGRO: &'static str = "minecraft:mob_attack_no_aggro";
    pub const PLAYER_ATTACK: &'static str = "minecraft:player_attack";
    pub const SPEAR: &'static str = "minecraft:spear";
    pub const ARROW: &'static str = "minecraft:arrow";
    pub const TRIDENT: &'static str = "minecraft:trident";
    pub const MOB_PROJECTILE: &'static str = "minecraft:mob_projectile";
    pub const SPIT: &'static str = "minecraft:spit";
    pub const WIND_CHARGE: &'static str = "minecraft:wind_charge";
    pub const FIREWORKS: &'static str = "minecraft:fireworks";
    pub const FIREBALL: &'static str = "minecraft:fireball";
    pub const UNATTRIBUTED_FIREBALL: &'static str = "minecraft:unattributed_fireball";
    pub const WITHER_SKULL: &'static str = "minecraft:wither_skull";
    pub const THROWN: &'static str = "minecraft:thrown";
    pub const INDIRECT_MAGIC: &'static str = "minecraft:indirect_magic";
    pub const THORNS: &'static str = "minecraft:thorns";
    pub const EXPLOSION: &'static str = "minecraft:explosion";
    pub const PLAYER_EXPLOSION: &'static str = "minecraft:player_explosion";
    pub const SONIC_BOOM: &'static str = "minecraft:sonic_boom";
    pub const BAD_RESPAWN_POINT: &'static str = "minecraft:bad_respawn_point";
    pub const OUTSIDE_BORDER: &'static str = "minecraft:outside_border";
    pub const GENERIC_KILL: &'static str = "minecraft:generic_kill";
    pub const MACE_SMASH: &'static str = "minecraft:mace_smash";
}

/// `DamageTypeTags` (`TagKey<DamageType>`), as full identifiers.
pub struct DamageTypeTags;

impl DamageTypeTags {
    pub const DAMAGES_HELMET: &'static str = "minecraft:damages_helmet";
    pub const BYPASSES_ARMOR: &'static str = "minecraft:bypasses_armor";
    pub const BYPASSES_SHIELD: &'static str = "minecraft:bypasses_shield";
    pub const BYPASSES_INVULNERABILITY: &'static str = "minecraft:bypasses_invulnerability";
    pub const BYPASSES_COOLDOWN: &'static str = "minecraft:bypasses_cooldown";
    pub const BYPASSES_EFFECTS: &'static str = "minecraft:bypasses_effects";
    pub const BYPASSES_RESISTANCE: &'static str = "minecraft:bypasses_resistance";
    pub const BYPASSES_ENCHANTMENTS: &'static str = "minecraft:bypasses_enchantments";
    pub const IS_FIRE: &'static str = "minecraft:is_fire";
    pub const IS_PROJECTILE: &'static str = "minecraft:is_projectile";
    pub const WITCH_RESISTANT_TO: &'static str = "minecraft:witch_resistant_to";
    pub const IS_EXPLOSION: &'static str = "minecraft:is_explosion";
    pub const IS_FALL: &'static str = "minecraft:is_fall";
    pub const IS_DROWNING: &'static str = "minecraft:is_drowning";
    pub const IS_FREEZING: &'static str = "minecraft:is_freezing";
    pub const IS_LIGHTNING: &'static str = "minecraft:is_lightning";
    pub const NO_ANGER: &'static str = "minecraft:no_anger";
    pub const NO_WOLF_RETALIATION: &'static str = "minecraft:no_wolf_retaliation";
    pub const NO_IMPACT: &'static str = "minecraft:no_impact";
    pub const ALWAYS_MOST_SIGNIFICANT_FALL: &'static str = "minecraft:always_most_significant_fall";
    pub const WITHER_IMMUNE_TO: &'static str = "minecraft:wither_immune_to";
    pub const SULFUR_CUBE_WITH_BLOCK_IMMUNE_TO: &'static str =
        "minecraft:sulfur_cube_with_block_immune_to";
    pub const IGNITES_ARMOR_STANDS: &'static str = "minecraft:ignites_armor_stands";
    pub const BURNS_ARMOR_STANDS: &'static str = "minecraft:burns_armor_stands";
    pub const AVOIDS_GUARDIAN_THORNS: &'static str = "minecraft:avoids_guardian_thorns";
    pub const ALWAYS_TRIGGERS_SILVERFISH: &'static str = "minecraft:always_triggers_silverfish";
    pub const ALWAYS_HURTS_ENDER_DRAGONS: &'static str = "minecraft:always_hurts_ender_dragons";
    pub const NO_KNOCKBACK: &'static str = "minecraft:no_knockback";
    pub const ALWAYS_KILLS_ARMOR_STANDS: &'static str = "minecraft:always_kills_armor_stands";
    pub const CAN_BREAK_ARMOR_STAND: &'static str = "minecraft:can_break_armor_stand";
    pub const BYPASSES_WOLF_ARMOR: &'static str = "minecraft:bypasses_wolf_armor";
    pub const IS_PLAYER_ATTACK: &'static str = "minecraft:is_player_attack";
    pub const BURN_FROM_STEPPING: &'static str = "minecraft:burn_from_stepping";
    pub const PANIC_CAUSES: &'static str = "minecraft:panic_causes";
    pub const PANIC_ENVIRONMENTAL_CAUSES: &'static str = "minecraft:panic_environmental_causes";
    pub const IS_MACE_SMASH: &'static str = "minecraft:mace_smash";
}
