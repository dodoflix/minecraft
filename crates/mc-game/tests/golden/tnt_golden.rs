//! Primed TNT entities vs the real vanilla classes (`tests/oracle/scenarios/tnt_chain.txt`): gravity/drag/bounce, the fuse, the
//! explosion pushing other primed TNTs, block destruction with drop decay (level RNG order) and the blocks left.
use crate::common;

use crate::common::entity_oracle::*;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::item::primed_tnt::PrimedTnt;
use mc_game::world::entity::{Entity, EntityId};
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

fn describe(name: &str, e: &PrimedTnt) -> String {
    let b = e.base();
    let (p, d) = (b.position(), b.delta_movement());
    format!(
        "E {name} minecraft:tnt pos={:?},{:?},{:?} delta={:?},{:?},{:?} rot={:?},{:?} og={} fall={:?} fire={} air={} alive={} pose=standing fuse={}",
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
        i32::from(e.is_alive()),
        e.get_fuse()
    )
}

#[test]
fn primed_tnt_chain_matches_vanilla() {
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = registries();
    for x in -3..=40 {
        for z in -12..=12 {
            level
                .set_block_and_update(BlockPos::new(x, -61, z), Block::GRASS_BLOCK.default_state());
            level.set_block_and_update(BlockPos::new(x, -62, z), Block::DIRT.default_state());
            level.set_block_and_update(BlockPos::new(x, -63, z), Block::DIRT.default_state());
            level.set_block_and_update(BlockPos::new(x, -64, z), Block::BEDROCK.default_state());
        }
    }
    let set = |level: &mut ServerLevel, x, y, z, s: BlockState| {
        level.set_block_and_update(BlockPos::new(x, y, z), s)
    };
    LevelAccessor::random(&mut level).set_seed(77);
    set(&mut level, 12, -60, 0, Block::STONE.default_state());
    set(&mut level, 12, -60, 1, Block::STONE.default_state());
    set(&mut level, 13, -60, 1, Block::DIRT.default_state());
    let specs = [
        ("t1", 10.5, -60.0, 0.5, 6),
        ("t2", 11.5, -58.0, 0.5, 40),
        ("t3", 10.5, -57.0, 3.5, 15),
        ("t4", 14.5, -59.5, -2.5, 70),
    ];
    let mut ids: Vec<(&str, EntityId)> = Vec::new();
    for (name, x, y, z, fuse) in specs {
        let mut t = PrimedTnt::new(mc_data::EntityType::TNT, level.next_entity_id());
        t.snap_to(x, y, z, 0.0, 0.0);
        t.set_fuse(fuse);
        ids.push((name, t.id()));
        level.add_fresh_entity(Box::new(t));
    }
    let mut actual = Vec::new();
    for t in 1..=80 {
        for &(_, id) in &ids {
            level.with_entity(id, |level, e| {
                if !e.is_removed() {
                    mc_game::world::entity::entity::common_tick(e);
                    e.tick(level);
                }
            });
        }
        actual.push(format!("T {t}"));
        for &(name, id) in &ids {
            if let Some(e) = level
                .get_entity(id)
                .and_then(|e| e.as_any().downcast_ref::<PrimedTnt>())
            {
                actual.push(describe(name, e));
            }
        }
    }
    for x in 6..=20 {
        for y in -62..=-54 {
            for z in -8..=8 {
                let st = level.get_block_state(BlockPos::new(x, y, z));
                if !st.is_air() {
                    actual.push(format!("B {x} {y} {z} {}", serialize(st)));
                }
            }
        }
    }
    let mut fixture = Fixture::load("tnt_chain");
    // removed entities are no longer in our level; vanilla keeps printing them with alive=0
    fixture
        .lines
        .retain(|l| !(l.starts_with("E ") && l.contains("alive=0")));
    fixture.assert_matches(
        &actual,
        &Options {
            ignore_kinds: vec!["#"],
            ..Default::default()
        },
    );
}
