//! Guard of the entity registrations (reads the vanilla `EntityTypes` source): every vanilla `EntityType` whose class is not a
//! `Mob` has a Rust factory, or is listed in `EXPECTED_MISSING` until its class lands (until then it is created as an inert
//! `PlainEntity`). The list only ever shrinks: a type that gains a factory without removing its line fails, and so does a line
//! that names no vanilla type. Mob types are accounted for by the mob tests, and the player type has no factory in vanilla.
//! The boat table is pinned separately: the type -> drop item table, the factories and the dispenser behaviours must all
//! name the same 22 vanilla boats, a boat missing from one of them fails.
use mc_data::{EntityType, Item};
use mc_game::world::dispenser::DISPENSE_BEHAVIORS;
use mc_game::world::entity::factory::has_factory;
use mc_game::world::entity::vehicle::boat::abstract_boat::{BoatKind, boat_spec};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// Non-mob vanilla entity types without a Rust class (type, what is not implemented). Delete the line when the class lands.
#[rustfmt::skip]
const EXPECTED_MISSING: &[(&str, &str)] = &[
    ("minecraft:block_display", "Display.BlockDisplay: block state, transformation, interpolation and the display data"),
    ("minecraft:chest_minecart", "MinecartChest: container items, loot table, open menu"),
    ("minecraft:command_block_minecart", "MinecartCommandBlock: command, last output, console tick"),
    ("minecraft:egg", "ThrownEgg: hit effects, chicken spawn"),
    ("minecraft:end_crystal", "EndCrystal: beam target, explosion on hurt (the dragon fight is a mob concern)"),
    ("minecraft:ender_pearl", "ThrownEnderpearl: teleport of the owner on hit"),
    ("minecraft:eye_of_ender", "EyeOfEnder: flight toward the stronghold and the drop or break"),
    ("minecraft:fishing_bobber", "FishingHook: cast, bite, retrieve and loot"),
    ("minecraft:furnace_minecart", "MinecartFurnace: fuel, push and drive"),
    ("minecraft:hopper_minecart", "MinecartHopper: container pull and push"),
    ("minecraft:item_display", "Display.ItemDisplay: item stack, transformation, interpolation and the display data"),
    ("minecraft:lingering_potion", "ThrownLingeringPotion: area effect cloud on hit"),
    ("minecraft:minecart", "Minecart: rail movement, riding, destroying"),
    ("minecraft:spawner_minecart", "MinecartSpawner: the spawner logic"),
    ("minecraft:splash_potion", "ThrownSplashPotion: splash effects on hit"),
    ("minecraft:text_display", "Display.TextDisplay: component, line width, background and the display data"),
    ("minecraft:tnt_minecart", "MinecartTNT: fuse and explosion"),
];

/// The vanilla boat entity types, each with `(drop item, kind, chest)` from its `boatFactory`/`chestBoatFactory`/
/// `raftFactory`/`chestRaftFactory` registration.
struct Boat {
    entity: String,
    item: String,
    kind: BoatKind,
    chest: bool,
}

struct Vanilla {
    /// Every non-mob type except the player (`createNothing`).
    non_mob: Vec<String>,
    boats: Vec<Boat>,
}

fn entity_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/net/minecraft/world/entity")
}

fn java_files(dir: &Path, out: &mut HashMap<String, PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read the vanilla entity sources") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            java_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "java") {
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            if stem == "package-info" {
                continue;
            }
            assert!(out.insert(stem.clone(), path).is_none(), "two {stem}.java");
        }
    }
}

fn ident_at(text: &str) -> &str {
    let end = text
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    &text[..end]
}

/// Whether `class` is `Mob` or extends it (following the `extends` chain through the vanilla sources).
fn is_mob(class: &str, files: &HashMap<String, PathBuf>) -> bool {
    let mut class = class.to_owned();
    loop {
        if class == "Mob" {
            return true;
        }
        // `Display.BlockDisplay`: a nested class lives in its outer class's file
        let (file, decl) = class.split_once('.').unwrap_or((&class, &class));
        let path = files
            .get(file)
            .unwrap_or_else(|| panic!("no vanilla source for {class}"));
        let text = std::fs::read_to_string(path).expect("read a vanilla source");
        let marker = format!("class {decl}");
        let at = text
            .match_indices(&marker)
            .find(|(i, _)| ident_at(&text[i + marker.len()..]).is_empty())
            .unwrap_or_else(|| panic!("{file}.java declares no class {decl}"))
            .0;
        let header = &text[at..at + text[at..].find('{').expect("class body")];
        let Some((_, parent)) = header.split_once(" extends ") else {
            return false;
        };
        class = ident_at(parent).to_owned();
    }
}

fn vanilla() -> Vanilla {
    let dir = entity_dir();
    let mut files = HashMap::new();
    java_files(&dir, &mut files);
    let text = std::fs::read_to_string(dir.join("EntityTypes.java")).expect("read EntityTypes");
    let mut non_mob = Vec::new();
    let mut boats = Vec::new();
    for chunk in text.split("public static final EntityType<").skip(1) {
        let (class, rest) = chunk.split_once("> ").expect("EntityType<Class>");
        let (constant, rest) = rest.split_once(" = register(").expect("register(");
        let builder = rest.split_once("Builder.").expect("a Builder").1;
        let builder = builder[..builder.find("MobCategory.").expect("category")].to_lowercase();
        let name = format!("minecraft:{}", constant.to_lowercase());
        if builder.contains("createnothing") || is_mob(class, &files) {
            continue;
        }
        if builder.contains("factory(") {
            let item = builder.split_once("items.").expect("a boat item").1;
            boats.push(Boat {
                entity: name.clone(),
                item: format!("minecraft:{}", ident_at(item)),
                kind: if builder.contains("raft") {
                    BoatKind::Raft
                } else {
                    BoatKind::Boat
                },
                chest: builder.contains("chest"),
            });
        }
        non_mob.push(name);
    }
    Vanilla { non_mob, boats }
}

/// The problems of `missing` against the vanilla non-mob types (`implemented` tells whether a type has a Rust factory).
fn problems(
    non_mob: &[String],
    missing: &[(&str, &str)],
    implemented: &dyn Fn(&str) -> bool,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut listed = BTreeSet::new();
    for &(name, reason) in missing {
        if !listed.insert(name) {
            out.push(format!("{name}: listed twice"));
        }
        if !non_mob.iter().any(|t| t == name) {
            out.push(format!("{name}: not a vanilla non-mob entity type"));
        }
        if implemented(name) {
            out.push(format!(
                "{name}: has a factory, delete it from EXPECTED_MISSING"
            ));
        }
        if reason.is_empty() {
            out.push(format!("{name}: no reason"));
        }
    }
    for name in non_mob {
        if !implemented(name) && !listed.contains(name.as_str()) {
            out.push(format!(
                "{name}: no factory and not in EXPECTED_MISSING (it would be created as an inert placeholder)"
            ));
        }
    }
    out
}

fn has(name: &str) -> bool {
    has_factory(EntityType::by_name(name).unwrap_or_else(|| panic!("{name} is not an entity type")))
}

#[test]
fn every_non_mob_entity_type_has_a_factory_or_a_missing_entry() {
    let vanilla = vanilla();
    let problems = problems(&vanilla.non_mob, EXPECTED_MISSING, &has);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    for name in [
        "minecraft:armor_stand",
        "minecraft:mannequin",
        "minecraft:cushion",
    ] {
        assert!(vanilla.non_mob.iter().any(|t| t == name), "{name}");
    }
    for name in ["minecraft:zombie", "minecraft:villager", "minecraft:player"] {
        assert!(!vanilla.non_mob.iter().any(|t| t == name), "{name}");
    }
    assert_eq!(vanilla.non_mob.len(), NON_MOB_TYPES);
}

/// Vanilla non-mob entity types (`createNothing` player excluded).
const NON_MOB_TYPES: usize = 70;

#[test]
fn a_non_mob_type_without_a_factory_fails_the_guard() {
    let vanilla = vanilla();
    let problems = problems(&vanilla.non_mob, &EXPECTED_MISSING[1..], &has);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("minecraft:block_display: no factory"));
}

#[test]
fn an_implemented_type_still_listed_as_missing_fails_the_guard() {
    let vanilla = vanilla();
    let mut missing = EXPECTED_MISSING.to_vec();
    missing.push(("minecraft:oak_boat", "boat"));
    let problems = problems(&vanilla.non_mob, &missing, &has);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("delete it from EXPECTED_MISSING"));
}

#[test]
fn every_vanilla_boat_is_in_the_boat_tables() {
    let vanilla = vanilla();
    assert_eq!(vanilla.boats.len(), 22);
    let dispensed: BTreeSet<&str> = DISPENSE_BEHAVIORS
        .iter()
        .flat_map(|r| r.items.iter().copied())
        .collect();
    let mut problems = Vec::new();
    for boat in &vanilla.boats {
        let ty = EntityType::by_name(&boat.entity).expect("a boat entity type");
        let item = Item::by_name(&boat.item).expect("a boat item");
        if !has_factory(ty) {
            problems.push(format!("{}: no factory", boat.entity));
        }
        if boat_spec(ty) != Some((item, boat.kind, boat.chest)) {
            problems.push(format!(
                "{}: boat_spec is {:?}, vanilla drops {} ({:?}, chest {})",
                boat.entity,
                boat_spec(ty),
                boat.item,
                boat.kind,
                boat.chest
            ));
        }
        if !dispensed.contains(boat.item.as_str()) {
            problems.push(format!("{}: no dispenser behaviour", boat.item));
        }
    }
    for ty in EntityType::iter() {
        let name = ty.identifier().to_string();
        if boat_spec(ty).is_some() && !vanilla.boats.iter().any(|b| b.entity == name) {
            problems.push(format!(
                "{name}: boat_spec lists a type vanilla has no boat factory for"
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
