//! `SpeleothemUtils`: shared helpers of the speleothem (pointed dripstone / sulfur spike) and large dripstone features.

use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::enums::SpeleothemThickness;
use mc_data::props;
use mc_game_core::world::level::biome_ext::is_water_at;
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::codec::BlockSet;
use mc_game_core::world::level::{BlockGetter, LevelAccessor};

/// `getSpeleothemHeight(xzDistanceFromCenter, speleothemRadius, scale, bluntness)`.
pub fn get_speleothem_height(
    mut xz_distance_from_center: f64,
    speleothem_radius: f64,
    scale: f64,
    bluntness: f64,
) -> f64 {
    if xz_distance_from_center < bluntness {
        xz_distance_from_center = bluntness;
    }
    let cutoff = 0.384;
    let r = xz_distance_from_center / speleothem_radius * cutoff;
    let part1 = 0.75 * r.powf(1.3333333333333333);
    let part2 = r.powf(0.6666666666666666);
    let part3 = 0.3333333333333333 * r.ln();
    let mut height_relative_to_max_radius = scale * (part1 - part2 - part3);
    height_relative_to_max_radius = height_relative_to_max_radius.max(0.0);
    height_relative_to_max_radius / cutoff * speleothem_radius
}

/// `isCircleMostlyEmbeddedInStone(level, center, xzRadius)`.
pub fn is_circle_mostly_embedded_in_stone(
    level: &dyn BlockGetter,
    center: BlockPos,
    xz_radius: i32,
) -> bool {
    if is_empty_or_water_or_lava_at(level, center) {
        return false;
    }
    let angle_increment = 6.0f32 / xz_radius as f32;
    let two_pi = (std::f64::consts::PI * 2.0) as f32;
    let mut angle = 0.0f32;
    while angle < two_pi {
        let dx = (mth::cos(angle as f64) * xz_radius as f32) as i32;
        let dz = (mth::sin(angle as f64) * xz_radius as f32) as i32;
        if is_empty_or_water_or_lava_at(level, center.offset(dx, 0, dz)) {
            return false;
        }
        angle += angle_increment;
    }
    true
}

/// `isEmptyOrWater(level, pos)`.
pub fn is_empty_or_water_at(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    is_empty_or_water(level.get_block_state(pos))
}

/// `isEmptyOrWaterOrLava(level, pos)`.
pub fn is_empty_or_water_or_lava_at(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    is_empty_or_water_or_lava(level.get_block_state(pos))
}

/// `createPointedBlock(direction, thickness, pointedBlock)`.
fn create_pointed_block(
    direction: Direction,
    thickness: SpeleothemThickness,
    pointed_block: Block,
) -> BlockState {
    pointed_block
        .default_state()
        .set(props::VERTICAL_DIRECTION, direction)
        .set(props::SPELEOTHEM_THICKNESS, thickness)
}

/// `buildBaseToTipColumn(direction, totalLength, mergedTip, consumer, pointedBlock)`.
pub fn build_base_to_tip_column(
    direction: Direction,
    total_length: i32,
    merged_tip: bool,
    mut consumer: impl FnMut(BlockState),
    pointed_block: Block,
) {
    if total_length >= 3 {
        consumer(create_pointed_block(
            direction,
            SpeleothemThickness::Base,
            pointed_block,
        ));
        for _ in 0..total_length - 3 {
            consumer(create_pointed_block(
                direction,
                SpeleothemThickness::Middle,
                pointed_block,
            ));
        }
    }
    if total_length >= 2 {
        consumer(create_pointed_block(
            direction,
            SpeleothemThickness::Frustum,
            pointed_block,
        ));
    }
    if total_length >= 1 {
        consumer(create_pointed_block(
            direction,
            if merged_tip {
                SpeleothemThickness::TipMerge
            } else {
                SpeleothemThickness::Tip
            },
            pointed_block,
        ));
    }
}

/// `growSpeleothem(level, startPos, tipDirection, height, mergedTip, baseBlock, pointedBlock, replaceableBlocks)`.
pub fn grow_speleothem(
    level: &mut dyn LevelAccessor,
    start_pos: BlockPos,
    tip_direction: Direction,
    height: i32,
    merged_tip: bool,
    base_block: Block,
    pointed_block: Block,
    replaceable_blocks: &BlockSet,
) {
    if is_base(
        level.get_block_state(start_pos.relative(tip_direction.get_opposite())),
        base_block,
        replaceable_blocks,
    ) {
        let mut pos = start_pos;
        build_base_to_tip_column(
            tip_direction,
            height,
            merged_tip,
            |mut state| {
                if state.block() == pointed_block {
                    state = state.set(props::WATERLOGGED, is_water_at(&*level, pos));
                }
                level.set_block(pos, state, 2, 512);
                pos = pos.relative(tip_direction);
            },
            pointed_block,
        );
    }
}

/// `placeBaseBlockIfPossible(level, pos, baseBlock, replaceableBlocks)`.
pub fn place_base_block_if_possible(
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    base_block: Block,
    replaceable_blocks: &BlockSet,
) -> bool {
    let state = level.get_block_state(pos);
    if replaceable_blocks.contains(state.block()) {
        level.set_block(pos, base_block.default_state(), 2, 512);
        true
    } else {
        false
    }
}

/// `isBaseOrLava(state, baseBlock, replaceableBlocks)`.
pub fn is_base_or_lava(
    state: BlockState,
    base_block: Block,
    replaceable_blocks: &BlockSet,
) -> bool {
    is_base(state, base_block, replaceable_blocks) || state.is_block(Block::LAVA)
}

/// `isBase(state, baseBlock, replaceableBlocks)`.
pub fn is_base(state: BlockState, base_block: Block, replaceable_blocks: &BlockSet) -> bool {
    state.is_block(base_block) || replaceable_blocks.contains(state.block())
}

/// `isEmptyOrWater(state)`.
pub fn is_empty_or_water(state: BlockState) -> bool {
    state.is_air() || state.is_block(Block::WATER)
}

/// `isNeitherEmptyNorWater(state)`.
pub fn is_neither_empty_nor_water(state: BlockState) -> bool {
    !state.is_air() && !state.is_block(Block::WATER)
}

/// `isEmptyOrWaterOrLava(state)`.
pub fn is_empty_or_water_or_lava(state: BlockState) -> bool {
    state.is_air() || state.is_block(Block::WATER) || state.is_block(Block::LAVA)
}
