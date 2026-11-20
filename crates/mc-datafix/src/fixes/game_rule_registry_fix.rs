//! `GameRuleRegistryFix`: the level's `GameRules` become `game_rules` keyed by registry ids with typed values.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_core::math::java::parse_int;
use mc_dfu::Dynamic;

/// How a rule value stored as a string is converted.
enum Rule {
    Bool,
    BoolInverted,
    /// Integer clamped to `min..=max`.
    Int(i32, i32),
}

/// The renamed rules in the order `makeRule` applies them.
const RULES: [(&str, &str, Rule); 59] = [
    (
        "allowEnteringNetherUsingPortals",
        "minecraft:allow_entering_nether_using_portals",
        Rule::Bool,
    ),
    (
        "announceAdvancements",
        "minecraft:show_advancement_messages",
        Rule::Bool,
    ),
    (
        "blockExplosionDropDecay",
        "minecraft:block_explosion_drop_decay",
        Rule::Bool,
    ),
    (
        "commandBlockOutput",
        "minecraft:command_block_output",
        Rule::Bool,
    ),
    (
        "enableCommandBlocks",
        "minecraft:command_blocks_work",
        Rule::Bool,
    ),
    (
        "commandBlocksEnabled",
        "minecraft:command_blocks_work",
        Rule::Bool,
    ),
    (
        "commandModificationBlockLimit",
        "minecraft:max_block_modifications",
        Rule::Int(1, i32::MAX),
    ),
    (
        "disableElytraMovementCheck",
        "minecraft:elytra_movement_check",
        Rule::BoolInverted,
    ),
    (
        "disablePlayerMovementCheck",
        "minecraft:player_movement_check",
        Rule::BoolInverted,
    ),
    ("disableRaids", "minecraft:raids", Rule::BoolInverted),
    ("doDaylightCycle", "minecraft:advance_time", Rule::Bool),
    ("doEntityDrops", "minecraft:entity_drops", Rule::Bool),
    (
        "doImmediateRespawn",
        "minecraft:immediate_respawn",
        Rule::Bool,
    ),
    ("doInsomnia", "minecraft:spawn_phantoms", Rule::Bool),
    (
        "doLimitedCrafting",
        "minecraft:limited_crafting",
        Rule::Bool,
    ),
    ("doMobLoot", "minecraft:mob_drops", Rule::Bool),
    ("doMobSpawning", "minecraft:spawn_mobs", Rule::Bool),
    ("doPatrolSpawning", "minecraft:spawn_patrols", Rule::Bool),
    ("doTileDrops", "minecraft:block_drops", Rule::Bool),
    (
        "doTraderSpawning",
        "minecraft:spawn_wandering_traders",
        Rule::Bool,
    ),
    ("doVinesSpread", "minecraft:spread_vines", Rule::Bool),
    ("doWardenSpawning", "minecraft:spawn_wardens", Rule::Bool),
    ("doWeatherCycle", "minecraft:advance_weather", Rule::Bool),
    ("drowningDamage", "minecraft:drowning_damage", Rule::Bool),
    (
        "enderPearlsVanishOnDeath",
        "minecraft:ender_pearls_vanish_on_death",
        Rule::Bool,
    ),
    ("fallDamage", "minecraft:fall_damage", Rule::Bool),
    ("fireDamage", "minecraft:fire_damage", Rule::Bool),
    (
        "forgiveDeadPlayers",
        "minecraft:forgive_dead_players",
        Rule::Bool,
    ),
    ("freezeDamage", "minecraft:freeze_damage", Rule::Bool),
    (
        "globalSoundEvents",
        "minecraft:global_sound_events",
        Rule::Bool,
    ),
    ("keepInventory", "minecraft:keep_inventory", Rule::Bool),
    (
        "lavaSourceConversion",
        "minecraft:lava_source_conversion",
        Rule::Bool,
    ),
    ("locatorBar", "minecraft:locator_bar", Rule::Bool),
    (
        "logAdminCommands",
        "minecraft:log_admin_commands",
        Rule::Bool,
    ),
    (
        "maxCommandChainLength",
        "minecraft:max_command_sequence_length",
        Rule::Int(0, i32::MAX),
    ),
    (
        "maxCommandForkCount",
        "minecraft:max_command_forks",
        Rule::Int(0, i32::MAX),
    ),
    (
        "maxEntityCramming",
        "minecraft:max_entity_cramming",
        Rule::Int(0, i32::MAX),
    ),
    (
        "minecartMaxSpeed",
        "minecraft:max_minecart_speed",
        Rule::Int(i32::MIN, i32::MAX),
    ),
    (
        "mobExplosionDropDecay",
        "minecraft:mob_explosion_drop_decay",
        Rule::Bool,
    ),
    ("mobGriefing", "minecraft:mob_griefing", Rule::Bool),
    (
        "naturalRegeneration",
        "minecraft:natural_health_regeneration",
        Rule::Bool,
    ),
    (
        "playersNetherPortalCreativeDelay",
        "minecraft:players_nether_portal_creative_delay",
        Rule::Int(0, i32::MAX),
    ),
    (
        "playersNetherPortalDefaultDelay",
        "minecraft:players_nether_portal_default_delay",
        Rule::Int(0, i32::MAX),
    ),
    (
        "playersSleepingPercentage",
        "minecraft:players_sleeping_percentage",
        Rule::Int(0, i32::MAX),
    ),
    (
        "projectilesCanBreakBlocks",
        "minecraft:projectiles_can_break_blocks",
        Rule::Bool,
    ),
    ("pvp", "minecraft:pvp", Rule::Bool),
    (
        "randomTickSpeed",
        "minecraft:random_tick_speed",
        Rule::Int(0, i32::MAX),
    ),
    (
        "reducedDebugInfo",
        "minecraft:reduced_debug_info",
        Rule::Bool,
    ),
    (
        "sendCommandFeedback",
        "minecraft:send_command_feedback",
        Rule::Bool,
    ),
    (
        "showDeathMessages",
        "minecraft:show_death_messages",
        Rule::Bool,
    ),
    (
        "snowAccumulationHeight",
        "minecraft:max_snow_accumulation_height",
        Rule::Int(0, 8),
    ),
    ("spawnMonsters", "minecraft:spawn_monsters", Rule::Bool),
    (
        "spawnRadius",
        "minecraft:respawn_radius",
        Rule::Int(i32::MIN, i32::MAX),
    ),
    (
        "spawnerBlocksEnabled",
        "minecraft:spawner_blocks_work",
        Rule::Bool,
    ),
    (
        "spectatorsGenerateChunks",
        "minecraft:spectators_generate_chunks",
        Rule::Bool,
    ),
    ("tntExplodes", "minecraft:tnt_explodes", Rule::Bool),
    (
        "tntExplosionDropDecay",
        "minecraft:tnt_explosion_drop_decay",
        Rule::Bool,
    ),
    ("universalAnger", "minecraft:universal_anger", Rule::Bool),
    (
        "waterSourceConversion",
        "minecraft:water_source_conversion",
        Rule::Bool,
    ),
];

/// `new GameRuleRegistryFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("GameRuleRegistryFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "GameRuleRegistryFix",
            &df.input_schema().get_type(&References::LEVEL),
            |input| {
                input.update_remainder(|tag| {
                    tag.rename_and_fix_field("GameRules", "game_rules", fix_game_rules)
                })
            },
        )
    })
}

fn fix_game_rules(game_rules: Dynamic) -> Dynamic {
    let do_fire_tick = parse_boolean(&game_rules.get("doFireTick").as_string_or("true"));
    let allow_fire_ticks_away_from_player = parse_boolean(
        &game_rules
            .get("allowFireTicksAwayFromPlayer")
            .as_string_or("false"),
    );
    let fire_spread_radius = if !do_fire_tick {
        0
    } else if !allow_fire_ticks_away_from_player {
        128
    } else {
        -1
    };
    let mut game_rules = game_rules;
    if fire_spread_radius != 128 {
        game_rules = game_rules.set(
            "minecraft:fire_spread_radius_around_player",
            game_rules.create_int(fire_spread_radius),
        );
    }
    game_rules = game_rules
        .remove("spawnChunkRadius")
        .remove("entitiesWithPassengersCanUsePortals")
        .remove("gameLoopFunction")
        .remove("doFireTick")
        .remove("allowFireTicksAwayFromPlayer");
    for (old, new, rule) in &RULES {
        game_rules = game_rules.rename_and_fix_field(old, new, |old_value| match rule {
            Rule::Bool => convert_boolean(&old_value),
            Rule::BoolInverted => convert_boolean_inverted(&old_value),
            Rule::Int(min, max) => convert_integer(old_value, *min, *max),
        });
    }
    game_rules
}

/// `Boolean.parseBoolean(s)`.
fn parse_boolean(s: &str) -> bool {
    s.eq_ignore_ascii_case("true")
}

fn convert_integer(old_value: Dynamic, min: i32, max: i32) -> Dynamic {
    let string_value = old_value.as_string_or("");
    match parse_int(&string_value) {
        Some(parsed_value) => old_value.create_int(parsed_value.max(min).min(max)),
        None => old_value,
    }
}

fn convert_boolean(old_value: &Dynamic) -> Dynamic {
    old_value.create_boolean(parse_boolean(&old_value.as_string_or("")))
}

fn convert_boolean_inverted(old_value: &Dynamic) -> Dynamic {
    old_value.create_boolean(!parse_boolean(&old_value.as_string_or("")))
}
