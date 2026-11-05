//! Codecs of the values that enchantment effects embed and that have no DFU codec elsewhere yet: `FloatProviders`,
//! `ParticleTypes.CODEC`, block states, `BlockPredicate` / `BlockStateProvider` holders, damage type holders, weighted
//! lists, `Level.ExplosionInteraction`.
//!
//! `BlockPredicate` and `BlockStateProvider` live in the levelgen layer without an encoder: their codec here decodes the
//! runtime object with the levelgen loaders and keeps the exact `encodeStart(NbtOps, ..)` form produced by a
//! [`Schema`] of the same codec.

use std::cell::OnceCell;
use std::sync::{Arc, LazyLock};

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_data::block::{Block, BlockState};
use mc_data::{GameEvent, ParticleType};
use mc_dfu::{Codec, DataResult, Dyn, Either, FLOAT, INT, MapCodec, STRING, record, string_enum_codec};
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::components::common::SoundEvent;
use mc_protocol::components::holder::{Holder, registry_by_name_codec};
use mc_protocol::components::{ItemStackTemplate, NoDirect};
use mc_protocol::packets::play::{ParticleOptions, PositionSource as ParticlePositionSource};
use mc_worldgen::provider::FloatProvider;
use serde_json::Value;

use super::codecs::{holder_file_codec, type_codec, vec3_codec};
use crate::registry::codec::{Lookup, RegistryDecodeContext, RegistryLookup, Schema};
use crate::registry::typed::damage_type::{DamageEffects, DamageScaling, DamageType, DeathMessageType};
use crate::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use crate::world::level::levelgen::stateproviders::{self, BlockStateProvider};

// ---- Level.ExplosionInteraction / Weighted ----------------------------------------------------------------------------

/// `Level.ExplosionInteraction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExplosionInteraction {
    None,
    Block,
    Mob,
    Tnt,
    Trigger,
}

impl ExplosionInteraction {
    pub fn codec() -> Codec<ExplosionInteraction> {
        static NAMES: [(ExplosionInteraction, &str); 5] = [
            (ExplosionInteraction::None, "none"),
            (ExplosionInteraction::Block, "block"),
            (ExplosionInteraction::Mob, "mob"),
            (ExplosionInteraction::Tnt, "tnt"),
            (ExplosionInteraction::Trigger, "trigger"),
        ];
        string_enum_codec(&NAMES)
    }
}

/// `Weighted<T>`.
#[derive(Clone, Debug, PartialEq)]
pub struct Weighted<T> {
    pub value: T,
    pub weight: i32,
}

/// `ExplosionParticleInfo`.
#[derive(Clone, Debug, PartialEq)]
pub struct ExplosionParticleInfo {
    pub particle: ParticleOptions,
    pub scaling: f32,
    pub speed: f32,
}

/// `WeightedList.codec(ExplosionParticleInfo.CODEC)`: a list of `{<info fields>, weight}` objects.
pub fn explosion_particle_infos_codec() -> Codec<Vec<Weighted<ExplosionParticleInfo>>> {
    let info = record!(ExplosionParticleInfo {
        particle: particle_options_codec().field_of("particle"),
        scaling: FLOAT.optional_field_of_default("scaling", 1.0),
        speed: FLOAT.optional_field_of_default("speed", 1.0),
    });
    record!(Weighted<ExplosionParticleInfo> { value: info, weight: mc_dfu::extra::NON_NEGATIVE_INT.field_of("weight") }).codec().list_of()
}

// ---- FloatProviders -------------------------------------------------------------------------------------------------------

fn float_provider_dispatch_arm(ty: &str) -> MapCodec<FloatProvider> {
    use FloatProvider as F;
    #[derive(Clone)]
    struct Constant {
        value: f32,
    }
    #[derive(Clone)]
    struct Uniform {
        min_inclusive: f32,
        max_exclusive: f32,
    }
    #[derive(Clone)]
    struct Normal {
        mean: f32,
        deviation: f32,
        min: f32,
        max: f32,
    }
    #[derive(Clone)]
    struct Trapezoid {
        min: f32,
        max: f32,
        plateau: f32,
    }
    match ty {
        "minecraft:constant" => record!(Constant { value: FLOAT.field_of("value") }).xmap(
            |c| F::constant(c.value),
            |f: &F| match f {
                F::Constant(v) => Constant { value: *v },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:uniform" => record!(Uniform { min_inclusive: FLOAT.field_of("min_inclusive"), max_exclusive: FLOAT.field_of("max_exclusive") })
            .validate(|u: Uniform| {
                if u.max_exclusive <= u.min_inclusive {
                    let m = format!(
                        "Max must be larger than min, min: {}, max: {}",
                        mc_core::math::java::java_float_to_string(u.min_inclusive),
                        mc_core::math::java::java_float_to_string(u.max_exclusive)
                    );
                    DataResult::error(move || m.clone())
                } else {
                    DataResult::success(u)
                }
            })
            .xmap(
                |u| F::Uniform { min: u.min_inclusive, max: u.max_exclusive },
                |f: &F| match f {
                    F::Uniform { min, max } => Uniform { min_inclusive: *min, max_exclusive: *max },
                    _ => unreachable!("dispatched by type"),
                },
            ),
        "minecraft:clamped_normal" => record!(Normal {
            mean: FLOAT.field_of("mean"),
            deviation: FLOAT.field_of("deviation"),
            min: FLOAT.field_of("min"),
            max: FLOAT.field_of("max"),
        })
        .validate(|c: Normal| {
            if c.max < c.min {
                let m = format!(
                    "Max must be larger than min: [{}, {}]",
                    mc_core::math::java::java_float_to_string(c.min),
                    mc_core::math::java::java_float_to_string(c.max)
                );
                DataResult::error(move || m.clone())
            } else {
                DataResult::success(c)
            }
        })
        .xmap(
            |c| F::ClampedNormal { mean: c.mean, deviation: c.deviation, min: c.min, max: c.max },
            |f: &F| match f {
                F::ClampedNormal { mean, deviation, min, max } => Normal { mean: *mean, deviation: *deviation, min: *min, max: *max },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:trapezoid" => record!(Trapezoid { min: FLOAT.field_of("min"), max: FLOAT.field_of("max"), plateau: FLOAT.field_of("plateau") })
            .validate(|c: Trapezoid| {
                let jf = mc_core::math::java::java_float_to_string;
                if c.max < c.min {
                    let m = format!("Max must be larger than min: [{}, {}]", jf(c.min), jf(c.max));
                    DataResult::error(move || m.clone())
                } else if c.plateau > c.max - c.min {
                    let m = format!("Plateau can at most be the full span: [{}, {}]", jf(c.min), jf(c.max));
                    DataResult::error(move || m.clone())
                } else {
                    DataResult::success(c)
                }
            })
            .xmap(
                |c| F::Trapezoid { min: c.min, max: c.max, plateau: c.plateau },
                |f: &F| match f {
                    F::Trapezoid { min, max, plateau } => Trapezoid { min: *min, max: *max, plateau: *plateau },
                    _ => unreachable!("dispatched by type"),
                },
            ),
        _ => unreachable!("type_codec only yields registered names"),
    }
}

/// `FloatProviders.CODEC`.
fn float_provider_base_codec() -> Codec<FloatProvider> {
    static CODEC: LazyLock<Codec<FloatProvider>> = LazyLock::new(|| {
        let dispatch = type_codec("minecraft:float_provider_type").dispatch(
            "type",
            |f: &FloatProvider| match f {
                FloatProvider::Constant(_) => "minecraft:constant",
                FloatProvider::Uniform { .. } => "minecraft:uniform",
                FloatProvider::ClampedNormal { .. } => "minecraft:clamped_normal",
                FloatProvider::Trapezoid { .. } => "minecraft:trapezoid",
            },
            |ty: &&'static str| float_provider_dispatch_arm(ty),
        );
        FLOAT.either(&dispatch).xmap(
            |e| match e {
                Either::Left(v) => FloatProvider::constant(v),
                Either::Right(f) => f,
            },
            |f: &FloatProvider| match f {
                FloatProvider::Constant(v) => Either::Left(*v),
                other => Either::Right(other.clone()),
            },
        )
    });
    CODEC.clone()
}

/// `FloatProviders.CODEC` / `codec(min)` / `codec(min, max)`.
pub fn float_provider_codec(min: Option<f32>, max: Option<f32>) -> Codec<FloatProvider> {
    let jf = mc_core::math::java::java_float_to_string;
    float_provider_base_codec().validate(move |v: FloatProvider| {
        if let Some(lo) = min {
            if v.min() < lo {
                let m = format!("Value provider too low: {} [{}-{}]", jf(lo), jf(v.min()), jf(v.max()));
                return DataResult::error(move || m.clone());
            }
        }
        if let Some(hi) = max {
            if v.max() > hi {
                let m = format!("Value provider too high: {} [{}-{}]", jf(hi), jf(v.min()), jf(v.max()));
                return DataResult::error(move || m.clone());
            }
        }
        DataResult::success(v)
    })
}

// ---- BlockState codecs ----------------------------------------------------------------------------------------------------

/// `BlockState.FULL_CODEC`: `{id, properties?}` where `properties` lists every property of the block.
pub fn block_state_full_codec() -> Codec<BlockState> {
    static CODEC: LazyLock<Codec<BlockState>> = LazyLock::new(|| {
        registry_by_name_codec::<Block>().dispatch(
            "id",
            |s: &BlockState| s.block(),
            |block: &Block| {
                let block = *block;
                let default = block.default_state();
                if block.properties().len() == 0 {
                    return MapCodec::unit(default);
                }
                STRING
                    .unbounded_map(&STRING)
                    .lenient_optional_field_of("properties")
                    .xmap(
                        move |props: Option<Vec<(String, String)>>| match props {
                            None => default,
                            Some(props) => {
                                // each property: the named string value, or the default state's value when absent/invalid
                                let mut state = default;
                                for (name, value) in &props {
                                    if let Some(next) = state.with_property_str(name, value) {
                                        state = next;
                                    }
                                }
                                state
                            }
                        },
                        |s: &BlockState| Some(s.values().map(|(def, i)| (def.name.to_owned(), def.values[i].to_owned())).collect()),
                    )
            },
        )
    });
    CODEC.clone()
}

/// `BlockState.CODEC`: a bare block id (default state) or the full form.
pub fn block_state_codec() -> Codec<BlockState> {
    registry_by_name_codec::<Block>().either(&block_state_full_codec()).xmap(
        |e| match e {
            Either::Left(block) => block.default_state(),
            Either::Right(state) => state,
        },
        |s: &BlockState| if *s == s.block().default_state() { Either::Left(s.block()) } else { Either::Right(*s) },
    )
}

// ---- ParticleTypes.CODEC --------------------------------------------------------------------------------------------------

fn argb_color_codec() -> Codec<i32> {
    static VECTOR4F: LazyLock<Codec<[f32; 4]>> = LazyLock::new(|| {
        FLOAT.list_of().comap_flat_map(|l| mc_dfu::extra::fixed_size(l, 4).map(|v| [v[0], v[1], v[2], v[3]]), |v: &[f32; 4]| v.to_vec())
    });
    INT.with_alternative_conv(&VECTOR4F, |v: [f32; 4]| {
        // `ARGB.colorFromFloat(v.w(), v.x(), v.y(), v.z())`
        let ch = |f: f32| ((f * 255.0).floor() as i32) & 0xFF;
        (ch(v[3]) << 24) | (ch(v[0]) << 16) | (ch(v[1]) << 8) | ch(v[2])
    })
}

fn scale_codec() -> Codec<f32> {
    FLOAT.validate(|v| {
        if (0.01..=4.0).contains(&v) {
            DataResult::success(v)
        } else {
            let m = format!("Value must be within range [0.01;4.0]: {}", mc_core::math::java::java_float_to_string(v));
            DataResult::error(move || m.clone())
        }
    })
}

fn particle_arm(t: ParticleType) -> MapCodec<ParticleOptions> {
    use ParticleOptions as P;
    let rgb = || mc_dfu::extra::RGB_COLOR_CODEC.clone();
    match t {
        ParticleType::BLOCK | ParticleType::BLOCK_MARKER | ParticleType::FALLING_DUST | ParticleType::DUST_PILLAR | ParticleType::BLOCK_CRUMBLE => {
            block_state_codec().field_of("block_state").xmap(
                move |state| P::Block { particle_type: t, state },
                |p: &P| match p {
                    P::Block { state, .. } => *state,
                    _ => unreachable!("dispatched by type"),
                },
            )
        }
        ParticleType::ENTITY_EFFECT | ParticleType::TINTED_LEAVES | ParticleType::FLASH => argb_color_codec().field_of("color").xmap(
            move |color| P::Color { particle_type: t, color },
            |p: &P| match p {
                P::Color { color, .. } => *color,
                _ => unreachable!("dispatched by type"),
            },
        ),
        ParticleType::DRAGON_BREATH => FLOAT
            .optional_field_of_default("power", 1.0)
            .xmap(
                move |power| P::Power { particle_type: t, power },
                |p: &P| match p {
                    P::Power { power, .. } => *power,
                    _ => unreachable!("dispatched by type"),
                },
            ),
        ParticleType::EFFECT | ParticleType::INSTANT_EFFECT => {
            #[derive(Clone)]
            struct Spell {
                color: i32,
                power: f32,
            }
            record!(Spell { color: rgb().optional_field_of_default("color", -1), power: FLOAT.optional_field_of_default("power", 1.0) }).xmap(
                move |s| P::Spell { particle_type: t, color: s.color, power: s.power },
                |p: &P| match p {
                    P::Spell { color, power, .. } => Spell { color: *color, power: *power },
                    _ => unreachable!("dispatched by type"),
                },
            )
        }
        ParticleType::GEYSER | ParticleType::GEYSER_PLUME => mc_dfu::extra::POSITIVE_INT.field_of("water_blocks").xmap(
            move |water_blocks| P::Geyser { particle_type: t, water_blocks },
            |p: &P| match p {
                P::Geyser { water_blocks, .. } => *water_blocks,
                _ => unreachable!("dispatched by type"),
            },
        ),
        ParticleType::GEYSER_BASE | ParticleType::GEYSER_POOF => {
            struct Base {
                water_blocks: i32,
                burst_impulse_base: f32,
            }
            record!(Base {
                water_blocks: mc_dfu::extra::POSITIVE_INT.field_of("water_blocks"),
                burst_impulse_base: FLOAT.field_of("burst_impulse_base"),
            })
            .xmap(
                move |b| P::GeyserBase { particle_type: t, water_blocks: b.water_blocks, burst_impulse_base: b.burst_impulse_base },
                |p: &P| match p {
                    P::GeyserBase { water_blocks, burst_impulse_base, .. } => Base { water_blocks: *water_blocks, burst_impulse_base: *burst_impulse_base },
                    _ => unreachable!("dispatched by type"),
                },
            )
        }
        ParticleType::DUST => {
            struct Dust {
                color: i32,
                scale: f32,
            }
            record!(Dust { color: rgb().field_of("color"), scale: scale_codec().field_of("scale") }).xmap(
                |d| P::Dust { color: d.color, scale: d.scale.clamp(0.01, 4.0) },
                |p: &P| match p {
                    P::Dust { color, scale } => Dust { color: *color, scale: *scale },
                    _ => unreachable!("dispatched by type"),
                },
            )
        }
        ParticleType::DUST_COLOR_TRANSITION => {
            struct Transition {
                from_color: i32,
                to_color: i32,
                scale: f32,
            }
            record!(Transition {
                from_color: rgb().field_of("from_color"),
                to_color: rgb().field_of("to_color"),
                scale: scale_codec().field_of("scale"),
            })
            .xmap(
                |d| P::DustColorTransition { from_color: d.from_color, to_color: d.to_color, scale: d.scale.clamp(0.01, 4.0) },
                |p: &P| match p {
                    P::DustColorTransition { from_color, to_color, scale } => {
                        Transition { from_color: *from_color, to_color: *to_color, scale: *scale }
                    }
                    _ => unreachable!("dispatched by type"),
                },
            )
        }
        ParticleType::SCULK_CHARGE => FLOAT.field_of("roll").xmap(
            |roll| P::SculkCharge { roll },
            |p: &P| match p {
                P::SculkCharge { roll } => *roll,
                _ => unreachable!("dispatched by type"),
            },
        ),
        ParticleType::SHRIEK => INT.field_of("delay").xmap(
            |delay| P::Shriek { delay },
            |p: &P| match p {
                P::Shriek { delay } => *delay,
                _ => unreachable!("dispatched by type"),
            },
        ),
        ParticleType::ITEM => ItemStackTemplate::codec().field_of("item").xmap(
            P::Item,
            |p: &P| match p {
                P::Item(item) => item.clone(),
                _ => unreachable!("dispatched by type"),
            },
        ),
        ParticleType::VIBRATION => {
            struct Vibration {
                destination: ParticlePositionSource,
                arrival_in_ticks: i32,
            }
            record!(Vibration {
                destination: safe_position_source_codec().field_of("destination"),
                arrival_in_ticks: INT.field_of("arrival_in_ticks"),
            })
            .xmap(
                |v| P::Vibration { destination: v.destination, arrival_in_ticks: v.arrival_in_ticks },
                |p: &P| match p {
                    P::Vibration { destination, arrival_in_ticks } => Vibration { destination: *destination, arrival_in_ticks: *arrival_in_ticks },
                    _ => unreachable!("dispatched by type"),
                },
            )
        }
        ParticleType::TRAIL => {
            struct Trail {
                target: Vec3,
                color: i32,
                duration: i32,
            }
            record!(Trail {
                target: vec3_codec().field_of("target"),
                color: rgb().field_of("color"),
                duration: mc_dfu::extra::POSITIVE_INT.field_of("duration"),
            })
            .xmap(
                |t| P::Trail { target: t.target, color: t.color, duration: t.duration },
                |p: &P| match p {
                    P::Trail { target, color, duration } => Trail { target: *target, color: *color, duration: *duration },
                    _ => unreachable!("dispatched by type"),
                },
            )
        }
        simple => MapCodec::unit(P::Simple(simple)),
    }
}

/// `VibrationParticleOption.SAFE_POSITION_SOURCE_CODEC`: `PositionSource.CODEC` minus entity sources.
fn safe_position_source_codec() -> Codec<ParticlePositionSource> {
    struct BlockPos {
        pos: mc_core::pos::BlockPos,
    }
    let block = record!(BlockPos {
        pos: INT
            .list_of()
            .comap_flat_map(
                |l| mc_dfu::extra::fixed_size(l, 3).map(|v| mc_core::pos::BlockPos::new(v[0], v[1], v[2])),
                |p: &mc_core::pos::BlockPos| vec![p.x(), p.y(), p.z()],
            )
            .field_of("pos"),
    });
    type_codec("minecraft:position_source_type").dispatch(
        "type",
        |p: &ParticlePositionSource| match p {
            ParticlePositionSource::Block(_) => "minecraft:block",
            ParticlePositionSource::Entity { .. } => "minecraft:entity",
        },
        move |ty: &&'static str| match *ty {
            "minecraft:block" => block.clone().xmap(
                |b| ParticlePositionSource::Block(b.pos),
                |p: &ParticlePositionSource| match p {
                    ParticlePositionSource::Block(pos) => BlockPos { pos: *pos },
                    _ => unreachable!("dispatched by type"),
                },
            ),
            _ => MapCodec::from_fns(
                |_, _: &ParticlePositionSource, _| {},
                |_, _| DataResult::error_str("Entity position sources are not allowed"),
                Vec::new,
            ),
        },
    )
}

/// `ParticleTypes.CODEC`.
pub fn particle_options_codec() -> Codec<ParticleOptions> {
    static CODEC: LazyLock<Codec<ParticleOptions>> = LazyLock::new(|| {
        type_codec("minecraft:particle_type").dispatch(
            "type",
            |p: &ParticleOptions| p.particle_type().name(),
            |ty: &&'static str| particle_arm(ParticleType::by_name(ty).expect("registered particle type")),
        )
    });
    CODEC.clone()
}

// ---- holders --------------------------------------------------------------------------------------------------------------

/// `DamageType.DIRECT_CODEC`.
fn damage_type_direct_codec() -> Codec<DamageType> {
    static SCALING: [(DamageScaling, &str); 3] = [
        (DamageScaling::Never, "never"),
        (DamageScaling::WhenCausedByLivingNonPlayer, "when_caused_by_living_non_player"),
        (DamageScaling::Always, "always"),
    ];
    static EFFECTS: [(DamageEffects, &str); 6] = [
        (DamageEffects::Hurt, "hurt"),
        (DamageEffects::Thorns, "thorns"),
        (DamageEffects::Drowning, "drowning"),
        (DamageEffects::Burning, "burning"),
        (DamageEffects::Poking, "poking"),
        (DamageEffects::Freezing, "freezing"),
    ];
    static DEATH: [(DeathMessageType, &str); 3] = [
        (DeathMessageType::Default, "default"),
        (DeathMessageType::FallVariants, "fall_variants"),
        (DeathMessageType::IntentionalGameDesign, "intentional_game_design"),
    ];
    record!(DamageType {
        msg_id: STRING.field_of("message_id"),
        scaling: string_enum_codec(&SCALING).field_of("scaling"),
        exhaustion: FLOAT.field_of("exhaustion"),
        effects: string_enum_codec(&EFFECTS).optional_field_of_default("effects", DamageEffects::Hurt),
        death_message_type: string_enum_codec(&DEATH).optional_field_of_default("death_message_type", DeathMessageType::Default),
    })
    .codec()
}

/// `DamageType.CODEC` (`RegistryCodecs.holder(DAMAGE_TYPE)`: a reference or an inline definition).
pub fn damage_type_holder_codec() -> Codec<Holder<DamageType>> {
    holder_file_codec("minecraft:damage_type", &damage_type_direct_codec(), true)
}

/// `GameEvent.CODEC` (`RegistryFixedCodec`).
pub fn game_event_codec() -> Codec<GameEvent> {
    super::codecs::holder_fixed_codec::<NoDirect>("minecraft:game_event").flat_xmap(
        |h| match h {
            Holder::Reference(id) => match GameEvent::by_name(&id.to_string()) {
                Some(e) => DataResult::success(e),
                None => DataResult::error_str("unknown game event"),
            },
            Holder::Direct(never) => match never {},
        },
        |e: &GameEvent| DataResult::success(Holder::Reference(e.identifier())),
    )
}

/// `ExtraCodecs.compactListCodec(SoundEvent.CODEC, SoundEvent.CODEC.sizeLimitedListOf(255))`.
pub fn play_sound_codec() -> Codec<Vec<Holder<SoundEvent>>> {
    let sound = SoundEvent::holder_codec();
    mc_dfu::extra::compact_list_codec_with(&sound, &sound.size_limited_list_of(255))
}

// ---- BlockPredicate / BlockStateProvider ----------------------------------------------------------------------------------

struct StaticLookup;

impl RegistryLookup for StaticLookup {
    fn contains(&self, registry: &str, id: &Identifier) -> Lookup {
        if mc_protocol::registry_context::registry_contains(registry, id) { Lookup::Yes } else { Lookup::No }
    }
    fn tag_exists(&self, registry: &str, tag: &Identifier) -> Lookup {
        if mc_protocol::registry_context::get().tag_exists(registry, tag) { Lookup::Yes } else { Lookup::No }
    }
}

fn convert_with_scope(schema: &Schema, json: &Value) -> Result<Tag, String> {
    super::codecs::with_scoped_ctx(|ctx| schema.convert(json, ctx), || schema.convert(json, &RegistryDecodeContext::new(&StaticLookup)))
}

fn dyn_to_json(d: &Dyn) -> Result<Value, String> {
    serde_json::from_str(&d.to_json_string()).map_err(|e| e.to_string())
}

thread_local! {
    static GEN_ENV: OnceCell<GenEnv> = const { OnceCell::new() };
}

/// The worldgen data the levelgen loaders decode with (the vanilla data pack; `matching_biomes` needs it).
struct GenEnv {
    worldgen: mc_worldgen::data::WorldgenRegistries,
    biomes: mc_worldgen::biome::BiomeRegistry,
    source: Arc<dyn mc_worldgen::data::WorldgenDataSource>,
}

fn with_gen_ctx<R>(f: impl FnOnce(&crate::world::level::levelgen::registries::DecodeCtx<'_>) -> R) -> R {
    GEN_ENV.with(|cell| {
        let env = cell.get_or_init(|| {
            let source: Arc<dyn mc_worldgen::data::WorldgenDataSource> = Arc::new(mc_worldgen::data::McDataSource);
            let worldgen = mc_worldgen::data::WorldgenRegistries::new(source.clone());
            let biomes = mc_worldgen::biome::BiomeRegistry::load(&worldgen).expect("vanilla biomes");
            GenEnv { worldgen, biomes, source }
        });
        let cells = crate::world::level::levelgen::registries::Cells::new(env.source.clone());
        f(&crate::world::level::levelgen::registries::DecodeCtx { worldgen: &env.worldgen, biomes: &env.biomes, cells: &cells })
    })
}

fn vec3i_offset_schema(max: i32, required: bool) -> Schema {
    let _ = required;
    Schema::custom(move |v, _| {
        let list = v.as_array().ok_or_else(|| format!("Not a list: {v}"))?;
        let ints: Vec<i32> = list
            .iter()
            .map(|e| e.as_i64().or_else(|| e.as_f64().map(|f| f as i64)).map(|i| i as i32).ok_or_else(|| format!("Not a number: {e}")))
            .collect::<Result<_, _>>()?;
        if ints.len() != 3 {
            return Err(format!("Input is not a list of 3 elements"));
        }
        if ints.iter().any(|i| i.unsigned_abs() as i64 > max as i64) {
            return Err(format!(
                "Position out of range, expected at most {max}: Vec3i{{x={}, y={}, z={}}}",
                ints[0], ints[1], ints[2]
            ));
        }
        let mut out = mc_nbt::ListTag::new();
        for i in ints {
            out.push(Tag::Int(i));
        }
        Ok(Tag::List(out))
    })
}

fn offset_field() -> crate::registry::codec::Field {
    crate::registry::codec::default("offset", vec3i_offset_schema(16, false), Value::from(vec![0, 0, 0]))
}

fn block_state_schema() -> Schema {
    Schema::custom(|v, _| {
        let state = parse_block_state_json(v)?;
        Ok(block_state_tag(state, true))
    })
}

fn parse_block_state_json(v: &Value) -> Result<BlockState, String> {
    crate::world::level::levelgen::codec::parse_block_state(v).map_err(|e| e.to_string())
}

/// `BlockState.CODEC` / `FULL_CODEC` encoding of `state` (`full`: never the bare-id shortcut).
fn block_state_tag(state: BlockState, bare_ok: bool) -> Tag {
    if bare_ok && state == state.block().default_state() {
        return Tag::String(state.block().name().to_owned());
    }
    let mut compound = CompoundTag::new();
    compound.put("id", Tag::String(state.block().name().to_owned()));
    if state.block().properties().len() != 0 {
        let mut props = CompoundTag::new();
        for (def, i) in state.values() {
            props.put(def.name, Tag::String(def.values[i].to_owned()));
        }
        compound.put("properties", Tag::Compound(props));
    }
    Tag::Compound(compound)
}

fn vertical_anchor_schema() -> Schema {
    Schema::custom(|v, _| {
        let o = v.as_object().ok_or_else(|| format!("Not a map: {v}"))?;
        for key in ["absolute", "above_bottom", "below_top"] {
            if let Some(n) = o.get(key) {
                let n = n.as_i64().ok_or_else(|| format!("Not a number: {n}"))? as i32;
                if !(-2032..=2031).contains(&n) {
                    return Err(format!("Value {n} outside of range [-2032:2031]"));
                }
                let mut c = CompoundTag::new();
                c.put(key, Tag::Int(n));
                return Ok(Tag::Compound(c));
            }
        }
        Err("Input does not contain a key [absolute]: ".to_owned() + &v.to_string())
    })
}

fn block_predicate_schema() -> Schema {
    use crate::registry::codec::{FieldKind as K, req};
    fn resolve(name: &str) -> Option<Schema> {
        let block_set = |registry: &'static str| Schema::HolderSet { registry, always_list: false };
        let tag_key = Schema::custom(|v, ctx| {
            let s = v.as_str().ok_or_else(|| format!("Not a string: {v}"))?;
            let id = Identifier::read(s)?;
            let _ = ctx;
            Ok(Tag::String(id.to_string()))
        });
        let offset = || offset_field();
        Some(match name {
            "minecraft:all_of" | "minecraft:any_of" => Schema::Record(vec![req("predicates", Schema::list_of(Schema::Dispatch { key: "type", resolve }))]),
            "minecraft:not" => Schema::Record(vec![req("predicate", Schema::Dispatch { key: "type", resolve })]),
            "minecraft:true" => Schema::Record(vec![]),
            "minecraft:matching_blocks" => Schema::Record(vec![offset(), req("blocks", block_set("minecraft:block"))]),
            "minecraft:matching_fluids" => Schema::Record(vec![offset(), req("fluids", block_set("minecraft:fluid"))]),
            "minecraft:matching_block_tag" => Schema::Record(vec![offset(), req("tag", tag_key)]),
            "minecraft:matching_biomes" => Schema::Record(vec![req("biomes", block_set("minecraft:worldgen/biome"))]),
            "minecraft:has_sturdy_face" => Schema::Record(vec![offset(), req("direction", Schema::Enum(&["down", "up", "north", "south", "west", "east"]))]),
            "minecraft:solid" | "minecraft:replaceable" | "minecraft:inside_world_bounds" => Schema::Record(vec![offset()]),
            "minecraft:would_survive" => Schema::Record(vec![offset(), req("state", block_state_schema())]),
            "minecraft:unobstructed" => Schema::Record(vec![crate::registry::codec::default("offset", vec3i_offset_schema(i32::MAX, false), Value::from(vec![0, 0, 0]))]),
            "minecraft:height_range" => Schema::Record(vec![req("min_inclusive", vertical_anchor_schema()), req("max_inclusive", vertical_anchor_schema())]),
            "minecraft:volume_match" => Schema::Record(vec![
                req("min", vec3i_offset_schema(16, true)),
                req("max", vec3i_offset_schema(16, true)),
                req("match", Schema::Dispatch { key: "type", resolve }),
            ]),
            _ => return None,
        })
    }
    let _ = K::Required;
    Schema::Dispatch { key: "type", resolve }
}

/// A decoded `BlockPredicate` together with its `encodeStart(NbtOps)` form.
#[derive(Clone)]
pub struct BlockPredicateRef {
    pub predicate: Arc<dyn BlockPredicate>,
    pub nbt: Tag,
}

/// `BlockPredicate.CODEC`.
pub fn block_predicate_codec() -> Codec<BlockPredicateRef> {
    Codec::from_fns(
        |ops, input: &BlockPredicateRef, _prefix| DataResult::success(mc_dfu::NBT.convert_to(ops, &Dyn::from(&input.nbt))),
        |ops, input| {
            let json = match dyn_to_json(input) {
                Ok(j) => j,
                Err(e) => return DataResult::error(move || e.clone()),
            };
            let nbt = match convert_with_scope(&block_predicate_schema(), &json) {
                Ok(t) => t,
                Err(e) => return DataResult::error(move || e.clone()),
            };
            match with_gen_ctx(|ctx| blockpredicates::decode(&json, ctx)) {
                Ok(predicate) => {
                    let _ = ops;
                    DataResult::success((BlockPredicateRef { predicate, nbt }, input.clone()))
                }
                Err(e) => {
                    let m = e.to_string();
                    DataResult::error(move || m.clone())
                }
            }
        },
    )
}

/// A decoded `BlockStateProvider` (inline) with its `encodeStart(NbtOps)` form.
#[derive(Clone)]
pub struct BlockStateProviderRef {
    pub provider: Arc<dyn BlockStateProvider>,
    pub nbt: Tag,
}

fn block_state_provider_schema() -> Schema {
    // `Codec.xor(BlockState.FULL_CODEC, TYPED_CODEC)`: an object with `id` is a state, with `type` a typed provider.
    Schema::custom(|v, ctx| {
        let o = v.as_object().ok_or_else(|| format!("Not a map: {v}"))?;
        if o.contains_key("id") {
            parse_block_state_json(v).map(|s| block_state_tag(s, false))
        } else {
            // typed providers other than `simple` keep the JSON-to-NBT narrowing (see Known gaps)
            let _ = ctx;
            Ok(crate::registry::codec::json_to_nbt(v))
        }
    })
}

/// `BlockStateProvider.CODEC` (`RegistryCodecs.holder(BLOCK_STATE_PROVIDER, DIRECT_CODEC)`).
pub fn block_state_provider_holder_codec() -> Codec<Holder<BlockStateProviderRef>> {
    let direct = Codec::from_fns(
        |ops, input: &BlockStateProviderRef, _prefix| DataResult::success(mc_dfu::NBT.convert_to(ops, &Dyn::from(&input.nbt))),
        |_ops, input| {
            let json = match dyn_to_json(input) {
                Ok(j) => j,
                Err(e) => return DataResult::error(move || e.clone()),
            };
            let nbt = match convert_with_scope(&block_state_provider_schema(), &json) {
                Ok(t) => t,
                Err(e) => return DataResult::error(move || e.clone()),
            };
            match with_gen_ctx(|ctx| stateproviders::decode_direct(&json, ctx)) {
                Ok(provider) => DataResult::success((BlockStateProviderRef { provider, nbt }, input.clone())),
                Err(e) => {
                    let m = e.to_string();
                    DataResult::error(move || m.clone())
                }
            }
        },
    );
    holder_file_codec("minecraft:worldgen/block_state_provider", &direct, true)
}


/// The provider of a `worldgen/block_state_provider` registry entry (decoded on demand from its JSON).
pub fn resolve_block_state_provider(registries: &crate::registry::RegistryAccess, id: &Identifier) -> Option<Arc<dyn BlockStateProvider>> {
    let entry = registries.registry("minecraft:worldgen/block_state_provider")?.entry_by_key(id)?;
    with_gen_ctx(|ctx| stateproviders::decode_direct(&entry.json, ctx)).ok()
}
