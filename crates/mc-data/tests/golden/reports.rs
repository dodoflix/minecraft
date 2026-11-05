//! mc-data against the vanilla data generator reports (`ref/generated/reports`).

use crate::common::read_json;
use mc_data::block::{BLOCK_COUNT, Block, BlockState};
use mc_data::{ids, registry};

#[test]
fn counts() {
    assert_eq!(BLOCK_COUNT, 1286);
    assert_eq!(mc_data::block::STATE_COUNT, 35723);
    assert_eq!(registry::registries::ALL.len(), 95);
    assert_eq!(Block::registry().len(), 1286);
}

#[test]
fn registries_match_report() {
    let report = read_json("ref/generated/reports/registries.json");
    let report = report.as_object().unwrap();
    assert_eq!(report.len(), registry::registries::ALL.len());
    for (name, r) in report {
        let reg = registry::by_name(name).unwrap_or_else(|| panic!("missing registry {name}"));
        let entries = r["entries"].as_object().unwrap();
        assert_eq!(reg.len(), entries.len(), "{name}: entry count");
        assert_eq!(reg.default_name(), r["default"].as_str(), "{name}: default");
        for (entry, e) in entries {
            let id = e["protocol_id"].as_u64().unwrap() as u32;
            assert_eq!(reg.id_of(entry), Some(id), "{name}: {entry}");
            assert_eq!(reg.name_of(id), Some(entry.as_str()), "{name}: id {id}");
        }
    }
}

#[test]
fn blocks_and_states_match_report() {
    let report = read_json("ref/generated/reports/blocks.json");
    let report = report.as_object().unwrap();
    assert_eq!(report.len(), BLOCK_COUNT);
    let mut states = 0;
    for (name, b) in report {
        let block = Block::by_name(name).unwrap_or_else(|| panic!("missing block {name}"));
        assert_eq!(block.name(), name);
        // property names + value lists (the report lists properties sorted by name, like StateDefinition)
        let props: Vec<_> = block.properties().collect();
        let rp = b.get("properties").and_then(|p| p.as_object());
        assert_eq!(props.len(), rp.map_or(0, |p| p.len()), "{name}: property count");
        if let Some(rp) = rp {
            // StateDefinition orders properties by name; the report's object order depends on serde_json's
            // preserve_order feature (enabled workspace-wide by other crates), so sort explicitly.
            let mut rp: Vec<_> = rp.iter().collect();
            rp.sort_by(|a, b| a.0.cmp(b.0));
            for (def, (rname, rvalues)) in props.iter().zip(rp) {
                assert_eq!(def.name, rname, "{name}");
                let rv: Vec<&str> = rvalues.as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
                assert_eq!(def.values, rv.as_slice(), "{name}.{rname}");
            }
        }
        let rs = b["states"].as_array().unwrap();
        assert_eq!(block.state_count(), rs.len(), "{name}: state count");
        for st in rs {
            states += 1;
            let id = st["id"].as_u64().unwrap() as u32;
            let state = BlockState::from_id(id).unwrap();
            assert_eq!(state.block(), block, "{name}: state {id}");
            if st["default"].as_bool() == Some(true) {
                assert_eq!(block.default_state(), state, "{name}: default state");
            }
            let mut got: Vec<(String, String)> = state.values().map(|(d, i)| (d.name.to_string(), d.values[i].to_string())).collect();
            got.sort();
            let mut want: Vec<(String, String)> = st
                .get("properties")
                .and_then(|p| p.as_object())
                .map(|p| p.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string())).collect())
                .unwrap_or_default();
            want.sort();
            assert_eq!(got, want, "{name}: state {id}");
        }
    }
    assert_eq!(states, 35723);
}

#[test]
fn typed_ids() {
    assert_eq!(ids::Block::STONE.id(), 1);
    assert_eq!(ids::Block::STONE.name(), "minecraft:stone");
    assert_eq!(ids::Block::by_name("stone"), Some(ids::Block::STONE));
    assert_eq!(ids::Block::AIR.default_state().id(), 0);
    assert_eq!(ids::Item::by_name("minecraft:diamond_pickaxe").unwrap().name(), "minecraft:diamond_pickaxe");
    assert_eq!(ids::EntityType::PIG.id(), 101);
    assert_eq!(ids::EntityType::PLAYER.name(), "minecraft:player");
    assert!(ids::Block::by_name("minecraft:nope").is_none());
    assert_eq!(ids::Block::STONE.identifier().to_string(), "minecraft:stone");
}
