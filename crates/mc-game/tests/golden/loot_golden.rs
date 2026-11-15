//! Loot tables vs the real vanilla `LootTable.getRandomItems(params, seed)` (fixture from `scripts/golden-loot.sh`): every block loot
//! table x {empty hand, iron pickaxe, silk touch, fortune III, shears, axe} x seeds {0 (the table's random sequence), 1, 2, 3,
//! 12345} x {plain parameters, player + block entity + explosion radius}, every chest table x 4 seeds x {no entity, player with
//! luck 0, 1, 3}. Compared: every stack as `ItemStack.OPTIONAL_CODEC` NBT (item, count, component values) and, for seed 0, the state
//! of the table's random sequence after the roll.

use crate::common;

use std::collections::BTreeMap;

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::Item;
use mc_data::block::Block;
use mc_data::enums::GameType;
use mc_game::server::level::ServerLevel;
use mc_game::world::block_entity::new_block_entity;
use mc_game::world::entity::{Entity, ServerPlayer};
use mc_game::world::item::ItemStack;
use mc_game::world::level::storage::loot::LootContextParamSets;
use mc_game::world::level::storage::loot::LootParams;
use mc_game::world::level::storage::loot::loading::get_loot_table;
use mc_nbt::Tag;
use mc_protocol::codec::GameProfile;
use mc_protocol::components::values::basic::{ENCHANTMENTS, ItemEnchantments};
use mc_protocol::components::{CUSTOM_NAME, Holder};
use mc_text::component::Component;
use mc_world::level_storage::WorldDataConfiguration;

/// `Options.seed` of the oracle's vanilla level: the world seed random sequences derive from.
const WORLD_SEED: i64 = 4242;

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

fn snbt(stack: &ItemStack) -> Tag {
    stack.optional_to_tag().into_result().unwrap()
}

/// `RandomSequence.CODEC` NBT of the named sequence.
fn sequence_state(level: &ServerLevel, key: &Identifier) -> Tag {
    let mut state = None;
    level
        .server_state
        .loot
        .random_sequences
        .lock()
        .for_all_sequences(|id, sequence| {
            if id == key {
                state = Some(sequence.encode());
            }
        });
    state.expect("the roll created its sequence")
}

struct Case {
    header: String,
    sequence: Option<(String, Tag)>,
    stacks: Vec<Tag>,
}

#[test]
fn loot_tables_equal_vanilla() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_config, stem) = common::load_world(dir.path(), WorldDataConfiguration::DEFAULT, true)
        .expect("default world loads");
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = stem.registries.composite_access().clone();
    level.server_state.loot.world_seed = WORLD_SEED;
    let mut player = ServerPlayer::new(
        1,
        Identifier::parse("minecraft:overworld").unwrap(),
        GameProfile {
            id: uuid::Uuid::from_u128(1),
            name: "Looter".into(),
            properties: vec![],
        },
        Default::default(),
    );
    player.set_game_mode_for_player(GameType::Survival, None);
    player.set_pos(0.5, 64.0, 0.5);

    let text = mc_testkit::read_gz(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/loot_golden.txt.gz"),
    );
    let mut cases: Vec<Case> = Vec::new();
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("T ") {
            cases.push(Case {
                header: h.to_owned(),
                sequence: None,
                stacks: Vec::new(),
            });
        } else if let Some(r) = line.strip_prefix("R ") {
            let (id, state) = r.split_once(' ').unwrap();
            cases.last_mut().unwrap().sequence = Some((id.to_owned(), parse(state)));
        } else if let Some(i) = line.strip_prefix("I ") {
            cases.last_mut().unwrap().stacks.push(parse(i));
        }
    }
    assert!(cases.len() > 70000);
    let mut failures: BTreeMap<String, (String, String)> = BTreeMap::new();
    let origin = Vec3::at_center_of(&BlockPos::new(0, 64, 0));
    let named = {
        let mut s = ItemStack::new(Item::STONE);
        s.set(CUSTOM_NAME, Component::literal("Named"));
        s
    };
    for case in &cases {
        let parts: Vec<&str> = case.header.split(' ').collect();
        let (table_id, set, seed, tool_name, variant) = (
            parts[0],
            parts[1],
            parts[2].parse::<i64>().unwrap(),
            parts[3],
            parts[4],
        );
        let id = Identifier::parse(table_id).unwrap();
        let table = get_loot_table(&level.registries, &id);
        let got = if set == "block" {
            let block = Block::by_name(&format!(
                "minecraft:{}",
                id.path().strip_prefix("blocks/").unwrap()
            ))
            .unwrap();
            let state = block.default_state();
            let mut block_entity = new_block_entity(BlockPos::new(0, 64, 0), state);
            if let Some(be) = block_entity.as_mut() {
                be.apply_components_from_item_stack(&named);
            }
            let mut builder = LootParams::builder(&level)
                .origin(origin)
                .tool(&tool(tool_name))
                .block_state(state);
            if variant == "full" {
                builder = builder
                    .this_entity(&player)
                    .explosion_radius(4.0)
                    .block_entity(block_entity.as_deref());
            }
            let params = builder.create(&LootContextParamSets::BLOCK);
            table.get_random_items_with_seed(&params, seed)
        } else {
            let mut builder = LootParams::builder(&level).origin(origin);
            if let Some(luck) = variant.strip_prefix("luck") {
                builder = builder
                    .this_entity(&player)
                    .luck(luck.parse::<f32>().unwrap());
            }
            let params = builder.create(&LootContextParamSets::CHEST);
            table.get_random_items_with_seed(&params, seed)
        };
        let got_stacks: Vec<Tag> = got.iter().map(snbt).collect();
        let got_sequence = (seed == 0).then(|| {
            let key = table
                .random_sequence()
                .expect("seed 0 rolls use a sequence");
            (key.to_string(), sequence_state(&level, key))
        });
        if got_stacks != case.stacks || got_sequence != case.sequence {
            failures.insert(
                case.header.clone(),
                (
                    format!("{:?} {:?}", case.stacks, case.sequence),
                    format!("{got_stacks:?} {got_sequence:?}"),
                ),
            );
        }
    }
    let shown: Vec<String> = failures
        .iter()
        .take(15)
        .map(|(h, (e, g))| format!("{h}\n  vanilla {e}\n  ours    {g}"))
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} rolls differ:\n{}",
        failures.len(),
        cases.len(),
        shown.join("\n")
    );
}

fn parse(snbt: &str) -> Tag {
    mc_nbt::snbt::parse(snbt).unwrap_or_else(|e| panic!("bad fixture SNBT {snbt}: {e:?}"))
}
