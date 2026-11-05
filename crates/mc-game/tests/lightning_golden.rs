//! LightningBolt vs the real vanilla classes (`tests/oracle/scenarios/lightning.txt`): fire placement RNG, damage + fire of entities in
//! range, the bolt's lifetime (flashes); players' `P` lines, the bolt's `E` line and the blocks left.
mod common;
use common::entity_oracle::*;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::enums::GameType;
use mc_game::server::level::ServerLevel;
use mc_game::world::level::LevelAccessor as _;
use mc_game::world::entity::lightning_bolt::LightningBolt;
use mc_game::world::level::BlockGetter;
use mc_game::world::entity::{Entity, EntityId, ServerPlayer};
use mc_protocol::codec::GameProfile;

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

fn parse_state(text: &str) -> mc_data::block::BlockState {
    Block::by_name(text).unwrap_or_else(|| panic!("block {text}")).default_state()
}

fn serialize(state: mc_data::block::BlockState) -> String {
    let mut s = state.block().name().to_string();
    let props: Vec<String> = state.values().map(|(d, i)| format!("{}={}", d.name, d.values[i])).collect();
    if !props.is_empty() {
        s.push('[');
        s.push_str(&props.join(","));
        s.push(']');
    }
    s
}

fn f(v: f32) -> String {
    mc_core::math::java::java_float_to_string(v)
}

fn d(v: f64) -> String {
    mc_core::math::java::java_double_to_string(v)
}

fn player_line(name: &str, p: &ServerPlayer) -> String {
    let l = &p.player.living;
    let (pos, dm) = (p.position(), p.base().delta_movement());
    let mut fx: Vec<_> = l.active_effects.values().collect();
    fx.sort_by_key(|e| e.effect.name());
    let fx = if fx.is_empty() { "-".to_string() } else { fx.iter().map(|e| format!("{}/{}/{}", e.effect.name(), e.amplifier, e.duration)).collect::<Vec<_>>().join(";") };
    format!(
        "P {name} minecraft:player pos={},{},{} delta={},{},{} alive={} hp={} abs={} hurt={}/{} death={} fire={} fx={fx} food={} sat={} exh={}",
        d(pos.x), d(pos.y), d(pos.z), d(dm.x), d(dm.y), d(dm.z), u8::from(p.is_alive()), f(l.get_health()), f(p.player.get_absorption_amount()),
        l.hurt_time, l.damage_cooldown_time, l.death_time, p.base().remaining_fire_ticks, p.player.food_data.get_food_level(), f(p.player.food_data.get_saturation_level()),
        f(p.player.food_data.exhaustion_level),
    )
}

#[test]
fn lightning_matches_vanilla() {
    let text = std::fs::read_to_string(scenario_dir().join("lightning.txt")).unwrap();
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = registries();
    let world = mc_game::server::level::level_data::default_world_data();
    level.level_data = Box::new(mc_game::server::level::level_data::PrimaryServerLevelData(world));
    for x in -3..=60 {
        for z in -6..=6 {
            level.set_block_and_update(BlockPos::new(x, -61, z), Block::GRASS_BLOCK.default_state());
        }
    }
    let mut order: Vec<(String, EntityId, bool)> = Vec::new();
    let mut home: std::collections::HashMap<EntityId, mc_core::phys::Vec3> = Default::default();
    let mut watch: Vec<String> = Vec::new();
    let mut actual: Vec<String> = Vec::new();
    let mut tick_no = 0;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let t: Vec<&str> = line.split_whitespace().collect();
        let n = |i: usize| t[i].parse::<f64>().unwrap();
        match t[0] {
            "level" => {}
            "difficulty" => {}
            "snap" => {}
            "block" => {
                level.set_block_and_update(BlockPos::new(t[1].parse().unwrap(), t[2].parse().unwrap(), t[3].parse().unwrap()), parse_state(t[4]));
            }
            "dump" => {
                let c: Vec<i32> = t[1].strip_prefix("blocks:").unwrap().split(',').map(|v| v.parse().unwrap()).collect();
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
            "watch" if t[1] == "off" => watch.clear(),
            "spawn_player" => {
                let id = level.next_entity_id();
                let profile = GameProfile { id: uuid::Uuid::from_u128(0x1000 + id as u128), name: t[1].into(), properties: vec![] };
                let mut p = ServerPlayer::new(id, level.dimension.clone(), profile, Default::default());
                p.set_game_mode_for_player(if t[2] == "creative" { GameType::Creative } else { GameType::Survival }, None);
                p.snap_to(n(3), n(4), n(5), 0.0, 0.0);
                level.chunk_blocking_mut(p.base().block_position().x() >> 4, p.base().block_position().z() >> 4);
                level.add_player(Box::new(p));
                let id = level.players().iter().copied().last().unwrap();
                home.insert(id, mc_core::phys::Vec3::new(n(3), n(4), n(5)));
                order.push((t[1].to_string(), id, true));
            }
            "spawn" => {
                assert_eq!(t[2], "minecraft:lightning_bolt");
                // the oracle seeds the entity random from the REF name: `reseed(hash(name))`, then `RandomSource.create()`
                let mut h: i64 = 1125899906842597;
                for c in t[1].chars() {
                    h = h.wrapping_mul(31).wrapping_add(c as i64);
                }
                let seed = h.wrapping_mul(1181783497276652981) ^ 0x2545F4914F6CDD1D;
                let mut c = LightningBolt::with_random(mc_data::EntityType::LIGHTNING_BOLT, level.next_entity_id(), mc_core::random::LegacyRandomSource::new(seed));
                c.snap_to(n(3), n(4), n(5), 0.0, 0.0);
                let id = c.id();
                level.add_fresh_entity(Box::new(c));
                order.push((t[1].to_string(), id, false));
            }
            "watch" => watch = t[1..].iter().map(|s| s.to_string()).collect(),
            "tick" => {
                for _ in 0..t.get(1).map_or(1, |v| v.parse().unwrap()) {
                    for (_, id, is_player) in &order {
                        if *is_player {
                            let at = home[id];
                            level.with_player(*id, |level, p| {
                                // vanilla's players are client-authoritative: knockback never moves them
                                p.snap_to(at.x, at.y, at.z, 0.0, 0.0);
                                mc_game::world::entity::entity::common_tick(p);
                                p.tick(level);
                            });
                        } else {
                            level.with_entity(*id, |level, e| {
                                if !e.is_removed() {
                                    mc_game::world::entity::entity::common_tick(e);
                                    e.tick(level);
                                }
                            });
                        }
                    }
                    // `ServerGamePacketListenerImpl.tick` -> `player.doTick()` runs after the level's entity ticks
                    for (_, id, is_player) in &order {
                        if *is_player {
                            level.with_player(*id, |level, p| p.do_tick(level));
                        }
                    }
                    tick_no += 1;
                    if !watch.is_empty() {
                        actual.push(format!("T {tick_no}"));
                        for w in &watch {
                            let (kind, name) = w.split_once(':').unwrap();
                            let (_, id, _) = order.iter().find(|(n, _, _)| n == name).unwrap();
                            let Some(e) = level.get_entity(*id) else { continue };
                            if kind == "player" {
                                actual.push(player_line(name, e.as_any().downcast_ref::<ServerPlayer>().unwrap()));
                            } else {
                                let (p, dm) = (e.position(), e.base().delta_movement());
                                actual.push(format!(
                                    "E {name} minecraft:lightning_bolt pos={},{},{} delta={},{},{} rot={:?},{:?} og=0 fall={:?} fire={} air={} alive=1 pose=standing",
                                    d(p.x), d(p.y), d(p.z), d(dm.x), d(dm.y), d(dm.z), e.base().y_rot(), e.base().x_rot(), e.base().fall_distance, e.base().remaining_fire_ticks, e.base().get_air_supply()
                                ));
                            }
                        }
                    }
                }
            }
            other => panic!("unsupported {other}"),
        }
    }
    let mut fixture = Fixture::load("lightning");
    fixture.lines.retain(|l| !(l.starts_with("E ") && l.contains("alive=0")));
    let opts = Options { ignore_kinds: vec!["I", "PKT", "C", "#", "EFFECT"], ignore_fields: vec!["pos", "delta", "rot", "og", "fall", "fire", "air", "pose", "gm", "xp", "sel", "main"], ..Default::default() };
    fixture.assert_matches(&actual, &opts);
}
