//! Level.explode in a world with TNT blocks and non-player entities vs the real vanilla classes. Two scenarios:
//! `tnt_blocks.txt` (`TntBlock`: ignition by a neighbouring power source via `onPlace`/`neighborChanged` -> `prime`, TNT blocks hit by an
//! explosion turning into primed TNT with a short random fuse in `wasExploded`, the chain, drops of the destroyed ground) and
//! `explosion_misc.txt` (items, experience orbs, falling blocks and primed TNT at several distances: damage, knockback, drops).
//! Every non-player entity is printed after each watched tick; the blocks left are dumped at the end.
use crate::common;

use crate::common::entity_oracle::*;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_game::server::level::ServerLevel;
use mc_game::server::level::level_explode::Explode;
use mc_game::world::entity::experience_orb::ExperienceOrb;
use mc_game::world::entity::falling_block_entity::FallingBlockEntity;
use mc_game::world::entity::item::ItemEntity;
use mc_game::world::entity::item::primed_tnt::PrimedTnt;
use mc_game::world::entity::{Entity, EntityId};
use mc_game::world::item::ItemStack;
use mc_game::world::level::explosion::ExplosionInteraction;
use mc_game::world::level::{BlockGetter, LevelAccessor};

fn registries() -> mc_game::registry::RegistryAccess {
    static ACCESS: std::sync::OnceLock<mc_game::registry::RegistryAccess> =
        std::sync::OnceLock::new();
    ACCESS
        .get_or_init(|| {
            let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let dir = tempfile::tempdir().unwrap();
            let (_, stem) = common::load_world(
                dir.path(),
                mc_world::level_storage::WorldDataConfiguration::DEFAULT,
                true,
            )
            .expect("default world loads");
            stem.registries.composite_access().clone()
        })
        .clone()
}

fn parse_state(text: &str) -> BlockState {
    let (name, props) = match text.split_once('[') {
        Some((n, p)) => (n, Some(p.trim_end_matches(']'))),
        None => (text, None),
    };
    let mut state = Block::by_name(name)
        .unwrap_or_else(|| panic!("block {name}"))
        .default_state();
    for kv in props.into_iter().flat_map(|p| p.split(',')) {
        let (k, v) = kv.split_once('=').unwrap();
        state = state
            .with_property_str(k, v)
            .unwrap_or_else(|| panic!("property {kv} of {text}"));
    }
    state
}

fn serialize(state: BlockState) -> String {
    let mut s = state.block().name().to_string();
    let props: Vec<String> = state
        .values()
        .map(|(d, i)| format!("{}={}", d.name, d.values[i]))
        .collect();
    if !props.is_empty() {
        s.push('[');
        s.push_str(&props.join(","));
        s.push(']');
    }
    s
}

fn type_name(e: &dyn Entity) -> String {
    let n = e.get_type().data().name;
    if n.contains(':') {
        n.to_string()
    } else {
        format!("minecraft:{n}")
    }
}

/// The `E` line of the oracle driver for the entity types of these scenarios.
fn describe(name: &str, e: &dyn Entity) -> String {
    let b = e.base();
    let (p, d) = (b.position(), b.delta_movement());
    let ty = type_name(e);
    let common = format!(
        "pos={:?},{:?},{:?} delta={:?},{:?},{:?} rot={:?},{:?} og={} fall={:?} fire={} air={} alive={} pose=standing",
        p.x,
        p.y,
        p.z,
        d.x,
        d.y,
        d.z,
        b.y_rot(),
        b.x_rot(),
        i32::from(b.on_ground()),
        b.fall_distance,
        b.remaining_fire_ticks,
        b.get_air_supply(),
        i32::from(e.is_alive())
    );
    if let Some(t) = e.as_any().downcast_ref::<PrimedTnt>() {
        format!("E {name} {ty} {common} fuse={}", t.get_fuse())
    } else if let Some(i) = e.as_any().downcast_ref::<ItemEntity>() {
        let item = i.get_item();
        format!(
            "E {name} {ty} {common} item={}*{} age={} pickup={} health={} owner={} thrower={}",
            item.item().name(),
            item.count(),
            i.age,
            i.pickup_delay,
            i.health,
            i.target.map_or("-".to_string(), |u| u.to_string()),
            i.thrower.map_or("-".to_string(), |u| u.to_string())
        )
    } else if let Some(o) = e.as_any().downcast_ref::<ExperienceOrb>() {
        format!(
            "E {name} {ty} {common} value={} count={} age={} health={}",
            o.get_value(),
            o.count,
            o.age,
            o.health
        )
    } else if let Some(f) = e.as_any().downcast_ref::<FallingBlockEntity>() {
        format!(
            "E {name} {ty} {common} state={} time={} hurt={} cancel={} drop={}",
            serialize(f.get_block_state()),
            f.time,
            f.hurts_entities(),
            f.is_drop_cancelled(),
            f.drop_item
        )
    } else {
        panic!("unexpected entity {ty}")
    }
}

fn run(scenario: &str) {
    // entity ids come from a process-wide counter: one scenario at a time
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let text = std::fs::read_to_string(scenario_dir().join(format!("{scenario}.txt"))).unwrap();
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = registries();
    // loot random sequences are seeded from the oracle world's seed (`VanillaLevel.Options.seed`)
    level.server_state.loot.world_seed = 4242;
    for x in -3..=40 {
        for z in -12..=12 {
            level
                .set_block_and_update(BlockPos::new(x, -61, z), Block::GRASS_BLOCK.default_state());
            level.set_block_and_update(BlockPos::new(x, -62, z), Block::DIRT.default_state());
            level.set_block_and_update(BlockPos::new(x, -63, z), Block::DIRT.default_state());
            level.set_block_and_update(BlockPos::new(x, -64, z), Block::BEDROCK.default_state());
        }
    }
    let i = |s: &str| s.parse::<i32>().unwrap();
    let f = |s: &str| s.parse::<f64>().unwrap();
    let mut watching = false;
    let mut names: std::collections::HashMap<EntityId, String> = Default::default();
    let mut counters: std::collections::HashMap<String, i32> = Default::default();
    let mut refs: Vec<(String, EntityId)> = Vec::new();
    let mut actual: Vec<String> = Vec::new();
    let mut ticks = 0;
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let a: Vec<&str> = line.split_whitespace().collect();
        match a[0] {
            "level" | "difficulty" => {}
            "note" => actual.push(format!("# {}", line[4..].trim())),
            "level_seed" => LevelAccessor::random(&mut level).set_seed(a[1].parse().unwrap()),
            "block" => {
                level.set_block_and_update(
                    BlockPos::new(i(a[1]), i(a[2]), i(a[3])),
                    parse_state(a[4]),
                );
            }
            "spawn_item" => {
                // spawn_item REF STACK X Y Z [vx vy vz] [count]
                let item = mc_data::Item::by_name(a[2]).unwrap_or_else(|| panic!("item {}", a[2]));
                let (count, motion) = match a.len() {
                    6 => (1, None),
                    9 => (1, Some((f(a[6]), f(a[7]), f(a[8])))),
                    10 => (i(a[9]), Some((f(a[6]), f(a[7]), f(a[8])))),
                    n => panic!("spawn_item with {n} tokens"),
                };
                let mut stack = ItemStack::new(item);
                stack.set_count(count);
                let (x, y, z) = (f(a[3]), f(a[4]), f(a[5]));
                let entity = match motion {
                    Some((dx, dy, dz)) => {
                        ItemEntity::with_stack_and_motion(&level, x, y, z, stack, dx, dy, dz)
                    }
                    None => ItemEntity::with_stack(&level, x, y, z, stack),
                };
                refs.push((a[1].to_string(), entity.id()));
                level.add_fresh_entity(Box::new(entity));
            }
            "spawn" => {
                // spawn REF TYPE X Y Z [yaw pitch] [SNBT]
                let ty = mc_data::EntityType::by_name(a[2])
                    .unwrap_or_else(|| panic!("entity type {}", a[2]));
                let mut entity = mc_game::world::entity::factory::create(
                    ty,
                    &level,
                    mc_game::world::entity::factory::EntitySpawnRequest {
                        reason: mc_data::enums::EntitySpawnReason::Command,
                        ignore_checks: false,
                    },
                )
                .unwrap();
                let n = a.len();
                let snbt = (n > 6 && a[n - 1].starts_with('{'))
                    .then(|| line[line.find(a[n - 1]).unwrap()..].to_string());
                let n = if snbt.is_some() { n - 1 } else { n };
                entity.snap_to(
                    f(a[3]),
                    f(a[4]),
                    f(a[5]),
                    if n > 7 { f(a[6]) as f32 } else { 0.0 },
                    if n > 7 { f(a[7]) as f32 } else { 0.0 },
                );
                if let Some(snbt) = snbt {
                    mc_game::world::entity::spawn_config::load_into(
                        &level,
                        entity.as_mut(),
                        &mc_nbt::snbt::parse_compound(&snbt).unwrap(),
                    );
                }
                refs.push((a[1].to_string(), entity.id()));
                level.add_fresh_entity(entity);
            }
            "watch" => watching = a[1] == "entities",
            "explode" => {
                let (x, y, z, r): (f64, f64, f64, f32) =
                    (f(a[1]), f(a[2]), f(a[3]), f(a[4]) as f32);
                let interaction = match *a.last().unwrap() {
                    "none" => ExplosionInteraction::None,
                    "block" => ExplosionInteraction::Block,
                    "mob" => ExplosionInteraction::Mob,
                    "tnt" => ExplosionInteraction::Tnt,
                    "trigger" => ExplosionInteraction::Trigger,
                    other => panic!("{other}"),
                };
                level.explode(
                    Explode::new(None, x, y, z, r, interaction).with_fire(a.contains(&"fire")),
                );
            }
            "tick" => {
                for _ in 0..a.get(1).map_or(1, |n| n.parse().unwrap()) {
                    // `entityTickList.forEach`: the entities that exist when the tick starts, in creation order
                    let everywhere =
                        mc_core::phys::AABB::new(-100.0, -100.0, -100.0, 100.0, 100.0, 100.0);
                    let mut snapshot: Vec<EntityId> =
                        level.get_entities(None, &everywhere, |_| true);
                    snapshot.sort_unstable();
                    for id in snapshot {
                        level.with_entity(id, |level, e| {
                            if !e.is_removed() {
                                mc_game::world::entity::entity::common_tick(e);
                                e.tick(level);
                            }
                        });
                    }
                    ticks += 1;
                    if watching {
                        actual.push(format!("T {ticks}"));
                        // the oracle names entities created by game logic `type_path#n` in creation (id) order when first dumped
                        let everywhere =
                            mc_core::phys::AABB::new(-100.0, -100.0, -100.0, 100.0, 100.0, 100.0);
                        let mut ids: Vec<EntityId> =
                            level.get_entities(None, &everywhere, |_| true);
                        ids.sort_unstable();
                        for id in ids {
                            let e = level.get_entity(id).unwrap();
                            if !e.is_alive() {
                                continue;
                            }
                            let name = match refs.iter().find(|(_, r)| *r == id) {
                                Some((name, _)) => name.clone(),
                                None => names
                                    .entry(id)
                                    .or_insert_with(|| {
                                        let path = type_name(e)
                                            .trim_start_matches("minecraft:")
                                            .to_string();
                                        let n = counters.entry(path.clone()).or_insert(0);
                                        *n += 1;
                                        format!("{path}#{n}")
                                    })
                                    .clone(),
                            };
                            actual.push(describe(&name, e));
                        }
                    }
                }
            }
            "dump" => {
                let c: Vec<i32> = a[1]
                    .strip_prefix("blocks:")
                    .unwrap()
                    .split(',')
                    .map(|v| v.parse().unwrap())
                    .collect();
                for x in c[0]..=c[3] {
                    for y in c[1]..=c[4] {
                        for z in c[2]..=c[5] {
                            let st = level.get_block_state(BlockPos::new(x, y, z));
                            if !st.is_air() {
                                actual.push(format!("B {x} {y} {z} {}", serialize(st)));
                            }
                        }
                    }
                }
            }
            other => panic!("unsupported scenario command {other}"),
        }
    }
    // Items created by game logic (`popResource`) draw their sideways motion and yaw from the entity's own random, which vanilla seeds
    // from the clock (the oracle from the command line); their merging and resting then depend on it: only the named items are compared.
    let drops = |line: &String| line.starts_with("E item#");
    let mut fixture = Fixture::load(scenario);
    fixture.lines.retain(|l| !drops(l));
    let actual: Vec<String> = actual.into_iter().filter(|l| !drops(l)).collect();
    fixture.assert_matches(
        &actual,
        &Options {
            ignore_kinds: vec!["#"],
            ignore_fields: vec!["rot"],
            ..Default::default()
        },
    );
}

#[test]
fn tnt_blocks_match_vanilla() {
    common::block_scenario::check("tnt_blocks");
}

#[test]
fn explosions_vs_entities_match_vanilla() {
    run("explosion_misc");
}
