//! The data codecs of the particle options: `ParticleType.codec()` (the options' own fields) and `ParticleTypes.CODEC`
//! (`{type: <particle type>, ...}`), over every `DynamicOps`.
use super::particle::{ParticleOptions, PositionSource};
use crate::components::holder::registry_by_name_codec;
use crate::components::ItemStackTemplate;
use mc_core::phys::vec3::Vec3;
use mc_core::pos::block_pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::ParticleType;
use mc_dfu::extra::{fixed_size, POSITIVE_INT, RGB_COLOR_CODEC};
use mc_dfu::{record, Codec, DataResult, Dyn, MapCodec, DOUBLE, FLOAT, INT, INT_STREAM};
use std::sync::LazyLock;

/// `ARGB.colorFromFloat(alpha, red, green, blue)`.
fn color_from_float(alpha: f32, red: f32, green: f32, blue: f32) -> i32 {
    let ch = |v: f32| ((v * 255.0).floor() as i32) & 0xFF;
    (ch(alpha) << 24) | (ch(red) << 16) | (ch(green) << 8) | ch(blue)
}

/// `ExtraCodecs.ARGB_COLOR_CODEC`: an int or an `[r, g, b, a]` float list.
pub static ARGB_COLOR_CODEC: LazyLock<Codec<i32>> = LazyLock::new(|| {
    let vector4f = FLOAT.list_of().comap_flat_map(|l| fixed_size(l, 4), |v: &Vec<f32>| v.clone());
    INT.with_alternative_conv(&vector4f, |v: Vec<f32>| color_from_float(v[3], v[0], v[1], v[2]))
});

/// `Vec3.CODEC`: a list of three doubles.
fn vec3_codec() -> Codec<Vec3> {
    DOUBLE.list_of().comap_flat_map(
        |l| fixed_size(l, 3).map(|v| Vec3::new(v[0], v[1], v[2])),
        |v: &Vec3| vec![v.x, v.y, v.z],
    )
}

/// The properties of `block` as a map (`StateDefinition.propertiesCodec()`): every property reads leniently (an
/// unreadable or missing value is the default state's), and encodes with all values.
fn properties_codec(block: Block) -> Codec<BlockState> {
    Codec::from_fns(
        move |ops, state: &BlockState, prefix| {
            let mut b = ops.map_builder();
            for p in block.properties() {
                if let Some(value) = state.get_property_str(p.name) {
                    b.add(p.name, ops.create_string(value));
                }
            }
            b.build(&prefix)
        },
        move |ops, input| {
            ops.get_map(input).map(|m| {
                let mut state = block.default_state();
                for p in block.properties() {
                    let parsed = m.get(p.name).and_then(|v| ops.get_string_value(v).result());
                    if let Some(next) = parsed.and_then(|value| state.with_property_str(p.name, &value)) {
                        state = next;
                    }
                }
                (state, input.clone())
            })
        },
    )
}

/// `BlockState.CODEC`: a block name (its default state) or `{id, properties?}`.
pub fn block_state_codec() -> Codec<BlockState> {
    let full = registry_by_name_codec::<Block>().dispatch(
        "id",
        |state: &BlockState| state.block(),
        |block: &Block| {
            let block = *block;
            let default = block.default_state();
            properties_codec(block)
                .lenient_optional_field_of("properties")
                .xmap(move |o| o.unwrap_or(default), |s: &BlockState| Some(*s))
        },
    );
    registry_by_name_codec::<Block>().either(&full).xmap(
        |e| e.map(|block| block.default_state(), |state| state),
        |state: &BlockState| {
            if *state == state.block().default_state() { mc_dfu::Either::Left(state.block()) } else { mc_dfu::Either::Right(*state) }
        },
    )
}

/// `BlockParticleOption.BLOCK_STATE_CODEC`: `BlockState.CODEC` or a bare block name.
fn block_particle_state_codec() -> Codec<BlockState> {
    block_state_codec().with_alternative_conv(&registry_by_name_codec::<Block>(), |block: Block| block.default_state())
}

/// `VibrationParticleOption.SAFE_POSITION_SOURCE_CODEC`: `PositionSource.CODEC` minus entity sources.
fn safe_position_source_codec() -> Codec<PositionSource> {
    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        Block,
        Entity,
    }
    let kind = mc_dfu::extra::IDENTIFIER.flat_xmap(
        |id| match id.to_string().as_str() {
            "minecraft:block" => DataResult::success(Kind::Block),
            "minecraft:entity" => DataResult::success(Kind::Entity),
            other => {
                let m = format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:position_source_type]: {other}");
                DataResult::error(move || m.clone())
            }
        },
        |k: &Kind| DataResult::success(mc_core::ident::Identifier::parse(if *k == Kind::Block { "minecraft:block" } else { "minecraft:entity" }).expect("valid")),
    );
    let block_pos = BLOCK_POS.clone();
    let uuid = crate::components::common::uuid_codec();
    kind.dispatch(
        "type",
        |s: &PositionSource| if matches!(s, PositionSource::Block(_)) { Kind::Block } else { Kind::Entity },
        move |k: &Kind| match k {
            Kind::Block => block_pos.field_of("pos").xmap(PositionSource::Block, |s: &PositionSource| match s {
                PositionSource::Block(p) => *p,
                PositionSource::Entity { .. } => BlockPos::new(0, 0, 0),
            }),
            Kind::Entity => {
                record!(EntityFields { y_offset: FLOAT.optional_field_of_default("y_offset", 0.0f32), source_entity: uuid.field_of("source_entity") })
                    .xmap(
                        |f| PositionSource::Entity { source_entity_id: 0, y_offset: f.y_offset },
                        |s: &PositionSource| match s {
                            PositionSource::Entity { y_offset, .. } => EntityFields { source_entity: uuid::Uuid::nil(), y_offset: *y_offset },
                            PositionSource::Block(_) => EntityFields { source_entity: uuid::Uuid::nil(), y_offset: 0.0 },
                        },
                    )
            }
        },
    )
    .validate(|s| match s {
        PositionSource::Entity { .. } => DataResult::error_str("Entity position sources are not allowed"),
        s => DataResult::success(s),
    })
}

static BLOCK_POS: LazyLock<Codec<BlockPos>> = LazyLock::new(|| {
    INT_STREAM.comap_flat_map(
        |v: Vec<i32>| {
            if v.len() == 3 {
                DataResult::success(BlockPos::new(v[0], v[1], v[2]))
            } else {
                DataResult::error_str("Input is not a list of 3 ints")
            }
        },
        |p: &BlockPos| vec![p.x(), p.y(), p.z()],
    )
});

#[derive(Clone)]
struct EntityFields {
    source_entity: uuid::Uuid,
    y_offset: f32,
}

/// `ScalableParticleOptionsBase.SCALE`: a float within `[0.01, 4.0]`.
static SCALE: LazyLock<Codec<f32>> = LazyLock::new(|| {
    FLOAT.validate(|v| {
        if (0.01f32..=4.0f32).contains(&v) {
            DataResult::success(v)
        } else {
            DataResult::error(move || format!("Value must be within range [0.01;4.0]: {}", mc_core::math::java::java_float_to_string(v)))
        }
    })
});

#[derive(Clone)]
struct DustFields {
    color: i32,
    scale: f32,
}
#[derive(Clone)]
struct DustTransitionFields {
    from_color: i32,
    to_color: i32,
    scale: f32,
}
#[derive(Clone)]
struct SpellFields {
    color: i32,
    power: f32,
}
#[derive(Clone)]
struct VibrationFields {
    destination: PositionSource,
    arrival_in_ticks: i32,
}
#[derive(Clone)]
struct TrailFields {
    target: Vec3,
    color: i32,
    duration: i32,
}
#[derive(Clone)]
struct GeyserBaseFields {
    water_blocks: i32,
    burst_impulse_base: f32,
}

impl ParticleOptions {
    /// `ParticleType.codec()`: the map codec of the options of `particle_type` (decoding yields options of that type;
    /// encoding a value of another shape writes nothing sensible, like a ClassCastException in vanilla).
    pub fn map_codec(t: ParticleType) -> MapCodec<ParticleOptions> {
        match t {
            ParticleType::BLOCK | ParticleType::BLOCK_MARKER | ParticleType::FALLING_DUST | ParticleType::DUST_PILLAR | ParticleType::BLOCK_CRUMBLE => {
                block_particle_state_codec()
                    .xmap(
                        move |state| ParticleOptions::Block { particle_type: t, state },
                        |o: &ParticleOptions| match o {
                            ParticleOptions::Block { state, .. } => *state,
                            _ => Block::AIR.default_state(),
                        },
                    )
                    .field_of("block_state")
            }
            ParticleType::ENTITY_EFFECT | ParticleType::TINTED_LEAVES | ParticleType::FLASH => ARGB_COLOR_CODEC
                .xmap(
                    move |color| ParticleOptions::Color { particle_type: t, color },
                    |o: &ParticleOptions| match o {
                        ParticleOptions::Color { color, .. } => *color,
                        _ => 0,
                    },
                )
                .field_of("color"),
            ParticleType::DRAGON_BREATH => FLOAT
                .xmap(
                    move |power| ParticleOptions::Power { particle_type: t, power },
                    |o: &ParticleOptions| match o {
                        ParticleOptions::Power { power, .. } => *power,
                        _ => 1.0,
                    },
                )
                .optional_field_of_default("power", ParticleOptions::Power { particle_type: t, power: 1.0 }),
            ParticleType::EFFECT | ParticleType::INSTANT_EFFECT => record!(SpellFields {
                power: FLOAT.optional_field_of_default("power", 1.0f32),
                color: RGB_COLOR_CODEC.optional_field_of_default("color", -1),
            })
            .xmap(
                move |f| ParticleOptions::Spell { particle_type: t, color: f.color, power: f.power },
                |o: &ParticleOptions| match o {
                    ParticleOptions::Spell { color, power, .. } => SpellFields { color: *color, power: *power },
                    _ => SpellFields { color: -1, power: 1.0 },
                },
            ),
            ParticleType::GEYSER | ParticleType::GEYSER_PLUME => POSITIVE_INT
                .field_of("water_blocks")
                .xmap(
                    move |water_blocks| ParticleOptions::Geyser { particle_type: t, water_blocks },
                    |o: &ParticleOptions| match o {
                        ParticleOptions::Geyser { water_blocks, .. } => *water_blocks,
                        _ => 1,
                    },
                ),
            ParticleType::GEYSER_BASE | ParticleType::GEYSER_POOF => record!(GeyserBaseFields {
                burst_impulse_base: FLOAT.field_of("burst_impulse_base"),
                water_blocks: POSITIVE_INT.field_of("water_blocks"),
            })
            .xmap(
                move |f| ParticleOptions::GeyserBase { particle_type: t, water_blocks: f.water_blocks, burst_impulse_base: f.burst_impulse_base },
                |o: &ParticleOptions| match o {
                    ParticleOptions::GeyserBase { water_blocks, burst_impulse_base, .. } => {
                        GeyserBaseFields { water_blocks: *water_blocks, burst_impulse_base: *burst_impulse_base }
                    }
                    _ => GeyserBaseFields { water_blocks: 1, burst_impulse_base: 0.0 },
                },
            ),
            ParticleType::DUST => record!(DustFields { scale: SCALE.field_of("scale"), color: RGB_COLOR_CODEC.field_of("color") }).xmap(
                |f| ParticleOptions::Dust { color: f.color, scale: f.scale },
                |o: &ParticleOptions| match o {
                    ParticleOptions::Dust { color, scale } => DustFields { color: *color, scale: *scale },
                    _ => DustFields { color: 0, scale: 1.0 },
                },
            ),
            ParticleType::DUST_COLOR_TRANSITION => record!(DustTransitionFields {
                scale: SCALE.field_of("scale"),
                to_color: RGB_COLOR_CODEC.field_of("to_color"),
                from_color: RGB_COLOR_CODEC.field_of("from_color"),
            })
            .xmap(
                |f| ParticleOptions::DustColorTransition { from_color: f.from_color, to_color: f.to_color, scale: f.scale },
                |o: &ParticleOptions| match o {
                    ParticleOptions::DustColorTransition { from_color, to_color, scale } => {
                        DustTransitionFields { from_color: *from_color, to_color: *to_color, scale: *scale }
                    }
                    _ => DustTransitionFields { from_color: 0, to_color: 0, scale: 1.0 },
                },
            ),
            ParticleType::SCULK_CHARGE => FLOAT.field_of("roll").xmap(
                |roll| ParticleOptions::SculkCharge { roll },
                |o: &ParticleOptions| match o {
                    ParticleOptions::SculkCharge { roll } => *roll,
                    _ => 0.0,
                },
            ),
            ParticleType::SHRIEK => INT.field_of("delay").xmap(
                |delay| ParticleOptions::Shriek { delay },
                |o: &ParticleOptions| match o {
                    ParticleOptions::Shriek { delay } => *delay,
                    _ => 0,
                },
            ),
            ParticleType::ITEM => ItemStackTemplate::codec()
                .xmap(
                    ParticleOptions::Item,
                    |o: &ParticleOptions| match o {
                        ParticleOptions::Item(item) => item.clone(),
                        _ => ItemStackTemplate::of(mc_data::Item::STONE),
                    },
                )
                .field_of("item"),
            ParticleType::VIBRATION => record!(VibrationFields {
                arrival_in_ticks: INT.field_of("arrival_in_ticks"),
                destination: safe_position_source_codec().field_of("destination"),
            })
            .xmap(
                |f| ParticleOptions::Vibration { destination: f.destination, arrival_in_ticks: f.arrival_in_ticks },
                |o: &ParticleOptions| match o {
                    ParticleOptions::Vibration { destination, arrival_in_ticks } => {
                        VibrationFields { destination: *destination, arrival_in_ticks: *arrival_in_ticks }
                    }
                    _ => VibrationFields { destination: PositionSource::Block(BlockPos::new(0, 0, 0)), arrival_in_ticks: 0 },
                },
            ),
            ParticleType::TRAIL => record!(TrailFields {
                duration: POSITIVE_INT.field_of("duration"),
                color: RGB_COLOR_CODEC.field_of("color"),
                target: vec3_codec().field_of("target"),
            })
            .xmap(
                |f| ParticleOptions::Trail { target: f.target, color: f.color, duration: f.duration },
                |o: &ParticleOptions| match o {
                    ParticleOptions::Trail { target, color, duration } => TrailFields { target: *target, color: *color, duration: *duration },
                    _ => TrailFields { target: Vec3::new(0.0, 0.0, 0.0), color: 0, duration: 1 },
                },
            ),
            _ => MapCodec::unit(ParticleOptions::Simple(t)),
        }
    }

    /// `ParticleTypes.CODEC`: the particle type's registry name under `type`, then its options.
    pub fn codec() -> Codec<ParticleOptions> {
        registry_by_name_codec::<ParticleType>().dispatch("type", |o: &ParticleOptions| o.particle_type(), |t: &ParticleType| Self::map_codec(*t))
    }

    /// Parse the options of `particle_type` from NBT `data` (`type.codec().codec().parse(NbtOps, data)`): the error message.
    pub fn parse_nbt(t: ParticleType, data: &mc_nbt::Tag) -> Result<ParticleOptions, String> {
        Self::map_codec(t).codec().parse(&mc_dfu::NBT, &Dyn::from(data)).into_result()
    }
}
