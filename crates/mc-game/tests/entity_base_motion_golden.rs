//! Entity base motion against the real vanilla server: the scenarios of `tests/oracle/scenarios/` that only use `level`, `fill`,
//! `block`, `spawn_item` (explicit motion), `watch` and `tick` are replayed here on a detached `ServerLevel` and every `E` line is
//! compared with `tests/fixtures/entity_oracle/NAME.txt` (position, motion, on-ground, fall distance, fire, air, alive, age).
//! Regenerate a fixture: `scripts/entity-oracle.sh crates/mc-game/tests/oracle/scenarios/NAME.txt -o crates/mc-game/tests/fixtures/entity_oracle/NAME.txt`.
mod common;
use common::entity_oracle::*;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::Item;
use mc_data::block::{Block, BlockState};
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::entity::{Entity, common_tick};
use mc_game::world::entity::item::ItemEntity;
use mc_game::world::entity::EntityId;
use mc_game::world::item::ItemStack;
use mc_game::world::level::LevelAccessor;

fn parse_state(text: &str) -> BlockState {
    let (name, props) = match text.split_once('[') {
        Some((n, p)) => (n, p.trim_end_matches(']')),
        None => (text, ""),
    };
    let block = Block::by_name(name.trim_start_matches("minecraft:")).or_else(|| Block::by_name(name)).unwrap_or_else(|| panic!("unknown block {name}"));
    let pairs: Vec<(&str, &str)> = props.split(',').filter(|s| !s.is_empty()).map(|kv| kv.split_once('=').expect("k=v")).collect();
    block.state_from_properties(pairs).unwrap_or_else(|| panic!("bad state {text}"))
}

fn fmt_vec(x: f64, y: f64, z: f64) -> String {
    format!("{x:?},{y:?},{z:?}")
}

fn registries() -> mc_game::registry::RegistryAccess {
    static ACCESS: std::sync::OnceLock<mc_game::registry::RegistryAccess> = std::sync::OnceLock::new();
    ACCESS
        .get_or_init(|| {
            let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let dir = tempfile::tempdir().unwrap();
            let (_, stem) = common::load_world(dir.path(), mc_world::level_storage::WorldDataConfiguration::DEFAULT, true).expect("default world loads");
            stem.registries.composite_access().clone()
        })
        .clone()
}

/// Replays `scenario` and returns the dump lines the Java driver prints (E lines restricted to the compared fields).
fn replay(scenario: &str) -> Vec<String> {
    let text = std::fs::read_to_string(scenario_dir().join(format!("{scenario}.txt"))).unwrap();
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = registries();
    // `level flat`: grass at y=-61 (bedrock and dirt below are irrelevant for these scenarios)
    for x in -48..=64 {
        for z in -16..=16 {
            level.set_block(BlockPos::new(x, -61, z), Block::GRASS_BLOCK.default_state(), 3, 512);
        }
    }
    for cx in -4..=4 {
        for cz in -2..=2 {
            level.entity_manager.update_chunk_status(mc_core::pos::ChunkPos::new(cx, cz), mc_game::world::level::entity::Visibility::Ticking);
        }
    }
    let mut refs: Vec<(String, EntityId)> = Vec::new();
    let mut out = Vec::new();
    let mut tick = 0;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let t: Vec<&str> = line.split_whitespace().collect();
        let num = |i: usize| t[i].parse::<f64>().unwrap();
        match t[0] {
            "level" | "watch" | "difficulty" | "preload" | "seed" | "gamerule" => {}
            "block" => {
                level.set_block(BlockPos::new(num(1) as i32, num(2) as i32, num(3) as i32), parse_state(t[4]), 3, 512);
            }
            "fill" => {
                let state = parse_state(t[7]);
                let (x0, x1) = ((num(1) as i32).min(num(4) as i32), (num(1) as i32).max(num(4) as i32));
                let (y0, y1) = ((num(2) as i32).min(num(5) as i32), (num(2) as i32).max(num(5) as i32));
                let (z0, z1) = ((num(3) as i32).min(num(6) as i32), (num(3) as i32).max(num(6) as i32));
                for x in x0..=x1 {
                    for y in y0..=y1 {
                        for z in z0..=z1 {
                            level.set_block(BlockPos::new(x, y, z), state, 3, 512);
                        }
                    }
                }
            }
            "spawn_item" => {
                let item = Item::by_name(t[2].trim_start_matches("minecraft:")).expect("item");
                let e = ItemEntity::with_stack_and_motion(&level, num(3), num(4), num(5), ItemStack::with_count(item, 1), num(6), num(7), num(8));
                let id = e.id();
                assert!(level.add_fresh_entity(Box::new(e)));
                refs.push((t[1].to_string(), id));
            }
            "tick" => {
                for _ in 0..t.get(1).map_or(1, |n| n.parse().unwrap()) {
                    for id in level.entity_manager.tick_list.snapshot() {
                        level.with_entity(id, |level, e| {
                            if !e.is_removed() {
                                common_tick(e);
                                e.tick(level);
                            }
                        });
                    }
                    level.tick_entity_management();
                    tick += 1;
                    out.push(format!("T {tick}"));
                    for (name, id) in &refs {
                        let Some(e) = level.get_entity(*id) else { continue };
                        let b = e.base();
                        let age = e.as_any().downcast_ref::<ItemEntity>().map_or(0, |i| i.age);
                        out.push(format!(
                            "E {name} {} pos={} delta={} og={} fall={:?} fire={} air={} alive={} age={age}",
                            e.get_type().data().name,
                            fmt_vec(b.x(), b.y(), b.z()),
                            fmt_vec(b.delta_movement().x, b.delta_movement().y, b.delta_movement().z),
                            i32::from(b.on_ground()),
                            b.fall_distance,
                            b.remaining_fire_ticks,
                            b.get_air_supply(),
                            i32::from(e.is_alive()),
                        ));
                    }
                }
            }
            other => panic!("unsupported scenario command {other}"),
        }
    }
    out
}

fn compare_scenario(name: &str) {
    let fixture = Fixture::load(name);
    let actual = replay(name);
    let opts = Options { ignore_fields: vec!["rot", "pose", "item", "pickup", "health", "owner", "thrower", "age"], ignore_kinds: vec!["#", "C", "PKT"], ..Default::default() };
    fixture.assert_matches(&actual, &opts);
}

#[test]
fn item_motion_matches_vanilla() {
    compare_scenario("item_motion");
}
