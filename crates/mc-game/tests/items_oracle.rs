//! Replays the `item_*` entity-oracle scenarios (`tests/oracle/scenarios`, vanilla output in
//! `tests/fixtures/entity_oracle`) on a real in-process `ServerLevel` (void generator, blocks placed by the test) and
//! compares every watched `E` line bit-exactly. Regenerate fixtures: `scripts/entity-oracle.sh <scenario> -o <fixture>`.
mod common;
use common::entity_oracle::*;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::item::ItemEntity;
use mc_game::world::entity::{Entity, EntityId};
use mc_game::world::item::ItemStack;
use mc_game::world::level::LevelAccessor;
use mc_data::Item;

/// The flat world of the oracle: grass at y=-61 (the player stands at y=-60).
fn flat_level() -> ServerLevel {
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    for x in -3..=26 {
        for z in -4..=4 {
            level.set_block_and_update(BlockPos::new(x, -61, z), Block::GRASS_BLOCK.default_state());
        }
    }
    level
}

fn fill(level: &mut ServerLevel, (x1, y1, z1): (i32, i32, i32), (x2, y2, z2): (i32, i32, i32), state: BlockState) {
    for x in x1..=x2 {
        for y in y1..=y2 {
            for z in z1..=z2 {
                level.set_block_and_update(BlockPos::new(x, y, z), state);
            }
        }
    }
}

/// `spawn_item REF STACK X Y Z vx vy vz` + `snap REF X Y Z yaw pitch`.
fn spawn_item(level: &mut ServerLevel, stack: ItemStack, pos: (f64, f64, f64), motion: (f64, f64, f64), yaw: f32) -> EntityId {
    let mut e = ItemEntity::with_stack_and_motion(level, pos.0, pos.1, pos.2, stack, motion.0, motion.1, motion.2);
    e.set_pos(pos.0, pos.1, pos.2);
    e.set_rot(yaw, 0.0);
    let id = e.id();
    level.add_fresh_entity(Box::new(e));
    id
}

/// One server tick of the entities (`ServerLevel.tickNonPassenger` in spawn order).
fn tick_entities(level: &mut ServerLevel, ids: &[EntityId]) {
    for &id in ids {
        level.with_entity(id, |level, e| {
            if !e.is_removed() {
                level.tick_non_passenger(e);
            }
        });
    }
}

fn vec3(a: f64, b: f64, c: f64) -> String {
    format!("{a:?},{b:?},{c:?}")
}

/// The `E` line of the oracle driver for an item entity.
fn describe_item(name: &str, e: &ItemEntity) -> String {
    let b = e.base();
    let p = b.position();
    let d = b.delta_movement();
    let item = e.get_item();
    format!(
        "E {name} minecraft:item pos={} delta={} rot={:?},{:?} og={} fall={:?} fire={} air={} alive={} pose=standing item={}*{} age={}",
        vec3(p.x, p.y, p.z),
        vec3(d.x, d.y, d.z),
        b.y_rot(),
        b.x_rot(),
        i32::from(b.on_ground()),
        b.fall_distance,
        b.remaining_fire_ticks,
        b.get_air_supply(),
        i32::from(e.is_alive()),
        item.item().name(),
        item.count(),
        e.age
    )
}

#[test]
fn item_fall_matches_vanilla() {
    let mut level = flat_level();
    fill(&mut level, (10, -61, -2), (16, -61, 2), Block::PACKED_ICE.default_state());
    fill(&mut level, (20, -61, -2), (22, -56, 2), Block::WATER.default_state());
    let diamond = || ItemStack::new(Item::DIAMOND);
    let names = ["a", "b", "c", "w"];
    let ids = [
        spawn_item(&mut level, diamond(), (0.5, -50.0, 0.5), (0.0, 0.0, 0.0), 0.0),
        spawn_item(&mut level, diamond(), (3.5, -58.0, 0.5), (0.1, 0.2, -0.05), 90.0),
        spawn_item(&mut level, diamond(), (11.5, -60.0, 0.5), (0.3, 0.0, 0.0), 0.0),
        spawn_item(&mut level, diamond(), (21.5, -52.0, 0.5), (0.0, 0.0, 0.0), 0.0),
    ];
    let mut actual = Vec::new();
    for t in 1..=100 {
        tick_entities(&mut level, &ids);
        actual.push(format!("T {t}"));
        for (name, &id) in names.iter().zip(&ids) {
            if let Some(e) = level.get_entity(id).and_then(|e| e.as_any().downcast_ref::<ItemEntity>()) {
                actual.push(describe_item(name, e));
            }
        }
    }
    Fixture::load("item_fall").assert_matches(&actual, &Options::default());
}

fn golden_lines(prefix: &str) -> Vec<Vec<String>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/item_entity_golden.txt");
    let text = std::fs::read_to_string(path).unwrap();
    text.lines().filter(|l| l.starts_with(prefix)).map(|l| l.split(' ').skip(1).map(str::to_string).collect()).collect()
}

#[test]
fn experience_orb_split_and_value_match_vanilla() {
    use mc_game::world::entity::experience_orb::ExperienceOrb;
    let splits = golden_lines("orb_split ");
    assert_eq!(splits.len(), 5000);
    for line in &splits {
        let n: i32 = line[0].parse().unwrap();
        let mut amount = n;
        let mut parts = Vec::new();
        while amount > 0 {
            let v = ExperienceOrb::get_experience_value(amount);
            amount -= v;
            parts.push(v.to_string());
        }
        assert_eq!(parts.join(","), line[1], "split of {n}");
    }
    for line in golden_lines("orb_value ") {
        let v: i32 = line[0].parse().unwrap();
        assert_eq!(ExperienceOrb::get_experience_value(v).to_string(), line[1], "value {v}");
    }
}

#[test]
fn item_merge_rules_match_vanilla() {
    use mc_game::world::entity::item::{are_mergable, merge_stacks};
    use mc_game::world::item::ItemStackExt as _;
    let make = |kind: &str, count: i32| -> ItemStack {
        let mut s = match kind {
            "diamond" | "named_diamond" => ItemStack::new(Item::DIAMOND),
            "stone" => ItemStack::new(Item::STONE),
            "ender_pearl" => ItemStack::new(Item::ENDER_PEARL),
            _ => ItemStack::new(Item::IRON_SWORD),
        };
        if kind == "named_diamond" {
            s.set(mc_protocol::components::CUSTOM_NAME, mc_text::Component::literal("x"));
        }
        if kind == "damaged_sword" {
            s.set_damage_value(5);
        }
        s.copy_with_count(count)
    };
    let lines = golden_lines("merge ");
    assert!(lines.len() > 1000);
    for l in &lines {
        let (a, ca, b, cb) = (&l[0], l[1].parse::<i32>().unwrap(), &l[2], l[3].parse::<i32>().unwrap());
        let (to, mut from) = (make(a, ca), make(b, cb));
        let mergable = are_mergable(&to, &from);
        let new_to = merge_stacks(&to, &mut from, 64);
        assert_eq!(
            format!("{mergable} {} {}", new_to.count(), from.count()),
            format!("{} {} {}", l[4], l[5], l[6]),
            "merge {a}*{ca} <- {b}*{cb}"
        );
    }
}
