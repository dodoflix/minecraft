//! `net.minecraft.world.level.levelgen.feature`: the `Feature` interface (26.3: a data-driven `worldgen/feature` entry is
//! the feature itself, config included), the type table and shared helpers.
//!
//! Each feature type lives in its own file under this module and registers itself:
//! ```ignore
//! #[linkme::distributed_slice(FEATURE_TYPES)]
//! static ORE: TypeEntry<dyn Feature> = TypeEntry { name: "ore", decode: OreFeature::decode };
//! ```
//! Area modules (`tree`, ...) add their own `pub mod` lines to this file.

pub mod end_gateway_feature;
pub mod end_platform_feature;

use super::codec::{LoadResult, TypeEntry, decode_dispatch};
use super::env::GenEnv;
use super::holder::Holder;
use super::registries::{self, DecodeCtx};
use super::world_gen_level::WorldGenLevel;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use serde_json::Value;
use std::sync::Arc;

pub mod fill_layer_feature;

/// `Feature`.
pub trait Feature: Send + Sync {
    /// `place(level, chunkGenerator, random, origin)`.
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool;

    /// `feature instanceof AbstractHugeMushroomFeature`: its `foliageRadius()` (`MushroomBlock.isValidBonemealTarget`).
    fn huge_mushroom_foliage_radius(&self) -> Option<i32> {
        None
    }

    /// `feature instanceof TreeFeature`: `trunkPlacer().getBaseHeight()` (`TreeGrower.getMinimumHeight`).
    fn tree_trunk_base_height(&self) -> Option<i32> {
        None
    }

    /// `getSubFeatures()`.
    fn get_sub_features(&self) -> Vec<Holder<dyn Feature>> {
        Vec::new()
    }
}

#[linkme::distributed_slice]
pub static FEATURE_TYPES: [TypeEntry<dyn Feature>];

/// `Feature.DIRECT_CODEC`.
pub fn decode_direct(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
    decode_dispatch(value, ctx, &FEATURE_TYPES)
}

/// `Feature.setBlock(level, pos, state)`.
/// `Feature.DIRECT_CODEC` as a check of an inline NBT definition (`/place feature`, `ResourceOrIdArgument`): the `type`
/// dispatch fails with the DFU texts (not a map, no `type`, unknown type), the feature is then decoded by the
/// levelgen loader, whose text for a bad config differs from DFU's.
pub fn check_direct_definition(tag: &mc_nbt::Tag) -> Result<(), String> {
    use mc_dfu::{Codec, DataResult, MapCodec};
    let key: Codec<String> = mc_dfu::extra::IDENTIFIER.flat_xmap(
        |id: mc_core::ident::Identifier| {
            if id.namespace() == "minecraft" && FEATURE_TYPES.iter().any(|e| e.name == id.path()) {
                DataResult::success(id.to_string())
            } else {
                DataResult::error(move || {
                    format!(
                        "Unknown registry key in ResourceKey[minecraft:root / minecraft:worldgen/feature_type]: {id}"
                    )
                })
            }
        },
        |name: &String| {
            DataResult::success(mc_core::ident::Identifier::read(name).expect("a checked type id"))
        },
    );
    let config = MapCodec::<()>::from_fns(
        |_, _, _| {},
        |_, map| {
            let mut object = serde_json::Map::new();
            for (k, v) in map.entries() {
                let parsed = k
                    .as_str()
                    .zip(serde_json::from_str(&v.to_json_string()).ok());
                if let Some((k, v)) = parsed {
                    object.insert(k.to_owned(), v);
                }
            }
            match crate::world::item::enchantment::value_codecs::with_gen_registries(|w, b| {
                registries::GenRegistries::decode_inline_feature(w, b, &Value::Object(object))
            }) {
                Ok(_) => DataResult::success(()),
                Err(e) => {
                    let text = e.to_string();
                    DataResult::error(move || text.clone())
                }
            }
        },
        Vec::new,
    );
    let codec = key.dispatch("type", |_: &()| String::new(), move |_| config.clone());
    mc_dfu::parse_nbt(&codec, tag).into_result().map(|_| ())
}

pub fn set_block(level: &mut dyn WorldGenLevel, pos: BlockPos, state: BlockState) {
    level.set_block_and_update(pos, state);
}

/// `Feature.safeSetBlock(level, pos, state, canReplace)`.
pub fn safe_set_block(
    level: &mut dyn WorldGenLevel,
    pos: BlockPos,
    state: BlockState,
    can_replace: impl Fn(BlockState) -> bool,
) {
    if can_replace(level.get_block_state(pos)) {
        level.set_block(pos, state, 2, 512);
    }
}

/// `Feature.markAboveForPostProcessing(level, placePos)`.
pub fn mark_above_for_post_processing(level: &mut dyn WorldGenLevel, place_pos: BlockPos) {
    let mut pos = place_pos;
    for _ in 0..2 {
        pos = pos.relative(Direction::Up);
        if level.get_block_state(pos).is_air() {
            return;
        }
        level.mark_pos_for_post_processing(pos);
    }
}

/// Registered by `no_op` (kept here so the table is never empty and the pipeline has a trivially testable feature).
pub struct NoOpFeature;

impl Feature for NoOpFeature {
    fn place(
        &self,
        _level: &mut dyn WorldGenLevel,
        _generator: &GenEnv,
        _random: &mut dyn RandomSource,
        _origin: BlockPos,
    ) -> bool {
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static NO_OP: TypeEntry<dyn Feature> = TypeEntry {
    name: "no_op",
    decode: |_, _| Ok(Arc::new(NoOpFeature)),
};

pub mod tree;
