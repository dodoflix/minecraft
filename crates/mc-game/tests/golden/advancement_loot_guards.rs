//! Guards of the command, advancement and loot layers (hermetic): every advancement trigger has a recorded scenario that
//! dumps the player's advancements, every vanilla loot table is rolled by a golden, and the command-layer hooks of
//! `commands/host.rs` are not constants.

use std::collections::BTreeSet;
use std::path::Path;

use mc_game::advancements::triggers::TRIGGER_TYPES;

/// Trigger -> the scenario (`tests/oracle/scenarios/NAME.txt`, recorded under `tests/fixtures/entity_oracle/`) in which it
/// fires and the award shows in a `dump advancements:`.
const OBSERVED: &[(&str, &str)] = &[
    ("allay_drop_item_on_block", "adv_allay_note_block"),
    ("any_block_use", "adv_block_use"),
    ("avoid_vibration", "adv_avoid_vibration"),
    ("bee_nest_destroyed", "blk_adv_block_events"),
    ("bred_animals", "adv_bred_animals"),
    ("brewed_potion", "mnu_brewing_stand"),
    ("channeled_lightning", "ent_trident_hit_advancement"),
    ("construct_beacon", "blk_adv_block_events"),
    ("consume_item", "adv_consume_items"),
    ("crafter_recipe_crafted", "blk_adv_block_events"),
    ("cured_zombie_villager", "mob_zombie_villager_cure"),
    ("default_block_use", "adv_block_use"),
    ("effects_changed", "adv_effects_changed"),
    ("enchanted_item", "mnu_enchant_table"),
    ("enter_block", "adv_enter_block_sleep"),
    ("entity_hurt_player", "adv_hurt_player"),
    ("entity_killed_player", "adv_kill_triggers"),
    ("fall_after_explosion", "adv_totem_explosion"),
    ("fall_from_height", "adv_fall_height"),
    ("filled_bucket", "adv_buckets"),
    ("fishing_rod_hooked", "ent_fishing_hooked_entity"),
    ("hero_of_the_village", "adv_raid_hero"),
    ("impossible", "adv_join_packet"),
    ("inventory_changed", "adv_inventory_changed"),
    ("item_durability_changed", "adv_durability"),
    ("item_used_on_block", "blk_adv_item_used_on_block"),
    ("kill_mob_near_sculk_catalyst", "adv_sculk_kills"),
    ("killed_by_arrow", "adv_arrow_kills"),
    ("levitation", "adv_levitation"),
    ("lightning_strike", "adv_lightning_strike"),
    ("location", "adv_location_triggers"),
    ("placed_block", "blk_adv_placed_block"),
    (
        "player_generates_container_loot",
        "ent_container_minecart_loot",
    ),
    ("player_hurt_entity", "adv_hurt_entity"),
    ("player_interacted_with_entity", "adv_entity_interaction"),
    ("player_killed_entity", "adv_kill_triggers"),
    ("player_sheared_equipment", "adv_entity_interaction"),
    ("recipe_crafted", "adv_recipe_crafted"),
    ("recipe_unlocked", "mnu_recipe_unlock"),
    ("ride_entity_in_lava", "adv_ride_lava"),
    ("shot_crossbow", "adv_arrow_kills"),
    ("slept_in_bed", "adv_enter_block_sleep"),
    ("slide_down_block", "blk_honey_slide"),
    ("spear_mobs", "adv_spear_mobs"),
    ("started_riding", "ent_ride_advancement"),
    ("summoned_entity", "adv_summoned_golems"),
    ("tame_animal", "adv_tamed_animals"),
    ("target_hit", "blk_adv_block_events"),
    ("thrown_item_picked_up_by_entity", "adv_thrown_pickup"),
    ("thrown_item_picked_up_by_player", "adv_allay_delivery"),
    ("tick", "adv_join_packet"),
    ("used_ender_eye", "adv_ender_eye"),
    ("used_totem", "adv_totem_explosion"),
    ("using_item", "adv_using_item"),
    ("villager_trade", "adv_villager_trades"),
    ("voluntary_exile", "adv_voluntary_exile"),
];

/// Triggers without an observation, and why.
const UNOBSERVED: &[(&str, &str)] = &[
    (
        "changed_dimension",
        "needs a replay that holds several levels and portal travel",
    ),
    (
        "nether_travel",
        "needs a replay that holds several levels and portal travel",
    ),
];

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
        .map(|(t, _)| *t)
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
    for (trigger, scenario) in OBSERVED {
        let script = std::fs::read_to_string(dir.join(format!("oracle/scenarios/{scenario}.txt")));
        match script {
            Ok(text) if text.contains("advancements") => {}
            Ok(_) => problems.push(format!("{trigger}: {scenario} never dumps advancements")),
            Err(_) => problems.push(format!("{trigger}: no scenario {scenario}")),
        }
        let fixtures = dir.join("fixtures/entity_oracle");
        if !fixtures.join(format!("{scenario}.txt")).exists()
            && !fixtures.join(format!("{scenario}.txt.gz")).exists()
        {
            problems.push(format!("{trigger}: {scenario} has no recorded fixture"));
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
