//! Loot engine vs the real vanilla classes over synthetic definitions (fixture from `scripts/loot-synthetic-oracle.sh`): every case
//! is one JSON definition (loot table, item modifier, predicate, number provider or slot source) that vanilla's codec decodes
//! (or rejects with a message), validates (problem paths and texts) and evaluates with seeds 1, 2, 3. Compared per case: the
//! decode error, the validation problems (as a set), and for each seed the stacks as `ItemStack.OPTIONAL_CODEC` NBT, the
//! predicate result, or the provider values (including `ArithmeticException`).

use crate::common;

use std::collections::BTreeMap;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::Item;
use mc_data::block::Block;
use mc_data::enums::GameType;
use mc_dfu::{Codec, DataResult, Dyn, JSON};
use mc_game::registry::codec::{Lookup, RegistryDecodeContext, RegistryLookup};
use mc_game::registry::keys;
use mc_game::server::level::ServerLevel;
use mc_game::world::block_entity::{BlockEntity, new_block_entity};
use mc_game::world::damagesource::DamageSource;
use mc_game::world::entity::equipment::EquipmentSlot;
use mc_game::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game::world::item::ItemStack;
use mc_game::world::item::slot::{SlotProvider, slot_source_codec};
use mc_game::world::level::storage::loot::context::{ContextKeySet, VisitedEntry};
use mc_game::world::level::storage::loot::functions::function_codec;
use mc_game::world::level::storage::loot::holder::with_decode_context;
use mc_game::world::level::storage::loot::predicates::condition_codec;
use mc_game::world::level::storage::loot::providers::number::{
    float_provider_codec, int_provider_codec,
};
use mc_game::world::level::storage::loot::table::loot_table_direct_codec;
use mc_game::world::level::storage::loot::validation::{ProblemCollector, ValidationContextSource};
use mc_game::world::level::storage::loot::{LootContext, LootContextParamSets, LootParams};
use mc_game::world::scores::objective_criteria::{ObjectiveCriteria, RenderType};
use mc_nbt::Tag;
use mc_protocol::codec::GameProfile;
use mc_protocol::components::values::basic::{ENCHANTMENTS, ItemEnchantments};
use mc_protocol::components::{CUSTOM_NAME, Holder};
use mc_text::component::Component;
use mc_world::level_storage::WorldDataConfiguration;

const SEEDS: [i64; 3] = [1, 2, 3];

/// One line of a case's output (the oracle prints the same).
#[derive(PartialEq, Debug, Clone)]
enum Out {
    Error(String),
    Problem(String),
    Seed(i64),
    Stack(Tag),
    Bool(bool),
    Number(String, String),
    /// The evaluation threw (vanilla prints the exception class; only the fact is compared).
    Threw,
}

fn parse_out(line: &str) -> Out {
    let rest = |p: &str| line.strip_prefix(p).unwrap().to_owned();
    if line.starts_with("E ") {
        Out::Error(rest("E "))
    } else if line.starts_with("V ") {
        Out::Problem(rest("V "))
    } else if line.starts_with("S ") {
        Out::Seed(rest("S ").parse().unwrap())
    } else if line.starts_with("I ") {
        Out::Stack(parse_snbt(&rest("I ")))
    } else if line.starts_with("B ") {
        Out::Bool(rest("B ") == "true")
    } else if line.starts_with("X ") {
        Out::Threw
    } else if line.starts_with("N ") {
        let r = rest("N ");
        let (a, b) = r.split_once(' ').unwrap();
        Out::Number(a.to_owned(), b.to_owned())
    } else {
        panic!("unknown oracle line {line}")
    }
}

fn parse_snbt(snbt: &str) -> Tag {
    mc_nbt::snbt::parse(snbt).unwrap_or_else(|e| panic!("bad fixture SNBT {snbt}: {e:?}"))
}

/// Registry lookups of a decode over the level's registries (`RegistryOps.create(JsonOps, access)`).
struct AccessLookup<'a>(&'a mc_game::registry::RegistryAccess);

impl RegistryLookup for AccessLookup<'_> {
    fn contains(&self, registry: &str, id: &Identifier) -> Lookup {
        match self.0.registry(registry) {
            Some(r) if r.contains(id) => Lookup::Yes,
            Some(_) => Lookup::No,
            None => Lookup::NoRegistry,
        }
    }

    fn tag_exists(&self, registry: &str, tag: &Identifier) -> Lookup {
        match self.0.registry(registry) {
            Some(r) if r.tag(tag).is_some() => Lookup::Yes,
            Some(_) => Lookup::No,
            None => Lookup::NoRegistry,
        }
    }
}

struct World {
    level: ServerLevel,
    looter: EntityId,
    victim: EntityId,
    chest_state: mc_data::block::BlockState,
    chest_entity: Box<dyn BlockEntity>,
    pickaxe: ItemStack,
}

fn enchanted(item: Item, enchantment: &str, level: i32) -> ItemStack {
    let mut s = ItemStack::new(item);
    s.set(
        ENCHANTMENTS,
        ItemEnchantments::new(vec![(
            Holder::Reference(Identifier::parse(enchantment).unwrap()),
            level,
        )]),
    );
    s
}

fn player(level: &ServerLevel, id: i32, name: &str, x: f64) -> ServerPlayer {
    let mut p = ServerPlayer::new(
        id,
        Identifier::parse("minecraft:overworld").unwrap(),
        GameProfile {
            id: mc_core::uuid_util::create_offline_player_uuid(name),
            name: name.into(),
            properties: vec![],
        },
        Default::default(),
    );
    let _ = level;
    p.set_game_mode_for_player(GameType::Survival, None);
    p.set_pos(x, 64.0, 0.5);
    p
}

fn world(registries: &mc_game::registry::RegistryAccess) -> World {
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = registries.clone();
    level.chunk_source.set_view_distance(4);
    level.chunk_source.set_simulation_distance(4);
    for x in -1..=1 {
        for z in -1..=1 {
            level.chunk_blocking_mut(x, z);
        }
    }
    let mut looter = player(&level, 1, "Looter", 0.5);
    let victim = player(&level, 2, "Victim", 2.5);
    let (looter_id, victim_id) = (looter.id(), victim.id());
    let inv = &mut looter.player.inventory;
    inv.equipment_set(
        EquipmentSlot::MainHand,
        enchanted(Item::IRON_SWORD, "minecraft:looting", 3),
    );
    inv.equipment_set(EquipmentSlot::Head, ItemStack::new(Item::IRON_HELMET));
    let items = inv.get_non_equipment_items_mut();
    items[1] = ItemStack::with_count(Item::COBBLESTONE, 16);
    let mut gem = ItemStack::new(Item::DIAMOND);
    gem.set(CUSTOM_NAME, Component::literal("Gem"));
    items[2] = gem;
    items[3] = ItemStack::with_count(Item::APPLE, 5);
    items[9] = ItemStack::with_count(Item::STICK, 3);
    items[10] = ItemStack::with_count(Item::DIRT, 64);
    {
        let mut sb = level.scoreboard_mut();
        sb.add_objective(
            "synth",
            ObjectiveCriteria::DUMMY,
            Component::literal("synth"),
            RenderType::Integer,
            false,
            None,
        )
        .unwrap();
        for (name, value) in [("Looter", 5), ("Victim", 12)] {
            sb.get_or_create_player_score(
                &mc_game::world::scores::score::for_name_only(name),
                "synth",
            )
            .unwrap()
            .set(value)
            .unwrap();
        }
    }
    let data = mc_nbt::snbt::parse_compound("{n:7,f:1.5d,s:\"text\",l:[1,2,3],c:{a:1}}").unwrap();
    level.server_state.loot.command_storage = Arc::new(move |id: &Identifier| {
        if id.to_string() == "synth:data" {
            data.clone()
        } else {
            Default::default()
        }
    });
    let chest_state = Block::by_name("minecraft:chest")
        .unwrap()
        .default_state()
        .with_property_str("facing", "east")
        .unwrap()
        .with_property_str("type", "left")
        .unwrap()
        .with_property_str("waterlogged", "true")
        .unwrap();
    let mut chest_entity = new_block_entity(BlockPos::new(0, 64, 0), chest_state).expect("chest");
    let mut named = ItemStack::new(Item::STONE);
    named.set(CUSTOM_NAME, Component::literal("Named"));
    chest_entity.apply_components_from_item_stack(&named);
    level.add_player(Box::new(looter));
    level.add_player(Box::new(victim));
    World {
        level,
        looter: looter_id,
        victim: victim_id,
        chest_state,
        chest_entity,
        pickaxe: enchanted(Item::DIAMOND_PICKAXE, "minecraft:fortune", 2),
    }
}

fn param_set(profile: &str) -> ContextKeySet {
    match profile {
        "empty" => LootContextParamSets::EMPTY,
        "origin" | "command" => LootContextParamSets::COMMAND,
        "block" => LootContextParamSets::BLOCK,
        "kill" => LootContextParamSets::ENTITY,
        "chest" => LootContextParamSets::CHEST,
        "enchloc" => LootContextParamSets::ENCHANTED_LOCATION,
        "enchitem" => LootContextParamSets::ENCHANTED_ITEM,
        "slots" => LootContextParamSets::COMMAND_SLOT_SOURCE,
        other => panic!("profile {other}"),
    }
}

/// The parameters of `profile` (same as the oracle's `params`).
fn params<'a>(
    w: &'a World,
    profile: &str,
    set: &ContextKeySet,
    damage: &'a DamageSource,
    slots: &'a dyn SlotProvider,
) -> LootParams<'a> {
    let origin = Vec3::at_center_of(&BlockPos::new(0, 64, 0));
    let looter = w.level.get_entity(w.looter).expect("Looter");
    let victim = w.level.get_entity(w.victim).expect("Victim");
    let b = LootParams::builder(&w.level);
    let b = match profile {
        "empty" => b,
        "origin" => b.origin(origin),
        "command" => b.origin(origin).this_entity(looter),
        "block" => b
            .origin(origin)
            .block_state(w.chest_state)
            .tool(&w.pickaxe)
            .this_entity(looter)
            .block_entity(Some(&*w.chest_entity))
            .explosion_radius(4.0),
        "kill" => b
            .origin(origin)
            .this_entity(victim)
            .damage_source(damage)
            .attacking_entity(Some(looter))
            .direct_attacking_entity(Some(looter))
            .last_damage_player(Some(looter)),
        "chest" => b.origin(origin).this_entity(looter).luck(2.0),
        "enchloc" => b
            .origin(origin)
            .this_entity(looter)
            .enchantment_level(2)
            .enchantment_active(true),
        "enchitem" => b.tool(&w.pickaxe).enchantment_level(3),
        "slots" => b.origin(origin).this_entity(looter).container(slots),
        other => panic!("profile {other}"),
    };
    b.create(set)
}

fn decode<T: Clone + Send + Sync + 'static>(
    w: &World,
    codec: Codec<T>,
    json: &str,
    out: &mut Vec<Out>,
) -> Option<T> {
    let lookup = AccessLookup(&w.level.registries);
    let ctx = RegistryDecodeContext::new(&lookup);
    let value = Dyn::from_json_str(json).expect("case JSON parses");
    let parse = || codec.parse(&JSON, &value);
    match mc_game::world::item::enchantment::codecs::with_decode_context(&ctx, || {
        with_decode_context(&ctx, parse)
    }) {
        DataResult::Success(v) => Some(v),
        DataResult::Error { message, .. } => {
            out.push(Out::Error(message.get().replace('\n', "\\n")));
            None
        }
    }
}

fn validate(
    w: &World,
    set_name: &str,
    set: ContextKeySet,
    registry: &'static str,
    id: &str,
    check: &dyn Fn(&mc_game::world::level::storage::loot::validation::ValidationContext),
    out: &mut Vec<Out>,
) {
    let problems = ProblemCollector::new();
    let source = ValidationContextSource::new(&problems, &w.level.registries);
    let key = Identifier::parse(&format!("synthetic:{id}")).unwrap();
    let context =
        source
            .context(set)
            .enter_element(&format!("{{{key}@{registry}}}"), registry, &key);
    check(&context);
    problems.for_each(|path, problem| {
        out.push(Out::Problem(format!(
            "{set_name} {path} | {}{}",
            sorted_parameters(&problem.description.replace('\n', "\\n")),
            if problem.fatal { " | fatal" } else { "" }
        )));
    });
}

/// The parameter list of "Parameters [..] are not provided" comes from a `Set.of` in vanilla, whose order changes per JVM run.
fn sorted_parameters(description: &str) -> String {
    let Some(list) = description
        .strip_prefix("Parameters [")
        .and_then(|d| d.strip_suffix("] are not provided in this context"))
    else {
        return description.to_owned();
    };
    let mut parts: Vec<&str> = list.split(", ").collect();
    parts.sort_unstable();
    format!(
        "Parameters [{}] are not provided in this context",
        parts.join(", ")
    )
}

fn bits(f: f32) -> String {
    format!("{:x}", if f.is_nan() { 0x7fc0_0000 } else { f.to_bits() })
}

fn run_case(w: &World, line: &str) -> Vec<Out> {
    let f: Vec<&str> = line.split(" || ").collect();
    let (kind, id, profile, input, json) = (f[0], f[1], f[2], f[3], f[4]);
    let set = param_set(profile);
    let looter = w.level.get_entity(w.looter).expect("Looter");
    let damage = w.level.damage_sources().player_attack(looter);
    let slots = mc_game::server::commands::item::slots::EntitySlots::new(looter);
    let params = params(w, profile, &set, &damage, &slots);
    let context = |seed: i64| {
        LootContext::builder(&params)
            .with_optional_random_seed(seed)
            .create(None)
    };
    let mut out = Vec::new();
    let all = LootContextParamSets::ALL_PARAMS;
    macro_rules! both_validations {
        ($registry:expr, $value:expr) => {{
            validate(
                w,
                profile,
                set,
                $registry,
                id,
                &|c| $value.validate(c),
                &mut out,
            );
            validate(
                w,
                "generic",
                all,
                $registry,
                id,
                &|c| $value.validate(c),
                &mut out,
            );
        }};
    }
    match kind {
        "table" => {
            let Some(table) = decode(w, loot_table_direct_codec(), json, &mut out) else {
                return out;
            };
            validate(
                w,
                "table",
                table.get_param_set(),
                keys::LOOT_TABLE,
                id,
                &|c| table.validate(c),
                &mut out,
            );
            for seed in SEEDS {
                out.push(Out::Seed(seed));
                for s in table.get_random_items_with_seed(&params, seed) {
                    out.push(Out::Stack(s.optional_to_tag().into_result().unwrap()));
                }
            }
        }
        "modifier" => {
            let Some(function) = decode(w, function_codec(), json, &mut out) else {
                return out;
            };
            both_validations!(keys::ITEM_MODIFIER, function);
            let input = parse_stack(input);
            for seed in SEEDS {
                out.push(Out::Seed(seed));
                let mut ctx = context(seed);
                let result = function.apply(input.clone(), &mut ctx);
                out.push(Out::Stack(result.optional_to_tag().into_result().unwrap()));
            }
        }
        "predicate" => {
            let Some(condition) = decode(w, condition_codec(), json, &mut out) else {
                return out;
            };
            both_validations!(keys::PREDICATE, condition);
            for seed in SEEDS {
                out.push(Out::Seed(seed));
                let mut ctx = context(seed);
                ctx.push_visited_element(VisitedEntry::predicate(&condition));
                out.push(Out::Bool(condition.test(&mut ctx)));
            }
        }
        "int" => {
            let Some(provider) = decode(w, int_provider_codec(), json, &mut out) else {
                return out;
            };
            both_validations!(keys::CONTEXT_INT_PROVIDER, provider);
            for seed in SEEDS {
                out.push(Out::Seed(seed));
                let unsafe_value = match provider.get_int_unsafe(&mut context(seed)) {
                    Ok(v) => v.to_string(),
                    Err(_) => "ERR".to_owned(),
                };
                out.push(Out::Number(
                    unsafe_value,
                    provider.get_int(&mut context(seed)).to_string(),
                ));
            }
        }
        "float" => {
            let Some(provider) = decode(w, float_provider_codec(), json, &mut out) else {
                return out;
            };
            both_validations!(keys::CONTEXT_FLOAT_PROVIDER, provider);
            for seed in SEEDS {
                out.push(Out::Seed(seed));
                let unsafe_value = match provider.get_float_unsafe(&mut context(seed)) {
                    Ok(v) => bits(v),
                    Err(_) => "ERR".to_owned(),
                };
                out.push(Out::Number(
                    unsafe_value,
                    bits(provider.get_float(&mut context(seed))),
                ));
            }
        }
        "slots" => {
            let Some(source) = decode(w, slot_source_codec(), json, &mut out) else {
                return out;
            };
            both_validations!(keys::SLOT_SOURCE, source);
            for seed in SEEDS {
                out.push(Out::Seed(seed));
                for s in source.provide(&context(seed)).item_copies() {
                    out.push(Out::Stack(s.optional_to_tag().into_result().unwrap()));
                }
            }
        }
        other => panic!("kind {other}"),
    }
    out
}

fn parse_stack(snbt: &str) -> ItemStack {
    if snbt == "-" {
        return ItemStack::empty();
    }
    ItemStack::optional_from_tag(&parse_snbt(snbt))
        .into_result()
        .unwrap_or_else(|e| panic!("bad input stack {snbt}: {e}"))
}

/// Validation problems are an unordered multiset in the comparison (vanilla's collector order follows its own traversal).
fn normalized(mut v: Vec<Out>) -> Vec<Out> {
    let mut problems: Vec<Out> = v
        .iter()
        .filter(|o| matches!(o, Out::Problem(_)))
        .cloned()
        .collect();
    problems.sort_by_key(|o| format!("{o:?}"));
    v.retain(|o| !matches!(o, Out::Problem(_)));
    problems.extend(v);
    problems
}

#[test]
fn synthetic_loot_definitions_equal_vanilla() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_config, stem) = common::load_world(dir.path(), WorldDataConfiguration::DEFAULT, true)
        .expect("default world loads");
    let w = world(stem.registries.composite_access());
    let text = mc_testkit::read_gz(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/loot_synthetic.txt.gz"),
    );
    let mut cases: Vec<(String, Vec<Out>)> = Vec::new();
    for line in text.lines() {
        if let Some(case) = line.strip_prefix("C ") {
            cases.push((case.to_owned(), Vec::new()));
        } else if !line.is_empty() {
            cases.last_mut().unwrap().1.push(parse_out(line));
        }
    }
    let mut failures: BTreeMap<String, (String, String)> = BTreeMap::new();
    for (case, expected) in &cases {
        let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_case(&w, case)))
            .unwrap_or_else(|_| vec![Out::Threw]);
        let (expected, got) = (normalized(expected.clone()), normalized(got));
        if expected != got {
            let name = case.split(" || ").nth(1).unwrap().to_owned();
            let first = expected
                .iter()
                .zip(&got)
                .position(|(a, b)| a != b)
                .unwrap_or(expected.len().min(got.len()));
            failures.insert(
                name,
                (
                    format!("{:?}", expected.get(first)),
                    format!("{:?}", got.get(first)),
                ),
            );
        }
    }
    let shown: Vec<String> = failures
        .iter()
        .take(25)
        .map(|(h, (e, g))| format!("{h}\n  vanilla {e}\n  ours    {g}"))
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        shown.join("\n")
    );
}
