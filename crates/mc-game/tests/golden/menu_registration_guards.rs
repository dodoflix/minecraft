//! Guard of the menu registrations (hermetic): every `MenuType` of the vanilla registry has a Rust menu that something
//! opens (`MENU_OPENERS`), or is listed in `EXPECTED_MISSING` until its menu lands. Both lists only ever shrink: a menu
//! that appears without an entry fails, and so does an entry that no longer matches the sources.
use crate::block_registration_guards::rust_files;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// (menu, file with the constructor, constructor, file that opens it), files relative to `crates/`. The constructor file
/// names `MenuType::<MENU>` and defines `fn <constructor>`; the opener file calls `<Type>::<constructor>(`. An empty
/// opener means vanilla has no caller of that factory.
const MENU_OPENERS: &[(&str, &str, &str, &str)] = &[
    (
        "minecraft:generic_9x1",
        CONTAINER_MENUS,
        "ChestMenu::one_row",
        "",
    ),
    (
        "minecraft:generic_9x2",
        CONTAINER_MENUS,
        "ChestMenu::two_rows",
        "",
    ),
    (
        "minecraft:generic_9x3",
        CONTAINER_MENUS,
        "ChestMenu::three_rows",
        CONTAINER_OPENER,
    ),
    (
        "minecraft:generic_9x4",
        CONTAINER_MENUS,
        "ChestMenu::four_rows",
        "",
    ),
    (
        "minecraft:generic_9x5",
        CONTAINER_MENUS,
        "ChestMenu::five_rows",
        "",
    ),
    (
        "minecraft:generic_9x6",
        CONTAINER_MENUS,
        "ChestMenu::six_rows",
        CONTAINER_OPENER,
    ),
    (
        "minecraft:generic_3x3",
        CONTAINER_MENUS,
        "DispenserMenu::new",
        BLOCKS_CONTAINER_PROVIDER,
    ),
    (
        "minecraft:shulker_box",
        CONTAINER_MENUS,
        "ShulkerBoxMenu::new",
        CONTAINER_OPENER,
    ),
    (
        "minecraft:hopper",
        CONTAINER_MENUS,
        "HopperMenu::new",
        BLOCKS_CONTAINER_PROVIDER,
    ),
    (
        "minecraft:anvil",
        "mc-game-core/src/world/inventory/anvil_menu.rs",
        "AnvilMenu::new",
        "mc-game-core/src/world/level/block/impls/anvil_block.rs",
    ),
    (
        "minecraft:beacon",
        "mc-game-core/src/world/inventory/beacon_menu.rs",
        "BeaconMenu::new",
        "mc-game-blocks/src/world/level/block/impls/beacon_block.rs",
    ),
    (
        "minecraft:brewing_stand",
        "mc-game-core/src/world/inventory/brewing_stand_menu.rs",
        "BrewingStandMenu::new",
        "mc-game-blocks/src/world/block_entity/brewing_stand_block_entity.rs",
    ),
    (
        "minecraft:enchantment",
        "mc-game-core/src/world/inventory/enchantment_menu.rs",
        "EnchantmentMenu::new",
        "mc-game-blocks/src/world/level/block/impls/enchanting_table_block.rs",
    ),
    (
        "minecraft:grindstone",
        "mc-game-core/src/world/inventory/grindstone_menu.rs",
        "GrindstoneMenu::new",
        "mc-game-blocks/src/world/level/block/impls/grindstone_block.rs",
    ),
    (
        "minecraft:lectern",
        "mc-game-core/src/world/inventory/lectern_menu.rs",
        "LecternMenu::new",
        "mc-game-blocks/src/world/level/block/impls/lectern_block.rs",
    ),
    (
        "minecraft:merchant",
        "mc-game-core/src/world/inventory/merchant_menu.rs",
        "MerchantMenu::new",
        "mc-game-core/src/world/entity/npc/villager/villager.rs",
    ),
    (
        "minecraft:loom",
        "mc-game-core/src/world/inventory/loom_menu.rs",
        "LoomMenu::new",
        "mc-game-blocks/src/world/level/block/impls/loom_block.rs",
    ),
    (
        "minecraft:cartography_table",
        "mc-game-core/src/world/inventory/cartography_table_menu.rs",
        "CartographyTableMenu::new",
        "mc-game-blocks/src/world/level/block/impls/cartography_table_block.rs",
    ),
    (
        "minecraft:stonecutter",
        "mc-game-core/src/world/inventory/stonecutter_menu.rs",
        "StonecutterMenu::new",
        "mc-game-blocks/src/world/level/block/impls/stonecutter_block.rs",
    ),
    (
        "minecraft:crafting",
        "mc-game-core/src/world/inventory/crafting_menu.rs",
        "CraftingMenu::new",
        "mc-game-blocks/src/world/level/block/impls/crafting_table_block.rs",
    ),
    (
        "minecraft:crafter_3x3",
        "mc-game-blocks/src/world/block_entity/crafter_menu.rs",
        "CrafterMenu::new",
        BLOCKS_CONTAINER_PROVIDER,
    ),
    (
        "minecraft:smithing",
        "mc-game-core/src/world/inventory/smithing_menu.rs",
        "SmithingMenu::new",
        "mc-game-blocks/src/world/level/block/impls/smithing_table_block.rs",
    ),
];

const CONTAINER_MENUS: &str = "mc-game-core/src/world/inventory/container_menus.rs";
const CONTAINER_OPENER: &str = "mc-game-core/src/world/block_entity/container_menus.rs";
const BLOCKS_CONTAINER_PROVIDER: &str =
    "mc-game-blocks/src/world/block_entity/container_menu_provider.rs";

/// Menus without a Rust menu yet (menu, what is not implemented). Delete the line when the menu lands.
const EXPECTED_MISSING: &[(&str, &str)] = &[
    ("minecraft:furnace", "furnace menu"),
    ("minecraft:smoker", "smoker menu"),
    ("minecraft:blast_furnace", "blast furnace menu"),
];

type Opener = (&'static str, &'static str, &'static str, &'static str);

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn registry_menus() -> Vec<String> {
    let text =
        std::fs::read_to_string(crates_dir().join("../ref/generated/reports/registries.json"))
            .expect("read registries.json");
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    report["minecraft:menu"]["entries"]
        .as_object()
        .expect("menu registry entries")
        .keys()
        .cloned()
        .collect()
}

/// Whether `text` names `ident` as a whole word (`MenuType::FURNACE` is not in `MenuType::BLAST_FURNACE`).
fn names(text: &str, ident: &str) -> bool {
    text.match_indices(ident).any(|(i, _)| {
        !text[i + ident.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
            && !text[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_')
    })
}

/// The problems of the tables against the registry menus and the Rust sources (`read` returns a file under `crates/`,
/// `sources` is every Rust source).
fn problems(
    menus: &[String],
    openers: &[Opener],
    missing: &[(&str, &str)],
    read: &dyn Fn(&str) -> String,
    sources: &[String],
) -> Vec<String> {
    let mut out = Vec::new();
    let mut claimed = BTreeSet::new();
    let constant = |menu: &str| {
        format!(
            "MenuType::{}",
            menu.strip_prefix("minecraft:")
                .unwrap_or(menu)
                .to_uppercase()
        )
    };
    for &(menu, ctor_file, ctor, opener_file) in openers {
        if !claimed.insert(menu) {
            out.push(format!("{menu}: listed twice"));
        }
        if !menus.iter().any(|m| m == menu) {
            out.push(format!("{menu}: not a registry menu"));
        }
        let (ty, func) = ctor.split_once("::").expect("constructor is Type::fn");
        let text = read(ctor_file);
        if !names(&text, &constant(menu)) || !text.contains(&format!("fn {func}")) {
            out.push(format!(
                "{menu}: {ctor_file} does not define `{func}` with {}",
                constant(menu)
            ));
        }
        if !opener_file.is_empty() && !read(opener_file).contains(&format!("{ty}::{func}(")) {
            out.push(format!("{menu}: {opener_file} never calls `{ctor}`"));
        }
    }
    for &(menu, _) in missing {
        if !claimed.insert(menu) {
            out.push(format!("{menu}: listed twice"));
        }
        if !menus.iter().any(|m| m == menu) {
            out.push(format!("{menu}: not a registry menu"));
        }
        if sources.iter().any(|s| names(s, &constant(menu))) {
            out.push(format!(
                "{menu}: {} is used in the sources, move the entry to MENU_OPENERS",
                constant(menu)
            ));
        }
    }
    for menu in menus {
        if !claimed.contains(menu.as_str()) {
            out.push(format!(
                "{menu}: no opener in MENU_OPENERS and not in EXPECTED_MISSING"
            ));
        }
    }
    out
}

fn check(openers: &[Opener], missing: &[(&str, &str)]) -> Vec<String> {
    let crates = crates_dir();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&crates).expect("read crates/") {
        let src = entry.expect("crate directory").path().join("src");
        if src.is_dir() {
            rust_files(&src, &mut files);
        }
    }
    let sources: Vec<String> = files
        .iter()
        .map(|f| std::fs::read_to_string(f).expect("read a source file"))
        .collect();
    problems(
        &registry_menus(),
        openers,
        missing,
        &|f| std::fs::read_to_string(crates.join(f)).unwrap_or_else(|e| panic!("{f}: {e}")),
        &sources,
    )
}

#[test]
fn every_menu_type_has_an_opener_or_a_missing_entry() {
    let problems = check(MENU_OPENERS, EXPECTED_MISSING);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert_eq!(registry_menus().len(), 25);
}

#[test]
fn a_menu_without_an_opener_fails_the_guard() {
    let problems = check(&MENU_OPENERS[1..], EXPECTED_MISSING);
    assert_eq!(
        problems,
        ["minecraft:generic_9x1: no opener in MENU_OPENERS and not in EXPECTED_MISSING"]
    );
}

#[test]
fn an_implemented_menu_still_listed_as_missing_fails_the_guard() {
    let mut missing = EXPECTED_MISSING.to_vec();
    missing.push(("minecraft:anvil", "anvil menu"));
    let problems = check(
        &MENU_OPENERS[..9]
            .iter()
            .chain(&MENU_OPENERS[10..])
            .copied()
            .collect::<Vec<_>>(),
        &missing,
    );
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("move the entry to MENU_OPENERS"));
}
