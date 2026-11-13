//! Guard of the mob registrations, the counterpart of `entity_registration_guards` (which pins the non-mob types). Reads the
//! vanilla `EntityTypes` and class sources and pins, for the 90 vanilla `Mob` types:
//! * every mob type has a Rust factory, or is listed in `EXPECTED_MISSING` with what is not implemented (until then it is
//!   created as an inert `PlainEntity` that keeps its NBT);
//! * every mob type with a vanilla `SpawnPlacements.register` has its spawn predicate, or is listed in
//!   `EXPECTED_MISSING_PREDICATE` (its class has to be missing too: a class registers its own predicate);
//! * every living type has a `DefaultAttributes` supplier.
//!
//! Both lists only ever shrink: a type that gains a factory or predicate without removing its line fails, and so does a line
//! that names no vanilla type.
use super::entity_registration_guards::{has, mob_and_living_types, problems};
use mc_data::EntityType;
use mc_game::world::entity::spawn_placements::missing_predicates;

/// Mob types without a Rust class (type, what is not implemented). Alphabetical.
const EXPECTED_MISSING: &[(&str, &str)] = &[
    (
        "minecraft:allay",
        "Allay: item fetching flyer, brain, duplication",
    ),
    (
        "minecraft:armadillo",
        "Armadillo: scared roll-up state machine, scute shedding, brushing",
    ),
    (
        "minecraft:bee",
        "Bee: hive, pollination, anger and sting behaviours",
    ),
    (
        "minecraft:camel",
        "Camel: dash and sit state, rider controls, saddle menu",
    ),
    (
        "minecraft:camel_husk",
        "CamelHusk: Camel subclass with jockey spawning",
    ),
    (
        "minecraft:copper_golem",
        "CopperGolem: chest item transport, weathering, statue form",
    ),
    (
        "minecraft:ender_dragon",
        "EnderDragon: phase brain and multipart body",
    ),
    (
        "minecraft:evoker",
        "Evoker: spell casting illager with fangs and vexes",
    ),
    (
        "minecraft:goat",
        "Goat: brain, ramming and long jumps, horns",
    ),
    (
        "minecraft:happy_ghast",
        "HappyGhast: rideable harness flyer",
    ),
    (
        "minecraft:illusioner",
        "Illusioner: blinding and mirror image casting illager",
    ),
    (
        "minecraft:nautilus",
        "Nautilus: swimming mount with armor and saddle",
    ),
    (
        "minecraft:pillager",
        "Pillager: crossbow raider with patrol behaviour",
    ),
    (
        "minecraft:ravager",
        "Ravager: stun, roar and charge attack raider mount",
    ),
    (
        "minecraft:sniffer",
        "Sniffer: digging, seed finding, egg laying",
    ),
    (
        "minecraft:vindicator",
        "Vindicator: axe illager with johnny naming",
    ),
    (
        "minecraft:wandering_trader",
        "WanderingTrader: trade menu, despawn timer, llama companion",
    ),
    (
        "minecraft:witch",
        "Witch: potion throwing and drinking, raid buffs",
    ),
    (
        "minecraft:wither",
        "Wither: boss event, charged state, skull attacks",
    ),
    (
        "minecraft:zombie_nautilus",
        "ZombieNautilus: undead Nautilus mount",
    ),
];

/// Mob types with a vanilla spawn placement whose predicate is not registered: exactly the types of `EXPECTED_MISSING`
/// that have one (type, what is not implemented). Alphabetical.
const EXPECTED_MISSING_PREDICATE: &[(&str, &str)] = &[
    ("minecraft:armadillo", "Armadillo.checkArmadilloSpawnRules"),
    ("minecraft:camel", "Camel.checkCamelSpawnRules"),
    ("minecraft:goat", "Goat.checkGoatSpawnRules"),
    (
        "minecraft:nautilus",
        "AbstractNautilus.checkNautilusSpawnRules",
    ),
    (
        "minecraft:pillager",
        "PatrollingMonster.checkPatrollingMonsterSpawnRules",
    ),
];

/// `SpawnFrameworkGolden` fixture: its `registered` lines are the `DATA_BY_TYPE` keys of the vanilla `SpawnPlacements`.
const GOLDEN: &str = include_str!("../fixtures/spawn_framework_golden.txt");

/// Vanilla mob types.
const MOB_TYPES: usize = 90;

fn registered() -> Vec<String> {
    GOLDEN
        .lines()
        .filter_map(|l| l.strip_prefix("registered "))
        .map(str::to_owned)
        .collect()
}

fn has_predicate(name: &str) -> bool {
    let ty = EntityType::by_name(name).unwrap_or_else(|| panic!("{name} is not an entity type"));
    !missing_predicates().contains(&ty)
}

fn with_placement(mob: &[String]) -> Vec<String> {
    mob.iter()
        .filter(|name| registered().contains(name))
        .cloned()
        .collect()
}

#[test]
fn every_mob_type_has_a_factory_or_a_missing_entry() {
    let (mob, _) = mob_and_living_types();
    assert_eq!(mob.len(), MOB_TYPES);
    let found = problems(&mob, EXPECTED_MISSING, &has, "factory");
    assert!(found.is_empty(), "{}", found.join("\n"));
    for name in ["minecraft:zombie", "minecraft:villager", "minecraft:wither"] {
        assert!(mob.iter().any(|t| t == name), "{name}");
    }
    for name in [
        "minecraft:player",
        "minecraft:armor_stand",
        "minecraft:boat",
    ] {
        assert!(!mob.iter().any(|t| t == name), "{name}");
    }
}

#[test]
fn every_mob_type_with_a_spawn_placement_has_a_predicate_or_a_missing_entry() {
    let (mob, _) = mob_and_living_types();
    let placed = with_placement(&mob);
    // `SpawnPlacements.register` takes `EntityType<T extends Mob>`: every registered type is a mob
    assert_eq!(
        placed.len(),
        registered().len(),
        "a non-mob type is registered"
    );
    let found = problems(
        &placed,
        EXPECTED_MISSING_PREDICATE,
        &has_predicate,
        "spawn predicate",
    );
    assert!(found.is_empty(), "{}", found.join("\n"));
    let mut missing: Vec<&str> = missing_predicates().iter().map(|t| t.name()).collect();
    missing.sort_unstable();
    let expected: Vec<&str> = EXPECTED_MISSING_PREDICATE.iter().map(|&(n, _)| n).collect();
    assert_eq!(missing, expected);
    for (name, _) in EXPECTED_MISSING_PREDICATE {
        assert!(
            EXPECTED_MISSING.iter().any(|(n, _)| n == name),
            "{name}: a class that exists registers its own spawn predicate"
        );
    }
    assert!(has_predicate("minecraft:elder_guardian"));
}

#[test]
fn every_living_type_has_a_default_attributes_supplier() {
    let (mob, living) = mob_and_living_types();
    assert!(mob.iter().all(|m| living.contains(m)));
    let mut problems = Vec::new();
    for name in &living {
        let ty =
            EntityType::by_name(name).unwrap_or_else(|| panic!("{name} is not an entity type"));
        if ty.data().default_attributes.is_empty() {
            problems.push(format!("{name}: no DefaultAttributes supplier"));
        }
    }
    for ty in EntityType::iter() {
        let name = ty.identifier().to_string();
        if !ty.data().default_attributes.is_empty() && !living.contains(&name) {
            problems.push(format!("{name}: has attributes but is not a living type"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn a_mob_type_without_a_factory_or_entry_fails_the_guard() {
    let (mob, _) = mob_and_living_types();
    let without_zombie = |name: &str| name != "minecraft:zombie" && has(name);
    let found = problems(&mob, EXPECTED_MISSING, &without_zombie, "factory");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("minecraft:zombie: no factory"));
}

#[test]
fn a_stale_missing_entry_fails_the_guard() {
    let (mob, _) = mob_and_living_types();
    let mut missing = EXPECTED_MISSING.to_vec();
    missing.push(("minecraft:zombie", "Zombie"));
    let found = problems(&mob, &missing, &has, "factory");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("delete it from EXPECTED_MISSING"),
        "{found:?}"
    );
    let no_factory = |name: &str| name != "minecraft:oak_boat" && has(name);
    let mut missing = EXPECTED_MISSING.to_vec();
    missing.push(("minecraft:oak_boat", "not a mob"));
    let found = problems(&mob, &missing, &no_factory, "factory");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("not a vanilla type"), "{found:?}");
}

#[test]
fn a_mob_type_without_a_predicate_or_entry_fails_the_guard() {
    let (mob, _) = mob_and_living_types();
    let placed = with_placement(&mob);
    let without_elder = |name: &str| name != "minecraft:elder_guardian" && has_predicate(name);
    let found = problems(
        &placed,
        EXPECTED_MISSING_PREDICATE,
        &without_elder,
        "spawn predicate",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("minecraft:elder_guardian: no spawn predicate"));
}
