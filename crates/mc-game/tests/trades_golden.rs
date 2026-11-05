//! Villager / wandering trader trades vs the real vanilla (fixture from `scripts/golden-trades.sh`, `tests/oracle/TradesOracle.java`):
//! the decoded `villager_trade` / `trade_set` registries (re-encoded, canonical digest), seeded generation for every profession x
//! level x villager type x seed, the real `getOffers()` path (level random sequences), the wandering trader, offers mutated like a
//! trading session, the saved form (`MerchantOffers.CODEC`), the wire form and the offers packet.
mod common;

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_data::EntityType;
use mc_game::registry::keys;
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::entity::{Entity, EntityData};
use mc_game::world::item::trading::trade_sets::{WANDERING_TRADER_BUYING, WANDERING_TRADER_COMMON, WANDERING_TRADER_UNCOMMON, profession_trade_set, trade_set_key};
use mc_game::world::item::trading::{
    MerchantOffers, TradeSet, VillagerTrade, add_offers_from_trade_set, add_offers_from_trade_set_in, get_trade_set,
};
use mc_game::world::level::saveddata::random_sequences::RandomSequences;
use mc_game::world::level::storage::loot::server_access::LootServices;
use mc_game::world::level::storage::loot::{LootContext, LootContextParamSets, LootParams};
use mc_nbt::Tag;
use mc_protocol::codec::Encode;
use mc_protocol::components::values::entity::VILLAGER_VARIANT;
use mc_protocol::components::{DataComponentMap, Holder};
use mc_protocol::packets::play::ClientboundMerchantOffersPacket;
use mc_world::level_storage::WorldDataConfiguration;
use sha1::{Digest, Sha1};

const PROFESSIONS: [&str; 13] =
    ["armorer", "butcher", "cartographer", "cleric", "farmer", "fisherman", "fletcher", "leatherworker", "librarian", "mason", "shepherd", "toolsmith", "weaponsmith"];

/// A merchant for the merchant predicates: position and the villager type component (`Villager.get(VILLAGER_VARIANT)`).
struct TestMerchant {
    base: EntityData,
    variant: Option<Identifier>,
}

impl TestMerchant {
    fn new(ty: EntityType, variant: Option<&str>, pos: (f64, f64, f64)) -> TestMerchant {
        let mut base = EntityData::new(ty, 1, 300, |_| {});
        base.set_pos_raw_and_box(pos.0, pos.1, pos.2);
        TestMerchant { base, variant: variant.map(|v| Identifier::parse(&format!("minecraft:{v}")).unwrap()) }
    }
    fn villager(variant: &str) -> TestMerchant {
        TestMerchant::new(EntityType::VILLAGER, Some(variant), (100.5, 64.0, -20.5))
    }
    fn trader() -> TestMerchant {
        TestMerchant::new(EntityType::WANDERING_TRADER, None, (10.5, 70.0, -3.5))
    }
}

impl Entity for TestMerchant {
    fn base(&self) -> &EntityData {
        &self.base
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.base
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn as_dyn(&self) -> &dyn Entity {
        self
    }
    fn as_dyn_mut(&mut self) -> &mut dyn Entity {
        self
    }
    fn entity_components(&self) -> DataComponentMap {
        let mut b = DataComponentMap::builder();
        if let Some(v) = &self.variant {
            b = b.set(VILLAGER_VARIANT, Holder::Reference(v.clone()));
        }
        b.build()
    }
}

struct Fixture {
    lines: Vec<(char, String)>,
}

impl Fixture {
    fn load() -> Fixture {
        let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trades_golden.txt")).unwrap();
        Fixture { lines: text.lines().filter_map(|l| l.split_once(' ')).map(|(t, r)| (t.chars().next().unwrap(), r.to_owned())).collect() }
    }
    fn of(&self, tag: char) -> Vec<Vec<&str>> {
        self.lines.iter().filter(|(t, _)| *t == tag).map(|(_, r)| r.split(' ').collect()).collect()
    }
}

fn level_with_registries(dir: &std::path::Path) -> ServerLevel {
    let (_config, stem) = common::load_world(dir, WorldDataConfiguration::DEFAULT, true).expect("default world loads");
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = stem.registries.composite_access().clone();
    reset_sequences(&mut level);
    level
}

/// A fresh server random-sequence state with the oracle world's seed (`VanillaLevel.Options.seed`).
fn reset_sequences(level: &mut ServerLevel) {
    level.server_state.loot = LootServices { random_sequences: Arc::new(parking_lot::Mutex::new(RandomSequences::default())), world_seed: 4242, ..LootServices::default() };
}

fn canon(tag: &Tag, out: &mut String) {
    use std::fmt::Write;
    match tag {
        Tag::Compound(c) => {
            let mut entries: Vec<_> = c.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            out.push('{');
            for (i, (k, v)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write!(out, "{k}:").unwrap();
                canon(v, out);
            }
            out.push('}');
        }
        Tag::List(l) => {
            out.push('[');
            for (i, v) in l.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canon(v, out);
            }
            out.push(']');
        }
        Tag::Byte(v) => write!(out, "{v}b").unwrap(),
        Tag::Short(v) => write!(out, "{v}s").unwrap(),
        Tag::Int(v) => write!(out, "{v}i").unwrap(),
        Tag::Long(v) => write!(out, "{v}l").unwrap(),
        Tag::Float(v) => write!(out, "{:x}f", v.to_bits()).unwrap(),
        Tag::Double(v) => write!(out, "{:x}d", v.to_bits()).unwrap(),
        Tag::String(s) => write!(out, "\"{s}\"").unwrap(),
        Tag::ByteArray(a) => write!(out, "B[{}]", a.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ")).unwrap(),
        Tag::IntArray(a) => write!(out, "I[{}]", a.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ")).unwrap(),
        Tag::LongArray(a) => write!(out, "L[{}]", a.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ")).unwrap(),
        Tag::End => panic!("end tag"),
    }
}

fn canon_text(tag: &Tag) -> String {
    let mut s = String::new();
    canon(tag, &mut s);
    s
}

fn sha1(s: &str) -> String {
    Sha1::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn offers_canon(offers: &MerchantOffers) -> String {
    let tag = mc_dfu::encode_nbt(&MerchantOffers::codec(), offers).into_result().expect("offers encode");
    canon_text(&tag)
}

fn wire_hex(offers: &MerchantOffers) -> String {
    let mut out = Vec::new();
    offers.encode_stream(&mut out).unwrap();
    out.iter().map(|b| format!("{b:02x}")).collect()
}

fn seeded(level: &ServerLevel, merchant: &TestMerchant, key: &Identifier, offers: &mut MerchantOffers, seed: i64) {
    let set = get_trade_set(&level.registries, key).expect("trade set");
    let params = LootParams::builder(level)
        .origin(merchant.position())
        .this_entity(merchant)
        .additional_cost_component_allowed()
        .create(&LootContextParamSets::VILLAGER_TRADE);
    let mut context = LootContext::builder(&params).with_optional_random_seed(seed).create(None);
    add_offers_from_trade_set_in(&mut context, offers, &set);
}

fn real(level: &ServerLevel, merchant: &TestMerchant, keys: &[Identifier]) -> MerchantOffers {
    let mut offers = MerchantOffers::new();
    for key in keys {
        add_offers_from_trade_set(level, merchant, &mut offers, key);
    }
    offers
}

fn wandering_keys() -> Vec<Identifier> {
    [WANDERING_TRADER_BUYING, WANDERING_TRADER_UNCOMMON, WANDERING_TRADER_COMMON].iter().map(|p| trade_set_key(p)).collect()
}

fn report(failures: Vec<String>, total: usize) {
    assert!(failures.is_empty(), "{} of {total} differ:\n{}", failures.len(), failures.iter().take(12).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn trades_equal_vanilla() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut level = level_with_registries(dir.path());
    let fx = Fixture::load();
    let mut failures = Vec::new();

    // data-pack registries
    let digests: HashMap<(String, String), String> = fx.of('E').iter().map(|r| ((r[0].to_owned(), r[1].to_owned()), r[2].to_owned())).collect();
    let mut checked = 0;
    for (registry, name) in [(keys::VILLAGER_TRADE, "villager_trade"), (keys::TRADE_SET, "trade_set")] {
        for entry in level.registries.registry(registry).expect("registry").entries() {
            let tag = if name == "trade_set" {
                let set = level.registries.get::<Arc<TradeSet>>(registry, &entry.id).expect("typed trade set");
                mc_dfu::encode_nbt(&TradeSet::codec(), set.as_ref()).into_result().unwrap()
            } else {
                let trade = level.registries.get::<Arc<VillagerTrade>>(registry, &entry.id).expect("typed trade");
                mc_dfu::encode_nbt(&VillagerTrade::codec(), trade.as_ref()).into_result().unwrap()
            };
            let got = sha1(&canon_text(&tag));
            match digests.get(&(name.to_owned(), entry.id.to_string())) {
                Some(expected) if *expected == got => {}
                other => failures.push(format!("registry {name} {}: vanilla {other:?} ours {got} {}", entry.id, canon_text(&tag))),
            }
            checked += 1;
        }
    }
    assert_eq!(checked, digests.len(), "entry counts differ");
    report(std::mem::take(&mut failures), checked);

    // seeded generation
    let cases = fx.of('S');
    let dumps: HashMap<&str, &str> = fx.of('D').into_iter().map(|r| (r[0], r[1])).collect();
    for r in &cases {
        let (prof, lvl, ty, seed, expected) = (r[0], r[1].parse::<i32>().unwrap(), r[2], r[3].parse::<i64>().unwrap(), r[4]);
        let key = profession_trade_set(&Identifier::parse(prof).unwrap(), lvl).unwrap();
        let mut offers = MerchantOffers::new();
        seeded(&level, &TestMerchant::villager(ty), &key, &mut offers, seed);
        let text = offers_canon(&offers);
        if sha1(&text) != expected {
            let vanilla = dumps.get(format!("{prof}/{lvl}/{ty}").as_str()).map_or("-", |d| *d);
            failures.push(format!("S {prof} {lvl} {ty} {seed}\n  vanilla {vanilla}\n  ours    {text}"));
        }
    }
    report(std::mem::take(&mut failures), cases.len());
    for r in fx.of('W') {
        let seed = r[0].parse::<i64>().unwrap();
        let merchant = TestMerchant::trader();
        let mut offers = MerchantOffers::new();
        for key in wandering_keys() {
            seeded(&level, &merchant, &key, &mut offers, seed);
        }
        let text = offers_canon(&offers);
        if sha1(&text) != r[1] {
            let vanilla = dumps.get(format!("wt/{seed}").as_str()).map_or("-", |d| *d);
            failures.push(format!("W {seed}\n  vanilla {vanilla}\n  ours    {text}"));
        }
    }
    report(std::mem::take(&mut failures), 4);

    // the real path: level random sequences, in the oracle's order
    reset_sequences(&mut level);
    let rows = fx.of('R');
    let mutated: HashMap<(String, String, String), Vec<&str>> = fx.of('M').into_iter().map(|r| ((r[0].to_owned(), r[1].to_owned(), r[2].to_owned()), r)).collect();
    let mut first = None;
    for r in &rows {
        let (prof, lvl, ty) = (r[0], r[1].parse::<i32>().unwrap(), r[2]);
        let key = profession_trade_set(&Identifier::parse(prof).unwrap(), lvl).unwrap();
        let offers = real(&level, &TestMerchant::villager(ty), &[key]);
        let (text, wire) = (offers_canon(&offers), wire_hex(&offers));
        if sha1(&text) != r[3] || !wire_equal(&wire, r[4]) {
            failures.push(format!("R {prof} {lvl} {ty}: nbt equal {} wire equal {}\n  ours {text}", sha1(&text) == r[3], wire_equal(&wire, r[4])));
            continue;
        }
        // saved + wire round trips
        let tag = mc_dfu::encode_nbt(&MerchantOffers::codec(), &offers).into_result().unwrap();
        let back = mc_dfu::parse_nbt(&MerchantOffers::codec(), &tag).into_result().expect("offers decode");
        let bytes = hex(&wire);
        let decoded = MerchantOffers::decode_stream(&mut &bytes[..]).expect("wire decode");
        if back != offers || decoded != offers {
            failures.push(format!("R {prof} {lvl} {ty}: round trip differs (saved {}, wire {})", back == offers, decoded == offers));
        }
        if first.is_none() && !offers.is_empty() {
            first = Some(offers.clone());
        }
        if let Some(m) = mutated.get(&(prof.to_owned(), lvl.to_string(), ty.to_owned())) {
            let mut offers = offers.clone();
            let mut info = String::new();
            for (i, o) in offers.iter_mut().enumerate() {
                for _ in 0..i % 4 {
                    o.increase_uses();
                }
                if i % 3 == 0 {
                    o.set_to_out_of_stock();
                }
                if i % 2 == 1 {
                    o.update_demand();
                }
                o.add_to_special_price_diff(if i % 2 == 0 { -3 } else { 5 });
                let (a, b) = (o.get_cost_a(), o.get_cost_b());
                let ok = o.satisfied_by(&a.copy(), &b.copy());
                let short_ok = o.satisfied_by(&a.copy_with_count((a.count() - 1).max(0)), &b.copy());
                let mut ta = a.copy_with_count(a.count() + 2);
                let mut tb = b.copy_with_count(if b.is_empty() { 0 } else { b.count() + 1 });
                let took = o.take(&mut ta, &mut tb);
                info.push_str(&format!("{}/{}/{ok}/{short_ok}/{took}/{}/{}/{}/{};", a.count(), b.count(), ta.count(), tb.count(), o.is_out_of_stock(), o.get_demand()));
            }
            let (text, wire) = (offers_canon(&offers), wire_hex(&offers));
            if sha1(&text) != m[3] || !wire_equal(&wire, m[4]) || info != m[5] {
                failures.push(format!("M {prof} {lvl} {ty}: nbt {} wire {}\n  vanilla {}\n  ours    {info}", sha1(&text) == m[3], wire_equal(&wire, m[4]), m[5]));
            }
        }
    }
    report(std::mem::take(&mut failures), rows.len());
    for r in fx.of('T') {
        let offers = real(&level, &TestMerchant::trader(), &wandering_keys());
        if sha1(&offers_canon(&offers)) != r[2] || !wire_equal(&wire_hex(&offers), r[3]) {
            failures.push(format!("T {}\n  ours {}", r[1], offers_canon(&offers)));
        }
    }
    report(std::mem::take(&mut failures), 6);

    // the offers packet
    let packet = ClientboundMerchantOffersPacket { container_id: 7, offers: first.expect("an offer").to_wire(), villager_level: 3, villager_xp: 150, show_progress: true, can_restock: false };
    let mut out = Vec::new();
    packet.encode(&mut out).unwrap();
    let got: String = out.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(got, fx.of('K')[0][0], "offers packet");
}

/// Wire equality. Vanilla writes an `ItemEnchantments` in the iteration order of an `Object2IntOpenHashMap` keyed by holder identity
/// hashes (JVM-run dependent), so items with several enchantments cannot be compared byte for byte: the bytes must be identical
/// except for that order, i.e. both decode to the same offers and have the same length.
fn wire_equal(ours: &str, vanilla: &str) -> bool {
    if ours == vanilla {
        return true;
    }
    let (a, b) = (hex(ours), hex(vanilla));
    a.len() == b.len() && MerchantOffers::decode_stream(&mut &a[..]).ok() == MerchantOffers::decode_stream(&mut &b[..]).ok()
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}
