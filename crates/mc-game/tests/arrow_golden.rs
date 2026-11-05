//! Arrow entities against the real vanilla server: the scenarios `tests/oracle/scenarios/arrow_*.txt` are replayed on a detached
//! `ServerLevel` and every `E` line (position, motion, rotation, flags, shake/life counters) is compared with
//! `tests/fixtures/entity_oracle/NAME.txt` (regenerate: `scripts/entity-oracle.sh SCENARIO -o FIXTURE`).
//! Replay subset: `level`, `difficulty`, `fill`, `block`, `spawn` (arrow / spectral_arrow / trident, optional SNBT applied with
//! `readAdditionalSaveData`), `entity_seed`, `velocity`, `watch`, `tick`.
mod common;
use common::entity_oracle::*;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource as _;
use mc_data::EntityType;
use mc_data::block::{Block, BlockState};
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::EntityId;
use mc_game::world::entity::entity::{Entity, common_tick};
use mc_game::world::entity::projectile::arrow::{AbstractArrow, Arrow, SpectralArrow, ThrownTrident};
use mc_game::world::level::LevelAccessor;

fn parse_state(text: &str) -> BlockState {
    let (name, props) = match text.split_once('[') {
        Some((n, p)) => (n, p.trim_end_matches(']')),
        None => (text, ""),
    };
    let block = Block::by_name(name).unwrap_or_else(|| panic!("unknown block {name}"));
    let pairs: Vec<(&str, &str)> = props.split(',').filter(|s| !s.is_empty()).map(|kv| kv.split_once('=').expect("k=v")).collect();
    block.state_from_properties(pairs).unwrap_or_else(|| panic!("bad state {text}"))
}

static ACCESS: std::sync::LazyLock<mc_game::registry::RegistryAccess> = std::sync::LazyLock::new(|| {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_, stem) = common::load_world(dir.path(), mc_world::level_storage::WorldDataConfiguration::DEFAULT, true).expect("default world loads");
    stem.registries.composite_access().clone()
});

fn registries() -> mc_game::registry::RegistryAccess {
    ACCESS.clone()
}

fn vec3(x: f64, y: f64, z: f64) -> String {
    format!("{x:?},{y:?},{z:?}")
}

/// The arrow suffix of the `E` line (`ground= crit= pierce= pickup= shake= life= dmg=`).
fn arrow_fields(e: &dyn Entity) -> String {
    let any = e.as_any();
    let a: &dyn AbstractArrow = if let Some(a) = any.downcast_ref::<Arrow>() {
        a
    } else if let Some(a) = any.downcast_ref::<SpectralArrow>() {
        a
    } else if let Some(a) = any.downcast_ref::<ThrownTrident>() {
        a
    } else {
        return String::new();
    };
    let d = a.arrow_data();
    format!(
        " ground={} crit={} pierce={} pickup={} shake={} life={} dmg={:?}",
        i32::from(a.is_in_ground()),
        i32::from(a.is_crit_arrow()),
        a.pierce_level(),
        d.pickup.ordinal(),
        d.shake_time,
        d.life,
        d.base_damage
    )
}

fn replay(scenario: &str) -> Vec<String> {
    let text = std::fs::read_to_string(scenario_dir().join(format!("{scenario}.txt"))).unwrap();
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = registries();
    for x in -48..=64 {
        for z in -16..=48 {
            level.set_block(BlockPos::new(x, -61, z), Block::GRASS_BLOCK.default_state(), 3, 512);
        }
    }
    for cx in -4..=4 {
        for cz in -2..=4 {
            level.entity_manager.update_chunk_status(mc_core::pos::ChunkPos::new(cx, cz), mc_game::world::level::entity::Visibility::Ticking);
        }
    }
    let mut refs: Vec<(String, EntityId)> = Vec::new();
    let mut out = Vec::new();
    let mut tick = 0;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let t: Vec<&str> = line.split_whitespace().collect();
        let num = |i: usize| t[i].parse::<f64>().unwrap();
        let id_of = |name: &str, refs: &[(String, EntityId)]| refs.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("no ref {name}")).1;
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
            "spawn" => {
                let ty = EntityType::by_name(t[2]).unwrap_or_else(|| panic!("entity type {}", t[2]));
                let eid = level.next_entity_id();
                let mut e: Box<dyn Entity> = match ty {
                    EntityType::ARROW => Box::new(Arrow::new(ty, eid)),
                    EntityType::SPECTRAL_ARROW => Box::new(SpectralArrow::new(ty, eid)),
                    EntityType::TRIDENT => Box::new(ThrownTrident::new(ty, eid)),
                    other => panic!("unsupported entity {other}"),
                };
                let (yaw, pitch) = if t.len() > 7 && !t[6].starts_with('{') { (num(6) as f32, num(7) as f32) } else { (0.0, 0.0) };
                e.snap_to(num(3), num(4), num(5), yaw, pitch);
                if let Some(snbt) = t.last().filter(|s| s.starts_with('{')) {
                    let tag = mc_nbt::snbt::parse_compound(snbt).expect("snbt");
                    e.read_additional_save_data(&tag);
                }
                assert!(level.add_fresh_entity(e));
                refs.push((t[1].to_string(), eid));
            }
            "entity_seed" => {
                let id = id_of(t[1], &refs);
                let seed: i64 = t[2].parse().unwrap();
                level.with_entity(id, |_, e| e.base_mut().random.set_seed(seed));
            }
            "velocity" => {
                let id = id_of(t[1], &refs);
                level.with_entity(id, |_, e| e.base_mut().set_delta_movement(mc_core::phys::Vec3::new(num(2), num(3), num(4))));
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
                        out.push(format!(
                            "E {name} {} pos={} delta={} rot={},{} og={} fall={:?} fire={} air={} alive={} pose=standing{}",
                            e.get_type().data().name,
                            vec3(b.x(), b.y(), b.z()),
                            vec3(b.delta_movement().x, b.delta_movement().y, b.delta_movement().z),
                            b.y_rot(),
                            b.x_rot(),
                            i32::from(b.on_ground()),
                            b.fall_distance,
                            b.remaining_fire_ticks,
                            b.get_air_supply(),
                            i32::from(e.is_alive()),
                            arrow_fields(e),
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
    let opts = Options { ignore_fields: vec!["pose"], ignore_kinds: vec!["#", "C", "PKT"], ..Default::default() };
    fixture.assert_matches(&actual, &opts);
}

#[test]
fn arrow_flight_matches_vanilla() {
    compare_scenario("arrow_flight");
}
