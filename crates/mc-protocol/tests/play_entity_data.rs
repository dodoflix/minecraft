//! Golden tests for the entity_data play group against vanilla's real `GameProtocols` codecs
//! (generator: tests/fixtures/play/GenEntityData.java).
mod common;
use common::*;

use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::phys::vec3::Vec3;
use mc_core::pos::block_pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::Pose;
use mc_data::{DataComponentType, EntityType, Item, ParticleType, SoundEvent};
use mc_protocol::components::{self as comp, DataComponentPatch, ItemStack, ItemStackTemplate};
use mc_protocol::codec::{read_varint, to_bytes, Angle, Either, GameProfile, GlobalPos, Holder, LpVec3, Property};
use mc_protocol::packets::common::HumanoidArm;
use mc_protocol::packets::play::*;
use mc_protocol::{Decode, Encode, Packet};
use mc_text::{ChatFormatting, Component, TranslatableArg};
use uuid::Uuid;

fn g() -> Golden {
    Golden::load("entitydata")
}

fn count(g: &Golden, label: &str) -> u32 {
    read_varint(&mut g.bytes(&format!("count_{label}"))).unwrap() as u32
}

fn uuid(msb: i64, lsb: i64) -> Uuid {
    Uuid::from_u64_pair(msb as u64, lsb as u64)
}

// ---------------------------------------------------------------- add_entity

#[allow(clippy::too_many_arguments)]
fn add_entity(
    g: &Golden,
    label: &str,
    id: i32,
    uuid: Uuid,
    pos: [f64; 3],
    x_rot: f32,
    y_rot: f32,
    r#type: EntityType,
    data: i32,
    movement: Vec3,
    head: f64,
) {
    let p = ClientboundAddEntityPacket {
        id,
        uuid,
        r#type,
        x: pos[0],
        y: pos[1],
        z: pos[2],
        movement: LpVec3(movement),
        x_rot: Angle::pack(x_rot),
        y_rot: Angle::pack(y_rot),
        y_head_rot: Angle::pack(head as f32),
        data,
    };
    let label = format!("add_entity_{label}");
    let expected = g.bytes(&label);
    let mut ours = Vec::new();
    mc_protocol::codec::write_varint(&mut ours, ClientboundAddEntityPacket::ID);
    p.encode(&mut ours);
    assert_eq!(hex(&ours), hex(expected), "encode {label}");
    // the low-precision movement does not survive a round trip: compare against its own quantisation
    let mut input = &expected[1..];
    let decoded = ClientboundAddEntityPacket::decode(&mut input).unwrap();
    assert!(input.is_empty());
    let quantised: LpVec3 = mc_protocol::codec::from_bytes(&to_bytes(&p.movement)).unwrap();
    assert_eq!(decoded, ClientboundAddEntityPacket { movement: quantised, ..p }, "decode {label}");
}

fn add_entity_packets(g: &Golden) {
    let u = uuid(0x0123456789abcdef, 0xfedcba9876543210u64 as i64);
    add_entity(g, "zero", 0, uuid(0, 0), [0.0; 3], 0.0, 0.0, EntityType::PIG, 0, Vec3::ZERO, 0.0);
    add_entity(g, "basic", 1, u, [1.5, 64.0, -2.25], 90.0, 180.0, EntityType::ZOMBIE, 7, Vec3::new(0.1, -0.2, 0.3), 45.0);
    add_entity(g, "neg_angles", 300, uuid(-1, -1), [-1e7, -64.0, 1e7], -45.0, -90.0, EntityType::PLAYER, -1, Vec3::new(-0.5, 0.0, 0.5), -170.5);
    add_entity(g, "wrap_angles", 70000, uuid(1, 2), [0.1, 0.2, 0.3], 359.9, 720.0, EntityType::ARROW, 123456, Vec3::new(1.0, 1.0, 1.0), 360.0);
    add_entity(g, "fast", i32::MAX, uuid(3, 4), [f64::MAX, f64::from_bits(1), -0.0], 0.0, 0.0, EntityType::TNT, i32::MIN, Vec3::new(10.0, -20.0, 30.0), 1.0);
    add_entity(g, "huge_movement", 5, uuid(5, 6), [0.0; 3], 0.0, 0.0, EntityType::ITEM, 0, Vec3::new(1.7e10, -1.7e10, 5e9), 0.0);
    add_entity(g, "tiny_movement", 5, uuid(5, 6), [0.0; 3], 0.0, 0.0, EntityType::ITEM, 0, Vec3::new(3e-5, -3e-5, 3e-5), 0.0);
    add_entity(g, "scale4", 6, uuid(7, 8), [0.0; 3], 0.0, 0.0, EntityType::ITEM, 0, Vec3::new(4.5, 0.0, -4.0), 0.0);
    add_entity(g, "scale3", 6, uuid(7, 8), [0.0; 3], 0.0, 0.0, EntityType::ITEM, 0, Vec3::new(3.0, 2.9, -2.0), 0.0);
    let last = EntityType::from_id(count(g, "entity_types") - 1).unwrap();
    add_entity(g, "last_type", 8, uuid(9, 9), [0.0; 3], 0.0, 0.0, last, 0, Vec3::ZERO, 0.0);
}

// ---------------------------------------------------------------- set_entity_data

fn sed(g: &Golden, label: &str, value: EntityDataValue) {
    g.check(&format!("sed_{label}"), &ClientboundSetEntityDataPacket { id: 7, packed_items: vec![DataValue { id: 5, value }] });
}

fn state(id: u32) -> BlockState {
    BlockState::from_id(id).unwrap()
}

fn last_state(g: &Golden) -> u32 {
    count(g, "block_states") - 1
}

fn block_pos(x: i32, y: i32, z: i32) -> BlockPos {
    BlockPos::new(x, y, z)
}

fn styled() -> Component {
    Component::literal("styled").with_formats(&[ChatFormatting::Red, ChatFormatting::Bold]).append(Component::literal(" tail"))
}

fn set_entity_data_scalars(g: &Golden) {
    g.check("sed_empty", &ClientboundSetEntityDataPacket { id: 0, packed_items: vec![] });
    g.check("sed_big_entity_id", &ClientboundSetEntityDataPacket { id: i32::MAX, packed_items: vec![] });
    g.check("sed_neg_entity_id", &ClientboundSetEntityDataPacket { id: -1, packed_items: vec![] });
    g.check(
        "sed_multi",
        &ClientboundSetEntityDataPacket {
            id: 300,
            packed_items: vec![
                DataValue { id: 0, value: EntityDataValue::Byte(0x20) },
                DataValue { id: 8, value: EntityDataValue::Float(20.0) },
                DataValue { id: 254, value: EntityDataValue::Boolean(true) },
                DataValue { id: 9, value: EntityDataValue::Pose(Pose::Crouching) },
                DataValue { id: 2, value: EntityDataValue::OptionalComponent(Some(Component::literal("Steve"))) },
            ],
        },
    );
    for v in [0i8, 127, -128, -1] {
        sed(g, &format!("byte_{v}"), EntityDataValue::Byte(v));
    }
    for v in [0, 1, -1, 127, 128, 300, i32::MAX, i32::MIN] {
        sed(g, &format!("int_{v}"), EntityDataValue::Int(v));
    }
    for v in [0i64, 1, -1, 1 << 40, i64::MAX, i64::MIN] {
        sed(g, &format!("long_{v}"), EntityDataValue::Long(v));
    }
    for (l, v) in [("0", 0.0f32), ("neg0", -0.0), ("1_5", 1.5), ("max", f32::MAX), ("min", f32::from_bits(1)), ("inf", f32::INFINITY), ("ninf", f32::NEG_INFINITY)] {
        sed(g, &format!("float_{l}"), EntityDataValue::Float(v));
    }
    for (l, s) in [
        ("empty", String::new()),
        ("ascii", "Hello, world".to_string()),
        ("unicode", "h\u{e9}llo \u{1F600} w\u{f6}rld \u{4e16}\u{754c}".to_string()),
        ("max", "a".repeat(32767)),
        ("max_nonbmp", "\u{1F600}".repeat(16383) + "x"),
    ] {
        sed(g, &format!("string_{l}"), EntityDataValue::String(s));
    }
    let translated = Component::translatable(
        "chat.type.text",
        vec![TranslatableArg::String("name".into()), TranslatableArg::Component(Component::literal("msg").with_format(ChatFormatting::Italic))],
    );
    sed(g, "component_literal", EntityDataValue::Component(Component::literal("hi")));
    sed(g, "component_empty", EntityDataValue::Component(Component::empty()));
    sed(g, "component_styled", EntityDataValue::Component(styled()));
    sed(g, "component_translatable", EntityDataValue::Component(translated));
    sed(g, "component_keybind", EntityDataValue::Component(Component::keybind("key.jump")));
    sed(g, "optional_component_empty", EntityDataValue::OptionalComponent(None));
    sed(g, "optional_component_present", EntityDataValue::OptionalComponent(Some(styled())));
    sed(g, "boolean_true", EntityDataValue::Boolean(true));
    sed(g, "boolean_false", EntityDataValue::Boolean(false));
    sed(g, "rotations_zero", EntityDataValue::Rotations(Rotations::new(0.0, 0.0, 0.0)));
    sed(g, "rotations_mixed", EntityDataValue::Rotations(Rotations::new(10.0, -20.0, 30.5)));
    sed(g, "rotations_wrap", EntityDataValue::Rotations(Rotations::new(400.0, -721.0, 360.0)));
    sed(g, "rotations_nonfinite", EntityDataValue::Rotations(Rotations::new(f32::INFINITY, f32::NAN, f32::NEG_INFINITY)));
    sed(g, "block_pos_origin", EntityDataValue::BlockPos(block_pos(0, 0, 0)));
    sed(g, "block_pos_neg", EntityDataValue::BlockPos(block_pos(-1, -1, -1)));
    sed(g, "block_pos_max", EntityDataValue::BlockPos(block_pos(33554431, 2047, -33554432)));
    sed(g, "block_pos_min", EntityDataValue::BlockPos(block_pos(-33554432, -2048, 33554431)));
    sed(g, "optional_block_pos_empty", EntityDataValue::OptionalBlockPos(None));
    sed(g, "optional_block_pos_present", EntityDataValue::OptionalBlockPos(Some(block_pos(100, 64, -100))));
    for d in Direction::ALL {
        sed(g, &format!("direction_{}", d.get_name()), EntityDataValue::Direction(d));
    }
    sed(g, "optional_living_entity_reference_empty", EntityDataValue::OptionalLivingEntityReference(None));
    sed(
        &g,
        "optional_living_entity_reference_present",
        EntityDataValue::OptionalLivingEntityReference(Some(uuid(0x0123456789abcdef, 0xfedcba9876543210u64 as i64))),
    );
    let last = last_state(g);
    for id in [0, 1, 100, 5000, last] {
        sed(g, &format!("block_state_{id}"), EntityDataValue::BlockState(state(id)));
    }
    sed(g, "optional_block_state_empty", EntityDataValue::OptionalBlockState(None));
    for id in [1, 100, last] {
        sed(g, &format!("optional_block_state_{id}"), EntityDataValue::OptionalBlockState(Some(state(id))));
    }
}

/// A present air state shares the wire form of "empty": vanilla (and we) read it back as empty.
fn optional_block_state_air_reads_back_empty(g: &Golden) {
    let label = "sed_optional_block_state_air";
    let p = ClientboundSetEntityDataPacket { id: 7, packed_items: vec![DataValue { id: 5, value: EntityDataValue::OptionalBlockState(Some(state(0))) }] };
    let mut ours = Vec::new();
    mc_protocol::codec::write_varint(&mut ours, ClientboundSetEntityDataPacket::ID);
    p.encode(&mut ours);
    assert_eq!(hex(&ours), hex(g.bytes(label)));
    let decoded = ClientboundSetEntityDataPacket::decode(&mut &g.bytes(label)[1..]).unwrap();
    assert_eq!(decoded.packed_items[0].value, EntityDataValue::OptionalBlockState(None));
}

fn set_entity_data_structured(g: &Golden) {
    sed(g, "particle_simple", EntityDataValue::Particle(ParticleOptions::Simple(ParticleType::FLAME)));
    sed(g, "particle_block", EntityDataValue::Particle(ParticleOptions::Block { particle_type: ParticleType::BLOCK, state: BlockState::from_id(1).unwrap() }));
    sed(g, "particle_dust", EntityDataValue::Particle(ParticleOptions::Dust { color: 0xFF8000, scale: 2.5 }));
    sed(g, "particles_empty", EntityDataValue::Particles(vec![]));
    sed(
        &g,
        "particles_mixed",
        EntityDataValue::Particles(vec![
            ParticleOptions::Simple(ParticleType::HEART),
            ParticleOptions::Shriek { delay: 10 },
            ParticleOptions::Block { particle_type: ParticleType::FALLING_DUST, state: state(77) },
            ParticleOptions::SculkCharge { roll: 0.5 },
        ]),
    );
    let (n_type, n_prof) = (count(g, "villager_type"), count(g, "villager_profession"));
    let vt = |i: u32| mc_data::VillagerType::from_id(i).unwrap();
    let vp = |i: u32| mc_data::VillagerProfession::from_id(i).unwrap();
    for (t, p, level) in [(0, 0, 1), (1, 2, 5), (n_type - 1, n_prof - 1, 3), (0, 0, 0), (2, 3, -3), (1, 1, 100000), (0, 1, i32::MAX)] {
        // the generator clamps `level` to >= 1 in the constructor exactly like `VillagerData.<init>`
        sed(g, &format!("villager_data_{t}_{p}_{level}"), EntityDataValue::VillagerData(VillagerData { r#type: vt(t), profession: vp(p), level: level.max(1) }));
    }
    sed(g, "optional_unsigned_int_empty", EntityDataValue::OptionalUnsignedInt(None));
    for v in [0, 1, 5, 127, 300, i32::MAX] {
        sed(g, &format!("optional_unsigned_int_{v}"), EntityDataValue::OptionalUnsignedInt(Some(v)));
    }
    for p in Pose::ALL {
        sed(g, &format!("pose_{}", p.ordinal()), EntityDataValue::Pose(*p));
    }
    macro_rules! variants {
        ($($label:literal => $variant:ident),*) => {$(
            let n = count(g, $label) as i32;
            for id in [0, 1, n - 1] {
                sed(g, &format!(concat!($label, "_{}"), id), EntityDataValue::$variant(id));
            }
        )*};
    }
    variants!(
        "cat_variant" => CatVariant, "cat_sound_variant" => CatSoundVariant, "cow_variant" => CowVariant,
        "cow_sound_variant" => CowSoundVariant, "wolf_variant" => WolfVariant, "wolf_sound_variant" => WolfSoundVariant,
        "frog_variant" => FrogVariant, "pig_variant" => PigVariant, "pig_sound_variant" => PigSoundVariant,
        "chicken_variant" => ChickenVariant, "chicken_sound_variant" => ChickenSoundVariant,
        "zombie_nautilus_variant" => ZombieNautilusVariant
    );
    let n = count(g, "painting_variant") as i32;
    for id in [0, 1, n - 1] {
        sed(g, &format!("painting_variant_{id}"), EntityDataValue::PaintingVariant(Holder::Reference(id)));
    }
    let direct = |w, h, title, author| EntityDataValue::PaintingVariant(Holder::Direct(PaintingVariant { width: w, height: h, asset_id: Identifier::parse("mod:custom_art").unwrap(), title, author }));
    sed(g, "painting_variant_direct_a", direct(2, 3, Some(Component::literal("Title")), Some(styled())));
    sed(g, "painting_variant_direct_b", direct(-1, 300, None, None));
    sed(g, "optional_global_pos_empty", EntityDataValue::OptionalGlobalPos(None));
    sed(
        &g,
        "optional_global_pos_overworld",
        EntityDataValue::OptionalGlobalPos(Some(GlobalPos { dimension: Identifier::parse("minecraft:overworld").unwrap(), pos: block_pos(1, 2, 3) })),
    );
    sed(
        &g,
        "optional_global_pos_custom",
        EntityDataValue::OptionalGlobalPos(Some(GlobalPos { dimension: Identifier::parse("mod:the_void").unwrap(), pos: block_pos(-30000000, -64, 30000000) })),
    );
    for (i, s) in [SnifferState::Idling, SnifferState::FeelingHappy, SnifferState::Scenting, SnifferState::Sniffing, SnifferState::Searching, SnifferState::Digging, SnifferState::Rising].into_iter().enumerate() {
        sed(g, &format!("sniffer_state_{i}"), EntityDataValue::SnifferState(s));
    }
    for (i, s) in [ArmadilloState::Idle, ArmadilloState::Rolling, ArmadilloState::Scared, ArmadilloState::Unrolling].into_iter().enumerate() {
        sed(g, &format!("armadillo_state_{i}"), EntityDataValue::ArmadilloState(s));
    }
    for (i, s) in [CopperGolemState::Idle, CopperGolemState::GettingItem, CopperGolemState::GettingNoItem, CopperGolemState::DroppingItem, CopperGolemState::DroppingNoItem].into_iter().enumerate() {
        sed(g, &format!("copper_golem_state_{i}"), EntityDataValue::CopperGolemState(s));
    }
    for (i, s) in [WeatheringCopperState::Unaffected, WeatheringCopperState::Exposed, WeatheringCopperState::Weathered, WeatheringCopperState::Oxidized].into_iter().enumerate() {
        sed(g, &format!("weathering_copper_state_{i}"), EntityDataValue::WeatheringCopperState(s));
    }
    sed(g, "vector3_zero", EntityDataValue::Vector3([0.0, 0.0, 0.0]));
    sed(g, "vector3_mixed", EntityDataValue::Vector3([1.5, -2.25, 1e10]));
    sed(g, "quaternion_identity", EntityDataValue::Quaternion([0.0, 0.0, 0.0, 1.0]));
    sed(g, "quaternion_mixed", EntityDataValue::Quaternion([0.5, -0.5, 0.5, -0.5]));
    sed(g, "humanoid_arm_0", EntityDataValue::HumanoidArm(HumanoidArm::Left));
    sed(g, "humanoid_arm_1", EntityDataValue::HumanoidArm(HumanoidArm::Right));
    use DyeColor::*;
    for (i, c) in [White, Orange, Magenta, LightBlue, Yellow, Lime, Pink, Gray, LightGray, Cyan, Purple, Blue, Brown, Green, Red, Black].into_iter().enumerate() {
        sed(g, &format!("dye_color_{i}"), EntityDataValue::DyeColor(c));
    }
}

fn template(item: Item, count: i32, components: DataComponentPatch) -> ItemStackTemplate {
    ItemStackTemplate::new(item, count, components).unwrap()
}

fn items(g: &Golden) {
    let mut named = ItemStack::new(Item::DIAMOND_SWORD);
    named.set(comp::UNBREAKABLE, ());
    named.set(comp::DAMAGE, 12);
    named.set(comp::MAX_STACK_SIZE, 16);
    let mut stripped = ItemStack::with_count(Item::DIAMOND_PICKAXE, 3);
    stripped.remove(DataComponentType::MAX_DAMAGE);
    sed(g, "item_stack_empty", EntityDataValue::ItemStack(ItemStack::empty()));
    sed(g, "item_stack_diamond", EntityDataValue::ItemStack(ItemStack::new(Item::DIAMOND)));
    sed(g, "item_stack_stone_64", EntityDataValue::ItemStack(ItemStack::with_count(Item::STONE, 64)));
    sed(g, "item_stack_components", EntityDataValue::ItemStack(named));
    sed(g, "item_stack_removed_component", EntityDataValue::ItemStack(stripped));
    let last = Item::from_id(count(g, "items") - 1).unwrap();
    sed(g, "item_stack_last_item", EntityDataValue::ItemStack(ItemStack::new(last)));

    let item = |i| ParticleOptions::Item(ItemStackTemplate::of(i));
    g.check_value("particle_item_diamond", &item(Item::DIAMOND));
    let patch = DataComponentPatch::builder().set(comp::DAMAGE, 3).remove(DataComponentType::MAX_STACK_SIZE).build();
    g.check_value("particle_item_count_components", &ParticleOptions::Item(template(Item::STONE, 5, patch)));
    sed(g, "particle_item", EntityDataValue::Particle(item(Item::APPLE)));
    sed(g, "particles_item", EntityDataValue::Particles(vec![item(Item::APPLE), ParticleOptions::Simple(ParticleType::FLAME)]));
    g.check(
        "level_particles_item",
        &ClientboundLevelParticlesPacket {
            particle: item(Item::EMERALD),
            override_limiter: false,
            always_show: true,
            x: 1.0,
            y: 2.0,
            z: 3.0,
            x_dist: 0.0,
            y_dist: 0.0,
            z_dist: 0.0,
            x_max_speed: 0.0,
            y_max_speed: 0.0,
            z_max_speed: 0.0,
            count: 1,
            randomization_type: RandomizationType::Default,
        },
    );
    let partial = |name: Option<&str>, id, properties| Either::Right(ResolvableProfilePartial { name: name.map(Into::into), id, properties });
    let profile = |contents| EntityDataValue::ResolvableProfile(ResolvableProfile { contents, skin_patch: PlayerSkinPatch::default() });
    sed(g, "resolvable_profile_name", profile(partial(Some("Steve"), None, vec![])));
    sed(g, "resolvable_profile_id", profile(partial(None, Some(uuid(1, 2)), vec![])));
    let prop = |n: &str, v: &str, s: Option<&str>| Property { name: n.into(), value: v.into(), signature: s.map(Into::into) };
    let full = GameProfile {
        id: uuid(0x0123456789abcdef, 0xfedcba9876543210u64 as i64),
        name: "Alex".into(),
        properties: vec![prop("textures", "base64value", Some("sig")), prop("other", "v", None)],
    };
    sed(g, "resolvable_profile_full", profile(Either::Left(full)));
    sed(g, "resolvable_profile_full_no_props", profile(Either::Left(GameProfile { id: uuid(3, 4), name: "Bob".into(), properties: vec![] })));
    sed(g, "resolvable_profile_static_empty", profile(partial(None, None, vec![])));
    for l in ["profile_name_too_long", "profile_too_many_props", "profile_partial_both", "profile_skin_patch", "profile_skin_patch_wide", "profile_property_name_too_long"] {
        redecode::<ClientboundSetEntityDataPacket>(g, &format!("redecode_sed_{l}"), true);
    }
    for l in ["item_stack_zero_count", "item_stack_bad_item", "item_stack_negative_count", "item_particle_bad_item", "item_particle_zero_count"] {
        redecode::<ClientboundSetEntityDataPacket>(g, &format!("redecode_sed_{l}"), true);
    }
}

// ---------------------------------------------------------------- particles

fn name(t: ParticleType) -> &'static str {
    t.name().strip_prefix("minecraft:").unwrap()
}

fn particle_types(g: &Golden) {
    let total = count(g, "particle_types");
    let mut simple = 0;
    for t in ParticleType::iter() {
        if t.data().class == "SimpleParticleType" {
            g.check_value(&format!("particle_simple_{}", name(t)), &ParticleOptions::Simple(t));
            simple += 1;
        }
    }
    assert_eq!(total as usize, ParticleType::iter().len());
    assert_eq!(simple, 106);

    let states = [0, 1, 5000, last_state(g)];
    for t in [ParticleType::BLOCK, ParticleType::BLOCK_MARKER, ParticleType::FALLING_DUST, ParticleType::DUST_PILLAR, ParticleType::BLOCK_CRUMBLE] {
        for s in states {
            g.check_value(&format!("particle_block_{}_{s}", name(t)), &ParticleOptions::Block { particle_type: t, state: state(s) });
        }
    }
    for t in [ParticleType::ENTITY_EFFECT, ParticleType::TINTED_LEAVES, ParticleType::FLASH] {
        for c in [0, -1, 0xFFFF0000u32 as i32, 0x12345678, i32::MIN] {
            g.check_value(&format!("particle_color_{}_{c}", name(t)), &ParticleOptions::Color { particle_type: t, color: c });
        }
    }
    for (i, p) in [0.0f32, 1.0, -2.5, 1e9].into_iter().enumerate() {
        g.check_value(&format!("particle_power_{i}"), &ParticleOptions::Power { particle_type: ParticleType::DRAGON_BREATH, power: p });
    }
    for t in [ParticleType::EFFECT, ParticleType::INSTANT_EFFECT] {
        for (l, color, power) in [("a", 0xFFFFFF, 1.0f32), ("b", -1, 0.25), ("c", 0x00FF7F, -3.0)] {
            g.check_value(&format!("particle_spell_{}_{l}", name(t)), &ParticleOptions::Spell { particle_type: t, color, power });
        }
    }
    for t in [ParticleType::GEYSER, ParticleType::GEYSER_PLUME] {
        for w in [0, 5, -3, i32::MAX] {
            g.check_value(&format!("particle_geyser_{}_{w}", name(t)), &ParticleOptions::Geyser { particle_type: t, water_blocks: w });
        }
    }
    for t in [ParticleType::GEYSER_BASE, ParticleType::GEYSER_POOF] {
        g.check_value(&format!("particle_geyser_base_{}_a", name(t)), &ParticleOptions::GeyserBase { particle_type: t, water_blocks: 3, burst_impulse_base: 1.5 });
        g.check_value(&format!("particle_geyser_base_{}_b", name(t)), &ParticleOptions::GeyserBase { particle_type: t, water_blocks: -1, burst_impulse_base: -0.5 });
    }
    let dust = |l: &str, color, scale| g.check_value(&format!("particle_{l}"), &ParticleOptions::Dust { color, scale });
    dust("dust_a", 0xFF0000, 1.0);
    dust("dust_b", 0x123456, 0.01);
    dust("dust_c", -1, 4.0);
    // the constructor clamps the scale to [0.01, 4]: vanilla's bytes hold the clamped scale, also when encoding the raw one
    dust("dust_clamp_hi", 0, 4.0);
    dust("dust_clamp_lo", 0, 0.01);
    assert_eq!(to_bytes(&ParticleOptions::Dust { color: 0, scale: 100.0 }), g.bytes("particle_dust_clamp_hi"));
    assert_eq!(to_bytes(&ParticleOptions::Dust { color: 0, scale: -3.0 }), g.bytes("particle_dust_clamp_lo"));
    let transition = |l: &str, from_color, to_color, scale| g.check_value(&format!("particle_{l}"), &ParticleOptions::DustColorTransition { from_color, to_color, scale });
    transition("dust_color_transition_a", 0x3A0AE0, 0xFF0000, 1.0);
    transition("dust_color_transition_b", 0, -1, 0.5);
    transition("dust_color_transition_clamp_hi", 1, 2, 4.0);
    transition("dust_color_transition_clamp_lo", 1, 2, 0.01);
    assert_eq!(to_bytes(&ParticleOptions::DustColorTransition { from_color: 1, to_color: 2, scale: 50.0 }), g.bytes("particle_dust_color_transition_clamp_hi"));
    assert_eq!(to_bytes(&ParticleOptions::DustColorTransition { from_color: 1, to_color: 2, scale: 0.0 }), g.bytes("particle_dust_color_transition_clamp_lo"));
    for (i, roll) in [0.0f32, -1.5, 3.14159].into_iter().enumerate() {
        g.check_value(&format!("particle_sculk_charge_{i}"), &ParticleOptions::SculkCharge { roll });
    }
    for d in [0, 1, 100, 300, -1, i32::MAX] {
        g.check_value(&format!("particle_shriek_{d}"), &ParticleOptions::Shriek { delay: d });
    }
    let vib = |l: &str, destination, arrival_in_ticks| g.check_value(&format!("particle_vibration_{l}"), &ParticleOptions::Vibration { destination, arrival_in_ticks });
    vib("block", PositionSource::Block(block_pos(1, -2, 3)), 20);
    vib("block_extreme", PositionSource::Block(block_pos(-33554432, 2047, 33554431)), 0);
    vib("entity", PositionSource::Entity { source_entity_id: 77777, y_offset: 1.5 }, 300);
    vib("negative_ticks", PositionSource::Block(block_pos(0, 0, 0)), -5);
    g.check_value("particle_trail_a", &ParticleOptions::Trail { target: Vec3::new(1.5, 64.0, -3.25), color: 0xFF00FF, duration: 40 });
    g.check_value("particle_trail_b", &ParticleOptions::Trail { target: Vec3::new(-1e9, 0.0, 1e-9), color: -1, duration: i32::MAX });
}

fn level_particles_packets(g: &Golden) {
    let stone = state(1);
    let kinds = [
        ParticleOptions::Simple(ParticleType::FLAME),
        ParticleOptions::Dust { color: 0xFF0000, scale: 1.0 },
        ParticleOptions::Block { particle_type: ParticleType::BLOCK, state: stone },
        ParticleOptions::Vibration { destination: PositionSource::Block(block_pos(4, 5, 6)), arrival_in_ticks: 10 },
        ParticleOptions::Simple(ParticleType::EXPLOSION),
    ];
    let rand = [RandomizationType::Default, RandomizationType::Alternative, RandomizationType::AlternativeWithSpeed];
    for (i, particle) in kinds.into_iter().enumerate() {
        // the generator increments `n` before using it
        let n = i as i32 + 1;
        g.check(
            &format!("level_particles_{i}"),
            &ClientboundLevelParticlesPacket {
                particle,
                override_limiter: n % 2 == 0,
                always_show: n % 3 == 0,
                x: 1.5,
                y: -64.25,
                z: 1e6,
                x_dist: 0.1,
                y_dist: 0.2,
                z_dist: 0.3,
                x_max_speed: 0.01,
                y_max_speed: 0.02,
                z_max_speed: 0.03,
                count: n * 7,
                randomization_type: rand[(n % 3) as usize],
            },
        );
    }
    let packet = |particle, override_limiter, always_show, pos: [f64; 3], dist: [f32; 3], speed: [f32; 3], count, randomization_type| ClientboundLevelParticlesPacket {
        particle,
        override_limiter,
        always_show,
        x: pos[0],
        y: pos[1],
        z: pos[2],
        x_dist: dist[0],
        y_dist: dist[1],
        z_dist: dist[2],
        x_max_speed: speed[0],
        y_max_speed: speed[1],
        z_max_speed: speed[2],
        count,
        randomization_type,
    };
    g.check(
        "level_particles_simple_ctor",
        &packet(ParticleOptions::Simple(ParticleType::HAPPY_VILLAGER), true, true, [0.0; 3], [0.0; 3], [0.5; 3], 0, RandomizationType::Default),
    );
    g.check(
        "level_particles_extreme",
        &packet(
            ParticleOptions::Simple(ParticleType::CRIT),
            false,
            false,
            [f64::MAX, -0.0, f64::from_bits(1)],
            [f32::MAX, -f32::MAX, f32::from_bits(1)],
            [-1.0, 1.0, 1e-9],
            i32::MAX,
            RandomizationType::AlternativeWithSpeed,
        ),
    );
    g.check(
        "level_particles_neg_count",
        &packet(ParticleOptions::Simple(ParticleType::CRIT), false, true, [0.0; 3], [0.0; 3], [0.0; 3], -1, RandomizationType::Default),
    );
    for (i, r) in rand.into_iter().enumerate() {
        g.check(&format!("level_particles_rand_{i}"), &packet(ParticleOptions::Simple(ParticleType::CRIT), false, true, [0.0; 3], [0.0; 3], [0.0; 3], 3, r));
    }
}

// ---------------------------------------------------------------- explode

fn explode_packets(g: &Golden) {
    let none = WeightedList::<ExplosionParticleInfo>(vec![]);
    let some = || {
        WeightedList(vec![
            Weighted { value: ExplosionParticleInfo { particle: ParticleOptions::Simple(ParticleType::POOF), scaling: 1.0, speed: 2.0 }, weight: 3 },
            Weighted { value: ExplosionParticleInfo { particle: ParticleOptions::Dust { color: 0x808080, scale: 1.5 }, scaling: 0.5, speed: 0.25 }, weight: 1 },
            Weighted {
                value: ExplosionParticleInfo { particle: ParticleOptions::Block { particle_type: ParticleType::BLOCK, state: state(1) }, scaling: -1.0, speed: 0.0 },
                weight: 0,
            },
        ])
    };
    let generic = || SoundEventHolder::Reference(SoundEvent::ENTITY_GENERIC_EXPLODE);
    let explosion = || ParticleOptions::Simple(ParticleType::EXPLOSION);
    g.check(
        "explode_basic",
        &ClientboundExplodePacket {
            center: Vec3::new(1.5, 64.0, -3.25),
            radius: 4.0,
            block_count: 10,
            player_knockback: None,
            explosion_particle: explosion(),
            explosion_sound: generic(),
            block_particles: none.clone(),
            play_sound: true,
        },
    );
    g.check(
        "explode_knockback",
        &ClientboundExplodePacket {
            center: Vec3::new(-1e9, 0.0, 1e9),
            radius: 0.0,
            block_count: 0,
            player_knockback: Some(Vec3::new(0.5, -0.25, 1.0)),
            explosion_particle: ParticleOptions::Simple(ParticleType::EXPLOSION_EMITTER),
            explosion_sound: generic(),
            block_particles: some(),
            play_sound: false,
        },
    );
    g.check(
        "explode_direct_var",
        &ClientboundExplodePacket {
            center: Vec3::ZERO,
            radius: 1.0,
            block_count: -1,
            player_knockback: Some(Vec3::ZERO),
            explosion_particle: ParticleOptions::Dust { color: 0xFFFFFF, scale: 1.0 },
            explosion_sound: SoundEventHolder::Direct(DirectSoundEvent { location: Identifier::parse("mod:boom").unwrap(), fixed_range: None }),
            block_particles: some(),
            play_sound: true,
        },
    );
    g.check(
        "explode_direct_fixed",
        &ClientboundExplodePacket {
            center: Vec3::ZERO,
            radius: f32::MAX,
            block_count: i32::MAX,
            player_knockback: None,
            explosion_particle: ParticleOptions::Simple(ParticleType::SMOKE),
            explosion_sound: SoundEventHolder::Direct(DirectSoundEvent {
                location: Identifier::parse("minecraft:entity.generic.explode").unwrap(),
                fixed_range: Some(24.5),
            }),
            block_particles: none,
            play_sound: false,
        },
    );
    let many = WeightedList(
        (0..200)
            .map(|i| Weighted { value: ExplosionParticleInfo { particle: ParticleOptions::Simple(ParticleType::POOF), scaling: i as f32, speed: (-i) as f32 }, weight: i })
            .collect(),
    );
    g.check(
        "explode_many_particles",
        &ClientboundExplodePacket {
            center: Vec3::ZERO,
            radius: 3.0,
            block_count: 100,
            player_knockback: None,
            explosion_particle: explosion(),
            explosion_sound: generic(),
            block_particles: many,
            play_sound: true,
        },
    );
    let last = SoundEvent::from_id(count(g, "sound_events") - 1).unwrap();
    g.check(
        "explode_last_sound",
        &ClientboundExplodePacket {
            center: Vec3::ZERO,
            radius: 1.0,
            block_count: 1,
            player_knockback: None,
            explosion_particle: explosion(),
            explosion_sound: SoundEventHolder::Reference(last),
            block_particles: WeightedList(vec![]),
            play_sound: true,
        },
    );
}

// ---------------------------------------------------------------- hostile input

/// Vanilla decoded `<label>.in` (packet id + body) with its real codec: either re-encoded to `<label>` or `ERR:<message>`.
/// `exact` additionally requires our error text to equal vanilla's (not possible for buffer-exhaustion errors, where
/// vanilla reports Netty's message).
fn redecode<P: Packet + Encode + Decode + std::fmt::Debug>(g: &Golden, label: &str, exact: bool) {
    let mut input = g.bytes(&format!("{label}.in"));
    let expected = g.bytes(label);
    assert_eq!(read_varint(&mut input).unwrap(), P::ID, "id {label}");
    match (P::decode(&mut input), expected.strip_prefix(b"ERR:")) {
        (Ok(p), None) => {
            let mut ours = Vec::new();
            mc_protocol::codec::write_varint(&mut ours, P::ID);
            p.encode(&mut ours);
            assert_eq!(hex(&ours), hex(expected), "re-encode {label}");
        }
        (Err(e), Some(msg)) => {
            if exact {
                assert_eq!(e.to_string(), String::from_utf8_lossy(msg), "error {label}");
            }
        }
        (Ok(p), Some(msg)) => panic!("{label}: vanilla failed ({}) but we decoded {p:?}", String::from_utf8_lossy(msg)),
        (Err(e), None) => panic!("{label}: vanilla decoded, we failed: {e}"),
    }
}

fn hostile_set_entity_data(g: &Golden) {
    let exact = [
        "oob_pose", "neg_pose", "oob_direction", "oob_sniffer", "oob_armadillo", "oob_copper_golem", "oob_weathering_hi",
        "oob_weathering_lo", "oob_arm", "oob_dye", "oob_block_state", "oob_optional_block_state", "neg_optional_block_state",
        "villager_level_zero", "villager_bad_type", "villager_bad_profession", "rotations_unnormalised", "optional_int_neg",
        "bad_serializer", "bad_serializer_neg", "bad_serializer_big", "string_too_long", "bad_particle", "particle_bad_block",
        "particles_neg_count", "particle_bad_position_source", "particle_shriek_trail",
    ];
    for l in exact {
        redecode::<ClientboundSetEntityDataPacket>(g, &format!("redecode_sed_{l}"), true);
    }
    for l in ["truncated", "unterminated"] {
        redecode::<ClientboundSetEntityDataPacket>(g, &format!("redecode_sed_{l}"), false);
    }
}

fn hostile_other_packets(g: &Golden) {
    for l in ["bad_type", "neg_type"] {
        redecode::<ClientboundAddEntityPacket>(g, &format!("redecode_add_entity_{l}"), true);
    }
    for l in ["bad_type", "rand_oob"] {
        redecode::<ClientboundLevelParticlesPacket>(g, &format!("redecode_particles_{l}"), true);
    }
    for l in ["neg_weight", "weight_overflow", "weight_max", "weights_zero", "bad_sound", "neg_list"] {
        redecode::<ClientboundExplodePacket>(g, &format!("redecode_explode_{l}"), true);
    }
}

fn serializer_ids(g: &Golden) {
    // 44 serializers: the VarInt after the accessor id of the last one is 43 (dye color)
    assert_eq!(count(g, "serializers"), 44);
    let bytes = g.bytes("sed_dye_color_0");
    assert_eq!(&bytes[bytes.len() - 4..], &[5, 43, 0, 255]);
}

#[test]
fn entity_data_golden() {
    let g = g();
    add_entity_packets(&g);
    set_entity_data_scalars(&g);
    optional_block_state_air_reads_back_empty(&g);
    set_entity_data_structured(&g);
    items(&g);
    particle_types(&g);
    level_particles_packets(&g);
    explode_packets(&g);
    hostile_set_entity_data(&g);
    hostile_other_packets(&g);
    serializer_ids(&g);
    g.assert_all_used();
}
