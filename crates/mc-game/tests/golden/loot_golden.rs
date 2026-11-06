//! Loot tables vs the real vanilla `LootTable.getRandomItems(params, seed)` (fixture from `scripts/golden-loot.sh`): every block loot
//! table x {empty hand, iron pickaxe, silk touch, fortune III, shears, axe} x 4 seeds, every chest table x 4 seeds. Compared: item,
//! count and the set of components in the stack's patch.

use crate::common;

use std::collections::BTreeMap;

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::Item;
use mc_data::block::Block;
use mc_game::server::level::ServerLevel;
use mc_game::world::item::ItemStack;
use mc_game::world::level::storage::loot::LootContextParamSets;
use mc_game::world::level::storage::loot::LootParams;
use mc_game::world::level::storage::loot::loading::get_loot_table;
use mc_protocol::components::Holder;
use mc_protocol::components::values::basic::{ENCHANTMENTS, ItemEnchantments};
use mc_world::level_storage::WorldDataConfiguration;

fn tool(name: &str) -> ItemStack {
    let enchanted = |item: Item, enchantment: &str, level: i32| {
        let mut s = ItemStack::new(item);
        s.set(
            ENCHANTMENTS,
            ItemEnchantments::new(vec![(
                Holder::Reference(Identifier::parse(enchantment).unwrap()),
                level,
            )]),
        );
        s
    };
    match name {
        "empty" => ItemStack::empty(),
        "iron_pickaxe" => ItemStack::new(Item::IRON_PICKAXE),
        "silk_touch_pickaxe" => enchanted(Item::DIAMOND_PICKAXE, "minecraft:silk_touch", 1),
        "fortune3_pickaxe" => enchanted(Item::DIAMOND_PICKAXE, "minecraft:fortune", 3),
        "shears" => ItemStack::new(Item::SHEARS),
        "diamond_axe" => ItemStack::new(Item::DIAMOND_AXE),
        other => panic!("unknown tool {other}"),
    }
}

fn render(stacks: &[ItemStack]) -> Vec<String> {
    stacks
        .iter()
        .map(|s| {
            let mut comps: Vec<String> = s
                .get_components_patch()
                .entries()
                .iter()
                .map(|(t, v)| format!("{}{}", if v.is_some() { "+" } else { "-" }, t.name()))
                .collect();
            comps.sort();
            format!("{} {} {}", s.item().name(), s.count(), comps.join(","))
                .trim_end()
                .to_owned()
        })
        .collect()
}

#[test]
fn loot_tables_equal_vanilla() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_config, stem) = common::load_world(dir.path(), WorldDataConfiguration::DEFAULT, true)
        .expect("default world loads");
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = stem.registries.composite_access().clone();

    let text = mc_testkit::read_gz(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/loot_golden.txt.gz"),
    );
    let mut cases: Vec<(String, Vec<String>)> = Vec::new();
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("T ") {
            cases.push((h.to_owned(), Vec::new()));
        } else if let Some(i) = line.strip_prefix("I ") {
            cases.last_mut().unwrap().1.push(i.trim_end().to_owned());
        }
    }
    assert!(cases.len() > 10000);
    let mut failures: BTreeMap<String, (Vec<String>, Vec<String>)> = BTreeMap::new();
    let origin = Vec3::at_center_of(&BlockPos::new(0, 64, 0));
    for (header, expected) in &cases {
        let parts: Vec<&str> = header.split(' ').collect();
        let (table_id, set, seed, tool_name) = (
            parts[0],
            parts[1],
            parts[2].parse::<i64>().unwrap(),
            parts[3],
        );
        let id = Identifier::parse(table_id).unwrap();
        let table = get_loot_table(&level.registries, &id);
        let params = if set == "block" {
            let block = Block::by_name(&format!(
                "minecraft:{}",
                id.path().strip_prefix("blocks/").unwrap()
            ))
            .unwrap();
            LootParams::builder(&level)
                .origin(origin)
                .tool(&tool(tool_name))
                .block_state(block.default_state())
                .create(&LootContextParamSets::BLOCK)
        } else {
            LootParams::builder(&level)
                .origin(origin)
                .create(&LootContextParamSets::CHEST)
        };
        let got = render(&table.get_random_items_with_seed(&params, seed));
        if &got != expected {
            failures.insert(header.clone(), (expected.clone(), got));
        }
    }
    let shown: Vec<String> = failures
        .iter()
        .take(15)
        .map(|(h, (e, g))| format!("{h}\n  vanilla {e:?}\n  ours    {g:?}"))
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} rolls differ:\n{}",
        failures.len(),
        cases.len(),
        shown.join("\n")
    );
}
