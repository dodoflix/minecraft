//! `EnchantmentHelper` vs the real vanilla classes (fixture `tests/fixtures/enchant_golden.txt`, generator
//! `tests/oracle/EnchantOracle.java`: `scripts/entity-oracle.sh crates/mc-game/tests/oracle/EnchantOracle.java > tests/fixtures/enchant_golden.txt`).
//!
//! Sections (first token of a line): `AV` getAvailableEnchantmentResults, `COST` getEnchantmentCost, `ENCH` enchantItem (+ the RNG state
//! after it), `PROT` getDamageProtection / isImmuneToDamage per armor enchantment x level x damage source, `DMG` modifyDamage /
//! modifyKnockback / modifyArmorEffectiveness / modifyFallBasedDamage per weapon enchantment x level x weapon x victim type x fall
//! distance, `DUR` processDurabilityChange sequences with seeded level random, `XP` repair-from-xp / block xp / ammo use / piercing,
//! `ATTR` forEachModifier per slot, `HAS` has(effect component) + hasTag. Float results are compared bit exactly.

use crate::common;

use std::collections::BTreeMap;

use mc_core::ident::Identifier;
use mc_core::random::RandomSource;
use mc_data::Item;
use mc_data::enums::GameType;
use mc_game::registry::RegistryAccess;
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::equipment::EquipmentSlot;
use mc_game::world::entity::plain::PlainEntity;
use mc_game::world::entity::{Entity, ServerPlayer};
use mc_game::world::item::ItemStack;
use mc_game::world::item::enchantment::actors::Actors;
use mc_game::world::item::enchantment::enchantment_effect_components::EffectComponent;
use mc_game::world::item::enchantment::enchantment_tags;
use mc_game::world::item::enchantment_helper as eh;
use mc_protocol::codec::GameProfile;
use mc_protocol::components::{ENCHANTMENTS, Holder, ItemEnchantments};

fn id(path: &str) -> Identifier {
    Identifier::parse(&format!("minecraft:{path}")).unwrap()
}

fn stack(item: &str, enchant: Option<(&str, i32)>) -> ItemStack {
    let mut s = ItemStack::new(Item::by_name(item).unwrap_or_else(|| panic!("item {item}")));
    if let Some((e, l)) = enchant {
        s.set(
            ENCHANTMENTS,
            ItemEnchantments::new(vec![(Holder::Reference(id(e)), l)]),
        );
    }
    s
}

fn bits(f: f32) -> String {
    (f.to_bits() as i32).to_string()
}

fn player(name: &str, n: i32, _level: &ServerLevel) -> ServerPlayer {
    let profile = GameProfile {
        id: uuid::Uuid::from_u128(n as u128),
        name: name.into(),
        properties: vec![],
    };
    let mut p = ServerPlayer::new(
        n,
        Identifier::parse("minecraft:overworld").unwrap(),
        profile,
        Default::default(),
    );
    p.set_game_mode_for_player(GameType::Survival, None);
    p
}

fn render_instances(
    list: &[mc_game::world::item::enchantment::enchantment_instance::EnchantmentInstance],
) -> String {
    list.iter()
        .map(|e| format!("{}:{}", e.enchantment.path(), e.level))
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn enchantment_helper_equals_vanilla() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_config, stem) = common::load_world(
        dir.path(),
        mc_world::level_storage::WorldDataConfiguration::DEFAULT,
        true,
    )
    .expect("default world loads");
    let registries: RegistryAccess = stem.registries.composite_access().clone();
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = registries.clone();
    let all: Vec<Identifier> = registries
        .registry("minecraft:enchantment")
        .unwrap()
        .names();

    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/enchant_golden.txt"),
    )
    .unwrap();
    let mut victim = player("Victim", 1, &level);
    let mut attacker = player("Attacker", 2, &level);
    let mut mismatches: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut check = |kind: &'static str, line: &str, actual: String, expected: &str| {
        *counts.entry(kind).or_default() += 1;
        if actual != expected {
            let v = mismatches.entry(kind).or_default();
            if v.len() < 6 {
                v.push(format!(
                    "{line}\n    ours:    {actual}\n    vanilla: {expected}"
                ));
            }
        }
    };
    let armor_slots = [
        (EquipmentSlot::Head, "iron_helmet"),
        (EquipmentSlot::Chest, "iron_chestplate"),
        (EquipmentSlot::Legs, "iron_leggings"),
        (EquipmentSlot::Feet, "iron_boots"),
    ];

    for line in text.lines() {
        let t: Vec<&str> = line.split(' ').collect();
        match t[0] {
            "AV" => {
                let item = stack(t[1], None);
                let r = eh::get_available_enchantment_results(
                    t[2].parse().unwrap(),
                    &item,
                    &registries,
                    &all,
                );
                let expected = line.splitn(4, ' ').nth(3).unwrap_or("");
                check("AV", &t[..3].join(" "), render_instances(&r), expected);
            }
            "COST" => {
                let item = stack(t[1], None);
                let seed: i64 = t[2].parse().unwrap();
                let books: i32 = t[3].parse().unwrap();
                let costs: Vec<String> = (0..3)
                    .map(|slot| {
                        eh::get_enchantment_cost(
                            &mut mc_core::random::create(seed + slot as i64),
                            slot,
                            books,
                            &item,
                        )
                        .to_string()
                    })
                    .collect();
                check(
                    "COST",
                    &t[..4].join(" "),
                    costs.join(" "),
                    &t[4..].join(" "),
                );
            }
            "ENCH" => {
                let item = stack(t[1], None);
                let mut random = mc_core::random::create(t[2].parse().unwrap());
                let res =
                    eh::enchant_item(&mut random, item, t[3].parse().unwrap(), &registries, None);
                let mut parts: Vec<String> = eh::get_enchantments_for_crafting(&res)
                    .entries()
                    .iter()
                    .map(|(h, l)| format!("{}:{}", h.name().unwrap().path(), l))
                    .collect();
                parts.sort();
                let actual = format!(
                    "{} {} next={}",
                    res.item().name().trim_start_matches("minecraft:"),
                    parts.join(","),
                    random.next_int()
                );
                check("ENCH", &t[..4].join(" "), actual, &t[4..].join(" "));
            }
            "PROT" => {
                let (name, lvl) = (t[1], t[2].parse::<i32>().unwrap());
                for (slot, item) in armor_slots {
                    victim
                        .player
                        .inventory
                        .equipment_set(slot, stack(item, Some((name, lvl))));
                }
                let src = source(&level, t[3]);
                let vid = victim.id();
                let p =
                    eh::get_damage_protection(&mut level, &mut Actors::of(&mut victim), vid, &src);
                let imm =
                    eh::is_immune_to_damage(&mut level, &mut Actors::of(&mut victim), vid, &src);
                check(
                    "PROT",
                    &t[..4].join(" "),
                    format!("{} imm={imm}", bits(p)),
                    &t[4..].join(" "),
                );
            }
            "DMG" => {
                let (name, lvl, weapon_name, victim_type, fall) = (
                    t[1],
                    t[2].parse::<i32>().unwrap(),
                    t[3],
                    t[4],
                    t[5].parse::<f64>().unwrap(),
                );
                let weapon = stack(weapon_name, Some((name, lvl)));
                attacker
                    .player
                    .inventory
                    .equipment_set(EquipmentSlot::MainHand, weapon.clone());
                attacker.base_mut().fall_distance = fall;
                let mut target =
                    PlainEntity::new(mc_data::EntityType::by_name(victim_type).unwrap(), 50);
                target.snap_to(5.5, -60.0, 0.5, 0.0, 0.0);
                let src = level.damage_sources().player_attack(&attacker);
                let tid = target.id();
                let mut actors = Actors::of(&mut attacker).and(&mut target);
                let actual = [
                    bits(eh::modify_damage(
                        &mut level,
                        &mut actors,
                        &weapon,
                        tid,
                        &src,
                        10.0,
                    )),
                    bits(eh::modify_knockback(
                        &mut level,
                        &mut actors,
                        &weapon,
                        tid,
                        &src,
                        0.4,
                    )),
                    bits(eh::modify_armor_effectiveness(
                        &mut level,
                        &mut actors,
                        &weapon,
                        tid,
                        &src,
                        1.0,
                    )),
                    bits(eh::modify_fall_based_damage(
                        &mut level,
                        &mut actors,
                        &weapon,
                        tid,
                        &src,
                        1.0,
                    )),
                ]
                .join(" ");
                check("DMG", &t[..6].join(" "), actual, &t[6..].join(" "));
            }
            "DUR" => {
                let (name, lvl, item, seed) = (
                    t[1],
                    t[2].parse::<i32>().unwrap(),
                    t[3],
                    t[4].parse::<i64>().unwrap(),
                );
                let s = stack(item, Some((name, lvl)));
                level.tick_state.random.set_seed(seed);
                let mut seq = String::new();
                for i in 0..40 {
                    seq.push_str(&format!(
                        "{},",
                        eh::process_durability_change(
                            &mut level,
                            &s,
                            if i % 5 == 4 { 3 } else { 1 }
                        )
                    ));
                }
                let actual = format!("{seq} next={}", level.tick_state.random.next_int());
                check("DUR", &t[..5].join(" "), actual, &t[5..].join(" "));
            }
            "XP" => {
                let (name, lvl) = (t[1], t[2].parse::<i32>().unwrap());
                let s = stack("diamond_pickaxe", Some((name, lvl)));
                let arrow = ItemStack::new(Item::ARROW);
                let mut parts = Vec::new();
                for amount in [1, 2, 5, 17, 100] {
                    level.tick_state.random.set_seed(7);
                    parts.push(format!(
                        "{}/{}/{}/{}",
                        eh::modify_durability_to_repair_from_xp(&mut level, &s, amount),
                        eh::process_block_experience(&mut level, &s, amount),
                        eh::process_ammo_use(&mut level, &s, &arrow, amount),
                        eh::get_piercing_count(&mut level, &s, &arrow)
                    ));
                }
                check("XP", &t[..3].join(" "), parts.join(" "), &t[3..].join(" "));
            }
            "ATTR" => {
                let (name, lvl, slot) = (
                    t[1],
                    t[2].parse::<i32>().unwrap(),
                    EquipmentSlot::by_name(t[3]).unwrap(),
                );
                let s = stack("iron_boots", Some((name, lvl)));
                let mut parts = Vec::new();
                eh::for_each_modifier(&registries, &s, slot, |attr, m| {
                    let op = match m.operation {
                        mc_protocol::packets::play::AttributeModifierOperation::AddValue => "add_value",
                        mc_protocol::packets::play::AttributeModifierOperation::AddMultipliedBase => "add_multiplied_base",
                        mc_protocol::packets::play::AttributeModifierOperation::AddMultipliedTotal => "add_multiplied_total",
                    };
                    parts.push(format!(
                        "{}|{}|{}|{op}",
                        attr.name(),
                        m.id,
                        m.amount.to_bits() as i64
                    ));
                });
                check(
                    "ATTR",
                    &t[..4].join(" "),
                    parts.join(";"),
                    &t[4..].join(" "),
                );
            }
            "HAS" => {
                let s = stack("diamond_sword", Some((t[1], 1)));
                let mut comps: Vec<&str> = EffectComponent::ALL
                    .iter()
                    .filter(|c| eh::has(&registries, &s, **c))
                    .map(|c| c.name().trim_start_matches("minecraft:"))
                    .collect();
                comps.sort();
                let mut tags: Vec<&str> = enchantment_tags::ALL
                    .iter()
                    .filter(|tag| eh::has_tag(&registries, &s, &Identifier::parse(tag).unwrap()))
                    .map(|tag| tag.trim_start_matches("minecraft:"))
                    .collect();
                tags.sort();
                check(
                    "HAS",
                    &format!("HAS {}", t[1]),
                    format!("{} | {}", comps.join(","), tags.join(",")),
                    line.splitn(3, ' ').nth(2).unwrap_or(""),
                );
            }
            "" => {}
            other => panic!("unknown section {other}"),
        }
    }
    eprintln!("compared: {counts:?}");
    if !mismatches.is_empty() {
        for (kind, list) in &mismatches {
            eprintln!("== {kind}: first mismatches");
            for m in list {
                eprintln!("  {m}");
            }
        }
        panic!(
            "enchantment goldens differ in: {:?}",
            mismatches.keys().collect::<Vec<_>>()
        );
    }
}

fn source(level: &ServerLevel, name: &str) -> mc_game::world::damagesource::DamageSource {
    let s = level.damage_sources();
    match name {
        "generic" => s.generic(),
        "in_fire" => s.in_fire(),
        "on_fire" => s.on_fire(),
        "lava" => s.lava(),
        "hot_floor" => s.hot_floor(),
        "cactus" => s.cactus(),
        "fall" => s.fall(),
        "drown" => s.drown(),
        "starve" => s.starve(),
        "freeze" => s.freeze(),
        "magic" => s.magic(),
        "lightning_bolt" => s.lightning_bolt(),
        "fell_out_of_world" => s.fell_out_of_world(),
        "wither" => s.wither(),
        "dragon_breath" => s.dragon_breath(),
        "sweet_berry_bush" => s.sweet_berry_bush(),
        "stalagmite" => s.stalagmite(),
        "explosion" => s.explosion(None, None),
        other => panic!("source {other}"),
    }
}
