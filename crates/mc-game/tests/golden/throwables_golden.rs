//! Thrown items vs the real vanilla classes (`tests/oracle/scenarios/throw_*.txt`, `firework_*.txt`): flight (gravity, drag, water
//! inertia), block hits, experience bottle orb rewards, firework lifetime/explosion. Entities are created directly with the
//! seed `entity_seed` gives them in the scenario.
use crate::common;

use crate::common::entity_oracle::*;
use mc_core::ident::Identifier;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_core::random::LegacyRandomSource;
use mc_data::block::{Block, BlockState};
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::experience_orb::ExperienceOrb;
use mc_game::world::entity::projectile::throwableitemprojectile::snowball::Snowball;
use mc_game::world::entity::projectile::throwableitemprojectile::thrown_experience_bottle::ThrownExperienceBottle;
use mc_game::world::entity::{Entity, EntityId};
use mc_game::world::item::ItemStack;
use mc_game::world::level::LevelAccessor;
use std::collections::HashMap;
use std::sync::LazyLock;

static REGISTRIES: LazyLock<mc_game::registry::RegistryAccess> = LazyLock::new(|| {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_, stem) = common::load_world(
        dir.path(),
        mc_world::level_storage::WorldDataConfiguration::DEFAULT,
        true,
    )
    .expect("default world loads");
    stem.registries.composite_access().clone()
});

/// The flat world of the oracle: grass at y=-61, bedrock -64.
fn flat_level() -> ServerLevel {
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = REGISTRIES.clone();
    for x in -4..=40 {
        for z in -12..=12 {
            level
                .set_block_and_update(BlockPos::new(x, -61, z), Block::GRASS_BLOCK.default_state());
            level.set_block_and_update(BlockPos::new(x, -62, z), Block::DIRT.default_state());
            level.set_block_and_update(BlockPos::new(x, -63, z), Block::DIRT.default_state());
            level.set_block_and_update(BlockPos::new(x, -64, z), Block::BEDROCK.default_state());
        }
    }
    level
}

fn fill(level: &mut ServerLevel, a: (i32, i32, i32), b: (i32, i32, i32), state: BlockState) {
    for x in a.0..=b.0 {
        for y in a.1..=b.1 {
            for z in a.2..=b.2 {
                level.set_block_and_update(BlockPos::new(x, y, z), state);
            }
        }
    }
}

/// Names of the entities like the oracle driver: refs of the scenario, then `type_path#n` in creation order.
struct Harness {
    level: ServerLevel,
    named: Vec<(String, EntityId)>,
    counters: HashMap<String, i32>,
    out: Vec<String>,
    tick: u32,
}

impl Harness {
    fn new(level: ServerLevel) -> Self {
        Harness {
            level,
            named: Vec::new(),
            counters: HashMap::new(),
            out: Vec::new(),
            tick: 0,
        }
    }

    /// `spawn REF ...` + `snap` + `velocity` + `entity_seed`.
    fn spawn(
        &mut self,
        name: &str,
        mut e: Box<dyn Entity>,
        pos: (f64, f64, f64),
        delta: (f64, f64, f64),
        seed: Option<i64>,
    ) {
        e.snap_to(pos.0, pos.1, pos.2, 0.0, 0.0);
        e.base_mut()
            .set_delta_movement_xyz(delta.0, delta.1, delta.2);
        if let Some(seed) = seed {
            e.base_mut().random = LegacyRandomSource::new(seed);
        }
        let id = e.id();
        self.level.add_fresh_entity(e);
        self.named.push((name.to_string(), id));
    }

    fn name_new(&mut self) {
        let big = AABB::new(-1.0e6, -1.0e4, -1.0e6, 1.0e6, 1.0e4, 1.0e6);
        let mut ids = self.level.get_entities(None, &big, |_| true);
        ids.sort_unstable();
        for id in ids {
            if self.named.iter().all(|(_, n)| *n != id) {
                let path = self
                    .level
                    .get_entity(id)
                    .unwrap()
                    .get_type()
                    .data()
                    .name
                    .trim_start_matches("minecraft:")
                    .to_string();
                let n = self.counters.entry(path.clone()).or_insert(0);
                *n += 1;
                self.named.push((format!("{path}#{n}"), id));
            }
        }
    }

    fn describe(name: &str, e: &dyn Entity) -> String {
        if let Some(orb) = e.as_any().downcast_ref::<ExperienceOrb>() {
            return format!("O {name} value={}", orb.get_value());
        }
        let b = e.base();
        let (p, d) = (b.position(), b.delta_movement());
        format!(
            "E {name} {} pos={:?},{:?},{:?} delta={:?},{:?},{:?} rot={:?},{:?} og={} fall={:?} fire={} air={} alive=1 pose=standing",
            e.get_type().data().name,
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
            b.get_air_supply()
        )
    }

    /// `tick N` with `watch entities`.
    fn run(&mut self, ticks: u32) {
        for _ in 0..ticks {
            let mut ids: Vec<EntityId> = self.named.iter().map(|(_, id)| *id).collect();
            ids.sort_unstable();
            for id in ids {
                self.level.with_entity(id, |level, e| {
                    if !e.is_removed() {
                        mc_game::world::entity::entity::common_tick(e);
                        e.tick(level);
                    }
                });
            }
            self.name_new();
            self.tick += 1;
            self.out.push(format!("T {}", self.tick));
            let mut live: Vec<(String, EntityId)> = self.named.clone();
            live.sort_by_key(|(_, id)| *id);
            for (name, id) in live {
                if let Some(e) = self.level.get_entity(id).filter(|e| e.is_alive()) {
                    self.out.push(Self::describe(&name, e));
                }
            }
        }
    }
}

/// Fixture lines of the `entities` watch: orbs reduced to their value (their own motion draws from an unseeded random);
/// entities that were removed stay out (the oracle lists live ones only, already).
fn fixture_lines(name: &str) -> Vec<String> {
    Fixture::load(name)
        .lines
        .iter()
        .map(|l| {
            if let Some(rest) = l.strip_prefix("E experience_orb#") {
                let (n, rest) = rest.split_once(' ').unwrap();
                let value = rest
                    .split_whitespace()
                    .find_map(|t| t.strip_prefix("value="))
                    .unwrap();
                format!("O experience_orb#{n} value={value}")
            } else {
                l.replacen("E ", "E ", 1)
            }
        })
        .collect()
}

fn check(h: &Harness, name: &str) {
    Fixture {
        name: name.to_string(),
        lines: fixture_lines(name),
    }
    .assert_matches(
        &h.out,
        &Options {
            ignore_kinds: vec!["#"],
            ..Default::default()
        },
    );
}

fn item(i: mc_data::Item) -> ItemStack {
    ItemStack::new(i)
}

#[test]
fn snowballs_match_vanilla() {
    let mut level = flat_level();
    fill(
        &mut level,
        (12, -60, -3),
        (12, -50, 3),
        Block::STONE.default_state(),
    );
    fill(
        &mut level,
        (20, -61, -3),
        (24, -56, 3),
        Block::STONE.default_state(),
    );
    fill(
        &mut level,
        (21, -60, -2),
        (23, -56, 2),
        Block::WATER.default_state(),
    );
    let mut h = Harness::new(level);
    let a = Snowball::at(&h.level, 0.5, -58.0, 0.5, &item(mc_data::Item::SNOWBALL));
    h.spawn("a", Box::new(a), (0.5, -58.0, 0.5), (0.6, 0.3, 0.1), None);
    let b = Snowball::at(&h.level, 2.5, -55.0, 0.5, &item(mc_data::Item::SNOWBALL));
    h.spawn("b", Box::new(b), (2.5, -55.0, 0.5), (1.2, 0.2, 0.0), None);
    let w = Snowball::at(&h.level, 20.5, -50.0, 0.5, &item(mc_data::Item::SNOWBALL));
    h.spawn("w", Box::new(w), (20.5, -50.0, 0.5), (0.5, -0.2, 0.0), None);
    h.run(40);
    check(&h, "throw_snowball");
}

#[test]
fn experience_bottle_matches_vanilla() {
    let mut level = flat_level();
    LevelAccessor::random(&mut level).set_seed(5);
    let mut h = Harness::new(level);
    let a = ThrownExperienceBottle::at(
        &h.level,
        0.5,
        -56.0,
        0.5,
        &item(mc_data::Item::EXPERIENCE_BOTTLE),
    );
    h.spawn(
        "a",
        Box::new(a),
        (0.5, -56.0, 0.5),
        (0.4, 0.1, 0.2),
        Some(99),
    );
    h.run(30);
    check(&h, "throw_xp_bottle");
}
