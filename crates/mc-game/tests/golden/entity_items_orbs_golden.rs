//! Item entities, experience orbs, falling blocks and pickup against the real vanilla server: the scenarios
//! `tests/oracle/scenarios/{item,orb,falling}_*.txt` are replayed on a detached `ServerLevel` (flat grass floor, players added to the
//! level) and every watched line is compared with `tests/fixtures/entity_oracle/NAME.txt`.
//!
//! Determinism like the oracle: `entity_counter N` pins the entity id counter (ids matter for resting items `(tickCount + id) % 4` and
//! orb merging `(id - id') % 40`), `RandomSource.create()` seeds follow the oracle shadow's per-thread uniquifier (`reseed hash(line)`
//! before every command, `hash(name)` when spawning, `8682522807148012 ^ (ticks + 1) * 0x9E3779B97F4A7C15` before every tick) and
//! `Level.random` starts at `seed ^ 0x5DEECE66D` (`level_seed N`).
//!
//! Replay subset: `level seed difficulty preload note gamerule entity_counter reseed level_seed block fill spawn spawn_item snap nbt
//! entity_seed velocity kill set spawn_player give select award hurt drop tick watch dump`.
use crate::common;

use crate::common::entity_oracle::*;
use mc_core::ident::Identifier;
use mc_core::math::java::{java_double_to_string as d, java_float_to_string as f};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::enums::GameType;
use mc_game::server::level::ServerLevel;
use mc_game::world::container::Container;
use mc_game::world::damagesource::DamageSource;
use mc_game::world::entity::entity::common_tick;
use mc_game::world::entity::experience_orb::ExperienceOrb;
use mc_game::world::entity::falling_block_entity::FallingBlockEntity;
use mc_game::world::entity::item::ItemEntity;
use mc_game::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game::world::item::ItemStack;
use mc_game::world::level::{BlockGetter, LevelAccessor};
use mc_protocol::codec::GameProfile;
use std::collections::{HashMap, HashSet};

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
        Some((n, p)) => (n, p.trim_end_matches(']')),
        None => (text, ""),
    };
    let block = Block::by_name(name.trim_start_matches("minecraft:"))
        .or_else(|| Block::by_name(name))
        .unwrap_or_else(|| panic!("unknown block {name}"));
    let pairs: Vec<(&str, &str)> = props
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|kv| kv.split_once('=').expect("k=v"))
        .collect();
    block
        .state_from_properties(pairs)
        .unwrap_or_else(|| panic!("bad state {text}"))
}

/// `EntityOracleDriver.hash(line)` (Java `long` arithmetic over UTF-16 units).
fn hash(s: &str) -> i64 {
    s.encode_utf16().fold(1125899906842597i64, |h, c| {
        h.wrapping_mul(31).wrapping_add(i64::from(c))
    })
}

fn item_text(s: &ItemStack) -> String {
    if s.is_empty() {
        return "empty".into();
    }
    let mut t = format!("{}*{}", s.item().name(), s.count());
    if s.is_damaged() {
        t.push_str(&format!("/dmg={}", s.get_damage_value()));
    }
    t
}

fn vec(x: f64, y: f64, z: f64) -> String {
    format!("{},{},{}", d(x), d(y), d(z))
}

struct Sim {
    level: ServerLevel,
    refs: Vec<(String, EntityId)>,
    known: HashSet<EntityId>,
    auto: HashMap<String, i32>,
    players: Vec<EntityId>,
    watches: Vec<String>,
    out: Vec<String>,
    ticks: i64,
    world_created: bool,
    sequences: common::player_actions::Sequences,
}

impl Sim {
    fn new(seed: i64) -> Self {
        let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
        level.registries = registries();
        let world = mc_game::server::level::level_data::default_world_data();
        level.level_data = Box::new(mc_game::server::level::level_data::PrimaryServerLevelData(
            world,
        ));
        // `level flat`: grass at y=-61
        for x in -48..=64 {
            for z in -16..=48 {
                level.set_block(
                    BlockPos::new(x, -61, z),
                    Block::GRASS_BLOCK.default_state(),
                    3,
                    512,
                );
            }
        }
        for cx in -4..=4 {
            for cz in -2..=4 {
                level.entity_manager.update_chunk_status(
                    mc_core::pos::ChunkPos::new(cx, cz),
                    mc_game::world::level::entity::Visibility::Ticking,
                );
            }
        }
        level.random().set_seed(seed ^ 0x5DEECE66D);
        mc_core::random::reseed_uniquifier(8682522807148012);
        Sim {
            level,
            refs: Vec::new(),
            known: HashSet::new(),
            auto: HashMap::new(),
            players: Vec::new(),
            watches: Vec::new(),
            out: Vec::new(),
            ticks: 0,
            world_created: false,
            sequences: Default::default(),
        }
    }

    fn id_of(&self, name: &str) -> EntityId {
        self.refs
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("unknown ref {name}"))
            .1
    }

    fn entity(&self, id: EntityId) -> Option<&dyn Entity> {
        self.level
            .get_entity(id)
            .or_else(|| self.level.entity_manager.get_removed(id))
    }

    fn register(&mut self, name: &str, id: EntityId) {
        self.known.insert(id);
        self.refs.push((name.to_string(), id));
    }

    /// Entities created by game logic get stable names `type_path#n` in creation (id) order.
    fn name_new_entities(&mut self) {
        let mut ids = self.level.entity_manager.all_visible_ids();
        ids.sort_unstable();
        for id in ids {
            if self.known.insert(id) && !self.refs.iter().any(|(_, r)| *r == id) {
                let ty = self
                    .level
                    .get_entity(id)
                    .map(|e| {
                        e.get_type()
                            .data()
                            .name
                            .trim_start_matches("minecraft:")
                            .to_string()
                    })
                    .unwrap_or_default();
                let n = self
                    .auto
                    .entry(ty.clone())
                    .and_modify(|n| *n += 1)
                    .or_insert(1);
                self.refs.push((format!("{ty}#{n}"), id));
            }
        }
    }

    fn uuid_name(&self, uuid: Option<uuid::Uuid>) -> String {
        let Some(u) = uuid else { return "-".into() };
        for (name, id) in &self.refs {
            if self.entity(*id).is_some_and(|e| e.uuid() == u) {
                return name.clone();
            }
        }
        u.to_string()
    }

    /// `describe(name, entity)` of the driver.
    fn describe(&self, name: &str, id: EntityId) -> Option<String> {
        let e = self.entity(id)?;
        let b = e.base();
        let player = e.as_any().downcast_ref::<ServerPlayer>();
        let mut s = format!(
            "{} {name} {}",
            if player.is_some() { "P" } else { "E" },
            e.get_type().data().name
        );
        let pos = b.position();
        let dm = b.delta_movement();
        s.push_str(&format!(
            " pos={} delta={} rot={},{} og={} fall={} fire={} air={} alive={} pose={}",
            vec(pos.x, pos.y, pos.z),
            vec(dm.x, dm.y, dm.z),
            f(b.y_rot()),
            f(b.x_rot()),
            i32::from(b.on_ground()),
            d(b.fall_distance),
            b.remaining_fire_ticks,
            b.get_air_supply(),
            i32::from(e.is_alive()),
            format!("{:?}", b.get_pose()).to_lowercase(),
        ));
        if let Some(p) = player {
            let l = &p.player.living;
            s.push_str(&format!(
                " hp={} abs={} hurt={}/{} death={}",
                f(l.get_health()),
                f(p.player.get_absorption_amount()),
                l.hurt_time,
                l.damage_cooldown_time,
                l.death_time
            ));
            s.push_str(&format!(
                " food={} sat={} exh={} xp={}/{}/{}",
                p.player.food_data.get_food_level(),
                f(p.player.food_data.get_saturation_level()),
                f(p.player.food_data.exhaustion_level),
                p.player.experience_level,
                p.player.total_experience,
                f(p.player.experience_progress)
            ));
        } else if let Some(i) = e.as_any().downcast_ref::<ItemEntity>() {
            s.push_str(&format!(" item={} age={}", item_text(&i.get_item()), i.age));
            s.push_str(&format!(
                " pickup={} health={} owner={} thrower={}",
                i.pickup_delay,
                i.health,
                self.uuid_name(i.target),
                self.uuid_name(i.thrower)
            ));
        } else if let Some(o) = e.as_any().downcast_ref::<ExperienceOrb>() {
            s.push_str(&format!(
                " value={} count={} age={} health={}",
                o.get_value(),
                o.count,
                o.age,
                o.health
            ));
        } else if let Some(fb) = e.as_any().downcast_ref::<FallingBlockEntity>() {
            s.push_str(&format!(
                " state={} time={} hurt={} cancel={} drop={}",
                mc_game::commands::arguments::blocks::block_state_parser::serialize(
                    fb.get_block_state()
                ),
                fb.time,
                fb.hurt_entities,
                fb.cancel_drop,
                fb.drop_item
            ));
        }
        Some(s)
    }

    fn dump(&mut self, what: &[String]) {
        for w in what {
            let (kind, arg) = w.split_once(':').unwrap_or((w.as_str(), ""));
            match kind {
                "player" | "entity" => {
                    let id = self.id_of(arg);
                    if let Some(line) = self.describe(arg, id) {
                        self.out.push(line);
                    }
                }
                "entities" => {
                    self.name_new_entities();
                    let mut all: Vec<(String, EntityId)> = self
                        .refs
                        .iter()
                        .filter(|(_, id)| {
                            !self.players.contains(id)
                                && self.entity(*id).is_some_and(|e| e.is_alive())
                        })
                        .cloned()
                        .collect();
                    all.sort_by_key(|(_, id)| *id);
                    for (name, id) in all {
                        if let Some(line) = self.describe(&name, id) {
                            self.out.push(line);
                        }
                    }
                }
                "inventory" => {
                    let id = self.id_of(arg);
                    let Some(p) = self
                        .level
                        .get_entity(id)
                        .and_then(|e| e.as_any().downcast_ref::<ServerPlayer>())
                    else {
                        continue;
                    };
                    let inv = &p.player.inventory;
                    let lines: Vec<String> = (0..inv.get_container_size())
                        .filter(|&i| !inv.get_item(i).is_empty())
                        .map(|i| format!("I {arg} {i} {}", item_text(inv.get_item(i))))
                        .collect();
                    self.out.extend(lines);
                }
                "blocks" => {
                    let c: Vec<i32> = arg.split(',').map(|n| n.parse().unwrap()).collect();
                    for x in c[0].min(c[3])..=c[0].max(c[3]) {
                        for y in c[1].min(c[4])..=c[1].max(c[4]) {
                            for z in c[2].min(c[5])..=c[2].max(c[5]) {
                                let st = self.level.get_block_state(BlockPos::new(x, y, z));
                                if !st.is_air() {
                                    self.out.push(format!("B {x} {y} {z} {}", mc_game::commands::arguments::blocks::block_state_parser::serialize(st)));
                                }
                            }
                        }
                    }
                }
                other => panic!("dump {other}"),
            }
        }
    }

    fn tick_once(&mut self) {
        mc_core::random::reseed_uniquifier(
            8682522807148012 ^ (self.ticks + 1).wrapping_mul(0x9E3779B97F4A7C15u64 as i64),
        );
        for id in self.level.entity_manager.tick_list.snapshot() {
            self.level.with_entity(id, |level, e| {
                if !e.is_removed() {
                    common_tick(e);
                    e.tick(level);
                }
            });
        }
        self.level.tick_entity_management();
        // the connection phase of the server tick: `ServerPlayer.doTick`
        for id in self.players.clone() {
            self.level.with_player(id, |level, p| p.do_tick(level));
        }
        self.ticks += 1;
        self.name_new_entities();
        if !self.watches.is_empty() {
            self.out.push(format!("T {}", self.ticks));
            for w in self.watches.clone() {
                self.dump(&[w]);
            }
        }
    }

    fn spawn(&mut self, t: &[&str]) {
        use mc_game::world::entity::factory::{EntitySpawnRequest, create};
        let name = t[1];
        mc_core::random::reseed_uniquifier(hash(name));
        let ty = mc_data::EntityType::by_name(&format!(
            "minecraft:{}",
            t[2].trim_start_matches("minecraft:")
        ))
        .unwrap_or_else(|| panic!("entity type {}", t[2]));
        let request = EntitySpawnRequest {
            reason: mc_data::enums::EntitySpawnReason::Command,
            ignore_checks: false,
        };
        let mut e =
            create(ty, &self.level, request).unwrap_or_else(|| panic!("cannot create {}", t[2]));
        let mut n = t.len();
        let snbt = if n > 6 && t[n - 1].starts_with('{') {
            n -= 1;
            Some(t[n])
        } else {
            None
        };
        let num = |i: usize| t[i].parse::<f64>().unwrap();
        e.snap_to(
            num(3),
            num(4),
            num(5),
            if n > 7 { num(6) as f32 } else { 0.0 },
            if n > 7 { num(7) as f32 } else { 0.0 },
        );
        if let Some(snbt) = snbt {
            merge_nbt(e.as_mut(), snbt);
        }
        let id = e.id();
        self.level.add_fresh_entity_with_passengers(vec![e]);
        self.register(name, id);
    }

    fn run_line(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        let cmd = t[0];
        if self.world_created && cmd != "reseed" {
            mc_core::random::reseed_uniquifier(hash(line));
        }
        if !matches!(cmd, "level" | "seed" | "difficulty" | "preload" | "note") {
            self.world_created = true;
        }
        let num = |i: usize| t[i].parse::<f64>().unwrap();
        match cmd {
            "level" | "seed" | "difficulty" | "preload" | "note" => {}
            "gamerule" => {
                let v = t[2] == "true";
                self.level.server_state.rules.update(|r| match t[1] {
                    "entity_drops" => r.entity_drops = v,
                    "mob_griefing" => r.mob_griefing = v,
                    "fire_damage" => r.fire_damage = v,
                    other => panic!("gamerule {other}"),
                });
            }
            "entity_counter" => {
                self.level.entity_id_counter =
                    Some(std::sync::atomic::AtomicI32::new(t[1].parse().unwrap()))
            }
            "reseed" => mc_core::random::reseed_uniquifier(t[1].parse().unwrap()),
            "level_seed" => self.level.random().set_seed(t[1].parse().unwrap()),
            "block" => {
                self.level.set_block(
                    BlockPos::new(num(1) as i32, num(2) as i32, num(3) as i32),
                    parse_state(t[4]),
                    3,
                    512,
                );
            }
            "fill" => {
                let state = parse_state(t[7]);
                let (x0, x1) = (
                    (num(1) as i32).min(num(4) as i32),
                    (num(1) as i32).max(num(4) as i32),
                );
                let (y0, y1) = (
                    (num(2) as i32).min(num(5) as i32),
                    (num(2) as i32).max(num(5) as i32),
                );
                let (z0, z1) = (
                    (num(3) as i32).min(num(6) as i32),
                    (num(3) as i32).max(num(6) as i32),
                );
                for x in x0..=x1 {
                    for y in y0..=y1 {
                        for z in z0..=z1 {
                            self.level.set_block(BlockPos::new(x, y, z), state, 3, 512);
                        }
                    }
                }
            }
            "spawn" => self.spawn(&t),
            "spawn_item" => {
                mc_core::random::reseed_uniquifier(hash(t[1]));
                let count = t.get(9).map_or(1, |c| c.parse().unwrap());
                let stack = common::player_actions::item_stack(t[2], count);
                let e = if t.len() >= 9 {
                    ItemEntity::with_stack_and_motion(
                        &self.level,
                        num(3),
                        num(4),
                        num(5),
                        stack,
                        num(6),
                        num(7),
                        num(8),
                    )
                } else {
                    ItemEntity::with_stack(&self.level, num(3), num(4), num(5), stack)
                };
                let id = e.id();
                assert!(self.level.add_fresh_entity(Box::new(e)));
                self.register(t[1], id);
            }
            "snap" => {
                let id = self.id_of(t[1]);
                self.level.with_entity(id, |_, e| {
                    let (yr, xr) = (e.base().y_rot(), e.base().x_rot());
                    e.snap_to(
                        num(2),
                        num(3),
                        num(4),
                        if t.len() > 5 { num(5) as f32 } else { yr },
                        if t.len() > 6 { num(6) as f32 } else { xr },
                    );
                });
            }
            "nbt" => {
                let id = self.id_of(t[1]);
                let snbt = &line[line.find(t[2]).unwrap()..];
                self.level.with_entity(id, |_, e| merge_nbt(e, snbt));
            }
            "entity_seed" => {
                let id = self.id_of(t[1]);
                self.level.with_entity(id, |_, e| {
                    e.base_mut().random.set_seed(t[2].parse().unwrap())
                });
            }
            "velocity" => {
                let id = self.id_of(t[1]);
                self.level.with_entity(id, |_, e| {
                    e.base_mut().set_delta_movement_xyz(num(2), num(3), num(4))
                });
            }
            "kill" => {
                let id = self.id_of(t[1]);
                self.level.with_entity(id, |_, e| e.discard());
            }
            "set" => {
                let id = self.id_of(t[1]);
                self.level.with_player(id, |level, p| match t[2] {
                    "xp_level" => p.set_experience_levels(t[3].parse().unwrap()),
                    "xp_points" => p.give_experience_points(level, t[3].parse().unwrap()),
                    "health" => p.player.living.set_health(t[3].parse().unwrap()),
                    other => panic!("set {other}"),
                });
            }
            "spawn_player" => {
                let id = self.level.next_entity_id();
                let profile = GameProfile {
                    id: uuid::Uuid::from_u128(0x5000 + id as u128),
                    name: t[1].into(),
                    properties: vec![],
                };
                let mut p = ServerPlayer::new(
                    id,
                    Identifier::parse("minecraft:overworld").unwrap(),
                    profile,
                    Default::default(),
                );
                p.set_game_mode_for_player(GameType::by_serialized_name(t[2]).unwrap(), None);
                p.snap_to(
                    num(3),
                    num(4),
                    num(5),
                    if t.len() > 7 { num(6) as f32 } else { 0.0 },
                    if t.len() > 7 { num(7) as f32 } else { 0.0 },
                );
                self.level.add_player(Box::new(p));
                self.players.push(id);
                self.register(t[1], id);
            }
            "award" => {
                let at = Vec3::new(num(1), num(2), num(3));
                let amount = t[4].parse().unwrap();
                if t.len() >= 8 {
                    ExperienceOrb::award_with_direction(
                        &mut self.level,
                        &at,
                        Vec3::new(num(5), num(6), num(7)),
                        amount,
                    );
                } else {
                    ExperienceOrb::award(&mut self.level, &at, amount);
                }
            }
            "hurt" => {
                let id = self.id_of(t[1]);
                let key = Identifier::parse(&format!("minecraft:{}", t[2])).unwrap();
                let data = self
                    .level
                    .registries
                    .get::<mc_game::registry::typed::damage_type::DamageType>(
                        "minecraft:damage_type",
                        &key,
                    )
                    .expect("damage type")
                    .clone();
                let source = DamageSource::new(key, std::sync::Arc::new(data), None, None, None);
                let r = self
                    .level
                    .with_entity(id, |level, e| {
                        e.hurt_server(level, &source, t[3].parse::<f64>().unwrap() as f32)
                    })
                    .unwrap_or(false);
                self.out
                    .push(format!("HURT {} {} {} -> {}", t[1], t[2], t[3], r));
            }
            "tick" => {
                for _ in 0..t.get(1).map_or(1, |n| n.parse().unwrap()) {
                    self.tick_once();
                }
            }
            "watch" => {
                if t.get(1) == Some(&"off") {
                    self.watches.clear();
                } else {
                    self.watches.extend(t[1..].iter().map(|s| s.to_string()));
                }
            }
            "dump" => {
                let what: Vec<String> = t[1..].iter().map(|s| s.to_string()).collect();
                self.dump(&what);
            }
            other => {
                let tokens: Vec<String> = t.iter().map(|s| s.to_string()).collect();
                if !common::player_actions::command(self, &tokens) {
                    panic!("unsupported scenario command `{other}`");
                }
            }
        }
        self.name_new_entities();
    }
}

impl common::player_actions::ActionHost for Sim {
    fn boot(&mut self) {}
    fn action_level(&mut self) -> &mut ServerLevel {
        &mut self.level
    }
    fn action_ref(&self, name: &str) -> EntityId {
        self.id_of(name)
    }
    fn next_sequence(&mut self, player: &str) -> i32 {
        self.sequences.next(player)
    }
    fn emit(&mut self, line: String) {
        self.out.push(line);
    }
}

/// `nbt REF SNBT` / the SNBT of `spawn`: merge into the entity's save data and `load` it again.
fn merge_nbt(e: &mut dyn Entity, snbt: &str) {
    let mut tag = mc_nbt::CompoundTag::new();
    e.save_without_id(&mut tag, &|_| None);
    let patch =
        mc_nbt::snbt::parse_compound(snbt).unwrap_or_else(|err| panic!("snbt {snbt}: {err:?}"));
    tag.merge(&patch);
    e.load(&tag).expect("entity load");
}

fn replay(name: &str) -> Vec<String> {
    let text = std::fs::read_to_string(scenario_dir().join(format!("{name}.txt"))).unwrap();
    let seed = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("seed "))
        .map_or(4242, |s| s.trim().parse().unwrap());
    let mut sim = Sim::new(seed);
    for line in text.lines() {
        sim.run_line(line);
    }
    sim.out
}

fn check(name: &str) {
    let opts = Options {
        ignore_kinds: vec!["PKT", "C", "#", "B", "DIG", "EFFECT", "A"],
        ignore_fields: vec!["pose"],
        ..Default::default()
    };
    // Player lines: the player physics fields (`og`, `fire`, ...) are covered by the player tests; only items/orbs/falling blocks compare them.
    let strip = |text: &str| -> Vec<String> {
        text.lines()
            .map(|l| {
                if l.starts_with("P ") {
                    l.split(' ')
                        .filter(|t| {
                            !["og", "fire", "fx", "gm", "sel", "main"]
                                .contains(&t.split('=').next().unwrap_or(""))
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    l.to_string()
                }
            })
            .collect()
    };
    let text = std::fs::read_to_string(fixture_dir().join(format!("{name}.txt"))).unwrap();
    let actual = strip(&replay(name).join("\n"));
    Fixture::from_text(name, &strip(&text).join("\n")).assert_matches(&actual, &opts);
}

#[test]
fn orb_motion_matches_vanilla() {
    check("orb_motion");
}

#[test]
fn orb_award_and_merge_match_vanilla() {
    check("orb_award");
}

#[test]
fn orb_follow_pickup_and_mending_match_vanilla() {
    check("orb_pickup");
}

#[test]
fn item_pickup_and_throw_match_vanilla() {
    check("item_pickup");
}

#[test]
fn item_damage_and_lifetime_match_vanilla() {
    check("item_damage");
}

#[test]
fn falling_blocks_match_vanilla() {
    check("falling_basic");
}

#[test]
fn falling_anvil_and_concrete_match_vanilla() {
    check("falling_special");
}
