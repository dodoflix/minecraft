//! Guards of the command, advancement and loot layers (hermetic): every advancement trigger has a recorded scenario that
//! dumps the player's advancements, every vanilla loot table is rolled by a golden, and the command-layer hooks of
//! `commands/host.rs` are not constants.

use std::collections::BTreeSet;
use std::path::Path;

use mc_game::advancements::triggers::TRIGGER_TYPES;

/// Trigger -> the scenario (`tests/oracle/scenarios/NAME.txt`, recorded under `tests/fixtures/entity_oracle/`) in which it
/// fires, and the advancement criterion of that trigger the fixture records as completed (`ADV PLAYER ADVANCEMENT CRITERION`).
/// An empty criterion marks a trigger that never completes (`impossible`): the fixture only lists its advancement in an
/// `update_advancements` packet.
const OBSERVED: &[(&str, &str, &str, &str)] = &[
    (
        "allay_drop_item_on_block",
        "adv_allay_note_block",
        "minecraft:husbandry/allay_deliver_cake_to_note_block",
        "allay_deliver_cake_to_note_block",
    ),
    ("any_block_use", "adv_block_use", "uses:block/any_any", "c"),
    (
        "avoid_vibration",
        "adv_avoid_vibration",
        "minecraft:adventure/avoid_vibration",
        "avoid_vibration",
    ),
    (
        "bee_nest_destroyed",
        "blk_adv_block_events",
        "minecraft:husbandry/silk_touch_nest",
        "silk_touch_nest",
    ),
    (
        "bred_animals",
        "adv_bred_animals",
        "minecraft:husbandry/bred_all_animals",
        "minecraft:cow",
    ),
    (
        "brewed_potion",
        "mnu_brewing_stand",
        "minecraft:nether/brew_potion",
        "potion",
    ),
    (
        "changed_dimension",
        "portal_nether_player",
        "minecraft:story/enter_the_nether",
        "entered_nether",
    ),
    (
        "channeled_lightning",
        "ent_lightning_channeling",
        "minecraft:adventure/very_very_frightening",
        "struck_villager",
    ),
    (
        "construct_beacon",
        "blk_adv_block_events",
        "minecraft:nether/create_beacon",
        "beacon",
    ),
    (
        "consume_item",
        "adv_consume_items",
        "minecraft:husbandry/balanced_diet",
        "apple",
    ),
    (
        "crafter_recipe_crafted",
        "blk_adv_block_events",
        "minecraft:adventure/crafters_crafting_crafters",
        "crafter_crafted_crafter",
    ),
    (
        "cured_zombie_villager",
        "mob_zombie_villager_cure",
        "minecraft:story/cure_zombie_villager",
        "cured_zombie",
    ),
    (
        "default_block_use",
        "adv_block_use",
        "uses:block/default_any",
        "c",
    ),
    ("effects_changed", "adv_effects_changed", "mv:eff/any", "c"),
    (
        "enchanted_item",
        "mnu_enchant_table",
        "minecraft:story/enchant_item",
        "enchanted_item",
    ),
    ("enter_block", "adv_enter_block_sleep", "mv:enter/any", "c"),
    (
        "entity_hurt_player",
        "adv_hurt_player",
        "minecraft:story/deflect_arrow",
        "deflected_projectile",
    ),
    (
        "entity_killed_player",
        "adv_kill_triggers",
        "minecraft:adventure/root",
        "killed_by_something",
    ),
    (
        "fall_after_explosion",
        "adv_totem_explosion",
        "minecraft:adventure/who_needs_rockets",
        "who_needs_rockets",
    ),
    ("fall_from_height", "adv_fall_height", "mv:fall/any", "c"),
    (
        "filled_bucket",
        "adv_buckets",
        "minecraft:husbandry/axolotl_in_a_bucket",
        "axolotl_bucket",
    ),
    (
        "fishing_rod_hooked",
        "ent_fishing_bite",
        "minecraft:husbandry/fishy_business",
        "cod",
    ),
    (
        "hero_of_the_village",
        "adv_raid_hero",
        "minecraft:adventure/hero_of_the_village",
        "hero_of_the_village",
    ),
    (
        "impossible",
        "adv_join_packet",
        "minecraft:recipes/root",
        "",
    ),
    (
        "inventory_changed",
        "adv_inventory_changed",
        "minecraft:adventure/salvage_sherd",
        "has_sherd",
    ),
    (
        "item_durability_changed",
        "adv_durability",
        "minecraft:nether/ride_strider",
        "used_warped_fungus_on_a_stick",
    ),
    (
        "item_used_on_block",
        "blk_adv_item_used_on_block",
        "minecraft:adventure/lighten_up",
        "lighten_up",
    ),
    (
        "kill_mob_near_sculk_catalyst",
        "adv_sculk_kills",
        "minecraft:adventure/kill_mob_near_sculk_catalyst",
        "kill_mob_near_sculk_catalyst",
    ),
    (
        "killed_by_arrow",
        "adv_arrow_kills",
        "minecraft:adventure/arbalistic",
        "arbalistic",
    ),
    (
        "levitation",
        "adv_levitation",
        "minecraft:end/levitate",
        "levitated",
    ),
    (
        "lightning_strike",
        "adv_lightning_strike",
        "combat:lightning/any",
        "c",
    ),
    (
        "location",
        "adv_location_triggers",
        "minecraft:adventure/adventuring_time",
        "minecraft:desert",
    ),
    (
        "nether_travel",
        "portal_nether_travel",
        "minecraft:nether/fast_travel",
        "travelled",
    ),
    (
        "placed_block",
        "blk_adv_placed_block",
        "minecraft:adventure/heart_transplanter",
        "place_creaking_heart_dormant",
    ),
    (
        "player_generates_container_loot",
        "ent_container_minecart_loot",
        "minecraft:nether/loot_bastion",
        "loot_bastion_other",
    ),
    (
        "player_hurt_entity",
        "adv_hurt_entity",
        "minecraft:adventure/shoot_arrow",
        "shot_arrow",
    ),
    (
        "player_interacted_with_entity",
        "adv_entity_interaction",
        "minecraft:adventure/brush_armadillo",
        "brush_armadillo",
    ),
    (
        "player_killed_entity",
        "adv_kill_triggers",
        "minecraft:adventure/kill_a_mob",
        "minecraft:skeleton",
    ),
    (
        "player_sheared_equipment",
        "adv_entity_interaction",
        "minecraft:husbandry/remove_wolf_armor",
        "remove_wolf_armor",
    ),
    (
        "recipe_crafted",
        "adv_recipe_crafted",
        "minecraft:adventure/craft_decorated_pot_using_only_sherds",
        "pot_crafted_using_only_sherds",
    ),
    (
        "recipe_unlocked",
        "mnu_recipe_unlock",
        "minecraft:recipes/brewing/blaze_powder",
        "has_the_recipe",
    ),
    (
        "ride_entity_in_lava",
        "adv_ride_lava",
        "minecraft:nether/ride_strider_in_overworld_lava",
        "ride_entity_distance",
    ),
    (
        "shot_crossbow",
        "adv_arrow_kills",
        "minecraft:adventure/ol_betsy",
        "shot_crossbow",
    ),
    (
        "slept_in_bed",
        "adv_enter_block_sleep",
        "minecraft:adventure/sleep_in_bed",
        "slept_in_bed",
    ),
    (
        "slide_down_block",
        "blk_honey_slide",
        "minecraft:adventure/honey_block_slide",
        "honey_block_slide",
    ),
    (
        "spear_mobs",
        "adv_spear_mobs",
        "minecraft:adventure/spear_many_mobs",
        "spear_many_mobs",
    ),
    (
        "started_riding",
        "ent_ride_advancement",
        "minecraft:husbandry/ride_a_boat_with_a_goat",
        "ride_a_boat_with_a_goat",
    ),
    (
        "summoned_entity",
        "adv_summoned_golems",
        "minecraft:adventure/summon_iron_golem",
        "summoned_golem",
    ),
    (
        "tame_animal",
        "adv_tamed_animals",
        "minecraft:husbandry/complete_catalogue",
        "minecraft:all_black",
    ),
    (
        "target_hit",
        "blk_adv_block_events",
        "minecraft:adventure/bullseye",
        "bullseye",
    ),
    (
        "thrown_item_picked_up_by_entity",
        "adv_thrown_pickup",
        "minecraft:husbandry/uh_oh",
        "pick_up_dropped_tnt",
    ),
    (
        "thrown_item_picked_up_by_player",
        "adv_allay_delivery",
        "minecraft:husbandry/allay_deliver_item_to_player",
        "allay_deliver_item_to_player",
    ),
    (
        "tick",
        "adv_join_packet",
        "minecraft:recipes/decorations/crafting_table",
        "unlock_right_away",
    ),
    (
        "used_ender_eye",
        "adv_ender_eye",
        "uses:eye/at_least_1358_5",
        "c",
    ),
    (
        "used_totem",
        "adv_totem_explosion",
        "minecraft:adventure/totem_of_undying",
        "used_totem",
    ),
    (
        "using_item",
        "adv_using_item",
        "minecraft:adventure/spyglass_at_parrot",
        "spyglass_at_parrot",
    ),
    (
        "villager_trade",
        "adv_villager_trades",
        "minecraft:adventure/trade",
        "traded",
    ),
    (
        "voluntary_exile",
        "adv_voluntary_exile",
        "rewards:omen",
        "omen",
    ),
];

/// Triggers without an observation, and why.
const UNOBSERVED: &[(&str, &str)] = &[];

fn tests_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

#[test]
fn every_trigger_is_observed_by_a_scenario_or_listed_as_a_gap() {
    let dir = tests_dir();
    let registered: BTreeSet<&str> = TRIGGER_TYPES.iter().map(|t| t.name).collect();
    let mut problems = Vec::new();
    let listed: Vec<&str> = OBSERVED
        .iter()
        .map(|(t, ..)| *t)
        .chain(UNOBSERVED.iter().map(|(t, _)| *t))
        .collect();
    for trigger in &registered {
        match listed.iter().filter(|t| *t == trigger).count() {
            0 => problems.push(format!("{trigger}: no scenario and no listed gap")),
            1 => {}
            _ => problems.push(format!("{trigger}: listed twice")),
        }
    }
    for trigger in &listed {
        if !registered.contains(trigger) {
            problems.push(format!("{trigger}: listed but not registered"));
        }
    }
    for (trigger, scenario, advancement, criterion) in OBSERVED {
        let script = std::fs::read_to_string(dir.join(format!("oracle/scenarios/{scenario}.txt")));
        match script {
            Ok(text) if text.contains("advancements") => {}
            Ok(_) => problems.push(format!("{trigger}: {scenario} never dumps advancements")),
            Err(_) => problems.push(format!("{trigger}: no scenario {scenario}")),
        }
        let fixtures = dir.join("fixtures/entity_oracle");
        let (plain, packed) = (
            fixtures.join(format!("{scenario}.txt")),
            fixtures.join(format!("{scenario}.txt.gz")),
        );
        let fixture = if plain.exists() {
            std::fs::read_to_string(plain).expect("fixture reads")
        } else if packed.exists() {
            mc_testkit::read_gz(packed)
        } else {
            problems.push(format!("{trigger}: {scenario} has no recorded fixture"));
            continue;
        };
        let proved = fixture.lines().any(|line| {
            if criterion.is_empty() {
                line.starts_with("PKT ") && line.contains(advancement)
            } else {
                line.strip_prefix("ADV ")
                    .and_then(|rest| rest.split_once(' '))
                    .is_some_and(|(_, rest)| rest == format!("{advancement} {criterion}"))
            }
        });
        if !proved {
            problems.push(format!(
                "{trigger}: {scenario} records no completed `{advancement}` criterion `{criterion}`"
            ));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

/// The loot table types of the vanilla pack; a new one needs a parameter set in the oracle and the replay.
const LOOT_TABLE_TYPES: &[&str] = &[
    "minecraft:archaeology",
    "minecraft:barter",
    "minecraft:block",
    "minecraft:block_interact",
    "minecraft:chest",
    "minecraft:entity",
    "minecraft:entity_interact",
    "minecraft:equipment",
    "minecraft:fishing",
    "minecraft:gift",
    "minecraft:shearing",
];

/// Table ids that a recorded fixture rolls: the first column after `T ` of `loot_golden` is the table, the second of
/// `loot_all_golden`.
fn rolled_tables() -> BTreeSet<String> {
    let mut rolled = BTreeSet::new();
    for (fixture, column) in [("loot_golden", 0), ("loot_all_golden", 1)] {
        let text = mc_testkit::read_gz(tests_dir().join(format!("fixtures/{fixture}.txt.gz")));
        for line in text.lines().filter_map(|l| l.strip_prefix("T ")) {
            rolled.insert(
                line.split(' ')
                    .nth(column)
                    .expect("table column")
                    .to_owned(),
            );
        }
    }
    rolled
}

#[test]
fn every_vanilla_loot_table_is_rolled_under_a_known_table_type() {
    let rolled = rolled_tables();
    let mut problems = Vec::new();
    let mut tables = 0;
    for path in mc_data::datapack::list("data/minecraft/loot_table/") {
        let id = path
            .strip_prefix("data/minecraft/loot_table/")
            .and_then(|p| p.strip_suffix(".json"))
            .map(|p| format!("minecraft:{p}"))
            .expect("a loot table json");
        tables += 1;
        if !rolled.contains(&id) {
            problems.push(format!("{id}: rolled by no golden"));
        }
        let json: serde_json::Value =
            serde_json::from_str(&mc_data::datapack::read_string(path).expect("table text"))
                .expect("table json");
        let kind = json["type"].as_str().unwrap_or("minecraft:empty");
        if !LOOT_TABLE_TYPES.contains(&kind) {
            problems.push(format!(
                "{id}: table type {kind} has no replayed parameter set"
            ));
        }
    }
    assert!(tables >= 1447, "only {tables} loot tables in the pack");
    assert!(problems.is_empty(), "{problems:#?}");
}

/// The command layer's hooks into the advancement manager, the loot predicates and the selected item: wired to the real
/// implementation, never a constant.
const HOST_SEAMS: &[&str] = &[
    "advancement_all",
    "advancement_get",
    "advancement_node",
    "advancements_match",
    "loot_predicate_matches",
    "player_advancement_award",
    "player_advancement_progress",
    "player_advancement_revoke",
    "player_advancements_flush_dirty",
    "player_selected_item_nbt",
];

/// The whitespace-free body of the top-level `pub fn name(`: from the first `{` line to the first `}` in column 0.
fn function_body(source: &str, name: &str) -> String {
    let rest = &source[source
        .find(&format!("pub fn {name}("))
        .unwrap_or_else(|| panic!("function {name} is gone"))..];
    let open = rest.find("{\n").expect("a body") + 2;
    let close = rest.find("\n}\n").expect("the end of the function");
    rest[open..close].split_whitespace().collect()
}

const CONSTANT_BODIES: &[&str] = &[
    "",
    "None",
    "false",
    "true",
    "Vec::new()",
    "vec![]",
    "()",
    "Default::default()",
];

#[test]
fn command_host_seams_are_not_constants() {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../mc-game-core/src/commands/host.rs"),
    )
    .expect("read host.rs");
    for seam in HOST_SEAMS {
        let body = function_body(&source, seam);
        assert!(
            !CONSTANT_BODIES.contains(&body.as_str()),
            "seam {seam} returns a constant: {body}"
        );
    }
}

#[test]
fn the_seam_check_recognises_a_constant() {
    let body = function_body("pub fn x(a: u8) -> bool {\n    false\n}\n", "x");
    assert!(CONSTANT_BODIES.contains(&body.as_str()));
}
