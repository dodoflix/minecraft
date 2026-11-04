//! Block state behaviour against golden values generated from the vanilla classes
//! (`tests/fixtures/GoldenBlocks.java` → `block_golden.json`).

mod common;
use common::golden;
use mc_core::direction::{Axis, Direction};
use mc_data::block::{Block, BlockState, PathComputationType, SupportType};
use mc_data::enums::{BedPart, Half, SlabType, StairsShape};
use mc_data::props::{self, Property};
use serde_json::Value;

fn aabbs(v: &Value) -> Vec<[f64; 6]> {
    v.as_array().unwrap().iter().map(|b| {
        let b = b.as_array().unwrap();
        std::array::from_fn(|i| b[i].as_f64().unwrap())
    }).collect()
}

#[test]
fn state_data_matches_vanilla() {
    let g = golden();
    assert!(g.len() > 700);
    for e in &g {
        let id = e["id"].as_u64().unwrap() as u32;
        let s = BlockState::from_id(id).unwrap();
        let ctx = format!("state {id} {s}");
        assert_eq!(s.block().id() as u64, e["block"].as_u64().unwrap(), "{ctx}");
        // toString(): Block{minecraft:x}[props]
        let ours = s.to_string();
        let (name, props) = ours.split_once('[').map_or((ours.as_str(), None), |(n, p)| (n, Some(p)));
        let want = match props {
            Some(p) => format!("Block{{{name}}}[{p}"),
            None => format!("Block{{{name}}}"),
        };
        assert_eq!(want, e["string"].as_str().unwrap());

        assert_eq!(s.get_light_emission() as u64, e["light"].as_u64().unwrap(), "{ctx}");
        assert_eq!(s.get_light_block() as u64, e["light_block"].as_u64().unwrap(), "{ctx}");
        let f = &e["flags"];
        let flag = |k: &str| f[k].as_bool().unwrap();
        assert_eq!(s.can_occlude(), flag("can_occlude"), "{ctx}");
        assert_eq!(s.is_solid_render(), flag("solid_render"), "{ctx}");
        assert_eq!(s.is_air(), flag("is_air"), "{ctx}");
        assert_eq!(s.propagates_skylight_down(), flag("propagates_skylight_down"), "{ctx}");
        assert_eq!(s.use_shape_for_light_occlusion(), flag("use_shape_for_light_occlusion"), "{ctx}");
        assert_eq!(s.is_redstone_conductor(), flag("is_redstone_conductor"), "{ctx}");
        assert_eq!(s.is_suffocating(), flag("is_suffocating"), "{ctx}");
        assert_eq!(s.is_view_blocking(), flag("is_view_blocking"), "{ctx}");
        assert_eq!(s.has_large_collision_shape(), flag("has_large_collision_shape"), "{ctx}");
        assert_eq!(s.requires_correct_tool_for_drops(), flag("requires_correct_tool_for_drops"), "{ctx}");
        assert_eq!(s.is_collision_shape_full_block(), flag("is_collision_shape_full_block"), "{ctx}");
        assert_eq!(s.is_solid(), flag("is_solid"), "{ctx}");
        assert_eq!(s.is_randomly_ticking(), flag("is_randomly_ticking"), "{ctx}");
        assert_eq!(s.is_signal_source(), flag("is_signal_source"), "{ctx}");
        assert_eq!(s.has_analog_output_signal(), flag("has_analog_output_signal"), "{ctx}");
        assert_eq!(s.can_be_replaced(), flag("can_be_replaced"), "{ctx}");
        assert_eq!(s.liquid(), flag("liquid"), "{ctx}");
        assert_eq!(s.ignited_by_lava(), flag("ignited_by_lava"), "{ctx}");
        assert_eq!(s.get_destroy_speed(), e["destroy_speed"].as_f64().unwrap() as f32, "{ctx}");
        assert_eq!(s.get_shade_brightness(), e["shade_brightness"].as_f64().unwrap() as f32, "{ctx}");
        assert_eq!(s.get_fluid_state().id() as u64, e["fluid_state"].as_u64().unwrap(), "{ctx}");

        let sturdy = e["face_sturdy"].as_array().unwrap();
        for d in Direction::ALL {
            for (i, t) in [SupportType::Full, SupportType::Center, SupportType::Rigid].into_iter().enumerate() {
                assert_eq!(s.is_face_sturdy(d, t), sturdy[d.ordinal() * 3 + i].as_bool().unwrap(), "{ctx} {d:?} {t:?}");
            }
        }
        let path = e["pathfindable"].as_array().unwrap();
        for (i, t) in [PathComputationType::Land, PathComputationType::Water, PathComputationType::Air].into_iter().enumerate() {
            assert_eq!(s.is_pathfindable(t), path[i].as_bool().unwrap(), "{ctx} {t:?}");
        }

        let sh = &e["shapes"];
        for (key, id) in [
            ("collision", s.collision_shape()),
            ("outline", s.shape()),
            ("occlusion", s.occlusion_shape()),
            ("support", s.block_support_shape()),
            ("interaction", s.interaction_shape()),
            ("visual", s.visual_shape()),
        ] {
            assert_eq!(id.data().aabbs, aabbs(&sh[key]).as_slice(), "{ctx} {key}");
        }
    }
}

#[test]
fn property_set_and_cycle_match_vanilla() {
    for e in &golden() {
        let id = e["id"].as_u64().unwrap() as u32;
        let s = BlockState::from_id(id).unwrap();
        for v in e["set"].as_array().unwrap() {
            let (p, val, new) = (v[0].as_str().unwrap(), v[1].as_str().unwrap(), v[2].as_u64().unwrap() as u32);
            assert_eq!(s.with_property_str(p, val).map(BlockState::id), Some(new), "{s}: {p}={val}");
        }
        for c in e["cycle"].as_array().unwrap() {
            let (p, new) = (c[0].as_str().unwrap(), c[1].as_u64().unwrap() as u32);
            let (def, cur) = s.values().find(|(d, _)| d.name == p).unwrap();
            let next = def.values[(cur + 1) % def.values.len()];
            assert_eq!(s.with_property_str(p, next).map(BlockState::id), Some(new), "{s}: cycle {p}");
        }
        // unknown property / value
        assert_eq!(s.with_property_str("no_such_property", "x"), None);
        assert_eq!(s.with_property_str(s.values().next().map_or("x", |(d, _)| d.name), "no_such_value"), None);
    }
}

/// Typed API == vanilla for a spread of typed property constants (boolean, integer, enum, subset enums).
fn check_typed<P: Property>(p: P, name: &str) {
    let mut checked = 0;
    for e in golden() {
        let s = BlockState::from_id(e["id"].as_u64().unwrap() as u32).unwrap();
        let Some(cur) = s.try_get(p) else {
            assert!(!s.block().has_property(p));
            continue;
        };
        checked += 1;
        let def = p.def();
        assert_eq!(def.name, name);
        let cur_idx = p.value_index(cur);
        assert_eq!(s.get_property_str(name), Some(def.values[cur_idx]), "{s}");
        for v in e["set"].as_array().unwrap().iter().filter(|v| v[0] == name) {
            let i = def.value_index(v[1].as_str().unwrap()).unwrap();
            assert_eq!(s.set(p, p.value(i)).id() as u64, v[2].as_u64().unwrap(), "{s}: set {name}={}", v[1]);
            assert_eq!(s.try_set(p, p.value(i)), Some(s.set(p, p.value(i))));
        }
        for c in e["cycle"].as_array().unwrap().iter().filter(|c| c[0] == name) {
            assert_eq!(s.cycle(p).id() as u64, c[1].as_u64().unwrap(), "{s}: cycle {name}");
        }
    }
    assert!(checked > 3, "{name}: only {checked} golden states have it");
}

#[test]
fn typed_properties_match_vanilla() {
    check_typed(props::WATERLOGGED, "waterlogged");
    check_typed(props::POWERED, "powered");
    check_typed(props::LIT, "lit");
    check_typed(props::OPEN, "open");
    check_typed(props::HALF, "half");
    check_typed(props::STAIRS_SHAPE, "shape");
    check_typed(props::SLAB_TYPE, "type");
    check_typed(props::HORIZONTAL_FACING, "facing");
    check_typed(props::FACING, "facing");
    check_typed(props::AXIS, "axis");
    check_typed(props::AGE_7, "age");
    check_typed(props::LEVEL, "level");
    check_typed(props::NORTH_REDSTONE, "north");
    check_typed(props::BED_PART, "part");
    check_typed(props::DOUBLE_BLOCK_HALF, "half");
}

#[test]
fn typed_property_semantics() {
    let stairs = Block::OAK_STAIRS.default_state();
    assert_eq!(stairs.get(props::HORIZONTAL_FACING), Direction::North);
    assert_eq!(stairs.get(props::HALF), Half::Bottom);
    assert_eq!(stairs.get(props::STAIRS_SHAPE), StairsShape::Straight);
    assert!(!stairs.get(props::WATERLOGGED));
    let s = stairs.set(props::HORIZONTAL_FACING, Direction::East).set(props::HALF, Half::Top).set(props::WATERLOGGED, true);
    assert_eq!(s.get(props::HORIZONTAL_FACING), Direction::East);
    assert_eq!(s.get(props::HALF), Half::Top);
    assert!(s.get(props::WATERLOGGED));
    assert_eq!(s.to_string(), "minecraft:oak_stairs[facing=east,half=top,shape=straight,waterlogged=true]");
    assert_eq!(s.try_get(props::AXIS), None);
    assert_eq!(s.try_set(props::AXIS, Axis::Y), None);
    assert!(!Block::OAK_STAIRS.has_property(props::AXIS));
    assert_eq!(Block::OAK_LOG.default_state().get(props::AXIS), Axis::Y);
    assert_eq!(Block::OAK_LOG.default_state().cycle(props::AXIS).get(props::AXIS), Axis::Z);
    assert_eq!(Block::OAK_LOG.default_state().cycle(props::AXIS).cycle(props::AXIS).cycle(props::AXIS).get(props::AXIS), Axis::Y);
    // properties that share a serialized name but not a value set are distinct constants
    assert_eq!(props::HORIZONTAL_FACING.values().len(), 4);
    assert_eq!(props::FACING.values().len(), 6);
    assert_eq!(props::FACING_HOPPER.values().len(), 5);
    assert_eq!(Block::HOPPER.default_state().get(props::FACING_HOPPER), Direction::Down);
    assert!(!Block::HOPPER.has_property(props::FACING));
    assert_eq!(Block::WHITE_BED.default_state().get(props::BED_PART), BedPart::Foot);
    assert_eq!(Block::OAK_SLAB.default_state().get(props::SLAB_TYPE), SlabType::Bottom);
    assert_eq!(Block::REDSTONE_WIRE.default_state().get(props::POWER), 0);
    assert_eq!(Block::REDSTONE_WIRE.default_state().set(props::POWER, 15).id(), Block::REDSTONE_WIRE.default_state().id() + 15 * 9);
}

#[test]
#[should_panic(expected = "does not have property")]
fn get_missing_property_panics() {
    Block::STONE.default_state().get(props::LIT);
}

#[test]
#[should_panic(expected = "cannot hold")]
fn set_out_of_range_integer_panics() {
    Block::REDSTONE_WIRE.default_state().set(props::POWER, 16);
}
