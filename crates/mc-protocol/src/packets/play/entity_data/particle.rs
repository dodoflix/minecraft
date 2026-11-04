//! `ParticleOptions` and `ParticleTypes.STREAM_CODEC` (registry id, then the payload of that particle type).
use mc_core::math::mth::clamp_f32;
use mc_core::phys::vec3::Vec3;
use mc_core::pos::block_pos::BlockPos;
use mc_data::ParticleType;
use mc_data::block::BlockState;

use crate::components::ItemStackTemplate;
use crate::codec::{DecodeResult, read_registry_id, write_varint};
use crate::{Decode, Encode};

/// `PositionSource.STREAM_CODEC`: `PositionSourceType` registry id, then the source.
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
#[mc(error = "No value with id {}")]
pub enum PositionSource {
    /// `BlockPositionSource`
    Block(BlockPos),
    /// `EntityPositionSource` (as decoded: by entity id)
    Entity {
        #[mc(var)]
        source_entity_id: i32,
        y_offset: f32,
    },
}

/// `ScalableParticleOptionsBase`: the constructor clamps the scale to `[0.01, 4.0]`.
fn clamp_scale(scale: f32) -> f32 {
    clamp_f32(scale, 0.01, 4.0)
}

/// Any `ParticleOptions` of vanilla's `ParticleTypes`. Variants that several particle types share carry the type;
/// it must be one of the types named in the variant docs (decoding only produces those; encoding writes the given id).
#[derive(Debug, Clone, PartialEq)]
pub enum ParticleOptions {
    /// A `SimpleParticleType` (no payload): every particle type not listed below.
    Simple(ParticleType),
    /// `BlockParticleOption`: `block`, `block_marker`, `falling_dust`, `dust_pillar`, `block_crumble`.
    Block {
        particle_type: ParticleType,
        state: BlockState,
    },
    /// `ColorParticleOption` (ARGB int): `entity_effect`, `tinted_leaves`, `flash`.
    Color {
        particle_type: ParticleType,
        color: i32,
    },
    /// `PowerParticleOption`: `dragon_breath`.
    Power {
        particle_type: ParticleType,
        power: f32,
    },
    /// `SpellParticleOption`: `effect`, `instant_effect`.
    Spell {
        particle_type: ParticleType,
        color: i32,
        power: f32,
    },
    /// `GeyserParticleOptions`: `geyser`, `geyser_plume`.
    Geyser {
        particle_type: ParticleType,
        water_blocks: i32,
    },
    /// `GeyserBaseParticleOptions`: `geyser_base`, `geyser_poof`.
    GeyserBase {
        particle_type: ParticleType,
        water_blocks: i32,
        burst_impulse_base: f32,
    },
    /// `DustParticleOptions` (`dust`)
    Dust { color: i32, scale: f32 },
    /// `DustColorTransitionOptions` (`dust_color_transition`)
    DustColorTransition {
        from_color: i32,
        to_color: i32,
        scale: f32,
    },
    /// `SculkChargeParticleOptions` (`sculk_charge`)
    SculkCharge { roll: f32 },
    /// `ShriekParticleOption` (`shriek`)
    Shriek { delay: i32 },
    /// `ItemParticleOption` (`item`)
    Item(ItemStackTemplate),
    /// `VibrationParticleOption` (`vibration`)
    Vibration {
        destination: PositionSource,
        arrival_in_ticks: i32,
    },
    /// `TrailParticleOption` (`trail`)
    Trail {
        target: Vec3,
        color: i32,
        duration: i32,
    },
}

impl ParticleOptions {
    /// `ParticleOptions.getType()`
    pub fn particle_type(&self) -> ParticleType {
        match self {
            Self::Simple(t)
            | Self::Block {
                particle_type: t, ..
            }
            | Self::Color {
                particle_type: t, ..
            }
            | Self::Power {
                particle_type: t, ..
            }
            | Self::Spell {
                particle_type: t, ..
            }
            | Self::Geyser {
                particle_type: t, ..
            }
            | Self::GeyserBase {
                particle_type: t, ..
            } => *t,
            Self::Dust { .. } => ParticleType::DUST,
            Self::DustColorTransition { .. } => ParticleType::DUST_COLOR_TRANSITION,
            Self::SculkCharge { .. } => ParticleType::SCULK_CHARGE,
            Self::Shriek { .. } => ParticleType::SHRIEK,
            Self::Item(_) => ParticleType::ITEM,
            Self::Vibration { .. } => ParticleType::VIBRATION,
            Self::Trail { .. } => ParticleType::TRAIL,
        }
    }
}

impl Encode for ParticleOptions {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.particle_type().id() as i32);
        match self {
            Self::Simple(_) => {}
            Self::Block { state, .. } => state.encode(out),
            Self::Color { color, .. } => color.encode(out),
            Self::Power { power, .. } => power.encode(out),
            Self::Spell { color, power, .. } => {
                color.encode(out);
                power.encode(out);
            }
            Self::Geyser { water_blocks, .. } => water_blocks.encode(out),
            Self::GeyserBase {
                water_blocks,
                burst_impulse_base,
                ..
            } => {
                water_blocks.encode(out);
                burst_impulse_base.encode(out);
            }
            Self::Dust { color, scale } => {
                color.encode(out);
                clamp_scale(*scale).encode(out);
            }
            Self::DustColorTransition {
                from_color,
                to_color,
                scale,
            } => {
                from_color.encode(out);
                to_color.encode(out);
                clamp_scale(*scale).encode(out);
            }
            Self::Item(item) => item.encode(out),
            Self::SculkCharge { roll } => roll.encode(out),
            Self::Shriek { delay } => write_varint(out, *delay),
            Self::Vibration {
                destination,
                arrival_in_ticks,
            } => {
                destination.encode(out);
                write_varint(out, *arrival_in_ticks);
            }
            Self::Trail {
                target,
                color,
                duration,
            } => {
                target.encode(out);
                color.encode(out);
                write_varint(out, *duration);
            }
        }
    }
}

impl Decode for ParticleOptions {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let t: ParticleType = read_registry_id(input)?;
        Ok(match t {
            ParticleType::BLOCK
            | ParticleType::BLOCK_MARKER
            | ParticleType::FALLING_DUST
            | ParticleType::DUST_PILLAR
            | ParticleType::BLOCK_CRUMBLE => Self::Block {
                particle_type: t,
                state: Decode::decode(input)?,
            },
            ParticleType::ENTITY_EFFECT | ParticleType::TINTED_LEAVES | ParticleType::FLASH => {
                Self::Color {
                    particle_type: t,
                    color: Decode::decode(input)?,
                }
            }
            ParticleType::DRAGON_BREATH => Self::Power {
                particle_type: t,
                power: Decode::decode(input)?,
            },
            ParticleType::EFFECT | ParticleType::INSTANT_EFFECT => Self::Spell {
                particle_type: t,
                color: Decode::decode(input)?,
                power: Decode::decode(input)?,
            },
            ParticleType::GEYSER | ParticleType::GEYSER_PLUME => Self::Geyser {
                particle_type: t,
                water_blocks: Decode::decode(input)?,
            },
            ParticleType::GEYSER_BASE | ParticleType::GEYSER_POOF => Self::GeyserBase {
                particle_type: t,
                water_blocks: Decode::decode(input)?,
                burst_impulse_base: Decode::decode(input)?,
            },
            ParticleType::DUST => Self::Dust {
                color: Decode::decode(input)?,
                scale: clamp_scale(f32::decode(input)?),
            },
            ParticleType::DUST_COLOR_TRANSITION => Self::DustColorTransition {
                from_color: Decode::decode(input)?,
                to_color: Decode::decode(input)?,
                scale: clamp_scale(f32::decode(input)?),
            },
            ParticleType::SCULK_CHARGE => Self::SculkCharge {
                roll: Decode::decode(input)?,
            },
            ParticleType::SHRIEK => Self::Shriek {
                delay: crate::codec::read_varint(input)?,
            },
            ParticleType::VIBRATION => Self::Vibration {
                destination: Decode::decode(input)?,
                arrival_in_ticks: crate::codec::read_varint(input)?,
            },
            ParticleType::TRAIL => Self::Trail {
                target: Decode::decode(input)?,
                color: Decode::decode(input)?,
                duration: crate::codec::read_varint(input)?,
            },
            ParticleType::ITEM => Self::Item(Decode::decode(input)?),
            _ => Self::Simple(t),
        })
    }
}
