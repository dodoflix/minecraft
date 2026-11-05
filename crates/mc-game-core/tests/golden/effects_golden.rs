//! Effect / attribute data classes against the real vanilla classes (`tests/fixtures/EffectsGolden.java`, regenerate
//! with `scripts/golden-effects.sh`): tick intervals, attribute templates, particles, potion colours,
//! `MobEffectInstance` update/tick/scale/compare/NBT scripts and `AttributeInstance` scripts (value bits after every
//! operation, modifier order, per-operation hash order).

use mc_core::ident::Identifier;
use mc_data::{Attribute, EntityType, MobEffect};
use mc_game::world::effect::MobEffectExt;
use mc_game::world::effect::potion_contents;
use mc_game::world::entity::attributes::{AttributeMap, AttributeModifier, AttributeModifierOperation};
use mc_game::world::entity::effect::MobEffectInstance;
use mc_nbt::Tag;
use mc_protocol::packets::play::ParticleOptions;

const GOLDEN: &str = include_str!("../../../mc-game/tests/fixtures/effects_golden.txt");

fn effect(name: &str) -> MobEffect {
    MobEffect::by_name(name).unwrap_or_else(|| panic!("effect {name}"))
}

fn parse_instance(snbt: &str) -> MobEffectInstance {
    let tag = mc_nbt::snbt::parse_compound(snbt).unwrap();
    MobEffectInstance::from_nbt(&tag).unwrap_or_else(|| panic!("decode {snbt}"))
}

/// `MobEffectInstance.CODEC` encode as SNBT.
fn state(i: &MobEffectInstance) -> String {
    Tag::Compound(i.to_nbt()).to_string()
}

fn hex(d: f64) -> String {
    format!("{:x}", d.to_bits())
}

fn op_by_name(name: &str) -> AttributeModifierOperation {
    match name {
        "add_value" => AttributeModifierOperation::AddValue,
        "add_multiplied_base" => AttributeModifierOperation::AddMultipliedBase,
        "add_multiplied_total" => AttributeModifierOperation::AddMultipliedTotal,
        other => panic!("operation {other}"),
    }
}

fn op_name(op: AttributeModifierOperation) -> &'static str {
    match op {
        AttributeModifierOperation::AddValue => "add_value",
        AttributeModifierOperation::AddMultipliedBase => "add_multiplied_base",
        AttributeModifierOperation::AddMultipliedTotal => "add_multiplied_total",
    }
}

fn op_ordinal(op: AttributeModifierOperation) -> usize {
    match op {
        AttributeModifierOperation::AddValue => 0,
        AttributeModifierOperation::AddMultipliedBase => 1,
        AttributeModifierOperation::AddMultipliedTotal => 2,
    }
}

#[test]
fn tick_intervals_categories_colors() {
    let mut n = 0;
    for line in GOLDEN.lines().filter(|l| l.starts_with("INTERVAL\t")) {
        let f: Vec<&str> = line.split('\t').collect();
        let e = effect(f[1]);
        assert_eq!(e.is_instantaneous(), f[2] == "true", "{}", f[1]);
        assert_eq!(format!("{:?}", e.data().category).to_uppercase(), f[3], "{}", f[1]);
        assert_eq!(e.get_color().to_string(), f[4], "{}", f[1]);
        for (amp, row) in f[5].split('|').filter(|r| !r.is_empty()).enumerate() {
            let actual: String = (0..=120).map(|t| if e.should_apply_effect_tick_this_tick(t, amp as i32) { '1' } else { '0' }).collect();
            assert_eq!(actual, row, "{} amplifier {amp}", f[1]);
        }
        n += 1;
    }
    assert!(n > 35);
}

#[test]
fn attribute_templates() {
    for line in GOLDEN.lines().filter(|l| l.starts_with("TEMPLATE\t")) {
        let f: Vec<&str> = line.split('\t').collect();
        let amp: i32 = f[2].parse().unwrap();
        let actual: String = effect(f[1])
            .create_modifiers(amp)
            .iter()
            .map(|(a, m)| format!("{},{},{},{};", a.identifier(), m.id, hex(m.amount), op_name(m.operation)))
            .collect();
        assert_eq!(actual, f[3], "{} amplifier {amp}", f[1]);
    }
}

#[test]
fn particles() {
    for line in GOLDEN.lines().filter(|l| l.starts_with("PARTICLE\t")) {
        let f: Vec<&str> = line.split('\t').collect();
        let instance = MobEffectInstance::new(effect(f[1]), 100, 0, f[2] == "true", true, true, None);
        let actual = match instance.effect.create_particle_options(&instance) {
            ParticleOptions::Simple(t) => format!("{{type:\"{}\"}}", t.identifier()),
            ParticleOptions::Color { particle_type, color } => format!("{{color:{color},type:\"{}\"}}", particle_type.identifier()),
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(actual, f[3], "{} ambient={}", f[1], f[2]);
    }
}

#[test]
fn potion_contents() {
    for line in GOLDEN.lines().filter(|l| l.starts_with("POTION\t")) {
        let f: Vec<&str> = line.split('\t').collect();
        let potion = mc_data::Potion::by_name(f[1]).unwrap();
        let contents = mc_protocol::components::PotionContents { potion: Some(potion), custom_color: None, custom_effects: vec![], custom_name: None };
        assert_eq!(potion_contents::get_color(&contents).to_string(), f[2], "{}", f[1]);
        assert_eq!(potion_contents::has_effects(&contents).to_string(), f[3], "{}", f[1]);
        let effects: String = potion_contents::get_all_effects(&contents).iter().map(|e| format!("{};", state(e))).collect();
        assert_eq!(effects, f.get(4).copied().unwrap_or(""), "{}", f[1]);
    }
    for line in GOLDEN.lines().filter(|l| l.starts_with("POTIONCUSTOM\t")) {
        let f: Vec<&str> = line.split('\t').collect();
        let instances: Vec<MobEffectInstance> = f[3].split(';').filter(|s| !s.is_empty()).map(parse_instance).collect();
        let custom_effects = instances
            .iter()
            .map(|i| mc_protocol::components::MobEffectInstance {
                effect: i.effect,
                duration: i.duration,
                amplifier: i.amplifier,
                ambient: i.ambient,
                visible: i.visible,
                show_icon: i.show_icon,
                hidden_effect: None,
            })
            .collect();
        let contents = mc_protocol::components::PotionContents { potion: None, custom_color: None, custom_effects, custom_name: None };
        assert_eq!(potion_contents::get_color(&contents).to_string(), f[1], "{line}");
        assert_eq!(potion_contents::get_color_or(&contents, 7).to_string(), f[2], "{line}");
    }
}

#[test]
fn mob_effect_instance_scripts() {
    let mut cur: Option<MobEffectInstance> = None;
    let mut scripts = 0;
    for line in GOLDEN.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "MEI" => {
                let i = parse_instance(f[3]);
                assert_eq!(state(&i), f[3], "codec round trip");
                cur = Some(i);
                scripts += 1;
            }
            "UPDATE" => {
                let c = cur.as_mut().unwrap();
                let changed = c.update(&parse_instance(f[1]));
                assert_eq!(changed.to_string(), f[2], "{line}");
                assert_eq!(state(c), f[3], "{line}");
            }
            "TICK" => {
                let c = cur.as_mut().unwrap();
                for _ in 0..f[1].parse::<i32>().unwrap() {
                    // `tickClient`
                    if c.has_remaining_duration() {
                        c.tick_down_duration();
                        c.downgrade_to_hidden_effect();
                    }
                }
                assert_eq!(state(c), f[2], "{line}");
            }
            "SCALE" => {
                let c = cur.as_ref().unwrap();
                let scale = f32::from_bits(f[1].parse::<i32>().unwrap() as u32);
                assert_eq!(state(&c.with_scaled_duration(scale)), f[2], "{line}");
            }
            "MISC" => {
                let c = cur.as_ref().unwrap();
                let other = parse_instance(f[1]);
                assert_eq!(c.compare_to(&other) as i32, f[2].parse::<i32>().unwrap(), "{line}");
                assert_eq!((c == &other).to_string(), f[3], "{line}");
                assert_eq!(c.ends_within(f[4].parse().unwrap()).to_string(), f[5], "{line}");
                assert_eq!(c.is_infinite_duration().to_string(), f[6], "{line}");
            }
            _ => {}
        }
    }
    assert_eq!(scripts, 400);
}

#[test]
fn attribute_instance_scripts() {
    let mut map = AttributeMap::new(EntityType::PLAYER);
    let mut attribute = Attribute::LUCK;
    let mut scripts = 0;
    for line in GOLDEN.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "ATTR" => {
                map = AttributeMap::new(EntityType::PLAYER);
                attribute = Attribute::by_name(f[2]).unwrap();
                let inst = map.get_instance(attribute).unwrap();
                assert_eq!(hex(inst.get_base_value()), f[4]);
                assert_eq!(hex(inst.get_value()), f[5]);
                scripts += 1;
            }
            "OP" => {
                // OP <desc...> = <value> <modifiers> <permanent>
                let eq = f.iter().position(|t| *t == "=").unwrap();
                let desc = &f[1..eq];
                let inst = map.get_instance(attribute).unwrap();
                let modifier = |d: &[&str]| AttributeModifier {
                    id: Identifier::parse(d[1]).unwrap(),
                    amount: f64::from_bits(u64::from_str_radix(d[2], 16).unwrap()),
                    operation: op_by_name(d[3]),
                };
                match desc[0] {
                    "add_transient" => inst.add_transient_modifier(modifier(desc)),
                    "add_permanent" => inst.add_permanent_modifier(modifier(desc)),
                    "replace_permanent" => inst.add_or_replace_permanent_modifier(modifier(desc)),
                    "remove" => {
                        inst.remove_modifier(&Identifier::parse(desc[1]).unwrap());
                    }
                    "base" => inst.set_base_value(f64::from_bits(u64::from_str_radix(desc[1], 16).unwrap())),
                    "remove_all" => inst.remove_modifiers(),
                    "dup" => assert!(inst.has_modifier(&Identifier::parse(desc[1]).unwrap()), "vanilla threw for a present id"),
                    other => panic!("op {other}"),
                }
                assert_eq!(hex(inst.get_value()), f[eq + 1], "{line}");
                let mods: String = inst.get_modifiers().map(|m| format!("{}:{}:{},", m.id, hex(m.amount), op_ordinal(m.operation))).collect();
                assert_eq!(mods, f.get(eq + 2).copied().unwrap_or(""), "{line}");
                let perm: String = inst.get_permanent_modifiers().map(|m| format!("{},", m.id)).collect();
                assert_eq!(perm, f.get(eq + 3).copied().unwrap_or(""), "{line}");
                let order: String = [AttributeModifierOperation::AddValue, AttributeModifierOperation::AddMultipliedBase, AttributeModifierOperation::AddMultipliedTotal]
                    .into_iter()
                    .map(|op| inst.get_modifiers_for_operation(op).map(|m| format!("{},", m.id.path())).collect::<String>() + "|")
                    .collect();
                assert_eq!(order, f.get(eq + 4).copied().unwrap_or(""), "{line}");
            }
            "COPY" => {
                let mut other = AttributeMap::new(EntityType::PLAYER);
                other.assign_all_values(&map);
                let copy = other.get_instance(attribute).unwrap();
                assert_eq!(hex(copy.get_value()), f[1], "{line}");
                assert_eq!(hex(copy.get_base_value()), f[2], "{line}");
                let ids: String = copy.get_modifiers().map(|m| format!("{},", m.id)).collect();
                assert_eq!(ids, f.get(3).copied().unwrap_or(""), "{line}");
            }
            _ => {}
        }
    }
    assert_eq!(scripts, 120);
}
