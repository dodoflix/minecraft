//! Every loot table the block golden does not roll, against the real vanilla call sites (fixture from `scripts/golden-loot-all.sh`):
//! the death loot of a spawned victim (`LivingEntity.dropFromLootTable`: normal, baby, burning and table-specific states x damage
//! sources x killer weapons x `DeathLootTableSeed`), gifts, shearing, equipment, archaeology, fishing, block and entity interaction,
//! bartering, the tables outside `chests/` with chest parameters and the trial chamber rewards with vault parameters. Every table
//! is rolled with seeds {0 (its random sequence or `Level.random`), 1, 7}; compared are the stacks (`ItemStack.OPTIONAL_CODEC`
//! NBT) and, for seed 0, the state of the table's random sequence.
//!
//! The fixture holds the scenario lines (`S`) that build the world (spawns, equipment, fire, level random) next to the rolls (`T`
//! header, `R` sequence state, `I` stacks): the replay runs the lines through the entity-tier scenario harness and rolls the same
//! parameters through our loot code.

use crate::common::entity_scenario::{Harness, parse_state};

use std::collections::BTreeMap;

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::Item;
use mc_game::server::level::ServerLevel;
use mc_game::world::damagesource::DamageSource;
use mc_game::world::entity::living_entity::death;
use mc_game::world::entity::projectile::fishing_hook::FishingHook;
use mc_game::world::entity::{Entity, EntityId};
use mc_game::world::item::ItemStack;
use mc_game::world::level::storage::loot::LootContextParamSets;
use mc_game::world::level::storage::loot::LootParams;
use mc_game::world::level::storage::loot::block_drops;
use mc_game::world::level::storage::loot::loading::get_loot_table;
use mc_nbt::Tag;

struct Case {
    kind: String,
    table: Identifier,
    keys: BTreeMap<String, String>,
    header: String,
    sequence: Option<(String, Tag)>,
    stacks: Vec<Tag>,
}

enum Event {
    Setup(String),
    Roll(Box<Case>),
}

fn parse(snbt: &str) -> Tag {
    mc_nbt::snbt::parse(snbt).unwrap_or_else(|e| panic!("bad fixture SNBT {snbt}: {e:?}"))
}

fn events() -> Vec<Event> {
    let text = mc_testkit::read_gz(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/loot_all_golden.txt.gz"),
    );
    let mut events = Vec::new();
    for line in text.lines() {
        if let Some(s) = line.strip_prefix("S ") {
            events.push(Event::Setup(s.to_owned()));
        } else if let Some(h) = line.strip_prefix("T ") {
            let mut parts = h.split(' ');
            let kind = parts.next().unwrap().to_owned();
            let table = Identifier::parse(parts.next().unwrap()).unwrap();
            let keys = parts
                .map(|p| {
                    let (k, v) = p.split_once('=').expect("key=value");
                    (k.to_owned(), v.to_owned())
                })
                .collect();
            events.push(Event::Roll(Box::new(Case {
                kind,
                table,
                keys,
                header: h.to_owned(),
                sequence: None,
                stacks: Vec::new(),
            })));
        } else if let Some(r) = line.strip_prefix("R ") {
            let (id, state) = r.split_once(' ').unwrap();
            if let Some(Event::Roll(c)) = events.last_mut() {
                c.sequence = Some((id.to_owned(), parse(state)));
            }
        } else if let Some(i) = line.strip_prefix("I ")
            && let Some(Event::Roll(c)) = events.last_mut()
        {
            c.stacks.push(parse(i));
        }
    }
    events
}

fn tool(name: &str) -> ItemStack {
    if name == "-" {
        ItemStack::empty()
    } else {
        ItemStack::new(Item::by_name(name).unwrap_or_else(|| panic!("unknown item {name}")))
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

/// `EntityOracleDriver.damageSource(type, by=, direct=)`.
fn damage_source(h: &Harness, case: &Case) -> DamageSource {
    let key = Identifier::parse(&format!("minecraft:{}", case.keys["source"])).unwrap();
    let data = h
        .level
        .registries
        .get::<mc_game::registry::typed::damage_type::DamageType>("minecraft:damage_type", &key)
        .expect("damage type")
        .clone();
    let entity = |name: &str| {
        (name != "-").then(|| {
            h.level
                .get_entity(h.refs[name])
                .unwrap_or_else(|| panic!("{name} is not in the level"))
        })
    };
    let by = entity(&case.keys["by"]);
    let direct = entity(&case.keys["direct"]).or(by);
    DamageSource::new(key, std::sync::Arc::new(data), direct, by, None)
}

fn entity<'a>(h: &'a Harness, name: &str) -> &'a dyn Entity {
    h.level
        .get_entity(h.refs[name])
        .unwrap_or_else(|| panic!("{name} is not in the level"))
}

/// Roll the case through our loot code (the consumer functions where our code has them, the parameters of the vanilla call site
/// otherwise).
fn roll(h: &mut Harness, case: &Case) -> Vec<ItemStack> {
    let k = &case.keys;
    let seed: i64 = k["seed"].parse().unwrap();
    let table = get_loot_table(&h.level.registries, &case.table);
    let origin = Vec3::new(8.5, -60.0, 8.5);
    match case.kind.as_str() {
        "death" => {
            let victim: EntityId = h.refs[&k["victim"]];
            let source = damage_source(h, case);
            let player_killed = k["pk"] == "1";
            if player_killed {
                let looter = entity(h, "Looter").uuid();
                h.level.with_entity(victim, |_, e| {
                    let living = e.living_mut().expect("living victim");
                    living.last_hurt_by_player = Some(looter);
                    living.last_hurt_by_player_memory_time = 100;
                });
            }
            let nat = k["nat"].clone();
            h.level
                .with_entity(victim, |level, e| {
                    let natural = e.get_loot_table().map_or("-".to_owned(), |i| i.to_string());
                    assert_eq!(natural, nat, "{}: the victim's own table", case.header);
                    // only a `Mob` keeps `DeathLootTableSeed`; any other living entity rolls unseeded
                    let expected = if e.mob().is_some() { seed } else { 0 };
                    assert_eq!(
                        e.get_loot_table_seed(),
                        expected,
                        "{}: DeathLootTableSeed",
                        case.header
                    );
                    death::loot_table_items(e, level, &source, player_killed, &case.table)
                })
                .expect("victim")
        }
        "gift" | "shearing" => {
            let id = h.refs[&k["this"]];
            let shears = tool(k.get("tool").map_or("-", String::as_str));
            if seed == 0 {
                let mut drops = Vec::new();
                h.level.with_entity(id, |level, e| {
                    if case.kind == "gift" {
                        mc_game::world::entity::mob::ext::drop_from_gift_loot_table(
                            level,
                            e,
                            &case.table,
                            |_, _, stack| drops.push(stack),
                        );
                    } else {
                        mc_game::world::entity::mob::ext::drop_from_shearing_loot_table(
                            level,
                            e,
                            &case.table,
                            &shears,
                            |_, _, stack| drops.push(stack),
                        );
                    }
                });
                return drops;
            }
            let this = entity(h, &k["this"]);
            let mut b = LootParams::builder(&h.level)
                .origin(this.position())
                .this_entity(this);
            let set = if case.kind == "gift" {
                LootContextParamSets::GIFT
            } else {
                b = b.tool(&shears);
                LootContextParamSets::SHEARING
            };
            table.get_random_items_with_seed(&b.create(&set), seed)
        }
        "equipment" => {
            let this = entity(h, &k["this"]);
            let params = LootParams::builder(&h.level)
                .origin(this.position())
                .this_entity(this)
                .create(&LootContextParamSets::EQUIPMENT);
            table.get_random_items_with_seed(&params, seed)
        }
        "archaeology" => {
            let params = LootParams::builder(&h.level)
                .origin(origin)
                .luck(k["luck"].parse().unwrap())
                .this_entity(entity(h, &k["this"]))
                .tool(&tool(&k["tool"]))
                .create(&LootContextParamSets::ARCHAEOLOGY);
            table.get_random_items_with_seed(&params, seed)
        }
        "fishing" => {
            let id = h.refs[&k["this"]];
            let open = k["open"] == "1";
            h.level.with_entity(id, |_, e| {
                e.as_any_mut()
                    .downcast_mut::<FishingHook>()
                    .expect("fishing bobber")
                    .set_open_water(open);
            });
            let hook = entity(h, &k["this"]);
            let params = LootParams::builder(&h.level)
                .origin(hook.position())
                .tool(&tool(&k["tool"]))
                .this_entity(hook)
                .luck(k["luck"].parse().unwrap())
                .create(&LootContextParamSets::FISHING);
            table.get_random_items_with_seed(&params, seed)
        }
        "interact" => {
            let pos = BlockPos::new(8, -60, 8);
            let state = parse_state(&k["state"]);
            let tool = tool(&k["tool"]);
            let by = (k["by"] != "-").then(|| entity(h, &k["by"]));
            if seed == 0 {
                return block_drops::block_interact_drops(
                    &h.level,
                    &case.table.to_string(),
                    pos,
                    state,
                    (k["tool"] != "-").then_some(&tool),
                    by,
                );
            }
            let mut b = LootParams::builder(&h.level)
                .block_state(state)
                .origin(Vec3::at_center_of(&pos))
                .interacting_entity(by);
            if k["tool"] != "-" {
                b = b.tool(&tool);
            }
            table.get_random_items_with_seed(&b.create(&LootContextParamSets::BLOCK_INTERACT), seed)
        }
        "entity_interact" => {
            let params = LootParams::builder(&h.level)
                .target_entity(entity(h, &k["target"]))
                .interacting_entity(Some(entity(h, &k["by"])))
                .tool(&tool(&k["tool"]))
                .create(&LootContextParamSets::ENTITY_INTERACT);
            table.get_random_items_with_seed(&params, seed)
        }
        "barter" => {
            let params = LootParams::builder(&h.level)
                .this_entity(entity(h, &k["this"]))
                .create(&LootContextParamSets::PIGLIN_BARTER);
            table.get_random_items_with_seed(&params, seed)
        }
        "chest" => {
            let mut b = LootParams::builder(&h.level).origin(origin);
            if let Some(luck) = k["variant"].strip_prefix("luck") {
                b = b
                    .this_entity(entity(h, "Looter"))
                    .luck(luck.parse::<f32>().unwrap());
            }
            table.get_random_items_with_seed(&b.create(&LootContextParamSets::CHEST), seed)
        }
        "vault" => {
            let params = LootParams::builder(&h.level)
                .origin(origin)
                .luck(k["luck"].parse().unwrap())
                .this_entity(entity(h, &k["this"]))
                .tool(&tool(&k["tool"]))
                .create(&LootContextParamSets::VAULT);
            table.get_random_items_with_seed(&params, seed)
        }
        other => panic!("unknown roll kind {other}"),
    }
}

/// The 188 tables the block golden does not roll, plus the 8 trial chamber rewards rolled again with vault parameters.
const TABLES: usize = 196;

#[test]
fn every_other_loot_table_equals_vanilla() {
    let mut h = Harness::new();
    h.label = "loot_all".to_owned();
    let events = events();
    let mut failures: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut tables = std::collections::BTreeSet::new();
    let mut rolls = 0;
    for event in &events {
        match event {
            Event::Setup(line) => {
                h.line_no += 1;
                h.line = line.clone();
                h.run();
            }
            Event::Roll(case) => {
                rolls += 1;
                tables.insert(case.table.to_string());
                let got = roll(&mut h, case);
                let got_stacks: Vec<Tag> = got.iter().map(snbt).collect();
                let table = get_loot_table(&h.level.registries, &case.table);
                let seed: i64 = case.keys["seed"].parse().unwrap();
                let got_sequence = (seed == 0 && case.sequence.is_some()).then(|| {
                    let key = table
                        .random_sequence()
                        .expect("a table with a recorded sequence has one");
                    (key.to_string(), sequence_state(&h.level, key))
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
        }
    }
    assert!(rolls > 8000, "fixture holds {rolls} rolls");
    assert_eq!(tables.len(), TABLES, "tables rolled");
    let shown: Vec<String> = failures
        .iter()
        .take(15)
        .map(|(header, (e, g))| format!("{header}\n  vanilla {e}\n  ours    {g}"))
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {rolls} rolls differ:\n{}",
        failures.len(),
        shown.join("\n")
    );
}
