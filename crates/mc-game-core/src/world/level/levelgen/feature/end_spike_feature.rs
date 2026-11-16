//! `EndSpikeFeature` (`end_spike`): the obsidian pillars of the End with their end crystals.

use super::end_spike::{EndSpike, get_spikes_for_level};
use super::{FEATURE_TYPES, Feature, set_block};
use crate::world::level::levelgen::GenEnv;
use crate::world::level::levelgen::WorldGenLevel;
use crate::world::level::levelgen::codec::{
    Fields, LoadResult, TypeEntry, parse_block_pos, parse_bool, parse_int, parse_list,
};
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource, generate_unique_seed};
use mc_core::uuid_util::uuid_to_int_array;
use mc_data::block::Block;
use mc_data::props;
use mc_nbt::{CompoundTag, ListTag};
use serde_json::Value;
use std::sync::Arc;

/// `EndSpike.CODEC`.
fn spike_from_json(value: &Value) -> LoadResult<EndSpike> {
    let fields = Fields::new(value)?;
    Ok(EndSpike::new(
        fields.optional("centerX", parse_int)?.unwrap_or(0),
        fields.optional("centerZ", parse_int)?.unwrap_or(0),
        fields.optional("radius", parse_int)?.unwrap_or(0),
        fields.optional("height", parse_int)?.unwrap_or(0),
        fields.optional("guarded", parse_bool)?.unwrap_or(false),
    ))
}

pub struct EndSpikeFeature {
    pub spikes: Vec<EndSpike>,
    pub crystal_invulnerable: bool,
    pub crystal_beam_target: Option<BlockPos>,
}

impl EndSpikeFeature {
    pub fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(EndSpikeFeature {
            spikes: fields.required("spikes", |v| parse_list(v, spike_from_json))?,
            crystal_invulnerable: fields
                .optional("crystal_invulnerable", parse_bool)?
                .unwrap_or(false),
            crystal_beam_target: fields.optional("crystal_beam_target", parse_block_pos)?,
        }))
    }

    fn place_spike(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        spike: &EndSpike,
    ) {
        let radius = spike.get_radius();
        let min_y = level.get_min_y();
        for pos in BlockPos::between_closed(
            &BlockPos::new(
                spike.get_center_x() - radius,
                min_y,
                spike.get_center_z() - radius,
            ),
            &BlockPos::new(
                spike.get_center_x() + radius,
                spike.get_height() + 10,
                spike.get_center_z() + radius,
            ),
        ) {
            if pos.dist_to_low_corner_sqr(
                spike.get_center_x() as f64,
                pos.y() as f64,
                spike.get_center_z() as f64,
            ) <= (radius * radius + 1) as f64
                && pos.y() < spike.get_height()
            {
                set_block(level, pos, Block::OBSIDIAN.default_state());
            } else if pos.y() > 65 {
                set_block(level, pos, Block::AIR.default_state());
            }
        }

        if spike.is_guarded() {
            for dx in -2i32..=2 {
                for dz in -2i32..=2 {
                    for dy in 0..=3 {
                        let is_x_side = dx.abs() == 2;
                        let is_z_side = dz.abs() == 2;
                        let top = dy == 3;
                        if is_x_side || is_z_side || top {
                            let x_edge = dx == -2 || dx == 2 || top;
                            let z_edge = dz == -2 || dz == 2 || top;
                            let state = Block::IRON_BARS
                                .default_state()
                                .set(props::NORTH, x_edge && dz != -2)
                                .set(props::SOUTH, x_edge && dz != 2)
                                .set(props::WEST, z_edge && dx != -2)
                                .set(props::EAST, z_edge && dx != 2);
                            set_block(
                                level,
                                BlockPos::new(
                                    spike.get_center_x() + dx,
                                    spike.get_height() + dy,
                                    spike.get_center_z() + dz,
                                ),
                                state,
                            );
                        }
                    }
                }
            }
        }

        // `EntityTypes.END_CRYSTAL.create(..)`, `setBeamTarget`, `setPermanentlyInvulnerable`, `snapTo(..)`, `addFreshEntity`.
        let x = spike.get_center_x() as f64 + 0.5;
        let y = (spike.get_height() + 1) as f64;
        let z = spike.get_center_z() as f64 + 0.5;
        let yaw = random.next_float() * 360.0;
        level.add_fresh_entity_nbt(end_crystal_tag(
            x,
            y,
            z,
            yaw,
            self.crystal_beam_target,
            self.crystal_invulnerable,
        ));
        let crystal_pos = BlockPos::new(mth::floor_f64(x), mth::floor_f64(y), mth::floor_f64(z));
        set_block(level, crystal_pos.below(), Block::BEDROCK.default_state());
        let fire =
            crate::world::level::block::impls::base_fire_block::get_state(&*level, crystal_pos);
        set_block(level, crystal_pos, fire);
    }
}

/// `Entity.UUID`: random in vanilla as well (`Mth.createInsecureUUID` over a fresh random).
fn random_uuid() -> Vec<i32> {
    let mut random = LegacyRandomSource::new(generate_unique_seed());
    uuid_to_int_array(&mth::create_insecure_uuid(&mut random)).to_vec()
}

/// `EndCrystal.save`: the entity NBT as `ProtoChunk.addEntity` stores it (the UUID is random in vanilla too).
fn end_crystal_tag(
    x: f64,
    y: f64,
    z: f64,
    yaw: f32,
    beam_target: Option<BlockPos>,
    invulnerable: bool,
) -> CompoundTag {
    let mut tag = CompoundTag::new();
    tag.put_string("id", "minecraft:end_crystal");
    let mut pos = ListTag::new();
    for v in [x, y, z] {
        pos.push(v);
    }
    tag.put("Pos", pos);
    let mut motion = ListTag::new();
    for _ in 0..3 {
        motion.push(0.0f64);
    }
    tag.put("Motion", motion);
    let mut rotation = ListTag::new();
    rotation.push(yaw);
    rotation.push(0.0f32);
    tag.put("Rotation", rotation);
    tag.put_double("fall_distance", 0.0);
    tag.put_short("Fire", 0);
    tag.put_short("Air", 300);
    tag.put_boolean("OnGround", false);
    tag.put_boolean("Invulnerable", invulnerable);
    tag.put_int("PortalCooldown", 0);
    tag.put_int_array("UUID", random_uuid());
    if let Some(target) = beam_target {
        tag.put_int_array("beam_target", vec![target.x(), target.y(), target.z()]);
    }
    tag.put_boolean("ShowBottom", true);
    tag
}

impl Feature for EndSpikeFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        _generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        let computed;
        let spikes = if self.spikes.is_empty() {
            computed = get_spikes_for_level(level);
            &computed
        } else {
            &self.spikes
        };
        for spike in spikes {
            if spike.is_center_within_chunk(origin) {
                self.place_spike(level, random, spike);
            }
        }
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static END_SPIKE: TypeEntry<dyn Feature> = TypeEntry {
    name: "end_spike",
    decode: EndSpikeFeature::decode,
};
