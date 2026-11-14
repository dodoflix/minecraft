//! A player file written by the real vanilla server is loaded into a `ServerPlayer` and saved again; the NBT must
//! equal the original (the vehicle/`Dimension` parts are supplied by the level in a real run).

use mc_core::ident::Identifier;
use mc_game::world::entity::ServerPlayer;
use mc_game::world::entity::entity::{self, Entity};
use mc_nbt::CompoundTag;
use mc_protocol::codec::GameProfile;

fn diff(a: &CompoundTag, b: &CompoundTag) -> String {
    let mut out = Vec::new();
    for (k, v) in a.iter() {
        match b.get(k) {
            None => out.push(format!("missing {k}: {v:?}")),
            Some(w) if w != v => out.push(format!("{k}: {v:?} != {w:?}")),
            _ => {}
        }
    }
    for k in b.keys() {
        if a.get(k).is_none() {
            out.push(format!("extra {k}: {:?}", b.get(k)));
        }
    }
    out.join("\n")
}

#[test]
fn vanilla_player_file_round_trips() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../mc-game/tests/fixtures/playerdata/vanilla_steve.dat"
    ))
    .unwrap();
    let original =
        mc_nbt::compress::read_compressed(&bytes, &mut mc_nbt::NbtAccounter::unlimited_heap())
            .unwrap();
    let uuid = entity::read_uuid(original.get("UUID")).expect("uuid");
    let mut player = ServerPlayer::new(
        1,
        Identifier::parse("minecraft:overworld").unwrap(),
        GameProfile {
            id: uuid,
            name: "Steve".into(),
            properties: vec![],
        },
        Default::default(),
    );
    let level = mc_game_core::server::level::ServerLevel::detached(
        Identifier::parse("minecraft:overworld").unwrap(),
    );
    player.load(&level, &original).unwrap();
    let none = |_| None;
    let saved = player.save_player_data(&none);
    // Documented differences: `XpSeed` 0 is replaced by a random seed on load (vanilla does the same); the order of
    // `attributes` is a Java hash order.
    let mut original = original;
    original.put_int("XpSeed", saved.get_int_or("XpSeed", 0));
    let sort_attributes = |c: &mut CompoundTag| {
        if let Some(list) = c.get_list("attributes") {
            let mut v: Vec<_> = list.iter().cloned().collect();
            v.sort_by_key(|t| {
                t.as_compound()
                    .and_then(|c| c.get_string("id"))
                    .unwrap_or("")
                    .to_string()
            });
            let mut l = mc_nbt::ListTag::new();
            for t in v {
                l.push(t);
            }
            c.put("attributes", l);
        }
    };
    let mut saved = saved;
    sort_attributes(&mut original);
    sort_attributes(&mut saved);
    let d = diff(&original, &saved);
    assert!(d.is_empty(), "player data differs:\n{d}");
}
