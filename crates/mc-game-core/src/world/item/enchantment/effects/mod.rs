//! `net.minecraft.world.item.enchantment.effects`: the effect types of enchantments and their codecs. Application lives in
//! [`apply`].

pub mod apply;
mod attribute_effect;
mod deferred;
mod impulse;
mod value_effect;

pub use attribute_effect::EnchantmentAttributeEffect;
pub use value_effect::EnchantmentValueEffect;

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::Vec3i;
use mc_data::{Block, EntityType, GameEvent, MobEffect};
use mc_dfu::{BOOL, Codec, DataResult, FLOAT, MapCodec, record};
use mc_protocol::components::{BlockItemStateProperties, Holder, RegistrySet, SoundEvent};
use mc_worldgen::provider::FloatProvider;

use super::codecs::{type_codec, vec3_codec, vec3i_codec};
use super::level_based_value::LevelBasedValue;
use super::value_codecs::{
    BlockPredicateRef, BlockStateProviderRef, ExplosionInteraction, ExplosionParticleInfo,
    Weighted, block_predicate_codec, block_state_provider_holder_codec, damage_type_holder_codec,
    explosion_particle_infos_codec, float_provider_codec, game_event_codec, particle_options_codec,
    play_sound_codec,
};
use crate::registry::typed::damage_type::DamageType;
use mc_protocol::packets::play::ParticleOptions;

const ENTITY_REGISTRY: &str = "minecraft:enchantment_entity_effect_type";
const LOCATION_REGISTRY: &str = "minecraft:enchantment_location_based_effect_type";

#[derive(Clone, Debug, PartialEq)]
pub struct ApplyMobEffect {
    pub to_apply: RegistrySet<MobEffect>,
    pub min_duration: LevelBasedValue,
    pub max_duration: LevelBasedValue,
    pub min_amplifier: LevelBasedValue,
    pub max_amplifier: LevelBasedValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DamageEntity {
    pub min_damage: LevelBasedValue,
    pub max_damage: LevelBasedValue,
    pub damage_type: Holder<DamageType>,
}

#[derive(Clone, Debug)]
pub struct ExplodeEffect {
    pub attribute_to_user: bool,
    pub damage_type: Option<Holder<DamageType>>,
    pub knockback_multiplier: Option<LevelBasedValue>,
    pub immune_blocks: Option<RegistrySet<Block>>,
    pub offset: Vec3,
    pub radius: LevelBasedValue,
    pub create_fire: bool,
    pub block_interaction: ExplosionInteraction,
    pub small_particle: ParticleOptions,
    pub large_particle: ParticleOptions,
    pub block_particles: Vec<Weighted<ExplosionParticleInfo>>,
    pub sound: Holder<SoundEvent>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ApplyEntityImpulse {
    pub direction: Vec3,
    pub coordinate_scale: Vec3,
    pub magnitude: LevelBasedValue,
}

#[derive(Clone, Debug)]
pub struct PlaySoundEffect {
    pub sound_events: Vec<Holder<SoundEvent>>,
    pub volume: FloatProvider,
    pub pitch: FloatProvider,
}

#[derive(Clone)]
pub struct ReplaceBlock {
    pub offset: Vec3i,
    pub predicate: Option<BlockPredicateRef>,
    pub block_state: Holder<BlockStateProviderRef>,
    pub trigger_game_event: Option<GameEvent>,
}

#[derive(Clone)]
pub struct ReplaceDisk {
    pub radius: LevelBasedValue,
    pub height: LevelBasedValue,
    pub offset: Vec3i,
    pub predicate: Option<BlockPredicateRef>,
    pub block_state: Holder<BlockStateProviderRef>,
    pub trigger_game_event: Option<GameEvent>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SetBlockProperties {
    pub properties: BlockItemStateProperties,
    pub offset: Vec3i,
    pub trigger_game_event: Option<GameEvent>,
}

#[derive(Clone, Debug)]
pub struct SpawnParticlesEffect {
    pub particle: ParticleOptions,
    pub horizontal_position: PositionSource,
    pub vertical_position: PositionSource,
    pub horizontal_velocity: VelocitySource,
    pub vertical_velocity: VelocitySource,
    pub speed: FloatProvider,
}

/// `SpawnParticlesEffect.PositionSourceType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionSourceType {
    EntityPosition,
    BoundingBox,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionSource {
    pub ty: PositionSourceType,
    pub offset: f32,
    pub scale: f32,
}

#[derive(Clone, Debug)]
pub struct VelocitySource {
    pub movement_scale: f32,
    pub base: FloatProvider,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SummonEntityEffect {
    pub entity_types: RegistrySet<EntityType>,
    pub join_team: bool,
}

/// `EnchantmentEntityEffect`.
#[derive(Clone)]
pub enum EnchantmentEntityEffect {
    AllOf(Vec<EnchantmentEntityEffect>),
    ApplyMobEffect(ApplyMobEffect),
    ChangeItemDamage { amount: LevelBasedValue },
    DamageEntity(DamageEntity),
    Explode(Box<ExplodeEffect>),
    Ignite { duration: LevelBasedValue },
    ApplyImpulse(ApplyEntityImpulse),
    ApplyExhaustion { amount: LevelBasedValue },
    PlaySound(PlaySoundEffect),
    ReplaceBlock(ReplaceBlock),
    ReplaceDisk(ReplaceDisk),
    RunFunction { function: Identifier },
    SetBlockProperties(SetBlockProperties),
    SpawnParticles(SpawnParticlesEffect),
    SummonEntity(SummonEntityEffect),
}

/// `EnchantmentLocationBasedEffect`: the entity effects (applied on every block change) plus `attribute`.
#[derive(Clone)]
pub enum EnchantmentLocationBasedEffect {
    AllOf(Vec<EnchantmentLocationBasedEffect>),
    Attribute(EnchantmentAttributeEffect),
    /// Never `EnchantmentEntityEffect::AllOf` (that is [`EnchantmentLocationBasedEffect::AllOf`]).
    Entity(EnchantmentEntityEffect),
}

impl EnchantmentEntityEffect {
    fn type_name(&self) -> &'static str {
        match self {
            EnchantmentEntityEffect::AllOf(_) => "minecraft:all_of",
            EnchantmentEntityEffect::ApplyMobEffect(_) => "minecraft:apply_mob_effect",
            EnchantmentEntityEffect::ChangeItemDamage { .. } => "minecraft:change_item_damage",
            EnchantmentEntityEffect::DamageEntity(_) => "minecraft:damage_entity",
            EnchantmentEntityEffect::Explode(_) => "minecraft:explode",
            EnchantmentEntityEffect::Ignite { .. } => "minecraft:ignite",
            EnchantmentEntityEffect::ApplyImpulse(_) => "minecraft:apply_impulse",
            EnchantmentEntityEffect::ApplyExhaustion { .. } => "minecraft:apply_exhaustion",
            EnchantmentEntityEffect::PlaySound(_) => "minecraft:play_sound",
            EnchantmentEntityEffect::ReplaceBlock(_) => "minecraft:replace_block",
            EnchantmentEntityEffect::ReplaceDisk(_) => "minecraft:replace_disk",
            EnchantmentEntityEffect::RunFunction { .. } => "minecraft:run_function",
            EnchantmentEntityEffect::SetBlockProperties(_) => "minecraft:set_block_properties",
            EnchantmentEntityEffect::SpawnParticles(_) => "minecraft:spawn_particles",
            EnchantmentEntityEffect::SummonEntity(_) => "minecraft:summon_entity",
        }
    }

    /// `EnchantmentEntityEffect.CODEC`.
    pub fn codec() -> Codec<EnchantmentEntityEffect> {
        static CODEC: std::sync::LazyLock<Codec<EnchantmentEntityEffect>> =
            std::sync::LazyLock::new(|| {
                type_codec(ENTITY_REGISTRY).dispatch(
                    "type",
                    |e: &EnchantmentEntityEffect| e.type_name(),
                    |ty: &&'static str| entity_arm(ty),
                )
            });
        CODEC.clone()
    }
}

impl EnchantmentLocationBasedEffect {
    fn type_name(&self) -> &'static str {
        match self {
            EnchantmentLocationBasedEffect::AllOf(_) => "minecraft:all_of",
            EnchantmentLocationBasedEffect::Attribute(_) => "minecraft:attribute",
            EnchantmentLocationBasedEffect::Entity(e) => e.type_name(),
        }
    }

    /// `EnchantmentLocationBasedEffect.CODEC`.
    pub fn codec() -> Codec<EnchantmentLocationBasedEffect> {
        static CODEC: std::sync::LazyLock<Codec<EnchantmentLocationBasedEffect>> =
            std::sync::LazyLock::new(|| {
                type_codec(LOCATION_REGISTRY).dispatch(
                    "type",
                    |e: &EnchantmentLocationBasedEffect| e.type_name(),
                    |ty: &&'static str| location_arm(ty),
                )
            });
        CODEC.clone()
    }
}

fn level_value(name: &str) -> MapCodec<LevelBasedValue> {
    LevelBasedValue::codec().field_of(name)
}

struct AmountRec {
    amount: LevelBasedValue,
}
struct DurationRec {
    duration: LevelBasedValue,
}
struct FunctionRec {
    function: Identifier,
}
struct AllOfEntityRec {
    effects: Vec<EnchantmentEntityEffect>,
}
struct AllOfLocationRec {
    effects: Vec<EnchantmentLocationBasedEffect>,
}

/// The `MapCodec` of one non-`all_of` entity effect type (shared by the entity and location dispatches).
fn entity_leaf(ty: &str) -> MapCodec<EnchantmentEntityEffect> {
    use EnchantmentEntityEffect as E;
    match ty {
        "minecraft:apply_mob_effect" => record!(ApplyMobEffect {
            to_apply: RegistrySet::<MobEffect>::codec().field_of("to_apply"),
            min_duration: level_value("min_duration"),
            max_duration: level_value("max_duration"),
            min_amplifier: level_value("min_amplifier"),
            max_amplifier: level_value("max_amplifier"),
        })
        .xmap(E::ApplyMobEffect, |e: &E| match e {
            E::ApplyMobEffect(x) => x.clone(),
            _ => unreachable!("dispatched by type"),
        }),
        "minecraft:change_item_damage" => record!(AmountRec {
            amount: level_value("amount")
        })
        .xmap(
            |r| E::ChangeItemDamage { amount: r.amount },
            |e: &E| match e {
                E::ChangeItemDamage { amount } => AmountRec {
                    amount: amount.clone(),
                },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:damage_entity" => record!(DamageEntity {
            min_damage: level_value("min_damage"),
            max_damage: level_value("max_damage"),
            damage_type: damage_type_holder_codec().field_of("damage_type"),
        })
        .xmap(E::DamageEntity, |e: &E| match e {
            E::DamageEntity(x) => x.clone(),
            _ => unreachable!("dispatched by type"),
        }),
        "minecraft:explode" => record!(ExplodeEffect {
            attribute_to_user: BOOL.optional_field_of_default("attribute_to_user", false),
            damage_type: damage_type_holder_codec().optional_field_of("damage_type"),
            knockback_multiplier: LevelBasedValue::codec()
                .optional_field_of("knockback_multiplier"),
            immune_blocks: RegistrySet::<Block>::codec().optional_field_of("immune_blocks"),
            offset: vec3_codec().optional_field_of_default("offset", Vec3::ZERO),
            radius: level_value("radius"),
            create_fire: BOOL.optional_field_of_default("create_fire", false),
            block_interaction: ExplosionInteraction::codec().field_of("block_interaction"),
            small_particle: particle_options_codec().field_of("small_particle"),
            large_particle: particle_options_codec().field_of("large_particle"),
            block_particles: explosion_particle_infos_codec()
                .optional_field_of_default("block_particles", Vec::new()),
            sound: SoundEvent::holder_codec().field_of("sound"),
        })
        .xmap(
            |e| E::Explode(Box::new(e)),
            |e: &E| match e {
                E::Explode(x) => (**x).clone(),
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:ignite" => record!(DurationRec {
            duration: level_value("duration")
        })
        .xmap(
            |r| E::Ignite {
                duration: r.duration,
            },
            |e: &E| match e {
                E::Ignite { duration } => DurationRec {
                    duration: duration.clone(),
                },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:apply_impulse" => record!(ApplyEntityImpulse {
            direction: vec3_codec().field_of("direction"),
            coordinate_scale: vec3_codec().field_of("coordinate_scale"),
            magnitude: level_value("magnitude"),
        })
        .xmap(E::ApplyImpulse, |e: &E| match e {
            E::ApplyImpulse(x) => x.clone(),
            _ => unreachable!("dispatched by type"),
        }),
        "minecraft:apply_exhaustion" => record!(AmountRec {
            amount: level_value("amount")
        })
        .xmap(
            |r| E::ApplyExhaustion { amount: r.amount },
            |e: &E| match e {
                E::ApplyExhaustion { amount } => AmountRec {
                    amount: amount.clone(),
                },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:play_sound" => record!(PlaySoundEffect {
            sound_events: play_sound_codec().field_of("sound"),
            volume: float_provider_codec(Some(1.0E-5), Some(10.0)).field_of("volume"),
            pitch: float_provider_codec(Some(1.0E-5), Some(2.0)).field_of("pitch"),
        })
        .xmap(E::PlaySound, |e: &E| match e {
            E::PlaySound(x) => x.clone(),
            _ => unreachable!("dispatched by type"),
        }),
        "minecraft:replace_block" => record!(ReplaceBlock {
            offset: vec3i_codec().optional_field_of_default("offset", Vec3i::ZERO),
            predicate: block_predicate_codec().optional_field_of("predicate"),
            block_state: block_state_provider_holder_codec().field_of("block_state"),
            trigger_game_event: game_event_codec().optional_field_of("trigger_game_event"),
        })
        .xmap(E::ReplaceBlock, |e: &E| match e {
            E::ReplaceBlock(x) => x.clone(),
            _ => unreachable!("dispatched by type"),
        }),
        "minecraft:replace_disk" => record!(ReplaceDisk {
            radius: level_value("radius"),
            height: level_value("height"),
            offset: vec3i_codec().optional_field_of_default("offset", Vec3i::ZERO),
            predicate: block_predicate_codec().optional_field_of("predicate"),
            block_state: block_state_provider_holder_codec().field_of("block_state"),
            trigger_game_event: game_event_codec().optional_field_of("trigger_game_event"),
        })
        .xmap(E::ReplaceDisk, |e: &E| match e {
            E::ReplaceDisk(x) => x.clone(),
            _ => unreachable!("dispatched by type"),
        }),
        "minecraft:run_function" => record!(FunctionRec {
            function: mc_dfu::extra::IDENTIFIER.field_of("function")
        })
        .xmap(
            |r| E::RunFunction {
                function: r.function,
            },
            |e: &E| match e {
                E::RunFunction { function } => FunctionRec {
                    function: function.clone(),
                },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:set_block_properties" => record!(SetBlockProperties {
            properties: BlockItemStateProperties::codec().field_of("properties"),
            offset: vec3i_codec().optional_field_of_default("offset", Vec3i::ZERO),
            trigger_game_event: game_event_codec().optional_field_of("trigger_game_event"),
        })
        .xmap(E::SetBlockProperties, |e: &E| match e {
            E::SetBlockProperties(x) => x.clone(),
            _ => unreachable!("dispatched by type"),
        }),
        "minecraft:spawn_particles" => record!(SpawnParticlesEffect {
            particle: particle_options_codec().field_of("particle"),
            horizontal_position: PositionSource::codec()
                .codec()
                .field_of("horizontal_position"),
            vertical_position: PositionSource::codec()
                .codec()
                .field_of("vertical_position"),
            horizontal_velocity: VelocitySource::codec()
                .codec()
                .field_of("horizontal_velocity"),
            vertical_velocity: VelocitySource::codec()
                .codec()
                .field_of("vertical_velocity"),
            speed: float_provider_codec(None, None)
                .optional_field_of_default("speed", FloatProvider::constant(0.0)),
        })
        .xmap(E::SpawnParticles, |e: &E| match e {
            E::SpawnParticles(x) => x.clone(),
            _ => unreachable!("dispatched by type"),
        }),
        "minecraft:summon_entity" => record!(SummonEntityEffect {
            entity_types: RegistrySet::<EntityType>::codec().field_of("entity"),
            join_team: BOOL.optional_field_of_default("join_team", false),
        })
        .xmap(E::SummonEntity, |e: &E| match e {
            E::SummonEntity(x) => x.clone(),
            _ => unreachable!("dispatched by type"),
        }),
        other => unreachable!("not an entity effect leaf type: {other}"),
    }
}

fn entity_arm(ty: &str) -> MapCodec<EnchantmentEntityEffect> {
    if ty == "minecraft:all_of" {
        record!(AllOfEntityRec {
            effects: Codec::lazy(EnchantmentEntityEffect::codec)
                .list_of()
                .field_of("effects")
        })
        .xmap(
            |r| EnchantmentEntityEffect::AllOf(r.effects),
            |e: &EnchantmentEntityEffect| match e {
                EnchantmentEntityEffect::AllOf(effects) => AllOfEntityRec {
                    effects: effects.clone(),
                },
                _ => unreachable!("dispatched by type"),
            },
        )
    } else {
        entity_leaf(ty)
    }
}

fn location_arm(ty: &str) -> MapCodec<EnchantmentLocationBasedEffect> {
    use EnchantmentLocationBasedEffect as L;
    match ty {
        "minecraft:all_of" => record!(AllOfLocationRec {
            effects: Codec::lazy(EnchantmentLocationBasedEffect::codec)
                .list_of()
                .field_of("effects")
        })
        .xmap(
            |r| L::AllOf(r.effects),
            |e: &L| match e {
                L::AllOf(effects) => AllOfLocationRec {
                    effects: effects.clone(),
                },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:attribute" => {
            EnchantmentAttributeEffect::map_codec().xmap(L::Attribute, |e: &L| match e {
                L::Attribute(a) => a.clone(),
                _ => unreachable!("dispatched by type"),
            })
        }
        _ => entity_leaf(ty).xmap(L::Entity, |e: &L| match e {
            L::Entity(x) => x.clone(),
            _ => unreachable!("dispatched by type"),
        }),
    }
}

impl PositionSource {
    /// `PositionSource.CODEC` (a `MapCodec`).
    pub fn codec() -> MapCodec<PositionSource> {
        use mc_dfu::string_enum_codec;
        static TYPES: [(PositionSourceType, &str); 2] = [
            (PositionSourceType::EntityPosition, "entity_position"),
            (PositionSourceType::BoundingBox, "in_bounding_box"),
        ];
        record!(PositionSource {
            ty: string_enum_codec(&TYPES).field_of("type"),
            offset: FLOAT.optional_field_of_default("offset", 0.0),
            scale: mc_dfu::extra::POSITIVE_FLOAT.optional_field_of_default("scale", 1.0),
        })
        .validate(|p: PositionSource| {
            if p.ty == PositionSourceType::EntityPosition && p.scale != 1.0 {
                DataResult::error_str("Cannot scale an entity position coordinate source")
            } else {
                DataResult::success(p)
            }
        })
    }
}

impl VelocitySource {
    /// `VelocitySource.CODEC` (a `MapCodec`).
    pub fn codec() -> MapCodec<VelocitySource> {
        record!(VelocitySource {
            movement_scale: FLOAT.optional_field_of_default("movement_scale", 0.0),
            base: float_provider_codec(None, None)
                .optional_field_of_default("base", FloatProvider::constant(0.0)),
        })
    }
}

impl PositionSource {
    /// `getCoordinate(position, center, boundingBoxSpan, random)`.
    pub fn get_coordinate(
        &self,
        position: f64,
        center: f64,
        bounding_box_span: f32,
        random: &mut dyn mc_core::random::RandomSource,
    ) -> f64 {
        let span = bounding_box_span * self.scale;
        let base = match self.ty {
            PositionSourceType::EntityPosition => position,
            PositionSourceType::BoundingBox => {
                center + (random.next_double() - 0.5) * f64::from(span)
            }
        };
        base + f64::from(self.offset)
    }
}

impl VelocitySource {
    /// `getVelocity(movement, random)`.
    pub fn get_velocity(
        &self,
        movement: f64,
        random: &mut dyn mc_core::random::RandomSource,
    ) -> f64 {
        movement * f64::from(self.movement_scale) + f64::from(self.base.sample(random))
    }
}
