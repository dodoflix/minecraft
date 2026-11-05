//! `advancements::predicates`: codec round trips and `matches` smoke tests over our own entities (the differential
//! test against the vanilla classes is `advancement_predicates_golden.rs`).

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_game::advancements::predicates::{DamageSourcePredicate, EntityPredicate, LocationPredicate};
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::{Entity, ServerPlayer};
use mc_protocol::codec::GameProfile;
use uuid::Uuid;

fn level() -> ServerLevel {
    ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap())
}

fn player(id: i32, name: &str) -> ServerPlayer {
    let mut p = ServerPlayer::new(id, Identifier::parse("minecraft:overworld").unwrap(), GameProfile { id: Uuid::from_u128(id as u128), name: name.into(), properties: vec![] }, Default::default());
    p.set_pos(1.5, 64.0, 2.5);
    p
}

fn decode<A: 'static>(codec: &mc_dfu::Codec<A>, json: &str) -> A {
    mc_dfu::parse_json(codec, json).into_result().unwrap_or_else(|e| panic!("{json}: {e}"))
}

#[test]
fn entity_predicate_codec_round_trips() {
    let json = r##"{"minecraft:entity_type":"minecraft:player","minecraft:distance":{"horizontal":{"min":3.0}},"minecraft:flags":{"is_sneaking":false},"minecraft:periodic_tick":5,"minecraft:vehicle":{"minecraft:entity_type":"#minecraft:boat"}}"##;
    let predicate = decode(&EntityPredicate::codec(), json);
    assert_eq!(predicate.parts().len(), 5);
    let encoded = mc_dfu::encode_json(&EntityPredicate::codec(), &predicate).into_result().unwrap();
    assert_eq!(decode(&EntityPredicate::codec(), &encoded), predicate);
    assert!(mc_dfu::parse_json(&EntityPredicate::codec(), r#"{"minecraft:nope":{}}"#).is_error());
}

#[test]
fn entity_predicate_matches_a_player() {
    let level = level();
    let mut p = player(7, "Steve");
    p.base_mut().tick_count = 10;
    let yes = decode(&EntityPredicate::codec(), r#"{"entity_type":"minecraft:player","periodic_tick":5,"flags":{"is_on_fire":false},"distance":{"absolute":{"max":10.0}}}"#);
    assert!(yes.matches(&level, Some(Vec3::new(1.5, 64.0, 3.5)), &p));
    let no = decode(&EntityPredicate::codec(), r#"{"entity_type":"minecraft:pig"}"#);
    assert!(!no.matches(&level, None, &p));
    let far = decode(&EntityPredicate::codec(), r#"{"distance":{"horizontal":{"min":3.0}}}"#);
    assert!(!far.matches(&level, Some(Vec3::new(1.5, 64.0, 3.5)), &p));
    assert!(!far.matches(&level, None, &p), "no origin: distance never matches");
}

#[test]
fn location_and_damage_source_codecs() {
    let location = decode(&LocationPredicate::codec(), r##"{"position":{"y":{"min":60.0}},"dimension":"minecraft:overworld","fluid":{"fluids":"#minecraft:water"},"block":{"blocks":"minecraft:stone"}}"##);
    let encoded = mc_dfu::encode_json(&LocationPredicate::codec(), &location).into_result().unwrap();
    assert_eq!(decode(&LocationPredicate::codec(), &encoded), location);
    let source = decode(&DamageSourcePredicate::codec(), r##"{"tags":[{"id":"#minecraft:is_fire","expected":true}],"is_direct":true}"##);
    let encoded = mc_dfu::encode_json(&DamageSourcePredicate::codec(), &source).into_result().unwrap();
    assert_eq!(decode(&DamageSourcePredicate::codec(), &encoded), source);
}
